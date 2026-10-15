#[path = "support/owned_pty_processes_v1.rs"]
mod owned;
use owned::{OwnedPtySession, ProcessIdentity};

struct ExactChild(std::process::Child);
impl ExactChild {
    fn new() -> Self {
        Self(
            std::process::Command::new("/bin/sleep")
                .arg("30")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        )
    }
    fn reap(&mut self) {
        self.0.kill().unwrap();
        self.0.wait().unwrap();
    }
}
impl Drop for ExactChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[test]
fn test_framework_process_identity_only_absence_errno_is_none_not_permission_or_invalid_data() {
    for errno in [libc::ENOENT, libc::ESRCH] {
        assert!(
            owned::finish_identity_read(Err(std::io::Error::from_raw_os_error(errno)))
                .unwrap()
                .is_none()
        );
    }
    for errno in [libc::EACCES, libc::EPERM, libc::EIO] {
        let error =
            owned::finish_identity_read(Err(std::io::Error::from_raw_os_error(errno))).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(errno));
    }
    let error = owned::finish_identity_read(Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "Malformed process ownership",
    )))
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn test_framework_process_identity_actual_reaped_child_kernel_probe_esrch_is_gone_not_fatal() {
    let mut child = ExactChild::new();
    let pid = i32::try_from(child.0.id()).unwrap();
    assert!(pid > 0 && pid != std::process::id() as i32);
    #[cfg(target_os = "linux")]
    let mut file = std::fs::File::open(format!("/proc/{pid}/stat")).unwrap();
    child.reap();
    #[cfg(target_os = "linux")]
    let error = {
        use std::io::Read;
        // The actual open kernel descriptor outlives this specific process.
        // Reading it now returns ESRCH rather than a fabricated NotFound.
        file.read_to_end(&mut Vec::new()).unwrap_err()
    };
    #[cfg(target_os = "macos")]
    let error = {
        // SAFETY: read-only positive exact direct-child PID, already reaped.
        assert_eq!(unsafe { libc::getsid(pid) }, -1);
        std::io::Error::last_os_error()
    };
    assert_eq!(error.raw_os_error(), Some(libc::ESRCH));
    assert!(owned::finish_identity_read(Err(error)).unwrap().is_none());
    assert!(owned::process_identity(pid).unwrap().is_none());
}

#[test]
fn test_framework_process_identity_live_child_retains_actual_uid_birth_and_session_then_disappears()
{
    let mut child = ExactChild::new();
    let pid = i32::try_from(child.0.id()).unwrap();
    let current = owned::process_identity(pid).unwrap().unwrap();
    assert_eq!(current.pid, pid);
    assert_eq!(current.parent, std::process::id() as i32);
    assert_eq!(current.uid, unsafe { libc::geteuid() });
    assert_eq!(current.session, unsafe { libc::getsid(pid) });
    assert!(current.birth.0 > 0 && !current.zombie);
    assert_eq!(
        owned::finish_identity_read(Ok(current.clone())).unwrap(),
        Some(current)
    );
    child.reap();
    assert!(owned::process_identity(pid).unwrap().is_none());
    assert!(owned::process_identity(0).is_err() && owned::process_identity(-1).is_err());
}

#[cfg(target_os = "linux")]
fn stat(pid: &str, state: &str, birth: &str) -> Vec<u8> {
    // Non-UTF8 and closing parentheses are legal command bytes. They must not
    // influence the actual fixed kernel ownership fields after the final ') '.
    let mut bytes = format!("{pid} (").into_bytes();
    bytes.extend_from_slice(b"strange ) command\xff");
    bytes.extend_from_slice(
        format!(") {state} 8 9 7 0 -1 0 0 0 0 0 0 0 0 0 0 0 0 0 {birth} 0\n").as_bytes(),
    );
    bytes
}

