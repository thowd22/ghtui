//! Nonblocking fixture children. Cleanup delegates to the existing cross-Unix
//! positive PID/UID/birth/SID owner; never signal a guessed group or reap -1.
use super::*;
use std::os::unix::{io::AsRawFd, process::CommandExt};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub(super) struct OwnedChild {
    child: Child,
    owner: super::owned::OwnedPtySession,
    reports: Arc<Mutex<Vec<super::owned::CleanupReport>>>,
    cleaned: bool,
}

pub(super) fn nonblocking(fd: i32) -> Result<(), String> {
    // SAFETY: inspect/set only this actual owned descriptor's flags.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err("Fixture descriptor could not be made nonblocking".into());
    }
    Ok(())
}

pub(super) fn alive(stop: &AtomicBool, deadline: Instant) -> Result<(), String> {
    if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
        Err("Owned TLS fixture cancelled or exceeded its fixed deadline".into())
    } else {
        Ok(())
    }
}

pub(super) fn wait_fd(
    fd: i32,
    events: i16,
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<(), String> {
    loop {
        alive(stop, deadline)?;
        let mut poll = libc::pollfd {
            fd,
            events,
            revents: 0,
        };
        // SAFETY: one initialized owned descriptor, finite20ms wait.
        let count = unsafe { libc::poll(&mut poll, 1, 20) };
        if count > 0 {
            if poll.revents & libc::POLLNVAL != 0 {
                return Err("Owned TLS fixture descriptor became invalid".into());
            }
            return Ok(());
        }
        if count < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            return Err("Owned TLS fixture descriptor poll failed".into());
        }
    }
}

