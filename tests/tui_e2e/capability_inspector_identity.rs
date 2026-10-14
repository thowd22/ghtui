//! These tests send real PTY input and synchronize the owned refresh process;
//! private inspector state must disappear after the verified principal changes.
use super::*;
use ghtui::storage::SyncFrame;
use std::process::Command;

const UID42_CANARY: &str = "CapabilityIdentity42PrivateBodyMustNeverBeDisplayed";
const UID43_CANARY: &str = "CapabilityIdentity43PrivateBodyMustNeverBeDisplayed";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transition {
    SameLoginNewNumericAccount,
    AuthenticationExpired,
}
impl Transition {
    fn expected_requests(self) -> usize {
        match self {
            Self::SameLoginNewNumericAccount => 12,
            Self::AuthenticationExpired => 9,
        }
    }
}

fn identity_user(uid: u64) -> GhStep {
    GhStep::read(
        &["api", "user", "--hostname", HOST, "--method", "GET"],
        &json!({"id":uid,"login":"fixture"}).to_string(),
    )
}
fn empty_queue(qualifier: &str) -> GhStep {
    GhStep::read(
        &[
            "api",
            "search/issues",
            "--hostname",
            HOST,
            "--method",
            "GET",
            "-f",
            &format!("q=is:pr is:open {qualifier}:fixture"),
            "-f",
            "per_page=100",
            "-f",
            "page=1",
        ],
        "{\"total_count\":0,\"incomplete_results\":false,\"items\":[]}",
    )
}
fn identity_protocol(transition: Transition) -> Vec<GhStep> {
    // Actual initial identity2 -> cached empty workspace -> automatic fresh
    // identity2 and independent authored/assigned/review-requested reads3.
    let mut steps = vec![version(), identity_user(42), version(), identity_user(42)];
    steps.extend(["author", "assignee", "review-requested"].map(empty_queue));
    steps.push(version());
    let mut changed = identity_user(43);
    // This fixture latency is not a readiness sleep: after its exact command
    // is journalled the test stops this positive PID, observes the stopped
    // process, and later explicitly resumes it. It prevents an unobservable
    // fast response racing the SIGSTOP at the command-entry boundary.
    changed.delay_ms = 1_000;
    if transition == Transition::AuthenticationExpired {
        changed.stdout.clear();
        changed.stderr = b"HTTP 401: authentication expired; use gh auth login\n".to_vec();
        changed.status = 1;
    }
    steps.push(changed);
    if transition == Transition::SameLoginNewNumericAccount {
        steps.extend(["author", "assignee", "review-requested"].map(empty_queue));
    }
    steps
}
fn identity_fixture(transition: Transition) -> Fixture {
    // Reuse the genuine dirty/staged/untracked repository and retained scoped
    // canary/draft/view fixture. Only this fresh child's scripted protocol is
    // replaced, never a shared fixture executable or developer environment.
    let fixture = fixture(Scenario::AuthDenied);
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    // Initialize the real journal schema before launch so the independent
    // zero-attempt assertion does not mistake a never-opened journal for an
    // application failure. No permit, attempt or publication is created.
    drop(
        ghtui::pr_write_journal::PrWriteJournal::open(
            &fixture.root.join("config/ghtui/cache.sqlite3"),
        )
        .unwrap(),
    );
    for (uid, body) in [(42, UID42_CANARY), (43, UID43_CANARY)] {
        let scope = Scope::github(HOST, Some(uid)).unwrap();
        cache
            .put(
                &scope,
                &Record::new(
                    "inspector-identity-canary",
                    "scoped-private-body",
                    "example/private",
                    json!({"id":uid,"body":body}),
                ),
            )
            .unwrap();
    }
    fixture.scripted_gh(&identity_protocol(transition));
    fixture
}
fn inbox_source() -> String {
    format!(
        "personal-inbox:{}",
        json!({"repositories":[],"excluded":[]})
    )
}
fn inbox_frame(fixture: &Fixture, uid: u64) -> Option<SyncFrame> {
    Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3"))
        .unwrap()
        .sync_frame(&Scope::github(HOST, Some(uid)).unwrap(), &inbox_source())
        .unwrap()
}
fn assert_empty_frame(frame: &SyncFrame) {
    assert_eq!(frame.source, inbox_source());
    assert_eq!(frame.generation, 1);
    assert!(frame.complete);
    assert_eq!(frame.records.len(), 1);
    let metadata = &frame.records[0];
    assert_eq!(metadata.kind, "inbox-meta");
    assert_eq!(metadata.id, inbox_source());
    assert!(metadata.repository.is_empty());
    assert_eq!(metadata.payload["coverage"]["loaded"], 0);
    assert_eq!(metadata.payload["coverage"]["total_matches"], 0);
    assert_eq!(metadata.payload["coverage"]["complete"], true);
    assert_eq!(metadata.payload["coverage"]["reasons"], json!([]));
}
fn assert_identity_storage(fixture: &Fixture, original: &SyncFrame, transition: Transition) {
    assert_storage(fixture, Scenario::AuthDenied);
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    for (uid, body) in [(42, UID42_CANARY), (43, UID43_CANARY)] {
        let scope = Scope::github(HOST, Some(uid)).unwrap();
        let rows = cache
            .list(&scope, Some("inspector-identity-canary"))
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "scoped-private-body");
        assert_eq!(rows[0].repository, "example/private");
        assert_eq!(rows[0].payload, json!({"id":uid,"body":body}));
    }
    assert_eq!(inbox_frame(fixture, 42).as_ref(), Some(original));
    match transition {
        Transition::SameLoginNewNumericAccount => {
            assert_empty_frame(&inbox_frame(fixture, 43).expect("separate committed UID43 source"));
        }
        Transition::AuthenticationExpired => {
            assert!(inbox_frame(fixture, 43).is_none());
        }
    }
    let sql = rusqlite::Connection::open(fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    for uid in [42, 43] {
        let scope = Scope::github(HOST, Some(uid)).unwrap();
        let attempts: usize = sql
            .query_row(
                "SELECT COUNT(*) FROM pr_write_attempts WHERE host=?1 AND principal=?2",
                rusqlite::params![scope.host, scope.login],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            attempts, 0,
            "Read-only inspector/refresh must not create an original write"
        );
    }
}

