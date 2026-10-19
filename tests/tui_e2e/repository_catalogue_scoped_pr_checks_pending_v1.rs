//! Actual initial-open cancellation: retain the original Checks while a real
//! strict-gh child is held before closing principal/private publication.
use super::*;
use std::process::Command;

struct HeldOriginalRepository {
    pid: i32,
    stopped: bool,
}
impl HeldOriginalRepository {
    fn stop(session: &mut Session, fixture: &Fixture, expected: &[GhStep]) -> Self {
        session.wait_for_command_count(fixture, 57);
        let journal = commands(session, fixture);
        assert_eq!(journal.len(), 57);
        let child = &journal[56];
        assert_eq!(child["matched"], true);
        assert_eq!(child["args"], json!(expected[56].args));
        assert_eq!(expected[56].args[1], format!("repos/{FIRST}"));
        let pid = i32::try_from(child["pid"].as_u64().unwrap()).unwrap();
        assert!(pid > 0);
        // SAFETY: only this fixture's exact positive live subprocess, never a
        // process group or an inferred PID from a timeout.
        assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        let guard = Self { pid, stopped: true };
        session
            .try_wait_for_condition(
                |_| {
                    let output = Command::new("ps")
                        .args(["-p", &pid.to_string(), "-o", "stat="])
                        .output()
                        .unwrap();
                    output.status.success()
                        && String::from_utf8_lossy(&output.stdout)
                            .trim_start()
                            .starts_with('T')
                },
                "Actual original repository child held before final principal",
                Duration::from_secs(8),
            )
            .unwrap();
        guard
    }
    fn reaped(&mut self, session: &mut Session) {
        if unsafe { libc::kill(self.pid, 0) } == 0 {
            assert_eq!(unsafe { libc::kill(self.pid, libc::SIGCONT) }, 0);
        }
        self.stopped = false;
        session
            .try_wait_for_condition(
                |_| {
                    (unsafe { libc::kill(self.pid, 0) }) == -1
                        && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
                },
                "Retired original Checks child actually reaped",
                Duration::from_secs(8),
            )
            .unwrap();
    }
}
impl Drop for HeldOriginalRepository {
    fn drop(&mut self) {
        if self.stopped {
            unsafe { libc::kill(self.pid, libc::SIGCONT) };
        }
    }
}
fn fixture_steps(cancel_and_reopen: bool) -> Vec<GhStep> {
    let mut expected = initial();
    expected.extend(resource_operation(true));
    let mut held = bridge();
    held[15].delay_ms = 15_000;
    held.truncate(16);
    expected.extend(held);
    assert_eq!(expected.len(), 57);
    if cancel_and_reopen {
        expected.extend(bridge());
        assert_eq!(expected.len(), 75);
    }
    expected
}
fn open_control(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Enter Open check]");
    } else {
        // `Enter Open check` is a key-bar token: one Enter opens the selected check.
        session.wait_for_control("[Enter Open check]");
        session.send_key(b"\r");
    }
}
/// While the original read is pending, `Open check` is unavailable: its token leaves the key
/// bar and its Enter key explains why instead of starting another read.
fn disabled_open_control(session: &mut Session) {
    session.assert_control_absent("[Enter Open check]");
    session.send_key(b"\r");
}
/// The original Checks page. `Enter Open check` is a key-bar token only while a check row is
/// selected and no read is pending, so [`open_control`] waits for it separately.
fn original_checks(session: &mut Session) {
    ready(
        session,
        &[
            "Exact observed build",
            "Required rules unavailable",
            "ci-actor",
            "[b Back]",
        ],
    );
    assert!(!session.screen().contains("Run 123 • attempt 2"));
    assert!(!session.screen().contains(DETAIL_BODY));
    assert!(!session.screen().contains(PRIVATE_DRAFT));
}
fn journey(mouse: bool, cancel_and_reopen: bool) {
    let expected = fixture_steps(cancel_and_reopen);
    let fixture = resource_configured(&expected);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    selected(&mut session, &fixture, mouse);
    choose(&mut session, mouse, "[k Checks]", b"k");
    session.wait_for_command_count(&fixture, 41);
    original_checks(&mut session);
    select_observed_row(&mut session, mouse, "Exact observed build");
    let sql = sql_state(&fixture);
    let trace = git_commands(&fixture);
    open_control(&mut session, mouse);
    let mut held = HeldOriginalRepository::stop(&mut session, &fixture, &expected);
    original_checks(&mut session);
    // Loading paints the key-bar `Esc Cancel` token.
    session.wait_for_control("[Esc Cancel]");
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    session.wait_for_main_frame();
    original_checks(&mut session);
    // Loading paints the key-bar `Esc Cancel` token.
    session.wait_for_control("[Esc Cancel]");
    // The disabled control's Enter key explains the pending original read and does
    // not enqueue a duplicate request.
    disabled_open_control(&mut session);
    overlay_ready(&mut session, "Action unavailable", &["[Esc Close]"]);
    let explanation = session
        .wait_for_overlay_body("Action unavailable")
        .split_whitespace()
        .collect::<String>();
    assert!(explanation.contains("WaitfortheoriginalobservedChecksread"));
    assert_eq!(commands(&session, &fixture).len(), 57);
    assert_eq!(sql_state(&fixture), sql);
    close_notice(&mut session, mouse);
    original_checks(&mut session);
    if cancel_and_reopen {
        choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
        session
            .try_wait_for_condition(
                |screen| {
                    screen.contains("Cancelled")
                        && screen.contains("Exact observed build")
                        && !screen
                            .lines()
                            .last()
                            .unwrap_or_default()
                            .contains("Esc Cancel")
                        && !screen.contains("Run 123 • attempt 2")
                },
                "Cancel retains the actual original Checks without a late run",
                Duration::from_secs(8),
            )
            .unwrap();
        held.reaped(&mut session);
        assert_eq!(commands(&session, &fixture).len(), 57);
        assert_eq!(sql_state(&fixture), sql);
        original_checks(&mut session);
        // Explicit fresh Open must use a new complete 18-child bracket; the
        // retired token/result cannot be reused as a successful observation.
        open_control(&mut session, mouse);
        session.wait_for_command_count(&fixture, 75);
        ready(
            &mut session,
            &[
                "Run 123 • attempt 2",
                "test (stable)",
                "alice",
                &"a".repeat(40),
            ],
        );
        assert!(!session.screen().contains("Cancelled"));
        assert!(!session.screen().contains(DETAIL_BODY));
        assert!(!session.screen().contains(PRIVATE_DRAFT));
        control(&mut session, mouse, "[b Back to PR checks]", b"b");
        original_checks(&mut session);
    }
    choose(&mut session, mouse, "[b Back]", b"b");
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains(ROW_TITLE)
                    && screen.contains("Cached • fetched")
                    && !screen.contains("Exact observed build")
                    && !screen.contains(DETAIL_TITLE)
                    && !screen.contains("Run 123 • attempt 2")
                    && !screen
                        .lines()
                        .last()
                        .unwrap_or_default()
                        .contains("Esc Cancel")
            },
            "Original Back restores cached Inbox, retires pending owner and hides private page",
            Duration::from_secs(8),
        )
        .unwrap();
    if !cancel_and_reopen {
        held.reaped(&mut session);
    }
    unchanged(&session, &fixture, &sql, &expected);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
    exact_reads(&session, &fixture, &expected);
    assert_eq!(sql_state(&fixture), sql);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
}

#[test]
fn scoped_check_pending_keyboard_initial_original_back_reaps_no_late_result() {
    journey(false, false);
}
#[test]
fn scoped_check_pending_mouse_initial_original_back_reaps_no_late_result() {
    journey(true, false);
}
#[test]
fn scoped_check_pending_keyboard_cancel_keeps_checks_then_fresh_open() {
    journey(false, true);
}
#[test]
fn scoped_check_pending_mouse_cancel_keeps_checks_then_fresh_open() {
    journey(true, true);
}
