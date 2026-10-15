//! One optional secure pager on its OWN PTY. Never gives up the host terminal,
//! forwards raw ANSI, executes shell input, or grants Git/GH/write authority.
pub(crate) mod filter;
mod identity;

use crate::providers::runner::CancellationToken;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use ratatui::style::{Color, Modifier, Style};
use std::{
    collections::VecDeque,
    fs::OpenOptions,
    io::{self, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU8, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(crate) const MAX_CAPTURE_V4: usize = 4 * 1024 * 1024;
const MAX_PROXY: usize = 16 * 1024 * 1024;
const MAX_PTY_OUTPUT: usize = 8 * 1024 * 1024;
const MAX_INPUT: usize = 64 * 1024;
const MAX_COMMAND: usize = 256;
const MAX_QUEUED: usize = 16;
const MAX_COLS: u16 = 240;
const MAX_ROWS: u16 = 60;
const POLL: Duration = Duration::from_millis(20);
const FRAME_TIME: Duration = Duration::from_millis(80);

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Bounds ONLY. Captured once at explicit Launch; cannot renew source access.
pub(crate) struct DisplayPermitV4 {
    deadline: Instant,
    cancel: CancellationToken,
}
impl DisplayPermitV4 {
    pub(crate) fn launch() -> Arc<Self> {
        Arc::new(Self {
            deadline: Instant::now() + Duration::from_secs(120),
            cancel: Default::default(),
        })
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    fn check(&self) -> io::Result<()> {
        if self.cancel.is_cancelled() {
            Err(invalid("External display cancelled"))
        } else if Instant::now() >= self.deadline {
            Err(invalid("External display120-second permit expired"))
        } else {
            Ok(())
        }
    }
}

/// One shared instance belongs to App, not a replaceable source/view. State2
/// permanently refuses relaunch after unproven cleanup until application restart.
#[derive(Default)]
#[doc(hidden)]
pub struct ViewerAdmissionV4(AtomicU8);
impl ViewerAdmissionV4 {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    pub(crate) fn blocked(&self) -> bool {
        self.0.load(Ordering::Acquire) != 0
    }
    fn acquire(self: &Arc<Self>) -> io::Result<Admission> {
        self.0
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|state| {
                invalid(if state == 2 {
                    "External viewer disabled: previous cleanup unproven"
                } else {
                    "Previous external viewer is still stopping"
                })
            })?;
        Ok(Admission {
            owner: self.clone(),
            finished: false,
        })
    }
}
struct Admission {
    owner: Arc<ViewerAdmissionV4>,
    finished: bool,
}
impl Admission {
    fn finish(&mut self, proven: bool) {
        self.owner
            .0
            .store(if proven { 0 } else { 2 }, Ordering::Release);
        self.finished = true;
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        if !self.finished {
            self.owner.0.store(2, Ordering::Release);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ViewerGeometryV4 {
    cols: u16,
    rows: u16,
    epoch: u64,
}
impl ViewerGeometryV4 {
    pub(crate) fn new(cols: u16, rows: u16, epoch: u64) -> io::Result<Self> {
        if !(12..=MAX_COLS).contains(&cols) || !(4..=MAX_ROWS).contains(&rows) {
            return Err(invalid("External PTY needs12..240 columns and4..60 rows"));
        }
        Ok(Self { cols, rows, epoch })
    }
    pub(crate) fn cols(self) -> u16 {
        self.cols
    }
    pub(crate) fn rows(self) -> u16 {
        self.rows
    }
    pub(crate) fn epoch(self) -> u64 {
        self.epoch
    }
    fn size(self) -> PtySize {
        PtySize {
            cols: self.cols,
            rows: self.rows,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    dev: u64,
    ino: u64,
    mode: u32,
    len: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}
impl Stamp {
    fn read(path: &Path) -> io::Result<Self> {
        let m = std::fs::symlink_metadata(path)?;
        if !m.is_file() || m.file_type().is_symlink() {
            return Err(invalid(
                "External input/program is not a regular nofollow file",
            ));
        }
        Ok(Self {
            dev: m.dev(),
            ino: m.ino(),
            mode: m.mode(),
            len: m.len(),
            mtime: (m.mtime(), m.mtime_nsec()),
            ctime: (m.ctime(), m.ctime_nsec()),
        })
    }
}
/// Constructed ONLY after an explicit bounded literal less --version probe.
/// Not arbitrary argv, not a shell setting, and not a security sandbox.
pub(crate) struct VerifiedPagerV4 {
    program: PathBuf,
    stamp: Stamp,
}
/// Unverified executable identity captured BEFORE its actual version child.
/// No pager can be launched from this closed candidate.
pub(crate) struct PagerProbeV4 {
    candidate: VerifiedPagerV4,
}
impl PagerProbeV4 {
    pub(crate) fn capture(program: PathBuf) -> io::Result<Self> {
        if !program.is_absolute() {
            return Err(invalid("Pager probe/path exceeds supported protocol"));
        }
        let stamp = Stamp::read(&program)?;
        if stamp.mode & 0o111 == 0 {
            return Err(invalid("Optional pager is not executable"));
        }
        Ok(Self {
            candidate: VerifiedPagerV4 { program, stamp },
        })
    }
    pub(crate) fn program(&self) -> &Path {
        &self.candidate.program
    }
    pub(crate) fn verify(self, stdout: &[u8]) -> io::Result<VerifiedPagerV4> {
        if stdout.len() > 4096 {
            return Err(invalid("Pager probe/path exceeds supported protocol"));
        }
        let first = std::str::from_utf8(stdout)
            .ok()
            .and_then(|s| s.lines().next())
            .ok_or_else(|| invalid("Unrecognized pager version"))?;
        let version = first
            .strip_prefix("less ")
            .and_then(|s| s.split_ascii_whitespace().next())
            .and_then(|s| s.split('.').next())
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|v| *v >= 704)
            .ok_or_else(|| invalid("Optional pager requires reviewed less704 or newer"))?;
        let _ = version;
        self.candidate.validate()?;
        Ok(self.candidate)
    }
}
impl VerifiedPagerV4 {
    #[cfg(test)]
    pub(crate) fn from_version(program: PathBuf, stdout: &[u8]) -> io::Result<Self> {
        PagerProbeV4::capture(program)?.verify(stdout)
    }
    fn validate(&self) -> io::Result<()> {
        if Stamp::read(&self.program)? == self.stamp {
            Ok(())
        } else {
            Err(invalid("Pager executable changed after its original probe"))
        }
    }
}

/// Only sanitized SGR/text is written here; original raw paths are never used
/// as temporary names. The input file and directory have one RAII owner.
pub(crate) struct PreparedPagerCaptureV4 {
    directory: tempfile::TempDir,
    directory_identity: (u64, u64),
    path: PathBuf,
    stamp: Stamp,
    bytes: usize,
}
impl PreparedPagerCaptureV4 {
    pub(crate) fn from_renderer(raw: &[u8]) -> io::Result<Self> {
        if raw.len() > MAX_CAPTURE_V4 {
            return Err(invalid("External capture exceeds4MiB"));
        }
        let mut safe = Vec::new();
        let mut filter = filter::TerminalFilterV4::capture();
        filter.feed(raw, &mut safe, MAX_CAPTURE_V4)?;
        filter.finish()?;
        let directory = tempfile::Builder::new()
            .prefix("ghtui-external-")
            .tempdir()?;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
        let path = directory.path().join("rendered.diff");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)?;
        let metadata = file.metadata()?;
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.nlink() != 1
            || !metadata.is_file()
        {
            return Err(invalid(
                "External input publication is not private owned regular file",
            ));
        }
        file.write_all(&safe)?;
        file.flush()?;
        file.set_permissions(std::fs::Permissions::from_mode(0o400))?;
        let bytes = safe.len();
        drop(safe);
        drop(file);
        let stamp = Stamp::read(&path)?;
        if stamp.len != bytes as u64 || stamp.mode & 0o777 != 0o400 {
            return Err(invalid("External input changed during publication"));
        }
        let d = std::fs::symlink_metadata(directory.path())?;
        let directory_identity = (d.dev(), d.ino());
        Ok(Self {
            directory,
            directory_identity,
            path,
            stamp,
            bytes,
        })
    }
    #[cfg(test)]
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
    pub(crate) fn prepared_charge(&self) -> usize {
        self.bytes + std::mem::size_of::<Self>() + self.path.capacity() + 4096
    }
    fn validate(&self) -> io::Result<()> {
        let d = std::fs::symlink_metadata(self.directory.path())?;
        if !d.is_dir()
            || d.file_type().is_symlink()
            || (d.dev(), d.ino()) != self.directory_identity
            || d.uid() != unsafe { libc::geteuid() }
            || d.mode() & 0o777 != 0o700
            || Stamp::read(&self.path)? != self.stamp
        {
            Err(invalid(
                "External private input no longer matches its publication",
            ))
        } else {
            Ok(())
        }
    }
    fn close(self) -> io::Result<()> {
        self.directory.close()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ViewerCommandV4 {
    Line(i8),
    Page(i8),
    First,
    Last,
    Match(i8),
    Search(String),
}
impl ViewerCommandV4 {
    fn bytes(&self) -> io::Result<Vec<u8>> {
        let bytes=match self{
            Self::Line(-1)=>b"k".to_vec(),Self::Line(1)=>b"j".to_vec(),
            Self::Page(-1)=>b"b".to_vec(),Self::Page(1)=>b" ".to_vec(),
            Self::First=>b"g".to_vec(),Self::Last=>b"G".to_vec(),
            Self::Match(-1)=>b"N".to_vec(),Self::Match(1)=>b"n".to_vec(),
            Self::Search(text) if !text.is_empty()&&text.len()<=MAX_COMMAND-2&&!text.chars().any(|c|c.is_control()||matches!(c,'\u{061c}'|'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'))=>{
                let mut bytes=Vec::with_capacity(text.len()+2);bytes.push(b'/');bytes.extend_from_slice(text.as_bytes());bytes.push(b'\n');bytes
            }
            _=>return Err(invalid("External input is not a fixed bounded pager action")),
        };
        Ok(bytes)
    }
}

pub(crate) struct ViewerCellV4 {
    pub(crate) text: String,
    pub(crate) style: Style,
    pub(crate) continuation: bool,
}
pub(crate) struct ViewerSnapshotV4 {
    pub(crate) owner: Arc<()>,
    pub(crate) geometry: ViewerGeometryV4,
    pub(crate) cells: Vec<ViewerCellV4>,
    pub(crate) cursor: (u16, u16),
    pub(crate) output_bytes: usize,
    pub(crate) retained: usize,
}
impl ViewerSnapshotV4 {
    pub(crate) fn belongs(&self, owner: &Arc<()>, geometry: ViewerGeometryV4) -> bool {
        Arc::ptr_eq(&self.owner, owner) && self.geometry == geometry
    }
}
pub(crate) struct ViewerExitV4 {
    pub(crate) reason: String,
    pub(crate) cleanup_proven: bool,
    pub(crate) leader: i32,
}
#[derive(Default)]
struct InputQueue {
    commands: VecDeque<Vec<u8>>,
    bytes: usize,
}
struct Shared {
    input: Mutex<InputQueue>,
    resize: Mutex<Option<ViewerGeometryV4>>,
    frames: Mutex<VecDeque<Arc<ViewerSnapshotV4>>>,
    exit: Mutex<Option<ViewerExitV4>>,
    cancel: CancellationToken,
    deadline: Instant,
    owner: Arc<()>,
    snapshot_peak: AtomicUsize,
}
pub(crate) struct PtyViewerHandleV4 {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
impl PtyViewerHandleV4 {
    pub(crate) fn launch(
        capture: PreparedPagerCaptureV4,
        pager: VerifiedPagerV4,
        permit: Arc<DisplayPermitV4>,
        geometry: ViewerGeometryV4,
        admission: &Arc<ViewerAdmissionV4>,
    ) -> io::Result<Self> {
        permit.check()?;
        capture.validate()?;
        pager.validate()?;
        let ticket = admission.acquire()?;
        let shared = Arc::new(Shared {
            input: Mutex::new(InputQueue::default()),
            resize: Mutex::new(None),
            frames: Mutex::new(VecDeque::with_capacity(2)),
            exit: Mutex::new(None),
            cancel: permit.cancel.clone(),
            deadline: permit.deadline,
            owner: Arc::new(()),
            snapshot_peak: AtomicUsize::new(0),
        });
        let worker_shared = shared.clone();
        // Retain recoverable ownership until the OS actually starts the
        // thread. A failed spawn must prove temporary deletion, not reset the
        // admission after merely dropping an opaque moved closure.
        let start = Arc::new(Mutex::new(Some((capture, pager, geometry, ticket))));
        let worker_start = start.clone();
        let worker = thread::Builder::new()
            .name("ghtui-external-pty".into())
            .spawn(move || {
                let payload = lock(&worker_start).take();
                if let Some((capture, pager, geometry, ticket)) = payload {
                    supervise(capture, pager, geometry, ticket, worker_shared);
                }
            });
        match worker {
            Ok(worker) => Ok(Self {
                shared,
                worker: Some(worker),
            }),
            Err(_) => {
                // The closure never started and could not spawn a child.
                if let Some((capture, _, _, mut ticket)) = lock(&start).take() {
                    ticket.finish(capture.close().is_ok());
                } else {
                    admission.0.store(2, Ordering::Release);
                }
                Err(invalid(
                    "Could not start external viewer supervisor; native retained",
                ))
            }
        }
    }
    pub(crate) fn owner(&self) -> &Arc<()> {
        &self.shared.owner
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.shared.deadline
    }
    pub(crate) fn cancel(&self) {
        self.shared.cancel.cancel();
        lock(&self.shared.frames).clear();
    }
    pub(crate) fn command(&self, command: ViewerCommandV4) -> io::Result<()> {
        check(&self.shared)?;
        let bytes = command.bytes()?;
        let mut queue = lock(&self.shared.input);
        if queue.commands.len() == MAX_QUEUED {
            return Err(invalid("External input queue full; action NOT sent"));
        }
        if queue
            .bytes
            .checked_add(bytes.len())
            .is_none_or(|n| n > MAX_INPUT)
        {
            return Err(invalid(
                "External input64KiB lifetime cap reached; action NOT sent",
            ));
        }
        queue.bytes += bytes.len();
        queue.commands.push_back(bytes);
        Ok(())
    }
    pub(crate) fn resize(&self, geometry: ViewerGeometryV4) {
        lock(&self.shared.frames).clear();
        *lock(&self.shared.resize) = Some(geometry);
    }
    pub(crate) fn snapshot(&self) -> Option<Arc<ViewerSnapshotV4>> {
        if self.shared.cancel.is_cancelled() {
            lock(&self.shared.frames).clear();
            return None;
        }
        let mut frames = lock(&self.shared.frames);
        let newest = frames.pop_back();
        frames.clear();
        newest
    }
    pub(crate) fn exit(&mut self) -> Option<ViewerExitV4> {
        let result = lock(&self.shared.exit).take();
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
        result
    }
}
impl Drop for PtyViewerHandleV4 {
    fn drop(&mut self) {
        self.cancel(); /*worker owns bounded cleanup; no foreground join*/
    }
}

struct ChildGuard {
    child: Box<dyn Child + Send + Sync>,
    owner: Option<identity::OwnedSession>,
    done: bool,
}
impl ChildGuard {
    fn cleanup(&mut self) -> identity::Cleanup {
        self.done = true;
        match &self.owner {
            Some(owner) => owner.cleanup(self.child.as_mut()),
            None => {
                // Capture failed before ANY SIGSTOP. The direct handle still owns
                // its unreaped leader; do not guess descendant/session ownership.
                let mut report = identity::Cleanup::default();
                report
                    .errors
                    .push("Could not prove external PTY session identity".into());
                if let Err(error) = self.child.kill() {
                    report
                        .errors
                        .push(error.to_string().chars().take(192).collect());
                }
                let deadline = Instant::now() + Duration::from_secs(3);
                while Instant::now() < deadline {
                    match self.child.try_wait() {
                        Ok(Some(_)) => return report,
                        Ok(None) => thread::sleep(Duration::from_millis(2)),
                        Err(error) => {
                            report
                                .errors
                                .push(error.to_string().chars().take(192).collect());
                            break;
                        }
                    }
                }
                if let Some(pid) = self.child.process_id().and_then(|p| i32::try_from(p).ok()) {
                    report.remaining.push(pid);
                }
                report
            }
        }
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.cleanup();
        }
    }
}

fn check(shared: &Shared) -> io::Result<()> {
    if shared.cancel.is_cancelled() {
        Err(invalid("External viewer cancelled"))
    } else if Instant::now() >= shared.deadline {
        Err(invalid("External viewer120-second permit expired"))
    } else {
        Ok(())
    }
}
fn supervise(
    capture: PreparedPagerCaptureV4,
    pager: VerifiedPagerV4,
    geometry: ViewerGeometryV4,
    mut admission: Admission,
    shared: Arc<Shared>,
) {
    let mut child_guard: Option<ChildGuard> = None;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> io::Result<()> {
        check(&shared)?;
        capture.validate()?;
        pager.validate()?;
        let pair = native_pty_system()
            .openpty(geometry.size())
            .map_err(|_| invalid("Could not allocate external PTY"))?;
        let fd = pair
            .master
            .as_raw_fd()
            .ok_or_else(|| invalid("Unix external PTY has no owned descriptor"))?;
        // SAFETY: live owned master FD; preserve existing descriptor flags.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut command = CommandBuilder::new(&pager.program);
        command.env_clear();
        command.cwd(capture.directory.path());
        command.umask(Some(0o077));
        for (key, value) in [
            ("TERM", "xterm-256color"),
            ("LANG", "C"),
            ("LESSCHARSET", "utf-8"),
            ("LESSSECURE", "1"),
            ("LESSSECURE_ALLOW", ""),
            ("LESSHISTFILE", "/dev/null"),
            ("LESSHISTSIZE", "0"),
            ("SHELL", "/bin/false"),
            ("PATH", "/usr/bin:/bin"),
        ] {
            command.env(key, value);
        }
        command.env("HOME", capture.directory.path());
        command.env("XDG_CONFIG_HOME", capture.directory.path());
        command.env("TMPDIR", capture.directory.path());
        command.args(["-R", "-K", "-L", "--"]);
        command.arg(&capture.path);
        check(&shared)?;
        capture.validate()?;
        pager.validate()?;
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|_| invalid("Optional pager could not be started"))?;
        child_guard = Some(ChildGuard {
            child,
            owner: None,
            done: false,
        });
        let guard = child_guard.as_mut().expect("just captured direct child");
        guard.owner = Some(identity::OwnedSession::capture(guard.child.as_ref())?);
        drop(pair.slave);
        check(&shared)?;
        pump(pair.master, geometry, &shared)
    }));
    let normal_eof = matches!(&result, Ok(Ok(())));
    let mut reason = match result {
        Ok(Ok(())) => "External pager closed".into(),
        Ok(Err(error)) => error.to_string().chars().take(256).collect(),
        Err(_) => "External pager worker failed; native retained".into(),
    };
    lock(&shared.frames).clear();
    let cleanup = child_guard.as_mut().map(ChildGuard::cleanup);
    let (mut proven, leader) = cleanup.map_or((true, 0), |report| {
        if normal_eof && report.exit_code.is_some_and(|code| code != 0) {
            reason = format!(
                "Optional pager exited{}; native retained",
                report.exit_code.unwrap_or(1)
            );
        }
        (report.proven(), report.leader)
    });
    // Drop/close private files BEFORE admitting another launch or announcing a
    // fully cleaned session. Never claim temp deletion solely from RAII intent.
    if capture.close().is_err() {
        proven = false;
        reason = "External private temporary cleanup incomplete; native retained".into();
    }
    admission.finish(proven);
    *lock(&shared.exit) = Some(ViewerExitV4 {
        reason,
        cleanup_proven: proven,
        leader,
    });
}

fn parser_reserve(geometry: ViewerGeometryV4) -> usize {
    // The pinned parser exposes cells, not private Vec capacities. This is a
    // conservative two-grid reserve including grow-on-insert row/cell slack,
    // temporary rows and parser state, NOT a fake actual-capacity observation.
    2 * (usize::from(geometry.rows) + 2)
        * (usize::from(geometry.cols) * 2 + 2)
        * std::mem::size_of::<vt100::Cell>()
        + 1024 * 1024
}
fn snapshot(
    parser: &vt100::Parser,
    geometry: ViewerGeometryV4,
    shared: &Shared,
    output_bytes: usize,
) -> io::Result<Arc<ViewerSnapshotV4>> {
    let count = usize::from(geometry.rows) * usize::from(geometry.cols);
    let mut cells = Vec::with_capacity(count);
    for row in 0..geometry.rows {
        for col in 0..geometry.cols {
            let cell = parser
                .screen()
                .cell(row, col)
                .ok_or_else(|| invalid("External parser geometry mismatch"))?;
            let mut style = Style::default()
                .fg(color(cell.fgcolor()))
                .bg(color(cell.bgcolor()));
            for (enabled, modifier) in [
                (cell.bold(), Modifier::BOLD),
                (cell.italic(), Modifier::ITALIC),
                (cell.underline(), Modifier::UNDERLINED),
                (cell.inverse(), Modifier::REVERSED),
            ] {
                if enabled {
                    style = style.add_modifier(modifier);
                }
            }
            cells.push(ViewerCellV4 {
                text: filter::safe_cell(&cell.contents()),
                style,
                continuation: cell.is_wide_continuation(),
            });
        }
    }
    let retained = std::mem::size_of::<ViewerSnapshotV4>()
        + 64
        + cells.capacity() * std::mem::size_of::<ViewerCellV4>()
        + cells.iter().map(|c| c.text.capacity()).sum::<usize>();
    // At most2 pending+1 displayed snapshot, including the new snapshot while
    // replacing the old pending frame. Charge the largest ACTUAL allocation
    // seen in this session: an in-flight prior-epoch frame may be larger after
    // a shrink. This integer is mechanical accounting, not retained history.
    let peak = shared
        .snapshot_peak
        .fetch_max(retained, Ordering::Relaxed)
        .max(retained);
    if peak
        .checked_mul(4)
        .and_then(|n| n.checked_add(parser_reserve(geometry) + 256 * 1024))
        .is_none_or(|n| n > MAX_PROXY)
    {
        return Err(invalid("External parser/snapshots exceed16MiB proxy bound"));
    }
    let result = Arc::new(ViewerSnapshotV4 {
        owner: shared.owner.clone(),
        geometry,
        cells,
        cursor: parser.screen().cursor_position(),
        output_bytes,
        retained,
    });
    check(shared)?;
    Ok(result)
}
fn color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn pump(
    master: Box<dyn MasterPty + Send>,
    mut geometry: ViewerGeometryV4,
    shared: &Shared,
) -> io::Result<()> {
    let fd = master
        .as_raw_fd()
        .ok_or_else(|| invalid("External master descriptor unavailable"))?;
    let mut parser = vt100::Parser::new(geometry.rows, geometry.cols, 0);
    let mut filter = filter::TerminalFilterV4::pager(geometry.rows, geometry.cols)?;
    let mut raw = [0u8; 8192];
    let mut safe = Vec::new();
    let mut total = 0usize;
    let mut pending = Vec::<u8>::new();
    let mut offset = 0usize;
    let mut last_frame = Instant::now() - FRAME_TIME;
    let mut dirty = false;
    loop {
        check(shared)?;
        if let Some(next) = lock(&shared.resize).take() {
            if next.epoch > geometry.epoch {
                geometry = next;
                // Recreate, not resize/reuse, so opaque old grid capacities
                // cannot accumulate. Actual SIGWINCH asks less to redraw.
                // Drop old grids BEFORE allocating their replacement; do not
                // hide an old+new parser peak behind a retained-only count.
                drop(parser);
                parser = vt100::Parser::new(geometry.rows, geometry.cols, 0);
                filter = filter::TerminalFilterV4::pager(geometry.rows, geometry.cols)?;
                master
                    .resize(geometry.size())
                    .map_err(|_| invalid("External child resize failed"))?;
                dirty = true;
            }
        }
        if offset == pending.len() {
            pending = lock(&shared.input).commands.pop_front().unwrap_or_default();
            offset = 0;
        }
        let mut pollfd = libc::pollfd {
            fd,
            events: libc::POLLIN
                | if offset < pending.len() {
                    libc::POLLOUT
                } else {
                    0
                },
            revents: 0,
        };
        // SAFETY: live owned master FD and one initialized poll descriptor.
        let ready = unsafe { libc::poll(&mut pollfd, 1, POLL.as_millis() as i32) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if pollfd.revents & libc::POLLNVAL != 0 {
            return Err(invalid("External PTY descriptor became invalid"));
        }
        if pollfd.revents & libc::POLLOUT != 0 && offset < pending.len() {
            let left = &pending[offset..];
            let written = unsafe { libc::write(fd, left.as_ptr().cast(), left.len()) };
            if written > 0 {
                offset += written as usize;
            } else if written < 0 {
                let error = io::Error::last_os_error();
                if !matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) {
                    return Err(error);
                }
            }
        }
        if pollfd.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
            let mut turn = 0usize;
            loop {
                check(shared)?;
                let read = unsafe { libc::read(fd, raw.as_mut_ptr().cast(), raw.len()) };
                if read == 0 {
                    return filter.finish();
                }
                if read < 0 {
                    let error = io::Error::last_os_error();
                    if error.raw_os_error() == Some(libc::EIO) {
                        return filter.finish();
                    }
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) {
                        break;
                    }
                    return Err(error);
                }
                let read = read as usize;
                total = total
                    .checked_add(read)
                    .filter(|n| *n <= MAX_PTY_OUTPUT)
                    .ok_or_else(|| invalid("External PTY8MiB output cap reached"))?;
                for input in raw[..read].chunks(256) {
                    check(shared)?;
                    safe.clear();
                    filter.feed(input, &mut safe, 64 * 1024)?;
                    for part in safe.chunks(256) {
                        check(shared)?;
                        parser.process(part);
                    }
                }
                dirty = true;
                turn += read;
                if turn >= 64 * 1024 {
                    break;
                }
            }
        }
        if dirty && last_frame.elapsed() >= FRAME_TIME {
            let frame = snapshot(&parser, geometry, shared, total)?;
            let mut frames = lock(&shared.frames);
            if frames.len() == 2 {
                frames.pop_front();
            }
            frames.push_back(frame);
            last_frame = Instant::now();
            dirty = false;
        }
    }
}

#[cfg(test)]
#[path = "owned_pty_viewer_v4/tests.rs"]
pub(crate) mod tests;