struct StoppedGh {
    pid: i32,
    stopped: bool,
}
impl StoppedGh {
    fn stop(session: &mut Session, fixture: &Fixture) -> Self {
        session.wait_for_command_count(fixture, 9);
        let rows = requests(session, fixture);
        assert_eq!(
            rows.len(),
            9,
            "Refresh must not get past its current-principal read"
        );
        let current = &rows[8];
        assert_eq!(current["matched"], true);
        assert_eq!(current["args"], json!(identity_user(43).args));
        let pid = i32::try_from(current["pid"].as_u64().unwrap()).unwrap();
        assert!(pid > 0);
        // SAFETY: the exact strict fixture journal identifies one positive,
        // currently live child PID. No process group or unrelated PID is used.
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            0,
            "Current-principal read is live"
        );
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        let guard = Self { pid, stopped: true };
        session
            .try_wait_for_condition(
                |_| {
                    let output = Command::new("ps")
                        .args(["-p", &pid.to_string(), "-o", "stat="])
                        .output()
                        .expect("POSIX process-state inspection");
                    output.status.success()
                        && String::from_utf8_lossy(&output.stdout)
                            .trim_start()
                            .starts_with('T')
                },
                "owned current-principal fixture process observed stopped",
                Duration::from_secs(8),
            )
            .unwrap();
        guard
    }
    fn release(&mut self) {
        assert!(self.stopped);
        // SAFETY: resumes only the positive owned child stopped above.
        assert_eq!(unsafe { libc::kill(self.pid, libc::SIGCONT) }, 0);
        self.stopped = false;
    }
    fn wait_reaped(&self, session: &mut Session) {
        session
            .try_wait_for_condition(
                |_| {
                    // SAFETY: signal0 checks the previously recorded positive
                    // PID; ESRCH proves it is no longer live or an unreaped child.
                    (unsafe { libc::kill(self.pid, 0) }) == -1
                        && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
                },
                "released current-principal fixture child reaped",
                Duration::from_secs(8),
            )
            .unwrap();
    }
}
impl Drop for StoppedGh {
    fn drop(&mut self) {
        if self.stopped {
            // Avoid leaking a stopped descendant if a meaningful assertion
            // fails; normal Session cleanup still owns cancellation/reaping.
            unsafe { libc::kill(self.pid, libc::SIGCONT) };
        }
    }
}
fn assert_identity_protocol(session: &Session, fixture: &Fixture, transition: Transition) {
    let rows = requests(session, fixture);
    let expected = identity_protocol(transition);
    assert_eq!(rows.len(), transition.expected_requests());
    assert_eq!(rows.len(), expected.len());
    for (index, (row, step)) in rows.iter().zip(expected).enumerate() {
        assert_eq!(row["step"], index);
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["stdin_hex"], "");
        assert!(row["args"][0] == "--version" || row["args"][5] == "GET");
    }
}
fn assert_no_private_text(session: &Session) {
    for body in [PRIVATE_CANARY, UID42_CANARY, UID43_CANARY, DRAFT] {
        assert!(
            !session.screen().contains(body),
            "Private scoped text leaked to current screen"
        );
        assert!(
            !String::from_utf8_lossy(&session.transcript).contains(body),
            "Private scoped text must never have been emitted to the real terminal"
        );
    }
}
fn wait_identity_completion(session: &mut Session, transition: Transition) {
    session
        .try_wait_for_condition(
            |screen| {
                !screen.contains(TITLE)
                    && !screen.contains("[PgDn Rows]")
                    && !screen.contains("account #42")
                    && !screen.contains("Loading Inbox")
                    && match transition {
                        Transition::SameLoginNewNumericAccount => {
                            screen.contains("Fresh • fetched")
                                && screen.contains("0 unique PRs across 0 queue matches • complete")
                        }
                        Transition::AuthenticationExpired => {
                            screen.contains("identity unavailable • private data hidden")
                                && screen
                                    .contains("Identity verification failed; private pages hidden")
                        }
                    }
            },
            "completed principal refresh drops original inspector, header and body",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| {
            session.save_artifacts();
            panic!("{error}; artifacts: {}", session.artifact_dir.display());
        });
}

