//! SOURCE-ONLY child of frozen V2 native continuations. Eight declarations,
//! strict real-binary keyboard/mouse protocols, NOT executed test results.
use super::*;
use std::process::Command;

const OLD_LOG: &str = "EXACT_MATCHED_JOB_LOG_CANARY";
const REJECTED_LOG: &str = "REJECTED_CURRENT_REFRESH_PRIVATE_CANARY";
const SQL_FENCE: &str = "INDEPENDENT_ORIGINAL_SOURCE_FENCE";

#[derive(Clone, Copy)]
enum CorrectiveCase {
    ClosingUid,
    GlobalRefresh,
    ClosingSql,
    CancelHeld,
}
struct HeldChild {
    pid: i32,
    stopped: bool,
}
impl HeldChild {
    fn actual(session: &mut Session, fixture: &Fixture, expected: &[GhStep]) -> Self {
        session.wait_for_command_count(fixture, 89);
        let journal = commands(session, fixture);
        assert_eq!(journal.len(), 89);
        let row = &journal[88];
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(expected[86].args));
        assert_eq!(expected[88].args[1], format!("repos/{FIRST}"));
        let pid = i32::try_from(row["pid"].as_u64().unwrap()).unwrap();
        assert!(pid > 0);
        // SAFETY: the strict journal's exact currently live child PID only.
        assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        let held = Self { pid, stopped: true };
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
                "Actual continuation closing repository child is stopped",
                Duration::from_secs(8),
            )
            .unwrap();
        held
    }
    fn release(&mut self) {
        if unsafe { libc::kill(self.pid, 0) } == 0 {
            assert_eq!(unsafe { libc::kill(self.pid, libc::SIGCONT) }, 0);
        }
        self.stopped = false;
    }
    fn reaped(&mut self, session: &mut Session) {
        self.release();
        session
            .try_wait_for_condition(
                |_| {
                    (unsafe { libc::kill(self.pid, 0) }) == -1
                        && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
                },
                "Original continuation child actually reaped",
                Duration::from_secs(8),
            )
            .unwrap();
    }
}
impl Drop for HeldChild {
    fn drop(&mut self) {
        if self.stopped {
            unsafe { libc::kill(self.pid, libc::SIGCONT) };
        }
    }
}

