//! Task-specific runner/auth journeys: actual input, killed child and exact effects.
use crate::support::{empty_inbox_steps, Fixture, GhStep, Session};
use serde_json::Value;
use std::{
    fs,
    time::{Duration, Instant},
};

#[test]
fn process_runner_gh_provider_auth_capabilities_keyboard_cancels_identity_retries_auth_and_preserves_git(
) {
    journey(false);
}
#[test]
fn process_runner_gh_provider_auth_capabilities_mouse_cancels_identity_retries_auth_and_preserves_git(
) {
    journey(true);
}
fn activate(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}
fn records(session: &Session, fixture: &Fixture) -> Vec<Value> {
    session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "preserved.txt",
        b"saved original\n",
        "original saved commit",
    );
    fs::write(
        fixture.repo().join("preserved.txt"),
        b"preserve these local edits\n",
    )
    .unwrap();
    let original_head = fixture.git(&["rev-parse", "HEAD"]);
    let original_status = fixture.git(&["status", "--porcelain=v1"]);
    let original_index = fixture.git(&["ls-files", "--stage"]);
    let mut old_user = GhStep::read(
        &["api", "user", "--hostname", "github.com", "--method", "GET"],
        "{\"login\":\"old-account-must-not-appear\",\"id\":42}",
    );
    old_user.delay_ms = 5_000;
    let mut authentication_error = GhStep::read(&["--version"], "");
    authentication_error.status = 1;
    authentication_error.stderr = b"HTTP 401: fixture auth expired".to_vec();
    let mut steps = vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        old_user,
        authentication_error,
    ];
    steps.extend(empty_inbox_steps("bob"));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    activate(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("Loading Inbox");
    session.wait_for_command_count(&fixture, 2);
    assert_eq!(records(&session, &fixture).len(), 2);
    activate(&mut session, mouse, "Search", b"3");
    session.assert_focused_view("Search");
    session.resize(80, 24);
    activate(&mut session, mouse, "Inbox", b"1");
    session.assert_focused_view("Inbox");
    activate(&mut session, mouse, "[Esc Cancel]", b"\x1b");
    session.wait_for_screen("Cancelled");
    assert!(!session.screen().contains("old-account-must-not-appear"));
    let calls = records(&session, &fixture);
    assert_eq!(
        calls.len(),
        2,
        "Cancellation must not retry an identity command"
    );
    let pid = calls[1]["pid"].as_u64().unwrap() as libc::pid_t;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        // SAFETY: recorded fixture PID, signal zero observes liveness only.
        if unsafe { libc::kill(pid, 0) } == -1 {
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Cancelled identity child was not reaped"
        );
        std::thread::yield_now();
    }
    activate(&mut session, mouse, "[r Refresh]", b"r");
    // The native footer presents the useful authentication state, while the
    // detailed transport diagnostic is longer than the narrow terminal row.
    session.wait_for_screen("Error: GitHub authentication failed");
    session.wait_for_command_count(&fixture, 3);
    assert_eq!(
        records(&session, &fixture).len(),
        3,
        "Authentication error must not send additional private requests"
    );
    assert!(!session.screen().contains("old-account-must-not-appear"));
    activate(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("Fresh • fetched");
    session.wait_for_screen("@bob");
    session.wait_for_command_count(&fixture, 10);
    let calls = records(&session, &fixture);
    assert_eq!(calls.len(), 10);
    assert!(calls.iter().all(|call| call["matched"] == true));
    assert_eq!(
        calls
            .iter()
            .filter(|call| call["args"][1] == "user")
            .count(),
        3
    );
    assert!(calls
        .iter()
        .all(|call| call["args"][0] == "--version" || call["args"][5] == "GET"));
    assert!(calls.iter().all(|call| call["stdin_hex"] == ""));
    assert!(!session.screen().contains("old-account-must-not-appear"));
    session.resize(120, 40);
    session.wait_for_screen("@bob");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), original_head);
    assert_eq!(fixture.git(&["status", "--porcelain=v1"]), original_status);
    assert_eq!(fixture.git(&["ls-files", "--stage"]), original_index);
    assert_eq!(
        fs::read(fixture.repo().join("preserved.txt")).unwrap(),
        b"preserve these local edits\n"
    );
    session.quit_and_assert_restored_terminal(mouse);
}
