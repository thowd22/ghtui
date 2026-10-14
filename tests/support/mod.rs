//! Real-binary PTY journeys. Input is terminal bytes, never direct app handlers.
#![allow(dead_code)]

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
pub mod cursorless_frames;
pub mod owned_pty_processes_v1;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// APFS requires valid UTF-8 filenames. Both fixtures still require exact raw
/// pathname handling: Linux uses an invalid byte and macOS uses a newline.
pub fn exact_worktree_component() -> std::ffi::OsString {
    use std::os::unix::ffi::OsStringExt;
    std::ffi::OsString::from_vec(if cfg!(target_os = "macos") {
        b"linked-\n".to_vec()
    } else {
        b"linked-\xff".to_vec()
    })
}

pub fn exact_worktree_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "linked-\\n"
    } else {
        "linked-\\xff"
    }
}
use tempfile::TempDir;

pub struct Fixture {
    pub temp: TempDir,
    pub root: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let temp = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let root = temp.path().to_path_buf();
        for name in ["home", "config", "cache", "bin", "repo"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        fs::DirBuilder::new()
            .mode(0o700)
            .create(root.join("config/ghtui"))
            .unwrap();
        let fixture = Self { temp, root };
        fixture.scripted_gh(&[]);
        fixture
    }

    pub fn repo(&self) -> PathBuf {
        self.root.join("repo")
    }

    /// Compile the network-free gh fixture and install it only on this child's PATH.
    pub fn scripted_gh(&self, steps: &[GhStep]) {
        static FAKE_GH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let binary = FAKE_GH.get_or_init(|| {
            let dir = tempfile::tempdir().unwrap().keep();
            let binary = dir.join("fake-gh");
            let output = Command::new("rustc")
                .args(["--edition=2021", "tests/fixtures/fake_gh.rs", "-o"])
                .arg(&binary)
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "fake gh compile failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            binary
        });
        let target = self.root.join("bin/gh");
        if !target.exists() {
            std::os::unix::fs::symlink(binary, target).unwrap();
        }
        let fixture = self.root.join("gh-fixture");
        fs::create_dir_all(&fixture).unwrap();
        for (index, step) in steps.iter().enumerate() {
            let prefix = fixture.join(format!("{index:03}"));
            let file = |suffix: &str| PathBuf::from(format!("{}.{}", prefix.display(), suffix));
            fs::write(file("args"), step.args.join("\n")).unwrap();
            fs::write(file("stdin"), &step.stdin).unwrap();
            fs::write(file("stdout"), &step.stdout).unwrap();
            fs::write(file("stderr"), &step.stderr).unwrap();
            fs::write(file("status"), step.status.to_string()).unwrap();
            fs::write(file("delay-ms"), step.delay_ms.to_string()).unwrap();
        }
    }

    pub fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(self.repo())
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            // Independent status checks must not refresh index stat-cache bytes.
            // Required write locks for explicit fixture add/commit operations still apply.
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    pub fn init_git(&self) {
        self.git(&["init", "--initial-branch=main"]);
        self.git(&["config", "user.name", "Fixture Author"]);
        self.git(&["config", "user.email", "fixture@example.invalid"]);
    }

    pub fn commit_file(&self, name: &str, body: &[u8], message: &str) {
        let path = self.repo().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
        self.git(&["add", "--", name]);
        self.git(&["commit", "-m", message]);
    }

    pub fn verify_git_effect(&self, args: &[&str], expected: &str) -> Result<(), String> {
        let actual = self.git(args);
        if actual == expected {
            Ok(())
        } else {
            Err(format!(
                "Git effect differed: expected {expected:?}, got {actual:?}"
            ))
        }
    }
}

#[derive(Clone)]
pub struct GhStep {
    pub args: Vec<String>,
    pub stdin: Vec<u8>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub status: i32,
    pub delay_ms: u64,
}

pub fn empty_inbox_steps(login: &str) -> Vec<GhStep> {
    let identity = || {
        vec![
            GhStep::read(&["--version"], "gh version 2.99.0 (offline fixture)\n"),
            GhStep::read(
                &["api", "user", "--hostname", "github.com", "--method", "GET"],
                &serde_json::json!({ "login": login, "id": if login == "bob" { 43 } else { 42 } })
                    .to_string(),
            ),
        ]
    };
    let mut steps = identity();
    steps.extend(identity());
    for qualifier in ["author", "assignee", "review-requested"] {
        steps.push(GhStep::read(
            &[
                "api",
                "search/issues",
                "--hostname",
                "github.com",
                "--method",
                "GET",
                "-f",
                &format!("q=is:pr is:open {qualifier}:{login}"),
                "-f",
                "per_page=100",
                "-f",
                "page=1",
            ],
            "{\"total_count\":0,\"incomplete_results\":false,\"items\":[]}",
        ));
    }
    steps
}

