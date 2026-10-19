//! DECLARED / UNRUN. Actual durable FILE attempts are inspected only after
//! restarting through genuine Personal Summary, never through blocked Files.
use super::*;
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession};
use std::time::Instant;
const ROOT_HISTORY: &str = "Thread publication history • inspect only";
const FILE_HISTORY: &str = "FILE comment publication history • inspect only";
const FILE_BODY: &str = "PRIVATE_NATIVE_WHOLE_FILE_BODY 界\nsecond FILE line";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RecoveryV75 {
    Pending,
    ConfirmedCleanupFailed,
    AccountLoss,
}
impl RecoveryV75 {
    pub(super) fn pending(self) -> bool {
        self != Self::ConfirmedCleanupFailed
    }
}
fn phase(uid: u64) -> Vec<GhStep> {
    let mut steps = identity("octocat");
    steps[1] = read(
        api("user"),
        json!({"id":uid,"login":"actual-personal-history-inspector"}),
    );
    if uid != 42 {
        return steps;
    }
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    steps.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.extend(identity("final-personal-history-inspector"));
    assert_eq!(steps.len(), 8);
    steps
}
pub(super) fn append_steps(steps: &mut Vec<GhStep>, mode: RecoveryV75) {
    let (mut summary, opened) = complete_steps();
    summary.truncate(opened);
    assert!(!summary.iter().any(|step| step
        .args
        .get(1)
        .is_some_and(|s| s.contains("files?per_page=100&page="))));
    steps.extend(summary);
    let mut list = phase(42);
    list[0].delay_ms = 6000;
    steps.extend(list);
    steps.extend(phase(42));
    steps.extend(phase(if mode == RecoveryV75::AccountLoss {
        43
    } else {
        42
    }));
}
fn commands(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn protected(fixture: &Fixture) -> Vec<String> {
    sql(fixture)
        .into_iter()
        .filter(|row| {
            row.starts_with("pr_write_")
                || row.starts_with("drafts:")
                || row.starts_with("actions_write_")
                || row.starts_with("saved_views:")
        })
        .collect()
}
fn assert_original_pending(fixture: &Fixture) {
    let db = rusqlite::Connection::open_with_flags(
        fixture.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let(encoded,id,head,fingerprint,state):(String,i64,String,String,String)=db.query_row("SELECT d.body,a.attempt_id,a.observed_head,a.payload_sha256,a.state FROM pr_write_attempts a JOIN pr_write_file_comment_origins_v75 o USING(attempt_id) JOIN drafts d ON d.resource LIKE 'file-comment-draft-v75/%' WHERE a.command='file-comment-create-v75'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
    let draft: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(draft["values"]["body"], FILE_BODY);
    assert_eq!(draft["publication"]["attempt_id"], id);
    assert_eq!(draft["publication"]["pr_node_id"], "PERSONAL_PR_1007");
    assert_eq!(draft["publication"]["observed_head"], head);
    assert_eq!(draft["publication"]["payload_sha256"], fingerprint);
    assert_eq!(state, "pending");
    assert!(draft.get("approved").is_none());
}
fn live_child(
    session: &mut Session,
    fixture: &Fixture,
    ordinal: usize,
) -> crate::support::owned_pty_processes_v1::ProcessIdentity {
    session.wait_for_command_count(fixture, ordinal + 1);
    let pid = i32::try_from(commands(fixture)[ordinal]["pid"].as_u64().unwrap()).unwrap();
    let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    let child = process_identity(pid)
        .unwrap()
        .expect("Actual held child lives before effect");
    assert!(owner.owns(&child) && !child.zombie);
    child
}
pub(super) fn hold_or_cancel(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    start: usize,
    mode: RecoveryV75,
) {
    let child = live_child(session, fixture, start + 6);
    assert_original_pending(fixture);
    if mode.pending() {
        choose(session, mouse, "[Esc Cancel]", b"\x1b");
        let end = Instant::now() + Duration::from_secs(8);
        loop {
            match process_identity(child.pid) {
                Ok(None) => break,
                Ok(Some(now))
                    if now.birth != child.birth
                        || now.uid != child.uid
                        || now.session != child.session =>
                {
                    break
                }
                Ok(Some(_)) => {}
                Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {}
                Err(error) => panic!("Original child identity observation failed: {error}"),
            }
            assert!(Instant::now() < end, "Original owned child still unreaped");
            session.poll_output(Duration::from_millis(10));
        }
    } else {
        rusqlite::Connection::open(fixture.root.join("config/ghtui/cache.sqlite3")).unwrap().execute_batch("CREATE TRIGGER personal_file_cleanup_failure_v75 BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'file-comment-draft-v75/%' BEGIN SELECT RAISE(ABORT,'actual Personal protected cleanup failure'); END;").unwrap();
    }
}
fn ready(session: &mut Session, title: &str, words: &[&str]) {
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, title).is_some_and(|body| {
                body.contains("[Esc Back]")
                    && body.contains("[q Quit]")
                    && words.iter().all(|word| body.contains(word))
            })
        },
        "Completed actual Personal-owned history frame",
        Duration::from_secs(8),
    );
}
fn disabled_open(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    title: &str,
    expected: usize,
) {
    let old = crate::support::overlay_body(session.completed_cursorless_screen().unwrap(), title)
        .unwrap();
    let at = session.completed_cursorless_end();
    if mouse {
        let (x, y) = session.wait_for_owned_completed_action(title, "[Enter Open]", &[]);
        session.click(x, y);
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_new_cursorless_condition(
        at,
        |frame| crate::support::overlay_body(frame, title).is_some_and(|body| body == old),
        "Disabled Open has no ownership/request/restore effect",
        Duration::from_secs(8),
    );
    assert_eq!(commands(fixture).len(), expected);
}
pub(super) fn inspect_restart(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    start: usize,
    mode: RecoveryV75,
) -> Session {
    let witness = protected(fixture);
    let db = rusqlite::Connection::open_with_flags(
        fixture.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let (state, receipt):(String,Option<String>) = db.query_row(
        "SELECT state,confirmed_json FROM pr_write_attempts WHERE command='file-comment-create-v75'",
        [],|row|Ok((row.get(0)?,row.get(1)?)),
    ).unwrap();
    let label = match state.as_str() {
        "confirmed" if mode == RecoveryV75::ConfirmedCleanupFailed => {
            let actual: Value = serde_json::from_str(receipt.as_ref().unwrap()).unwrap();
            assert_eq!(actual["resource_id"], "9181");
            assert_eq!(actual["node_id"], "NATIVE_FILE_COMMENT_9181");
            "Confirmed"
        }
        "pending" if mode.pending() => {
            assert!(receipt.is_none());
            "Pending"
        }
        "uncertain" if mode.pending() => {
            assert!(receipt.is_none());
            "Uncertain"
        }
        _ => panic!("Unexpected actual protected Personal publication basis"),
    };
    let mut session = session.restart_with_same_cache(fixture, 120, 40);
    session.wait_for_screen("ghtui");
    let refresh_frame = session.completed_cursorless_end();
    refresh(&mut session, mouse);
    session.wait_for_command_count(fixture, start + 7);
    session.wait_for_new_cursorless_condition(
        refresh_frame,
        |frame| {
            crate::support::main_footer_visible(frame)
                && crate::support::overlay_body(frame, "Inbox • focused view").is_some_and(|body| {
                    body.contains("Fresh • fetched")
                        && body.contains("1 unique PRs across 3 queue matches")
                        && body.contains("Actual personal source")
                        && !body.contains(BODY)
                })
        },
        "NEW genuinely refreshed Personal Inbox after complete identity/search bracket, not cached rows",
        Duration::from_secs(8),
    );
    choose(&mut session, mouse, "Actual personal source", b"\r");
    let (_, opened) = complete_steps();
    session.wait_for_command_count(fixture, start + opened);
    session.wait_for_cursorless_condition(
        |frame| {
            let s = frame.contents();
            s.contains(BODY)
                && crate::support::controls::control_painted(frame, "[J Thread history]")
                && !s.contains("Personal PR Files")
                && !s.contains(FILE_BODY)
        },
        "Genuine fresh Personal Summary permits protected inspection without Files/source mint",
        Duration::from_secs(8),
    );
    let base = start + opened;
    choose(&mut session, mouse, "[J Thread history]", b"J");
    let child = live_child(&mut session, fixture, base);
    ready(&mut session, ROOT_HISTORY, &["pending"]);
    disabled_open(&mut session, fixture, mouse, ROOT_HISTORY, base + 1);
    assert_eq!(protected(fixture), witness);
    assert!(process_identity(child.pid)
        .unwrap()
        .is_some_and(|now| now.birth == child.birth
            && now.uid == child.uid
            && now.session == child.session
            && !now.zombie));
    session.wait_for_command_count(fixture, base + 8);
    ready(
        &mut session,
        ROOT_HISTORY,
        &[
            "[F FILE history]",
            "Fresh current",
            "Read-only local history",
        ],
    );
    choose(&mut session, mouse, "[F FILE history]", b"F");
    session.wait_for_command_count(fixture, base + 16);
    ready(&mut session, FILE_HISTORY, &[label]);
    session.resize_owned_header(80, 24, FILE_HISTORY, &["[Enter Open]", "[Esc Back]"]);
    session.resize_owned_header(120, 40, FILE_HISTORY, &["[Enter Open]", "[Esc Back]"]);
    choose(&mut session, mouse, "[Enter Open]", b"\r");
    let end = base
        + if mode == RecoveryV75::AccountLoss {
            18
        } else {
            24
        };
    session.wait_for_command_count(fixture, end);
    if mode == RecoveryV75::AccountLoss {
        session.wait_for_cursorless_condition(|frame|{let s=frame.contents();s.contains("Original thread source unavailable")&&!s.contains(FILE_HISTORY)&&!s.contains(ROOT_HISTORY)&&!s.contains(BODY)&&!s.contains(FILE_BODY)&&!s.contains("FILE comment •")},"Fresh wrong numeric account hides all original Personal private body/history without replay",Duration::from_secs(8));
    } else {
        ready(&mut session, FILE_HISTORY, &["INSPECT ONLY"]);
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let old = crate::support::overlay_body(
                session.completed_cursorless_screen().unwrap(),
                FILE_HISTORY,
            )
            .unwrap();
            if old.contains("PRIVATE_NATIVE_WHOLE_FILE_BODY") {
                break;
            }
            let at = session.completed_cursorless_end();
            choose(&mut session, mouse, "[PgDn Next]", b"\x1b[6~");
            session.wait_for_new_cursorless_condition(
                at,
                |frame| {
                    crate::support::overlay_body(frame, FILE_HISTORY)
                        .is_some_and(|body| body != old && body.contains("[Esc Back]"))
                },
                "One acknowledged actual Personal FILE detail scroll",
                deadline.saturating_duration_since(Instant::now()),
            );
            assert!(Instant::now() < deadline);
        }
        disabled_open(&mut session, fixture, mouse, FILE_HISTORY, end);
        assert_eq!(protected(fixture), witness);
        choose(&mut session, mouse, "[Esc Back]", b"\x1b");
        ready(&mut session, FILE_HISTORY, &[label]);
        choose(&mut session, mouse, "[Esc Back]", b"\x1b");
        ready(&mut session, ROOT_HISTORY, &["[F FILE history]"]);
        choose(&mut session, mouse, "[Esc Back]", b"\x1b");
        session.wait_for_cursorless_condition(
            |frame| {
                let s = frame.contents();
                s.contains(BODY)
                    && crate::support::controls::control_painted(frame, "[J Thread history]")
                    && !s.contains(FILE_HISTORY)
                    && !s.contains(FILE_BODY)
            },
            "Pure Back returns exact original Personal Summary with historical body erased",
            Duration::from_secs(8),
        );
    }
    assert_eq!(protected(fixture), witness);
    assert_eq!(commands(fixture).len(), end);
    assert!(!commands(fixture).iter().skip(start).any(|row| row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str() == Some("PATCH")
            || v.as_str() == Some("DELETE")
            || v.as_str() == Some("PUT"))));
    session
}
macro_rules! pair {
    ($key:ident,$mouse:ident,$mode:ident) => {
        #[test]
        fn $key() {
            journey_with_recovery(false, Some(RecoveryV75::$mode));
        }
        #[test]
        fn $mouse() {
            journey_with_recovery(true, Some(RecoveryV75::$mode));
        }
    };
}
pair!(
    file_comments_v75_keyboard_personal_pending_restart_exact_summary_inspection_no_files_or_replay,
    file_comments_v75_mouse_personal_pending_restart_exact_summary_inspection_no_files_or_replay,
    Pending
);
pair!(
    file_comments_v75_keyboard_personal_confirmed_cleanup_failure_restart_honest_basis_no_ack,
    file_comments_v75_mouse_personal_confirmed_cleanup_failure_restart_honest_basis_no_ack,
    ConfirmedCleanupFailed
);
pair!(
    file_comments_v75_keyboard_personal_protected_restart_current_uid_loss_hides_body_no_retry,
    file_comments_v75_mouse_personal_protected_restart_current_uid_loss_hides_body_no_retry,
    AccountLoss
);
