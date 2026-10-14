use super::support::{owned_pty_processes_v1::process_identity, Fixture, Session};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

fn held_fixture(fixture: &Fixture) {
    static BINARY: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    let binary = BINARY.get_or_init(|| {
        let directory = tempfile::tempdir().unwrap().keep();
        let binary = directory.join("owned-held-provider");
        let output = std::process::Command::new("rustc")
            .args([
                "--edition=2021",
                "tests/fixtures/owned_pty_descendants_v1.rs",
                "-o",
            ])
            .arg(&binary)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        binary
    });
    // Remove only this fixture's original symlink, never its shared target.
    fs::remove_file(fixture.root.join("bin/gh")).unwrap();
    std::os::unix::fs::symlink(binary, fixture.root.join("bin/gh")).unwrap();
}

fn read_pid(session: &mut Session, fixture: &Fixture, name: &str) -> [i32; 4] {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Ok(record) = fs::read_to_string(fixture.root.join(name)) {
            if let Ok(values) = record
                .split_ascii_whitespace()
                .map(str::parse::<i32>)
                .collect::<Result<Vec<_>, _>>()
            {
                if let Ok(values) = values.try_into() {
                    return values;
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "held fixture {name} never became ready"
        );
        session.poll_output(Duration::from_millis(5));
    }
}

fn journey(mouse: bool, abrupt: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("unchanged.txt", b"preserve me\n", "immutable saved version");
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    held_fixture(&fixture);
    let root = fixture.root.as_os_str();
    let mut first = Session::launch_binary_os_env(
        std::path::Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        120,
        40,
        &[(std::ffi::OsStr::new("GHTUI_OWNED_PROCESS_FIXTURE"), root)],
    );
    first.wait_for_screen("ghtui");
    if mouse {
        first.resize(80, 24);
        first.click_visible_action("[r Refresh]");
    } else {
        first.send_key(b"r");
    }
    first.wait_for_screen("Loading Inbox");
    let app = first.owned_application_pid();
    let parent = read_pid(&mut first, &fixture, "held-parent.pid");
    let grandchild = read_pid(&mut first, &fixture, "held-grandchild.pid");
    assert_eq!(parent[1], app);
    assert_eq!(grandchild[1], parent[0]);
    assert_eq!(parent[2], parent[0]);
    assert_eq!(grandchild[2], grandchild[0]);
    assert_eq!(parent[3], app);
    assert_eq!(grandchild[3], app);
    assert!(parent[0] != app && grandchild[0] != parent[0]);
    let artifacts = first.artifact_dir.clone();
    if abrupt {
        first.terminate_abruptly();
        // App-only death must not end the inherited operation before recovery
        // tests have a chance to inspect it. Only actual Drop ends ownership.
        for pid in [parent[0], grandchild[0]] {
            assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
        }
        drop(first);
    } else {
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _owned_session = first;
            panic!("deliberate held-provider failure exercising Session Drop");
        }));
        assert!(panic.is_err());
        assert!(
            artifacts.join("terminal.raw").exists(),
            "panic did not retain its exact PTY artifacts"
        );
    }
    for pid in [app, parent[0], grandchild[0]] {
        assert_eq!(
            process_identity(pid).unwrap(),
            None,
            "owned PID {pid} was not killed and reaped"
        );
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
    assert!(
        !artifacts.join("owned-process-cleanup.json").exists(),
        "bounded cleanup reported an incomplete result"
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    // A distinct successful actual session provides positive input and kernel
    // termios/alternate-screen/cursor/mouse restoration evidence. The deliberate
    // panicking Drop record is never counted as completed positive coverage.
    let fresh = Fixture::new();
    let mut restored = Session::launch(&fresh, 80, 24);
    restored.wait_for_screen("ghtui");
    if mouse {
        restored.click_visible_action("[? Help]");
        restored.wait_for_screen("Help • keyboard and mouse");
        restored.click_visible_action("[Esc Close]");
    } else {
        restored.send_key(b"?");
        restored.wait_for_screen("Help • keyboard and mouse");
        restored.close_overlay_by_key("Help • keyboard and mouse");
    }
    assert!(restored.inspect_command_journal(&fresh).is_empty());
    restored.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn test_framework_keyboard_forced_panic_reaps_distinct_provider_groups_and_fresh_terminal() {
    journey(false, false);
}
#[test]
fn test_framework_mouse_forced_panic_reaps_distinct_provider_groups_and_fresh_terminal() {
    journey(true, false);
}
#[test]
fn test_framework_keyboard_abrupt_app_only_preserves_descendants_until_drop() {
    journey(false, true);
}
#[test]
fn test_framework_mouse_abrupt_app_only_preserves_descendants_until_drop() {
    journey(true, true);
}