pub(super) fn write_all(
    writer: &mut impl Write,
    fd: i32,
    mut bytes: &[u8],
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<(), String> {
    while !bytes.is_empty() {
        alive(stop, deadline)?;
        match writer.write(bytes) {
            Ok(0) => return Err("Owned TLS fixture short write".into()),
            Ok(n) => bytes = &bytes[n..],
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                wait_fd(fd, libc::POLLOUT, stop, deadline)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Err("Owned TLS fixture write failed".into()),
        }
    }
    Ok(())
}

impl OwnedChild {
    pub(super) fn spawn(
        mut command: Command,
        home: &Path,
        reports: &Arc<Mutex<Vec<super::owned::CleanupReport>>>,
    ) -> Result<Self, String> {
        super::owned::initialize_reaping().map_err(|_| "Owned fixture reaping unavailable")?;
        // Preserve ONLY explicitly supplied child-local CGI fields, not the
        // caller environment. All command constructors are private siblings.
        let supplied = command
            .get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(std::ffi::OsStr::to_owned)))
            .collect::<Vec<_>>();
        if supplied.len() > 24 {
            return Err("Fixture child environment bound exceeded".into());
        }
        // Only child environments. No repository/user/global maintenance edit.
        command.env_clear();
        for (key, value) in supplied {
            if let Some(value) = value {
                command.env(key, value);
            }
        }
        command
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_COUNT", "2")
            .env("GIT_CONFIG_KEY_0", "maintenance.autoDetach")
            .env("GIT_CONFIG_VALUE_0", "false")
            .env("GIT_CONFIG_KEY_1", "gc.autoDetach")
            .env("GIT_CONFIG_VALUE_1", "false")
            .env("LC_ALL", "C")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // SAFETY: setsid is async-signal-safe and runs only in the forked child.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        let mut child = command
            .spawn()
            .map_err(|_| "Required installed Git/OpenSSL fixture child failed to spawn")?;
        let owner = match super::owned::OwnedPtySession::capture(child.id()) {
            Ok(owner) => owner,
            Err(_) => {
                // std::Child still owns this unreaped exact child. No arbitrary PID.
                // Unix spawn returns an actual positive pid_t. Without captured
                // session identity, reaping this child cannot prove descendant
                // absence; record that uncertainty for the directory guard.
                let pid = child.id() as i32;
                let mut report = super::owned::CleanupReport {
                    application_pid: pid,
                    errors: vec![
                        "Fixture child session capture failed; exact session/descendant absence unproven"
                            .into(),
                    ],
                    ..Default::default()
                };
                if child.kill().is_ok() {
                    report.signalled.push(pid);
                }
                let until = Instant::now() + Duration::from_secs(1);
                while Instant::now() < until {
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        report.reaped.push(pid);
                        break;
                    }
                    thread::yield_now();
                }
                if report.reaped.is_empty() {
                    report.remaining.push(pid);
                }
                // A poisoned report mutex itself makes directory retention
                // mandatory. Never infer success from failing to append.
                if let Ok(mut reports) = reports.lock() {
                    reports.push(report);
                }
                return Err(
                    "Required fixture child did not retain exact owned session identity".into(),
                );
            }
        };
        let result = Self {
            child,
            owner,
            reports: reports.clone(),
            cleaned: false,
        };
        for fd in [
            result.child.stdin.as_ref().map(AsRawFd::as_raw_fd),
            result.child.stdout.as_ref().map(AsRawFd::as_raw_fd),
            result.child.stderr.as_ref().map(AsRawFd::as_raw_fd),
        ]
        .into_iter()
        .flatten()
        {
            nonblocking(fd)?;
        }
        Ok(result)
    }
    pub(super) fn running(&mut self) -> Result<bool, String> {
        let pid = self.owner.application_pid();
        let actual = super::owned::process_identity(pid)
            .map_err(|_| "Actual fixture child identity unavailable")?;
        if actual
            .as_ref()
            .is_some_and(|p| !self.owner.owns(p) || p.zombie)
        {
            return Ok(false);
        }
        self.child
            .try_wait()
            .map(|s| s.is_none())
            .map_err(|_| "Fixture child wait failed".into())
    }
    pub(super) fn pipes(&mut self) -> Result<(ChildStdin, ChildStdout), String> {
        Ok((
            self.child.stdin.take().ok_or("Fixture stdin unavailable")?,
            self.child
                .stdout
                .take()
                .ok_or("Fixture stdout unavailable")?,
        ))
    }
    pub(super) fn stderr(&mut self) -> Result<std::process::ChildStderr, String> {
        self.child
            .stderr
            .take()
            .ok_or_else(|| "Fixture stderr unavailable".into())
    }
    pub(super) fn collect(
        &mut self,
        input: &[u8],
        cap: usize,
        stop: &AtomicBool,
        deadline: Instant,
    ) -> Result<Vec<u8>, String> {
        let mut stdin = self.child.stdin.take();
        let mut stdout = self
            .child
            .stdout
            .take()
            .ok_or("Fixture stdout unavailable")?;
        let mut stderr = self
            .child
            .stderr
            .take()
            .ok_or("Fixture stderr unavailable")?;
        let (mut offset, mut out_open, mut err_open) = (0, true, true);
        let (mut output, mut error_bytes) = (Vec::new(), 0usize);
        loop {
            alive(stop, deadline)?;
            if let Some(writer) = stdin.as_mut() {
                if offset == input.len() {
                    stdin.take();
                } else {
                    match writer.write(&input[offset..]) {
                        Ok(0) => return Err("Fixture child input closed early".into()),
                        Ok(n) => offset += n,
                        Err(e)
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                            ) => {}
                        Err(_) => return Err("Fixture child input failed".into()),
                    }
                }
            }
            let mut bytes = [0u8; 8192];
            for (reader, open, keep) in [
                (&mut stdout as &mut dyn Read, &mut out_open, true),
                (&mut stderr as &mut dyn Read, &mut err_open, false),
            ] {
                if !*open {
                    continue;
                }
                match reader.read(&mut bytes) {
                    Ok(0) => *open = false,
                    Ok(n) => {
                        let total = output
                            .len()
                            .checked_add(error_bytes)
                            .and_then(|v| v.checked_add(n))
                            .ok_or("Fixture capture overflow")?;
                        if total > cap {
                            return Err("Fixture child exceeded its fixed capture bound".into());
                        }
                        if keep {
                            output.extend_from_slice(&bytes[..n]);
                        } else {
                            error_bytes += n;
                        }
                    }
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                        ) => {}
                    Err(_) => return Err("Fixture child capture failed".into()),
                }
            }
            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|_| "Fixture child wait failed")?
            {
                if !out_open && !err_open {
                    if !status.success() || offset != input.len() {
                        return Err(
                            "Required fixture child failed; private diagnostics withheld".into(),
                        );
                    }
                    self.cleanup()?;
                    return Ok(output);
                }
            }
            let mut fds = [
                libc::pollfd {
                    fd: stdout.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: stderr.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            // SAFETY: initialized owned pipe descriptors; short finite wait.
            unsafe {
                libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 2);
            }
        }
    }
    pub(super) fn cleanup(&mut self) -> Result<(), String> {
        if self.cleaned {
            return Ok(());
        }
        let mut report = self.owner.cleanup(&mut self.child);
        self.cleaned = true;
        let pid = report.application_pid;
        match super::owned::process_identity(pid) {
            Ok(None) => {}
            Ok(Some(_)) => report
                .errors
                .push("Exact fixture PID absence was not proven after cleanup".into()),
            Err(_) => report
                .errors
                .push("Post-cleanup fixture PID identity unavailable".into()),
        }
        let clean = report.remaining.is_empty() && report.errors.is_empty();
        self.reports
            .lock()
            .map_err(|_| "Fixture cleanup report poisoned")?
            .push(report);
        if clean {
            Ok(())
        } else {
            Err("Owned fixture process/session cleanup was not proven".into())
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

pub(super) fn run(
    command: Command,
    home: &Path,
    input: &[u8],
    cap: usize,
    reports: &Arc<Mutex<Vec<super::owned::CleanupReport>>>,
) -> Result<Vec<u8>, String> {
    let stop = AtomicBool::new(false);
    OwnedChild::spawn(command, home, reports)?.collect(
        input,
        cap,
        &stop,
        Instant::now() + Duration::from_secs(5),
    )
}