impl GhStep {
    pub fn read(args: &[&str], stdout: &str) -> Self {
        Self {
            args: args.iter().map(|arg| (*arg).into()).collect(),
            stdin: Vec::new(),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
            status: 0,
            delay_ms: 0,
        }
    }
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Session {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    receiver: mpsc::Receiver<Vec<u8>>,
    parser: vt100::Parser,
    cursorless_frames: cursorless_frames::CursorlessFrames,
    pub transcript: Vec<u8>,
    pub artifact_dir: PathBuf,
    fixture_dir: PathBuf,
    original_termios: Option<libc::termios>,
    pub input_paths: Vec<&'static str>,
    exited: bool,
    owned_processes: owned_pty_processes_v1::OwnedPtySession,
}

impl Session {
    /// Open a genuine loaded graph row, resolving focus from a completed frame
    /// rather than old row numbers. A click/Enter is not read completion.
    pub fn open_saved_graph_subject(&mut self, mouse: bool, subject: &str, revision: &str) {
        let timeout = std::time::Duration::from_secs(8);
        self.wait_for_cursorless_condition(
            |frame| {
                frame.contents().contains("[Home First loaded]")
                    && !frame.contents().contains("[Esc Cancel]")
            },
            "completed original graph before selecting a loaded member",
            timeout,
        );
        let before_home = self.completed_cursorless_end();
        if mouse {
            self.click_visible_action("[Home First loaded]");
        } else {
            self.send_key(b"\x1b[H");
        }
        let home_deadline = std::time::Instant::now() + timeout;
        loop {
            self.poll_output(std::time::Duration::from_millis(1));
            if self.completed_cursorless_end() > before_home
                && self.completed_cursorless_screen().is_some_and(|frame| {
                    let text = frame.contents();
                    text.contains("First loaded graph projection ready")
                        && !text.contains("[Esc Cancel]")
                })
            {
                break;
            }
            assert!(
                std::time::Instant::now() < home_deadline,
                "Original first loaded projection did not complete\n{}",
                self.screen()
            );
        }
        let deadline = std::time::Instant::now() + timeout;
        loop {
            self.poll_output(std::time::Duration::from_millis(1));
            let location = self
                .completed_cursorless_screen()
                .and_then(|frame| locate_screen(frame, subject));
            if let Some((column, row)) = location {
                if mouse {
                    self.click(column, row);
                    break;
                }
                if self
                    .completed_cursorless_screen()
                    .is_some_and(|frame| frame.cell(row, column).is_some_and(|cell| cell.inverse()))
                {
                    self.send_key(b"\r");
                    break;
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Original loaded subject never selected: {subject}\n{}",
                self.screen()
            );
            let previous = self.completed_cursorless_end();
            if mouse {
                let height = self.completed_cursorless_screen().unwrap().size().0;
                self.wheel(20, height.saturating_sub(4), true);
            } else {
                self.send_key(b"\x1b[B");
            }
            let frame_deadline = std::time::Instant::now() + timeout;
            while self.completed_cursorless_end() <= previous {
                self.poll_output(std::time::Duration::from_millis(10));
                assert!(
                    std::time::Instant::now() < frame_deadline,
                    "No completed frame after actual graph navigation"
                );
            }
        }
        let saved = format!("Saved: {revision}");
        let selected = format!("▶ selected {}", &revision[..8]);
        self.wait_for_cursorless_condition(
            |frame| {
                frame.contents().contains(&saved)
                    && frame.contents().contains(&selected)
                    && frame.contents().contains(subject)
                    && !frame.contents().contains("[Esc Cancel]")
                    && locate_screen(frame, "[g Graph]").is_some_and(|(column, row)| {
                        frame.cell(row, column).is_some_and(|cell| cell.inverse())
                    })
            },
            "exact graph member opened and original read completed",
            timeout,
        );
    }
    pub fn launch(fixture: &Fixture, cols: u16, rows: u16) -> Self {
        Self::launch_binary(
            Path::new(env!("CARGO_BIN_EXE_ghtui")),
            fixture,
            cols,
            rows,
            &[],
        )
    }

    pub fn launch_from(fixture: &Fixture, directory: &Path, cols: u16, rows: u16) -> Self {
        Self::launch_binary_from(
            Path::new(env!("CARGO_BIN_EXE_ghtui")),
            fixture,
            directory,
            cols,
            rows,
            &[],
        )
    }

    pub fn launch_from_with_args(
        fixture: &Fixture,
        directory: &Path,
        cols: u16,
        rows: u16,
        args: &[&str],
    ) -> Self {
        Self::launch_binary_from_os_env_with_args(
            Path::new(env!("CARGO_BIN_EXE_ghtui")),
            fixture,
            directory,
            cols,
            rows,
            &[],
            args,
        )
    }

    pub fn launch_binary(
        binary: &Path,
        fixture: &Fixture,
        cols: u16,
        rows: u16,
        env: &[(&str, &str)],
    ) -> Self {
        Self::launch_binary_from(binary, fixture, &fixture.repo(), cols, rows, env)
    }

    fn launch_binary_from(
        binary: &Path,
        fixture: &Fixture,
        directory: &Path,
        cols: u16,
        rows: u16,
        env: &[(&str, &str)],
    ) -> Self {
        let environment = env
            .iter()
            .map(|(key, value)| (std::ffi::OsStr::new(key), std::ffi::OsStr::new(value)))
            .collect::<Vec<_>>();
        Self::launch_binary_from_os_env(binary, fixture, directory, cols, rows, &environment)
    }

    pub fn launch_binary_os_env(
        binary: &Path,
        fixture: &Fixture,
        cols: u16,
        rows: u16,
        env: &[(&std::ffi::OsStr, &std::ffi::OsStr)],
    ) -> Self {
        Self::launch_binary_from_os_env(binary, fixture, &fixture.repo(), cols, rows, env)
    }

    fn launch_binary_from_os_env(
        binary: &Path,
        fixture: &Fixture,
        directory: &Path,
        cols: u16,
        rows: u16,
        env: &[(&std::ffi::OsStr, &std::ffi::OsStr)],
    ) -> Self {
        Self::launch_binary_from_os_env_with_args(binary, fixture, directory, cols, rows, env, &[])
    }

    fn launch_binary_from_os_env_with_args(
        binary: &Path,
        fixture: &Fixture,
        directory: &Path,
        cols: u16,
        rows: u16,
        env: &[(&std::ffi::OsStr, &std::ffi::OsStr)],
        args: &[&str],
    ) -> Self {
        owned_pty_processes_v1::initialize_reaping()
            .expect("initialize exact owned orphan reaping");
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let original_termios = read_termios(&*pair.master);
        let mut command = CommandBuilder::new(binary);
        command.args(args);
        command.cwd(directory);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        // A runner's preferences must not change color/avatar fixture semantics.
        // Individual no-color scenarios may re-add the variable below.
        command.env_remove("NO_COLOR");
        command.env("HOME", fixture.root.join("home"));
        command.env("XDG_CONFIG_HOME", fixture.root.join("config"));
        command.env("XDG_CACHE_HOME", fixture.root.join("cache"));
        command.env("GHTUI_CONFIG_DIR", fixture.root.join("config/ghtui"));
        command.env("GH_CONFIG_DIR", fixture.root.join("config/gh"));
        command.env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
        command.env("GH_HOST", "github.invalid");
        command.env("GH_TOKEN", "");
        command.env("GITHUB_TOKEN", "");
        command.env("GH_ENTERPRISE_TOKEN", "");
        command.env("GITHUB_ENTERPRISE_TOKEN", "");
        command.env("GIT_CONFIG_NOSYSTEM", "1");
        command.env("GIT_CONFIG_GLOBAL", "/dev/null");
        let mut paths = vec![fixture.root.join("bin")];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        command.env("PATH", std::env::join_paths(paths).unwrap());
        for (key, value) in env {
            command.env(key, value);
        }
        let mut child = pair.slave.spawn_command(command).unwrap();
        let owned_processes = match owned_pty_processes_v1::OwnedPtySession::capture(
            child.process_id().expect("PTY application PID"),
        ) {
            Ok(owned) => owned,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("could not establish exact PTY session ownership: {error}");
            }
        };
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let writer = pair.master.take_writer().unwrap();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut bytes = [0; 8192];
            loop {
                match reader.read(&mut bytes) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        if sender.send(bytes[..count].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Self {
            master: pair.master,
            child,
            writer,
            receiver,
            parser: vt100::Parser::new(rows, cols, 0),
            transcript: Vec::new(),
            artifact_dir: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/tui-artifacts")
                .join(format!(
                    "{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                )),
            fixture_dir: fixture.root.clone(),
            original_termios,
            cursorless_frames: Default::default(),
            input_paths: Vec::new(),
            exited: false,
            owned_processes,
        }
    }

    pub fn screen(&self) -> String {
        self.parser.screen().contents()
    }

    pub fn owned_application_pid(&self) -> i32 {
        self.owned_processes.application_pid()
    }

    pub fn completed_cursorless_screen(&self) -> Option<&vt100::Screen> {
        self.cursorless_frames.last.as_ref()
    }

    pub fn completed_cursorless_end(&self) -> usize {
        self.cursorless_frames.end
    }

    pub fn wait_for_cursorless_condition(
        &mut self,
        predicate: impl Fn(&vt100::Screen) -> bool,
        label: &str,
        timeout: Duration,
    ) {
        let deadline = Instant::now() + timeout;
        loop {
            self.poll_output(Duration::from_millis(1));
            if self.completed_cursorless_screen().is_some_and(&predicate) {
                return;
            }
            if Instant::now() >= deadline {
                self.save_artifacts();
                panic!(
                    "Missing complete cursorless frame: {label}\n{}; artifacts: {}",
                    self.screen(),
                    self.artifact_dir.display()
                );
            }
        }
    }

    pub fn send_key(&mut self, bytes: &[u8]) {
        self.input_paths.push("keyboard");
        self.write(bytes);
    }

    fn write(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).unwrap();
        self.writer.flush().unwrap();
    }

    pub fn paste(&mut self, text: &str) {
        self.send_key(format!("\x1b[200~{text}\x1b[201~").as_bytes());
    }

    pub fn click(&mut self, column: u16, row: u16) {
        self.input_paths.push("mouse");
        self.write(
            format!(
                "\x1b[<0;{};{}M\x1b[<0;{};{}m",
                column + 1,
                row + 1,
                column + 1,
                row + 1
            )
            .as_bytes(),
        );
    }

    pub fn locate(&self, text: &str) -> Option<(u16, u16)> {
        locate_screen(self.parser.screen(), text)
    }

    /// Extract only cells inside the observed titled box, excluding background
    /// side gutters. Coordinates come from actual terminal borders, not App state.
    pub fn visible_overlay_body(&self, title: &str) -> Option<String> {
        overlay_body(self.parser.screen(), title)
    }

    /// A title/body fragment can arrive before its last enclosing border cell.
    /// Wait for the actual owned box, without consulting application handlers.
    pub fn wait_for_overlay_body(&mut self, title: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            self.poll_output(Duration::from_millis(1));
            if let Some(body) = self.visible_overlay_body(title) {
                return body;
            }
            if Instant::now() >= deadline {
                self.save_artifacts();
                panic!(
                    "Owned overlay {title:?} did not finish drawing\n{}",
                    self.screen()
                );
            }
        }
    }

    pub fn click_visible_action(&mut self, label: &str) {
        self.wait_for_screen(label);
        let (column, row) = self.locate(label).unwrap();
        self.click(column, row);
    }

    pub fn click_owned_form_field(&mut self, phase: &str, label: &str) {
        let deadline = Instant::now() + Duration::from_secs(8);
        let (column, row) = loop {
            self.poll_output(Duration::from_millis(10));
            if let Some(position) = form_field_value_position(self.parser.screen(), phase, label) {
                break position;
            }
            assert!(
                Instant::now() < deadline,
                "Actual owned form field/footer never finished painting: {phase}/{label}\n{}",
                self.screen()
            );
        };
        self.click(column, row);
        loop {
            self.poll_output(Duration::from_millis(10));
            if self.parser.screen().cursor_position().0 == row {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Clicked form field did not own the actual caret: {phase}/{label}\n{}",
                self.screen()
            );
        }
    }

    /// Full-screen owned overlays replace the bottom toolbar. A restored body
    /// title alone can arrive before its clickable footer has been repainted.
    pub fn wait_for_main_frame(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            self.poll_output(Duration::from_millis(10));
            if main_footer_visible(self.parser.screen()) {
                return;
            }
            if Instant::now() >= deadline {
                self.save_artifacts();
                panic!(
                    "Actual restored main toolbar was not painted\n{}",
                    self.screen()
                );
            }
        }
    }

    pub fn cell_underlined(&self, row: u16, column: u16) -> bool {
        self.parser
            .screen()
            .cell(row, column)
            .is_some_and(|cell| cell.underline())
    }
    pub fn cell_inverted(&self, row: u16, column: u16) -> bool {
        self.parser
            .screen()
            .cell(row, column)
            .is_some_and(|cell| cell.inverse())
    }

    /// Drain actual terminal output, then wait briefly for a redraw; no application state access.
    pub fn poll_output(&mut self, duration: Duration) {
        let deadline = Instant::now() + duration;
        loop {
            while let Ok(bytes) = self.receiver.try_recv() {
                self.process(&bytes);
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return;
            };
            match self.receiver.recv_timeout(remaining) {
                Ok(bytes) => self.process(&bytes),
                Err(_) => return,
            }
        }
    }

    /// Traverse only with Tab and prove focus from the terminal's visible underline attribute.
    pub fn focus_visible_action(&mut self, label: &str) {
        self.wait_for_screen(label);
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            self.poll_output(Duration::from_millis(1));
            if let Some((column, row)) = self.locate(label) {
                if self.cell_underlined(row, column) {
                    return;
                }
            }
            if Instant::now() >= deadline {
                self.save_artifacts();
                panic!(
                    "Visible action {label:?} never gained keyboard focus\n{}; artifacts: {}",
                    self.screen(),
                    self.artifact_dir.display()
                );
            }
            self.send_key(b"\t");
            self.poll_output(Duration::from_millis(50));
        }
    }

