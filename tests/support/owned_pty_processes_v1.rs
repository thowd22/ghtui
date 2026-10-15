//! Test-harness ownership only: never signal a name, path, or process group.
#![allow(dead_code)]
use portable_pty::Child;
use std::{
    collections::BTreeMap,
    io, thread,
    time::{Duration, Instant},
};

pub const MAX_SCANNED_PIDS: usize = 65_536;
pub const MAX_OWNED_PIDS: usize = 1_024;
const MAX_STAT_BYTES: u64 = 16_384;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessIdentity {
    pub pid: i32,
    pub parent: i32,
    pub session: i32,
    pub uid: u32,
    pub birth: (u64, u64),
    pub zombie: bool,
}
impl ProcessIdentity {
    fn same_process(&self, other: &Self) -> bool {
        self.pid == other.pid
            && self.session == other.session
            && self.uid == other.uid
            && self.birth == other.birth
    }
}

#[derive(Debug)]
pub struct OwnedPtySession {
    leader: ProcessIdentity,
}

#[derive(Default, Debug, serde::Serialize)]
pub struct CleanupReport {
    pub application_pid: i32,
    pub signalled: Vec<i32>,
    pub reaped: Vec<i32>,
    pub gone: Vec<i32>,
    pub remaining: Vec<i32>,
    pub errors: Vec<String>,
}

pub fn initialize_reaping() -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        static RESULT: std::sync::OnceLock<Result<(), i32>> = std::sync::OnceLock::new();
        let result = RESULT.get_or_init(|| {
            // SAFETY: changes only this test executable's orphan adoption. We
            // subsequently wait only for individually verified owned PIDs.
            if unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) } == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error()
                    .raw_os_error()
                    .unwrap_or(libc::EIO))
            }
        });
        if let Err(error) = result {
            return Err(io::Error::from_raw_os_error(*error));
        }
    }
    Ok(())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

impl OwnedPtySession {
    pub fn capture(pid: u32) -> io::Result<Self> {
        let pid = i32::try_from(pid).map_err(|_| invalid("invalid application PID"))?;
        let leader = process_identity(pid)?.ok_or_else(|| invalid("missing PTY leader"))?;
        // The pinned portable-pty backend calls setsid before exec. Refuse to
        // infer ownership if that contract or the direct-child identity fails.
        // SAFETY: getpid/geteuid/getsid only inspect the calling process.
        let current = unsafe { libc::getpid() };
        if pid <= 0
            || pid == current
            || leader.parent != current
            || leader.session != pid
            || leader.uid != unsafe { libc::geteuid() }
            || unsafe { libc::getsid(0) } == pid
        {
            return Err(invalid(
                "application is not our distinct owned PTY session leader",
            ));
        }
        Ok(Self { leader })
    }

    pub fn application_pid(&self) -> i32 {
        self.leader.pid
    }

    pub fn owns(&self, candidate: &ProcessIdentity) -> bool {
        candidate.pid > 0
            && candidate.pid != unsafe { libc::getpid() }
            && candidate.session == self.leader.session
            && candidate.uid == self.leader.uid
            && candidate.birth >= self.leader.birth
            && (candidate.pid != self.leader.pid || candidate.same_process(&self.leader))
    }

    fn members(&self) -> io::Result<Vec<ProcessIdentity>> {
        if let Some(current) = process_identity(self.leader.pid)? {
            if !current.same_process(&self.leader) {
                return Err(invalid("original PTY leader PID has been replaced"));
            }
        }
        let mut owned = Vec::new();
        for pid in process_ids()? {
            // An original session ID remains reserved while its members exist.
            // Different process groups (the provider runner's normal behavior)
            // are intentionally included; different sessions are never included.
            if unsafe { libc::getsid(pid) } != self.leader.session {
                continue;
            }
            if let Some(identity) = process_identity(pid)? {
                if self.owns(&identity) {
                    check_bound(owned.len() + 1, MAX_OWNED_PIDS)?;
                    owned.push(identity);
                }
            }
        }
        Ok(owned)
    }