fn global_refresh(session: &mut Session, mouse: bool) {
    session.resize(120, 40);
    session.wait_for_main_frame();
    if mouse {
        // Deliberately select the physical GLOBAL footer, not the identically
        // labelled Actions toolbar. ASCII footer prefix makes its cell exact.
        // The key bar's single `r Refresh` token carries the global Refresh, as `r` does.
        let (x, y) = session.wait_for_control("[r Refresh]");
        assert_eq!(y, 39, "Actual global Refresh is the key-bar token");
        session.click(x, y);
    } else {
        session.send_key(b":");
        overlay_ready(
            session,
            "Commands · current context",
            &["Filter focused:", "[Enter Choose]"],
        );
        session.send_key(b"\x01");
        session.paste("Refresh current view");
        overlay_ready(
            session,
            "Commands · current context",
            &["> Refresh current view"],
        );
        session.send_key(b"\r");
        session.wait_for_overlay_close("Commands · current context");
    }
}
fn hidden_frame(session: &mut Session) {
    // No Close/action is sent to manufacture privacy cleanup. Observe the
    // first eventual completed frame after provider/SQL failure directly.
    session
        .try_wait_for_condition(
            |screen| {
                let words = visible_words(screen);
                (words.contains("old private results hidden")
                    || words.contains("Original repository scope is fenced"))
                    && !screen.contains(OLD_LOG)
                    && !screen.contains(REJECTED_LOG)
                    && !screen.contains("Run 123 • attempt 2")
                    && !screen.contains("Exact observed build")
                    && !screen.contains(DETAIL_BODY)
                    && !screen.contains(PRIVATE_DRAFT)
                    && !screen.contains(SQL_FENCE)
                    && !screen.contains("[Esc Cancel]")
            },
            "Current failed proof hides old/captured private results without Close",
            Duration::from_secs(8),
        )
        .unwrap();
}
fn expected(case: CorrectiveCase) -> Vec<GhStep> {
    let mut all = baseline(false);
    all.extend(logs());
    assert_eq!(all.len(), 76);
    let mut next = finite_logs();
    match case {
        CorrectiveCase::GlobalRefresh => {}
        CorrectiveCase::ClosingUid => {
            next[7].stdout = REJECTED_LOG.as_bytes().to_vec();
            next[11] = who(43);
            next.truncate(12);
        }
        CorrectiveCase::ClosingSql | CorrectiveCase::CancelHeld => {
            next[7].stdout = REJECTED_LOG.as_bytes().to_vec();
            next[12].delay_ms = 3000;
            next.truncate(13);
        }
    }
    all.extend(next);
    all
}
fn journey(mouse: bool, case: CorrectiveCase) {
    let expected = expected(case);
    let fixture = resource_configured(&expected);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_run(&mut session, &fixture, mouse);
    open_selected_job(&mut session, &fixture, mouse, 501, 61);
    control(&mut session, mouse, "[l Logs]", b"l");
    session.wait_for_command_count(&fixture, 76);
    ready(&mut session, &[OLD_LOG, "Scoped exact log line 00"]);
    let original_sql = sql_state(&fixture);
    let original_trace = git_commands(&fixture);
    global_refresh(&mut session, mouse);
    let mut final_sql = original_sql.clone();
    match case {
        CorrectiveCase::GlobalRefresh => {
            session.wait_for_command_count(&fixture, 91);
            ready(&mut session, &[FRESH_LOG, "Scoped exact log line 01"]);
            assert!(!session.screen().contains(OLD_LOG));
            assert!(!session.screen().contains(REJECTED_LOG));
            assert!(session.visible_overlay_body("Action unavailable").is_none());
            ready(
                &mut session,
                &["Original scoped selected-job logs refreshed"],
            );
            assert_eq!(sql_state(&fixture), original_sql);
            control(&mut session, mouse, "[b Back to PR checks]", b"b");
            ready(&mut session, &["Exact observed build", REQUIRED_SUMMARY]);
            assert!(!session.screen().contains(FRESH_LOG));
        }
        CorrectiveCase::ClosingUid => {
            session.wait_for_command_count(&fixture, 88);
            hidden_frame(&mut session);
            assert_eq!(sql_state(&fixture), original_sql);
            // Fenced follow-up cannot query a default source or private draft.
            choose(&mut session, mouse, "1 Inbox", b"1");
            global_refresh(&mut session, mouse);
            session.poll_output(Duration::from_millis(250));
            assert_eq!(commands(&session, &fixture).len(), 88);
        }
        CorrectiveCase::ClosingSql => {
            let mut held = HeldChild::actual(&mut session, &fixture, &expected);
            ready(&mut session, &[OLD_LOG, "[Esc Cancel]"]);
            let db = rusqlite::Connection::open(database(&fixture)).unwrap();
            let source: String = db.query_row("SELECT source FROM sync_reservations WHERE host=?1 AND login='user-id:42' AND source LIKE 'repository-catalogue-source-v1:%'", [HOST], |row|row.get(0)).unwrap();
            let suffix = source
                .strip_prefix("repository-catalogue-source-v1:")
                .unwrap();
            assert_eq!(suffix.len(), 64);
            let key = format!("repository-catalogue-intent-v1:{suffix}");
            assert_eq!(db.execute("INSERT INTO saved_views(host,login,name,payload) VALUES(?2,'user-id:42',?3,?1) ON CONFLICT(host,login,name) DO UPDATE SET payload=excluded.payload", rusqlite::params![SQL_FENCE,HOST,key]).unwrap(), 1, "Exactly the genuine observed original source changed independently");
            drop(db);
            final_sql = sql_state(&fixture);
            assert_ne!(final_sql, original_sql);
            held.release();
            hidden_frame(&mut session);
            held.reaped(&mut session);
            assert_eq!(
                commands(&session, &fixture).len(),
                89,
                "No final principal or fallback after closing original SQL refusal"
            );
            assert_eq!(sql_state(&fixture), final_sql);
        }
        CorrectiveCase::CancelHeld => {
            let mut held = HeldChild::actual(&mut session, &fixture, &expected);
            ready(&mut session, &[OLD_LOG, "[Esc Cancel]"]);
            choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
            session
                .try_wait_for_condition(
                    |screen| {
                        screen.contains("Cancelled")
                            && !screen.contains(OLD_LOG)
                            && !screen.contains(REJECTED_LOG)
                            && !screen.contains("[Esc Cancel]")
                    },
                    "Owned Cancel stops held read and retires loaded remote capture",
                    Duration::from_secs(8),
                )
                .unwrap();
            held.reaped(&mut session);
            choose(&mut session, mouse, "1 Inbox", b"1");
            ready(
                &mut session,
                &["Exact observed build", REQUIRED_SUMMARY, "[b Back]"],
            );
            assert!(!session.screen().contains(REJECTED_LOG));
            assert_eq!(commands(&session, &fixture).len(), 89);
            assert_eq!(sql_state(&fixture), original_sql);
        }
    }
    session.resize(if mouse { 80 } else { 120 }, if mouse { 24 } else { 40 });
    session.wait_for_main_frame();
    assert!(!session.screen().contains(PRIVATE_DRAFT));
    assert!(!session.screen().contains(REJECTED_LOG));
    assert_eq!(commands(&session, &fixture).len(), expected.len());
    assert_eq!(sql_state(&fixture), final_sql);
    no_attempts(&fixture);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), original_trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
    exact_reads(&session, &fixture, &expected);
    assert_eq!(sql_state(&fixture), final_sql);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), original_trace);
}

#[test]
fn scoped_corrective_keyboard_current_closing_uid_hides_same_completed_frame() {
    journey(false, CorrectiveCase::ClosingUid)
}
#[test]
fn scoped_corrective_mouse_current_closing_uid_hides_same_completed_frame() {
    journey(true, CorrectiveCase::ClosingUid)
}
#[test]
fn scoped_corrective_keyboard_palette_refresh_uses_genuine_scoped_capture_not_legacy_completed_frame_actions_scoped_checks_v1(
) {
    journey(false, CorrectiveCase::GlobalRefresh)
}
#[test]
fn scoped_corrective_mouse_global_footer_refresh_uses_genuine_scoped_capture_not_toolbar_completed_frame_actions_scoped_checks_v1(
) {
    journey(true, CorrectiveCase::GlobalRefresh)
}
#[test]
fn scoped_corrective_keyboard_closing_original_sql_hides_without_close_or_final_auth() {
    journey(false, CorrectiveCase::ClosingSql)
}
#[test]
fn scoped_corrective_mouse_closing_original_sql_hides_without_close_or_final_auth() {
    journey(true, CorrectiveCase::ClosingSql)
}
#[test]
fn scoped_corrective_keyboard_cancel_held_current_read_reaps_no_late_frame() {
    journey(false, CorrectiveCase::CancelHeld)
}
#[test]
fn scoped_corrective_mouse_cancel_held_current_read_reaps_no_late_frame() {
    journey(true, CorrectiveCase::CancelHeld)
}