    /// Forms show focus with reverse video. Follow real Tab events until that
    /// attribute is visible on the exact button, including Unicode-width offsets.
    pub fn focus_visible_form_action(&mut self, label: &str) {
        self.wait_for_screen(label);
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            self.poll_output(Duration::from_millis(1));
            if let Some((column, row)) = self.locate(label) {
                if self.cell_inverted(row, column) {
                    return;
                }
            }
            if Instant::now() >= deadline {
                self.save_artifacts();
                panic!(
                    "Visible form action {label:?} never gained keyboard focus\n{}; artifacts: {}",
                    self.screen(),
                    self.artifact_dir.display()
                );
            }
            self.send_key(b"\t");
            self.poll_output(Duration::from_millis(50));
        }
    }

    pub fn wheel(&mut self, column: u16, row: u16, down: bool) {
        self.input_paths.push("mouse");
        self.write(
            format!(
                "\x1b[<{};{};{}M",
                if down { 65 } else { 64 },
                column + 1,
                row + 1
            )
            .as_bytes(),
        );
    }

    pub fn drag(&mut self, from: (u16, u16), to: (u16, u16)) {
        self.input_paths.push("mouse");
        self.write(
            format!(
                "\x1b[<0;{};{}M\x1b[<32;{};{}M\x1b[<0;{};{}m",
                from.0 + 1,
                from.1 + 1,
                to.0 + 1,
                to.1 + 1,
                to.0 + 1,
                to.1 + 1
            )
            .as_bytes(),
        );
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.resize_with_footer(cols, rows, None);
    }
    /// Full-screen owned views replace the normal global footer. Require an
    /// actual SIGWINCH redraw at the new bottom AND the declared modal's own
    /// bottom controls; a title-only/stale-sized frame cannot satisfy this.
    pub fn resize_owned_modal(&mut self, cols: u16, rows: u16, title: &str, actions: &[&str]) {
        assert!(
            !title.is_empty()
                && actions.len() >= 2
                && actions.iter().all(|action| !action.is_empty())
        );
        self.resize_with_footer(cols, rows, Some((title, actions)));
    }
    fn resize_with_footer(&mut self, cols: u16, rows: u16, owned: Option<(&str, &[&str])>) {
        assert!(cols > 0 && rows > 0, "PTY dimensions must be nonzero");
        if self.parser.screen().size() == (rows, cols) {
            return;
        }
        self.poll_output(Duration::from_millis(1));
        let previous_bytes = self.transcript.len();
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        self.parser.set_size(rows, cols);
        self.cursorless_frames.invalidate_size();
        let footer_cursor = format!("\x1b[{rows};1H");
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            while let Ok(bytes) = self.receiver.try_recv() {
                self.process(&bytes);
            }
            let footer = self
                .parser
                .screen()
                .rows(0, cols)
                .nth(usize::from(rows - 1))
                .unwrap_or_default();
            let new_frame = self.transcript[previous_bytes..]
                .windows(footer_cursor.len())
                .any(|bytes| bytes == footer_cursor.as_bytes());
            let footer_ready = if let Some((title, actions)) = owned {
                let owned_footer = self
                    .parser
                    .screen()
                    .rows(0, cols)
                    .nth(usize::from(rows.saturating_sub(2)))
                    .unwrap_or_default();
                self.screen().contains(title)
                    && actions.iter().all(|action| owned_footer.contains(action))
                    && footer.contains('└')
                    && footer.contains('┘')
            } else {
                footer.contains("Help") && footer.contains("Quit")
            };
            if new_frame && footer_ready {
                return;
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                self.save_artifacts();
                panic!("Actual binary did not redraw {cols}x{rows} frame and bottom toolbar\n{}; artifacts: {}", self.screen(), self.artifact_dir.display());
            };
            match self
                .receiver
                .recv_timeout(remaining.min(Duration::from_millis(50)))
            {
                Ok(bytes) => self.process(&bytes),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.save_artifacts();
                    panic!(
                        "Terminal exited before the resized frame was visible; artifacts: {}",
                        self.artifact_dir.display()
                    );
                }
            }
        }
    }

    pub fn try_wait_for(&mut self, text: &str, timeout: Duration) -> Result<(), String> {
        self.try_wait_for_condition(|screen| screen.contains(text), text, timeout)
    }

    pub fn try_wait_for_condition(
        &mut self,
        condition: impl Fn(&str) -> bool,
        description: &str,
        timeout: Duration,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            while let Ok(bytes) = self.receiver.try_recv() {
                self.process(&bytes);
            }
            if condition(&self.screen()) {
                return Ok(());
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(format!("missing {description:?}\n{}", self.screen()));
            };
            match self
                .receiver
                .recv_timeout(remaining.min(Duration::from_millis(50)))
            {
                Ok(bytes) => self.process(&bytes),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(format!(
                        "terminal closed while waiting for {description:?}\n{}",
                        self.screen()
                    ))
                }
            }
        }
    }

    pub fn close_overlay_by_key(&mut self, title: &str) {
        self.send_key(b"\x1b");
        self.wait_for_overlay_close(title);
    }

    pub fn close_overlay_by_mouse(&mut self, title: &str) {
        self.click_visible_action("[Esc Close]");
        self.wait_for_overlay_close(title);
    }

    pub fn wait_for_overlay_close(&mut self, title: &str) {
        if let Err(error) = self.try_wait_for_condition(
            |screen| !screen.contains(title),
            "overlay to close",
            Duration::from_secs(8),
        ) {
            self.save_artifacts();
            panic!("{error}; artifacts: {}", self.artifact_dir.display());
        }
    }

    pub fn wait_for_screen(&mut self, text: &str) {
        if let Err(error) = self.try_wait_for(text, Duration::from_secs(8)) {
            self.save_artifacts();
            panic!("{error}; artifacts: {}", self.artifact_dir.display());
        }
    }

    fn process(&mut self, bytes: &[u8]) {
        let base = self.transcript.len();
        self.transcript.extend_from_slice(bytes);
        self.cursorless_frames
            .process(&mut self.parser, bytes, base);
    }

    pub fn assert_focused_view(&mut self, view: &str) {
        self.wait_for_screen(&format!("{view} • focused view"));
    }

    pub fn assert_cells(&self, row: u16, col: u16, text: &str) {
        for (offset, character) in text.chars().enumerate() {
            assert_eq!(
                self.parser
                    .screen()
                    .cell(row, col + offset as u16)
                    .unwrap()
                    .contents(),
                character.to_string()
            );
        }
    }

    pub fn inspect_command_journal(&self, fixture: &Fixture) -> String {
        fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap_or_default()
    }

    pub fn wait_for_command_count(&mut self, fixture: &Fixture, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if self.inspect_command_journal(fixture).lines().count() >= count {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "expected {count} fixture commands"
            );
            if let Ok(bytes) = self.receiver.recv_timeout(Duration::from_millis(20)) {
                self.process(&bytes);
            }
        }
    }

    pub fn restart_with_same_cache(&mut self, fixture: &Fixture, cols: u16, rows: u16) -> Session {
        self.quit_and_assert_restored_terminal(false);
        Self::launch(fixture, cols, rows)
    }

    /// Simulate sudden application death, not a graceful quit. Only the owned
    /// application PID is killed: descendants deliberately remain alive so
    /// recovery tests can observe inherited operation locks independently.
    /// SIGKILL cannot run terminal cleanup; this method makes no restoration
    /// claim, and the test must retain/clean up its generated descendant PIDs.
    pub fn terminate_abruptly(&mut self) {
        let pid = self
            .child
            .process_id()
            .expect("application child has a PID");
        assert!(
            self.child.try_wait().unwrap().is_none(),
            "application must still be running before abrupt termination"
        );
        let pid = i32::try_from(pid).expect("application PID fits the platform PID type");
        self.save_artifacts();
        // SAFETY: positive PID identifies this session's owned, unreaped child;
        // unlike a negative PID, this never signals a process group.
        assert_eq!(
            unsafe { libc::kill(pid, libc::SIGKILL) },
            0,
            "could not terminate owned application: {}",
            std::io::Error::last_os_error()
        );
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(
                    !status.success(),
                    "SIGKILL must not report graceful success"
                );
                self.exited = true;
                break;
            }
            assert!(Instant::now() < deadline, "killed application did not exit");
            self.poll_output(Duration::from_millis(20));
        }
        self.poll_output(Duration::from_millis(20));
        self.save_artifacts();
    }

    pub fn quit_and_assert_restored_terminal(&mut self, mouse: bool) {
        if mouse {
            self.click_visible_action("[q Quit]");
        } else {
            self.send_key(b"q");
        }
        self.assert_exit_and_restored(true);
    }

    pub fn assert_exit_and_restored(&mut self, success: bool) {
        self.assert_exit_state(success, true);
    }

    pub fn assert_startup_failure_preserved_terminal(&mut self) {
        self.assert_exit_state(false, false);
    }

    fn assert_exit_state(&mut self, success: bool, initialized: bool) {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            while let Ok(bytes) = self.receiver.try_recv() {
                self.process(&bytes);
            }
            if let Some(status) = self.child.try_wait().unwrap() {
                self.exited = true;
                assert_eq!(status.success(), success);
                break;
            }
            assert!(Instant::now() < deadline, "child failed to exit");
            if let Ok(bytes) = self.receiver.recv_timeout(Duration::from_millis(20)) {
                self.process(&bytes);
            }
        }
        // The PTY reader may deliver final bytes after wait observes process exit.
        while let Ok(bytes) = self.receiver.recv_timeout(Duration::from_millis(100)) {
            self.process(&bytes);
        }
        let raw = String::from_utf8_lossy(&self.transcript);
        if initialized {
            assert!(
                raw.contains("\x1b[?1049l"),
                "alternate screen was not restored"
            );
            assert!(raw.contains("\x1b[?25h"), "cursor was not restored");
            assert!(
                raw.contains("\x1b[?1006l"),
                "SGR mouse reporting was not disabled"
            );
        } else {
            assert!(
                !raw.contains("\x1b[?1049h"),
                "startup validation failed after entering alternate screen"
            );
            assert!(
                !raw.contains("\x1b[?1006h"),
                "startup validation failed after enabling mouse reporting"
            );
        }
        if let (Some(before), Some(after)) = (self.original_termios, read_termios(&*self.master)) {
            assert_eq!(
                before.c_lflag & (libc::ICANON | libc::ECHO),
                after.c_lflag & (libc::ICANON | libc::ECHO),
                "raw terminal mode was not restored"
            );
            assert_eq!(
                before.c_iflag, after.c_iflag,
                "input terminal flags changed"
            );
            assert_eq!(
                before.c_oflag, after.c_oflag,
                "output terminal flags changed"
            );
            assert_eq!(
                before.c_cflag, after.c_cflag,
                "control terminal flags changed"
            );
            assert_eq!(
                before.c_lflag, after.c_lflag,
                "local terminal flags changed"
            );
            assert_eq!(
                before.c_cc, after.c_cc,
                "terminal control characters changed"
            );
        }
    }

    pub fn save_artifacts(&self) {
        let _ = fs::create_dir_all(&self.artifact_dir);
        let _ = fs::write(self.artifact_dir.join("screen.txt"), self.screen());
        let _ = fs::write(self.artifact_dir.join("terminal.raw"), &self.transcript);
        let _ = fs::write(
            self.artifact_dir.join("screen.ansi"),
            self.parser.screen().contents_formatted(),
        );
        let _ = fs::copy(
            self.fixture_dir.join("gh-fixture/commands.jsonl"),
            self.artifact_dir.join("commands.jsonl"),
        );
        let _ = fs::write(
            self.artifact_dir.join("fixture-seed.txt"),
            "ghtui deterministic offline fixture v1\n",
        );
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let panicking = std::thread::panicking();
        if panicking {
            self.save_artifacts();
        }
        // Even an already reaped (including deliberately SIGKILLed) app can
        // have live provider descendants in different process groups. Cleanup
        // happens here, never in terminate_abruptly's recovery observation.
        let cleanup = self.owned_processes.cleanup(&mut *self.child);
        let complete = cleanup.errors.is_empty() && cleanup.remaining.is_empty();
        if !complete {
            let _ = fs::create_dir_all(&self.artifact_dir);
            if let Ok(bytes) = serde_json::to_vec_pretty(&cleanup) {
                let _ = fs::write(self.artifact_dir.join("owned-process-cleanup.json"), bytes);
            }
        }
        if let Some(path) = std::env::var_os("GHTUI_TEST_COVERAGE_FILE") {
            let record = serde_json::json!({"test":std::thread::current().name().unwrap_or("unknown"),"keyboard":self.input_paths.contains(&"keyboard"),"mouse":self.input_paths.contains(&"mouse"),"exited":self.exited && complete,"panicking":panicking,"cleanup_completed":complete});
            let _ = append_journey_record(Path::new(&path), &record);
        }
        // Preserve an original panic without double-panicking. A normal Drop
        // with incomplete cleanup must fail instead of granting positive E2E
        // coverage or silently claiming that descendants were reaped.
        if !panicking {
            assert!(
                complete,
                "owned PTY process cleanup incomplete: {cleanup:?}; artifacts: {}",
                self.artifact_dir.display()
            );
        }
    }
}