fn identity_journey(transition: Transition, mouse: bool) {
    let fixture = identity_fixture(transition);
    let git_before = git_snapshot(&fixture);
    let config_before = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, Scenario::AuthDenied);
    session.wait_for_screen("Recent commits — saved versions");
    activate(&mut session, mouse, "1 Inbox", b"1");
    activate(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_command_count(&fixture, 7);
    session.wait_for_screen("Fresh • fetched");
    session.wait_for_screen("0 unique PRs across 0 queue matches • complete");
    session.wait_for_screen(&format!("{HOST} / @fixture"));
    let original = inbox_frame(&fixture, 42).expect("actual committed initial UID42 source");
    assert_empty_frame(&original);
    assert!(inbox_frame(&fixture, 43).is_none());
    let git_trace = trace(&fixture);
    assert!(
        !git_trace.is_empty(),
        "Real child Git trace must establish a nonempty baseline"
    );
    assert_no_private_text(&session);

    // Begin an explicit refresh OUTSIDE the inspector, then prove its exact
    // current-principal child cannot complete before the overlay is opened.
    activate(&mut session, mouse, "[r Refresh]", b"r");
    let mut delayed = StoppedGh::stop(&mut session, &fixture);
    session.wait_for_screen("Loading Inbox");
    open(&mut session, mouse);
    wait_overlay(&mut session, "account #42");
    wait_overlay(&mut session, "@fixture");
    let wide_filter = session.locate("Filter focused:").unwrap();
    let wide_detail = session.locate("[+ Detail]").unwrap();
    session.resize(80, 24);
    wait_overlay(&mut session, "account #42");
    let narrow_filter = session.locate("Filter focused:").unwrap();
    let narrow_detail = session.locate("[+ Detail]").unwrap();
    session.send_key(b"\t\r");
    session.wait_for_screen("details focused");
    let narrow_body = session.locate("Action:").unwrap();
    assert_eq!(requests(&session, &fixture).len(), 9);
    assert_eq!(trace(&fixture), git_trace);
    assert_no_private_text(&session);
    delayed.release();
    session.wait_for_command_count(&fixture, transition.expected_requests());
    wait_identity_completion(&mut session, transition);
    delayed.wait_reaped(&mut session);
    assert_identity_protocol(&session, &fixture, transition);
    assert_identity_storage(&fixture, &original, transition);
    let completed_journal = session.inspect_command_journal(&fixture);
    assert_eq!(trace(&fixture), git_trace);

    // An actual old hit coordinate must no longer address InspectorAction.
    // The new empty Inbox contains no openable record at these body points.
    // A later explicit read-only reopen is a terminal event-order barrier for
    // all prior stale input, not a sleep or a direct application-state check.
    if mouse {
        for (column, row) in [narrow_filter, narrow_detail, narrow_body] {
            session.click(column, row);
        }
        session.wheel(narrow_body.0, narrow_body.1, true);
    } else {
        session.send_key(b"\r\x1b[B\r");
    }
    session.resize(120, 40);
    if mouse {
        for (column, row) in [wide_filter, wide_detail] {
            session.click(column, row);
        }
    } else {
        session.send_key(b"\r");
    }
    // A real Help/Close round trip is an event-order barrier for the stale
    // inputs and resize. It also independently proves the current shell is
    // usable: a retained inspector or another capturing overlay cannot pass
    // this transition. No direct handlers or readiness sleeps are involved.
    activate(&mut session, mouse, "[? Help]", b"?");
    session.wait_for_screen("Help • keyboard and mouse");
    if mouse {
        session.close_overlay_by_mouse("Help • keyboard and mouse");
    } else {
        session.close_overlay_by_key("Help • keyboard and mouse");
    }
    open(&mut session, mouse);
    match transition {
        Transition::SameLoginNewNumericAccount => {
            wait_overlay(&mut session, "account #43");
            wait_overlay(&mut session, "@fixture");
        }
        Transition::AuthenticationExpired => {
            wait_overlay(&mut session, "No verified numeric GitHub identity");
        }
    }
    assert!(!overlay_text(&session).contains("account #42"));
    assert_no_private_text(&session);
    close(&mut session, mouse, "Inbox");
    assert_eq!(session.inspect_command_journal(&fixture), completed_journal);
    assert_eq!(
        trace(&fixture),
        git_trace,
        "Stale hits, Enter and readonly reopen must add no Git"
    );
    assert_identity_protocol(&session, &fixture, transition);
    assert_identity_storage(&fixture, &original, transition);
    assert_eq!(git_snapshot(&fixture), git_before);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config_before
    );
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(session.inspect_command_journal(&fixture), completed_journal);
    assert_eq!(trace(&fixture), git_trace);
    assert_identity_storage(&fixture, &original, transition);
    assert_eq!(git_snapshot(&fixture), git_before);
}

#[test]
fn gh_provider_inspector_mouse_keyboard_resize_same_login_uid_change_keyboard() {
    identity_journey(Transition::SameLoginNewNumericAccount, false);
}
#[test]
fn gh_provider_inspector_mouse_keyboard_resize_same_login_uid_change_mouse() {
    identity_journey(Transition::SameLoginNewNumericAccount, true);
}
#[test]
fn gh_provider_inspector_mouse_keyboard_resize_authentication_expiry_keyboard() {
    identity_journey(Transition::AuthenticationExpired, false);
}
#[test]
fn gh_provider_inspector_mouse_keyboard_resize_authentication_expiry_mouse() {
    identity_journey(Transition::AuthenticationExpired, true);
}