    pub fn cleanup(&self, child: &mut dyn Child) -> CleanupReport {
        let mut report = CleanupReport {
            application_pid: self.leader.pid,
            ..Default::default()
        };
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut owned = BTreeMap::<i32, ProcessIdentity>::new();
        // Stop the owned leader first, then freeze its still-live session
        // descendants. A second bounded scan catches a fork racing the first.
        if let Err(error) = self.signal(&self.leader, libc::SIGSTOP) {
            report.errors.push(error.to_string());
        }
        'scan: for _ in 0..4 {
            match self.members() {
                Ok(members) => {
                    let previous = owned.len();
                    for identity in members {
                        if !owned.contains_key(&identity.pid) {
                            if let Err(error) = check_bound(owned.len() + 1, MAX_OWNED_PIDS) {
                                report.errors.push(error.to_string());
                                break 'scan;
                            }
                        }
                        if !identity.zombie {
                            if let Err(error) = self.signal(&identity, libc::SIGSTOP) {
                                report.errors.push(error.to_string());
                            }
                        }
                        owned.insert(identity.pid, identity);
                    }
                    if owned.len() == previous {
                        break;
                    }
                }
                Err(error) => {
                    report.errors.push(error.to_string());
                    break;
                }
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        // Keep the original leader in the set even if it exited between scans.
        owned
            .entry(self.leader.pid)
            .or_insert_with(|| self.leader.clone());
        for identity in owned.values() {
            match self.signal(identity, libc::SIGKILL) {
                Ok(true) => report.signalled.push(identity.pid),
                Ok(false) => {}
                Err(error) => report.errors.push(error.to_string()),
            }
        }
        let mut pending = owned;
        while !pending.is_empty() && Instant::now() < deadline {
            let mut finished = Vec::new();
            for (&pid, identity) in &pending {
                if pid == self.leader.pid {
                    match child.try_wait() {
                        Ok(Some(_)) => {
                            report.reaped.push(pid);
                            finished.push(pid);
                        }
                        Ok(None) => {}
                        Err(error) => {
                            report.errors.push(error.to_string());
                            finished.push(pid);
                        }
                    }
                    continue;
                }
                match process_identity(pid) {
                    Ok(Some(current)) if identity.same_process(&current) => {
                        // SAFETY: positive PID is an exact current owned
                        // descendant. WNOHANG cannot block or steal another
                        // child's status. Linux subreaper adopts these orphans;
                        // on macOS launchd may already have reaped them.
                        let result =
                            unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
                        if result == pid {
                            report.reaped.push(pid);
                            finished.push(pid);
                        } else if result < 0
                            && io::Error::last_os_error().raw_os_error() != Some(libc::ECHILD)
                        {
                            report.errors.push(io::Error::last_os_error().to_string());
                        }
                    }
                    Ok(_) => {
                        report.gone.push(pid);
                        finished.push(pid);
                    }
                    Err(error) => {
                        report.errors.push(error.to_string());
                        finished.push(pid);
                    }
                }
            }
            for pid in finished {
                pending.remove(&pid);
            }
            if !pending.is_empty() {
                thread::sleep(Duration::from_millis(2));
            }
        }
        report.remaining = pending.keys().copied().collect();
        report
    }

    fn signal(&self, expected: &ProcessIdentity, signal: i32) -> io::Result<bool> {
        if !self.owns(expected) {
            return Err(invalid("refusing a foreign process identity"));
        }
        let Some(current) = process_identity(expected.pid)? else {
            return Ok(false);
        };
        if !expected.same_process(&current) {
            return Err(invalid("process identity changed before signal"));
        }
        if current.zombie {
            return Ok(false);
        }
        // SAFETY: revalidated positive PID; never 0 or a negative process group.
        if unsafe { libc::kill(expected.pid, signal) } == 0 {
            Ok(true)
        } else if io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            Ok(false)
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

pub fn check_bound(count: usize, maximum: usize) -> io::Result<()> {
    if count > maximum {
        Err(invalid(
            "owned PTY process enumeration exceeded its fixed bound",
        ))
    } else {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub fn parse_linux_stat(bytes: &[u8], uid: u32) -> io::Result<ProcessIdentity> {
    check_bound(bytes.len(), MAX_STAT_BYTES as usize)?;
    let open = bytes
        .iter()
        .position(|byte| *byte == b'(')
        .ok_or_else(|| invalid("missing stat command"))?;
    let close = bytes
        .windows(2)
        .rposition(|pair| pair == b") ")
        .ok_or_else(|| invalid("missing stat command end"))?;
    if open >= close {
        return Err(invalid("malformed stat command"));
    }
    let number = |text: &str| text.parse::<i32>().map_err(|_| invalid("invalid stat PID"));
    let pid = number(
        std::str::from_utf8(&bytes[..open])
            .map_err(|_| invalid("invalid stat PID bytes"))?
            .trim(),
    )?;
    let tail =
        std::str::from_utf8(&bytes[close + 2..]).map_err(|_| invalid("invalid stat fields"))?;
    let fields: Vec<_> = tail.split_ascii_whitespace().collect();
    if fields.len() < 20
        || fields[0].len() != 1
        || !b"RSDZTtXxKWPI".contains(&fields[0].as_bytes()[0])
        || pid <= 0
    {
        return Err(invalid("truncated or invalid stat fields"));
    }
    let parent = number(fields[1])?;
    let session = number(fields[3])?;
    let birth = fields[19]
        .parse::<u64>()
        .map_err(|_| invalid("invalid process birth"))?;
    if parent < 0 || session <= 0 || birth == 0 {
        return Err(invalid("invalid stat ownership fields"));
    }
    Ok(ProcessIdentity {
        pid,
        parent,
        session,
        uid,
        birth: (birth, 0),
        zombie: fields[0] == "Z",
    })
}

#[cfg(target_os = "linux")]
pub fn process_identity(pid: i32) -> io::Result<Option<ProcessIdentity>> {
    use std::{io::Read, os::unix::fs::MetadataExt};
    if pid <= 0 {
        return Err(invalid("PID must be positive"));
    }
    let directory = std::path::PathBuf::from(format!("/proc/{pid}"));
    let result = (|| {
        let uid = std::fs::metadata(&directory)?.uid();
        let mut bytes = Vec::new();
        std::fs::File::open(directory.join("stat"))?
            .take(MAX_STAT_BYTES + 1)
            .read_to_end(&mut bytes)?;
        let identity = parse_linux_stat(&bytes, uid)?;
        if identity.pid != pid {
            return Err(invalid("stat PID does not match requested PID"));
        }
        Ok(identity)
    })();
    finish_identity_read(result)
}

/// A process can disappear while its kernel identity file is being read.
/// Only explicit absence errors may mean None; invalid/foreign data never does.
pub fn finish_identity_read(
    result: io::Result<ProcessIdentity>,
) -> io::Result<Option<ProcessIdentity>> {
    match result {
        Ok(identity) => Ok(Some(identity)),
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.raw_os_error() == Some(libc::ESRCH) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "macos")]
pub fn process_identity(pid: i32) -> io::Result<Option<ProcessIdentity>> {
    if pid <= 0 {
        return Err(invalid("PID must be positive"));
    }
    // SAFETY: valid zero-initialized C output structure and exact buffer size.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&info) as i32;
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    if read == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(None);
        }
        return Err(error);
    }
    if read != size || info.pbi_pid != pid as u32 {
        return Err(invalid("incomplete or mismatched process identity"));
    }
    let session = unsafe { libc::getsid(pid) };
    if session < 0 {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    Ok(Some(ProcessIdentity {
        pid,
        parent: info.pbi_ppid as i32,
        session,
        uid: info.pbi_uid,
        birth: (info.pbi_start_tvsec, info.pbi_start_tvusec),
        zombie: info.pbi_status == libc::SZOMB,
    }))
}

#[cfg(target_os = "linux")]
fn process_ids() -> io::Result<Vec<i32>> {
    let mut pids = Vec::new();
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i32>().ok())
        else {
            continue;
        };
        if pid > 0 {
            check_bound(pids.len() + 1, MAX_SCANNED_PIDS)?;
            pids.push(pid);
        }
    }
    Ok(pids)
}

#[cfg(target_os = "macos")]
fn process_ids() -> io::Result<Vec<i32>> {
    let mut pids = vec![0_i32; MAX_SCANNED_PIDS + 1];
    // SAFETY: output array is initialized, writable, and has the given byte size.
    let count = unsafe {
        libc::proc_listallpids(
            pids.as_mut_ptr().cast(),
            (pids.len() * std::mem::size_of::<i32>()) as i32,
        )
    };
    // Apple's libproc wrapper converts underlying enumeration failure to zero:
    // https://github.com/apple-oss-distributions/xnu/blob/main/libsyscall/wrappers/libproc/libproc.c
    // Zero cannot describe a valid system containing this test process.
    if count <= 0 {
        return Err(invalid("failed or empty macOS process enumeration"));
    }
    check_bound(count as usize, MAX_SCANNED_PIDS)?;
    pids.truncate(count as usize);
    pids.retain(|pid| *pid > 0);
    Ok(pids)
}