/// Serialize before append, and hold the process-wide lock through all writes.
/// Formatting directly into a shared append file emits many small writes;
/// concurrent terminal journeys can then corrupt each other's JSON records.
pub fn append_journey_record(path: &Path, record: &serde_json::Value) -> std::io::Result<()> {
    static WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let mut bytes = serde_json::to_vec(record).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    let _guard = WRITER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(&bytes)
}

/// Locate text using decoded terminal cells, not Unicode scalar counts. Wide
/// glyph continuation cells and combining marks must not shift mouse targets.
pub fn main_footer_visible(screen: &vt100::Screen) -> bool {
    let (rows, cols) = screen.size();
    screen
        .rows(0, cols)
        .nth(usize::from(rows.saturating_sub(1)))
        .is_some_and(|row| row.contains("Help]") && row.contains("Quit]"))
}

/// Observe only the owned PR action menu, not a selected conversation row
/// behind it. Two markers during an incomplete repaint are not an acknowledgement.
pub fn selected_pr_menu_row(screen: &str) -> Option<String> {
    let mut selected = screen
        .lines()
        .skip_while(|line| !line.contains("PR actions • native workflows"))
        .skip(1)
        .take_while(|line| !line.contains("[Esc Close]") && !line.contains('└'))
        .filter_map(|line| {
            line.split_once("│ > ")
                .and_then(|(_, text)| text.split_once('│'))
                .map(|(text, _)| text.trim().to_owned())
                .filter(|text| !text.is_empty())
        });
    let first = selected.next()?;
    selected.next().is_none().then_some(first)
}