#[cfg(target_os = "linux")]
#[test]
fn test_framework_owned_cleanup_stat_preserves_exact_kernel_identity_not_command_name() {
    let identity = owned::parse_linux_stat(&stat("42", "S", "314"), 123).unwrap();
    assert_eq!(
        identity,
        ProcessIdentity {
            pid: 42,
            parent: 8,
            session: 7,
            uid: 123,
            birth: (314, 0),
            zombie: false
        }
    );
    assert!(
        owned::parse_linux_stat(&stat("42", "Z", "314"), 123)
            .unwrap()
            .zombie
    );
}

#[cfg(target_os = "linux")]
#[test]
fn test_framework_owned_cleanup_stat_refuses_malformed_negative_zero_truncated_and_oversized() {
    for bytes in [
        b"".to_vec(),
        b"42 missing command".to_vec(),
        stat("-1", "S", "314"),
        stat("0", "S", "314"),
        stat("42", "?", "314"),
        stat("42", "S", "0"),
        stat("42", "S", "not-a-birth"),
        vec![b'a'; 16_385],
    ] {
        assert!(
            owned::parse_linux_stat(&bytes, 123).is_err(),
            "accepted malformed kernel record: {bytes:?}"
        );
    }
    let mut truncated = stat("42", "S", "314");
    truncated.truncate(truncated.iter().rposition(|byte| *byte == b')').unwrap() + 4);
    assert!(owned::parse_linux_stat(&truncated, 123).is_err());
}

#[test]
fn test_framework_owned_cleanup_fixed_bounds_reject_one_more_not_silently_truncate() {
    for limit in [owned::MAX_SCANNED_PIDS, owned::MAX_OWNED_PIDS] {
        assert!(owned::check_bound(limit, limit).is_ok());
        assert!(owned::check_bound(limit + 1, limit).is_err());
        assert!(owned::check_bound(usize::MAX, limit).is_err());
    }
    assert!(owned::process_identity(0).is_err());
    assert!(owned::process_identity(-1).is_err());
    assert!(OwnedPtySession::capture(std::process::id()).is_err());
    assert!(OwnedPtySession::capture(u32::MAX).is_err());
}

#[test]
fn test_framework_owned_cleanup_foreign_session_and_replaced_identity_never_gain_authority() {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    owned::initialize_reaping().unwrap();
    let first = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new("/bin/sleep");
    command.arg("60");
    let mut app = first.slave.spawn_command(command).unwrap();
    let owner = OwnedPtySession::capture(app.process_id().unwrap()).unwrap();
    let original = owned::process_identity(owner.application_pid())
        .unwrap()
        .unwrap();
    assert!(owner.owns(&original));
    for changed in [
        ProcessIdentity {
            session: original.session + 1,
            ..original.clone()
        },
        ProcessIdentity {
            uid: original.uid.wrapping_add(1),
            ..original.clone()
        },
        ProcessIdentity {
            birth: (original.birth.0 + 1, original.birth.1),
            ..original.clone()
        },
        ProcessIdentity {
            pid: 0,
            ..original.clone()
        },
        ProcessIdentity {
            pid: unsafe { libc::getpid() },
            ..original.clone()
        },
    ] {
        assert!(!owner.owns(&changed));
    }
    let second = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new("/bin/sleep");
    command.arg("60");
    let mut foreign = second.slave.spawn_command(command).unwrap();
    let foreign_owner = OwnedPtySession::capture(foreign.process_id().unwrap()).unwrap();
    let foreign_identity = owned::process_identity(foreign_owner.application_pid())
        .unwrap()
        .unwrap();
    assert!(!owner.owns(&foreign_identity));
    let report = owner.cleanup(&mut *app);
    assert!(report.errors.is_empty(), "{report:?}");
    assert!(report.remaining.is_empty(), "{report:?}");
    assert!(report.reaped.contains(&original.pid));
    assert_eq!(owned::process_identity(original.pid).unwrap(), None);
    assert_eq!(
        unsafe { libc::kill(foreign_identity.pid, 0) },
        0,
        "another genuine PTY session was signalled"
    );
    let report = foreign_owner.cleanup(&mut *foreign);
    assert!(
        report.errors.is_empty() && report.remaining.is_empty(),
        "{report:?}"
    );
}