/// A visible later field is not proof that its newly scrolled viewport arrived.
/// Return only the actual first field's value directly beneath its owned phase.
pub fn first_form_field_value<'a>(screen: &'a str, phase: &str, label: &str) -> Option<&'a str> {
    let mut lines = screen.lines();
    lines.find(|line| line.contains(phase))?;
    let heading = lines.next()?;
    if !heading.contains(label) {
        return None;
    }
    lines
        .next()?
        .split('│')
        .max_by_key(|part| part.len())
        .map(str::trim)
}

/// An underlying approval history can contain the same field name. Accept
/// only its actual owned form column after the phase and before a drawn footer.
pub fn form_field_value_position(
    screen: &vt100::Screen,
    phase: &str,
    label: &str,
) -> Option<(u16, u16)> {
    let (column, phase_row) = locate_screen(screen, phase)?;
    let (_, footer_row) = locate_screen(screen, "[Keep draft / Close]")?;
    if phase_row >= footer_row {
        return None;
    }
    let (_, columns) = screen.size();
    for row in phase_row + 1..footer_row.saturating_sub(1) {
        let text: String = (column..columns)
            .map(|x| {
                screen
                    .cell(row, x)
                    .map(|cell| cell.contents())
                    .unwrap_or_default()
            })
            .collect();
        if text.strip_prefix(label).is_some_and(|suffix| {
            suffix.starts_with(':')
                || suffix.starts_with(" (")
                    && suffix
                        .split('│')
                        .next()
                        .is_some_and(|owned| owned.contains(':'))
        }) {
            return Some((column, row + 1));
        }
    }
    None
}

pub fn locate_screen(screen: &vt100::Screen, text: &str) -> Option<(u16, u16)> {
    let (rows, columns) = screen.size();
    for row in 0..rows {
        let mut line = String::new();
        let mut starts = Vec::new();
        for column in 0..columns {
            let cell = screen.cell(row, column)?;
            if cell.is_wide_continuation() {
                continue;
            }
            starts.push((line.len(), column));
            let contents = cell.contents();
            if contents.is_empty() {
                line.push(' ');
            } else {
                line.push_str(&contents);
            }
        }
        if let Some(byte) = line.find(text) {
            return starts
                .iter()
                .rev()
                .find(|(start, _)| *start <= byte)
                .map(|(_, column)| (*column, row));
        }
    }
    None
}

/// A terminal's erased cell is a visible space, not absent text. Cursor motion
/// can leave such cells between words even when no literal space was emitted.
pub fn overlay_body(screen: &vt100::Screen, title: &str) -> Option<String> {
    let (title_column, top) = locate_screen(screen, title)?;
    let (rows, columns) = screen.size();
    let has = |row, column, text| {
        screen
            .cell(row, column)
            .is_some_and(|cell| cell.contents() == text)
    };
    let left = (0..title_column)
        .rev()
        .find(|column| has(top, *column, "┌"))?;
    let right = (title_column..columns).find(|column| has(top, *column, "┐"))?;
    let bottom = (top + 1..rows).find(|row| has(*row, left, "└") && has(*row, right, "┘"))?;
    let mut body = String::new();
    for row in top + 1..bottom {
        for column in left + 1..right {
            let cell = screen.cell(row, column)?;
            if cell.is_wide_continuation() {
                continue;
            }
            let contents = cell.contents();
            if contents.is_empty() {
                body.push(' ');
            } else {
                body.push_str(&contents);
            }
        }
        body.push('\n');
    }
    Some(body)
}

fn read_termios(master: &dyn MasterPty) -> Option<libc::termios> {
    let fd = master.as_raw_fd()?;
    let mut termios = std::mem::MaybeUninit::uninit();
    // SAFETY: tcgetattr initializes the provided termios only on successful return.
    if unsafe { libc::tcgetattr(fd, termios.as_mut_ptr()) } == 0 {
        Some(unsafe { termios.assume_init() })
    } else {
        None
    }
}
