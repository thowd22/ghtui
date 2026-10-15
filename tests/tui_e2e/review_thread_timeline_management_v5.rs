//! SOURCE ONLY / UNRUN. Real Timeline origin, never an event-to-thread guess.
//! Same submitted-comment factory, writer, SQL witness and exact transport.
use super::*;
use crate::support::owned_pty_processes_v1::ProcessIdentity;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FinishV5 {
    Confirm,
    PreCancel,
    PostCancel,
    AccountLoss,
    Back,
}

pub(super) fn timeline_ready(session: &mut Session) {
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            crate::support::main_footer_visible(frame)
                && text.contains(DETAIL_TITLE)
                && text.contains("[T All threads]")
                && text.contains("review_requested")
                && !text.contains(DETAIL_BODY)
                && crate::support::overlay_body(frame, THREAD_TITLE).is_none()
                && crate::support::overlay_body(frame, PANEL).is_none()
        },
        "Actual completed Timeline, explicit All threads (not event association)",
        Duration::from_secs(8),
    );
}
fn page_rows(session: &Session) -> Vec<String> {
    let frame = session.completed_cursorless_screen().unwrap();
    let (height, width) = frame.size();
    // Fixed visible cells, including blank padding, not VT row serialization.
    (0..height.saturating_sub(2))
        .map(|row| {
            (0..width)
                .filter_map(|column| {
                    let cell = frame.cell(row, column).unwrap();
                    if cell.is_wide_continuation() {
                        None
                    } else {
                        let text = cell.contents();
                        Some(if text.is_empty() { " ".into() } else { text })
                    }
                })
                .collect()
        })
        .collect()
}
pub(super) fn no_timeline_attempts(fixture: &Fixture, foreign_key: &str) {
    let db = raw(fixture);
    for table in ["pr_write_attempts", "actions_write_attempts"] {
        let count: u64 = db
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "Read/refusal cannot create a publication intent");
    }
    let foreign = db
        .prepare("SELECT resource,body FROM drafts WHERE login='user-id:43' ORDER BY resource")
        .unwrap()
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(foreign, vec![(foreign_key.into(), FOREIGN.into())]);
    let others: u64 = db.query_row("SELECT (SELECT count(*) FROM records WHERE login='user-id:43')+(SELECT count(*) FROM saved_views WHERE login='user-id:43')", [], |row| row.get(0)).unwrap();
    assert_eq!(others, 0);
}
pub(super) fn open_timeline_comments(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
) -> Vec<String> {
    selected(session, fixture, mouse);
    session.resize(120, 40);
    choose(session, mouse, "[t Timeline]", b"t");
    timeline_ready(session);
    assert_eq!(commands(session, fixture).len(), 31);
    let original = page_rows(session);
    choose(session, mouse, "[T All threads]", b"T");
    session.wait_for_command_count(fixture, 48);
    owned(
        session,
        &["ALL PR", "NATIVE_THREAD_9", "OUTDATED", "[c Comments]"],
    );
    for (cols, rows) in [(80, 24), (120, 40)] {
        session.resize_owned_header(cols, rows, THREAD_TITLE, &["[a All PR only]", "[Esc Back]"]);
        owned(session, &["ALL PR", "NATIVE_THREAD_9"]);
    }
    control_v2(session, mouse, THREAD_TITLE, "[c Comments]", b"c");
    session.wait_for_command_count(fixture, 59);
    panel_ready_v2(
        session,
        &["Comment #9008", "Comment #9009", "@native-thread-actor"],
    );
    control_v2(session, mouse, PANEL, "Comment #9009", b"\x1b[B");
    panel_ready_v2(session, &["> Comment #9009"]);
    for (cols, rows) in [(80, 24), (120, 40)] {
        session.resize_owned_header(cols, rows, PANEL, &["[e Edit]", "[Esc Back]"]);
        panel_ready_v2(session, &["> Comment #9009"]);
    }
    original
}
pub(super) fn pure_timeline_back(session: &mut Session, mouse: bool, original: &[String]) {
    control_v2(session, mouse, PANEL, "[Esc Back]", b"\x1b");
    owned(session, &["ALL PR", "NATIVE_THREAD_9"]);
    let endpoint = session.completed_cursorless_end();
    control_v2(session, mouse, THREAD_TITLE, "[Esc Back]", b"\x1b");
    session.wait_for_new_cursorless_condition(
        endpoint,
        |frame| {
            crate::support::main_footer_visible(frame)
                && frame.contents().contains("[T All threads]")
                && crate::support::overlay_body(frame, THREAD_TITLE).is_none()
                && crate::support::overlay_body(frame, PANEL).is_none()
        },
        "Pure Back restores the original Timeline without reload or guessed event",
        Duration::from_secs(8),
    );
    timeline_ready(session);
    assert_eq!(page_rows(session), original);
}
pub(super) fn assert_live_original(child: &ProcessIdentity) {
    let current = process_identity(child.pid)
        .unwrap()
        .expect("Actual original held child remains live during SQL proof");
    assert_eq!(
        (current.uid, current.birth, current.session),
        (child.uid, child.birth, child.session)
    );
    assert!(!current.zombie);
}
pub(super) fn wait_state(session: &mut Session, fixture: &Fixture, wanted: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let actual: String = raw(fixture)
            .query_row("SELECT state FROM pr_write_attempts", [], |row| row.get(0))
            .unwrap();
        if actual == wanted {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Original worker state {actual}, expected {wanted}"
        );
        session.poll_output(Duration::from_millis(1));
    }
}
fn journey(mouse: bool, operation: CommentOpV2, finish: FinishV5) {
    let mut steps = initial();
    steps.extend(browse(Operation::Reply));
    steps.extend(observation(Operation::Reply));
    if finish != FinishV5::Back {
        steps.extend(fresh_v2());
        steps.extend(fresh_v2());
        if finish == FinishV5::AccountLoss {
            steps.extend(principal(43));
        } else {
            let mut write = confirm_v2(operation);
            let held = if finish == FinishV5::PostCancel {
                21
            } else {
                12
            };
            write[held].delay_ms = 6000;
            if matches!(finish, FinishV5::PreCancel | FinishV5::PostCancel) {
                write.truncate(held + 1);
            }
            steps.extend(write);
        }
    }
    let fixture = resource_configured(&steps);
    Cache::open(&database(&fixture))
        .unwrap()
        .put_draft(
            &Scope::github(HOST, Some(43)).unwrap(),
            "foreign-timeline-comment",
            FOREIGN,
        )
        .unwrap();
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let original = open_timeline_comments(&mut session, &fixture, mouse);
    let unrelated = untouched_v2(&fixture);
    let trace = git_commands(&fixture);
    if finish == FinishV5::Back {
        let sql = sql_state(&fixture);
        pure_timeline_back(&mut session, mouse, &original);
        assert_eq!(sql_state(&fixture), sql);
        no_timeline_attempts(&fixture, "foreign-timeline-comment");
    } else {
        control_v2(
            &mut session,
            mouse,
            PANEL,
            operation.control(),
            operation.key(),
        );
        session.wait_for_command_count(&fixture, 75);
        form_ready_v2(&mut session, operation, false);
        if operation == CommentOpV2::Edit {
            if mouse {
                session.click_owned_form_field("PR INTENT • Preview first", "Body");
            }
            session.send_key(b"\x01");
            session.paste(EDITED);
            session
                .try_wait_for_condition(
                    |screen| {
                        screen.contains("Exact edited review comment 界")
                            && screen.contains("second line")
                    },
                    "Real Timeline edit replacement body",
                    Duration::from_secs(8),
                )
                .unwrap();
        }
        form_control_v2(&mut session, mouse, operation, "[Preview]");
        session.wait_for_command_count(&fixture, 91);
        form_ready_v2(&mut session, operation, true);
        let endpoint = session.completed_cursorless_end();
        form_control_v2(&mut session, mouse, operation, "[Confirm action]");
        if finish == FinishV5::AccountLoss {
            session.wait_for_command_count(&fixture, 93);
            session.wait_for_new_cursorless_condition(endpoint, |frame| {
                let text = frame.contents();
                text.contains("refresh explicitly") && !text.contains(THREAD_BODY) && !text.contains(EDITED) && !text.contains("native-thread-actor") && !text.contains(&operation.title()) && !text.contains("review_requested")
            }, "Changed actual numeric account suppresses original Timeline/Form before journal or POST", Duration::from_secs(8));
            no_timeline_attempts(&fixture, "foreign-timeline-comment");
        } else {
            let held = if finish == FinishV5::PostCancel {
                21
            } else {
                12
            };
            let child = held_child(&mut session, &fixture, 91 + held);
            let encoded: String = raw(&fixture)
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'thread-comment-draft-v2/%'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let value: Value = serde_json::from_str(&encoded).unwrap();
            assert_eq!(value["values"]["body"], operation.body());
            assert_eq!(value["version"], 2);
            assert!(value.get("approved").is_none());
            let (attempt, command, state, resource, payload): (i64,String,String,String,String) = raw(&fixture).query_row("SELECT attempt_id,command,state,selected_resource,payload_sha256 FROM pr_write_attempts", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).unwrap();
            assert_eq!(
                attempt,
                value["publication"]["attempt_id"].as_i64().unwrap()
            );
            assert_eq!(command, operation.command());
            assert_eq!(state, "pending");
            assert!(resource.starts_with("thread-comment-v2/"));
            assert_eq!(payload, value["publication"]["payload_sha256"]);
            let origin: String = raw(&fixture)
                .query_row(
                    "SELECT origin_json FROM pr_write_thread_comment_origins_v2",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let origin: Value = serde_json::from_str(&origin).unwrap();
            assert_eq!(origin["reference"], value["publication"]);
            assert_eq!(origin["seed"]["comment_id"], 9009);
            assert_eq!(origin["seed"]["author_account_id"], 77);
            assert_eq!(origin["seed"]["scope"]["login"], "user-id:42");
            assert_eq!(origin["seed"]["comment_commit"], "c".repeat(40));
            assert_eq!(origin["seed"]["head"], "a".repeat(40));
            assert_live_original(&child);
            if matches!(finish, FinishV5::PreCancel | FinishV5::PostCancel) {
                control_v2(
                    &mut session,
                    mouse,
                    &operation.title(),
                    "[Esc Cancel]",
                    b"\x1b",
                );
            } else {
                session.wait_for_command_count(&fixture, steps.len());
            }
            session.wait_for_new_cursorless_condition(
                endpoint,
                |frame| {
                    let text = frame.contents();
                    text.contains(if finish == FinishV5::Confirm {
                        "refresh explicitly"
                    } else {
                        "publication canceled"
                    }) && !text.contains(THREAD_BODY)
                        && !text.contains(EDITED)
                        && !text.contains("native-thread-actor")
                        && !text.contains("review_requested")
                        && !text.contains(&operation.title())
                },
                "One Timeline comment invocation/cancel retires only original private context",
                Duration::from_secs(8),
            );
            wait_original_child_gone(&mut session, &child);
            if finish == FinishV5::Confirm {
                let (state, effect): (String, String) = raw(&fixture)
                    .query_row(
                        "SELECT state,confirmed_json FROM pr_write_attempts",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .unwrap();
                assert_eq!(state, "confirmed");
                let effect: Value = serde_json::from_str(&effect).unwrap();
                assert_eq!(effect["resource_id"], "9009");
                assert_eq!(effect["node_id"], "NATIVE_COMMENT_9");
                assert_eq!(
                    effect["kind"],
                    if operation == CommentOpV2::Edit {
                        "comment"
                    } else {
                        "comment_deleted"
                    }
                );
                assert_eq!(raw(&fixture).query_row("SELECT count(*) FROM drafts WHERE resource LIKE 'thread-comment-draft-v2/%'", [], |row| row.get::<_,u64>(0)).unwrap(), 0);
            } else {
                wait_state(
                    &mut session,
                    &fixture,
                    if finish == FinishV5::PreCancel {
                        "pending"
                    } else {
                        "uncertain"
                    },
                );
                assert_eq!(raw(&fixture).query_row("SELECT body FROM drafts WHERE resource LIKE 'thread-comment-draft-v2/%'", [], |row| row.get::<_,String>(0)).unwrap(), encoded);
            }
        }
    }
    let writes = usize::from(matches!(finish, FinishV5::Confirm | FinishV5::PostCancel));
    exact_v2(&session, &fixture, &steps, writes);
    assert_eq!(untouched_v2(&fixture), unrelated);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_read_only_git(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn review_threads_v5_timeline_keyboard_actual_edit_typed_receipt() {
    journey(false, CommentOpV2::Edit, FinishV5::Confirm);
}
#[test]
fn review_threads_v5_timeline_mouse_actual_edit_typed_receipt() {
    journey(true, CommentOpV2::Edit, FinishV5::Confirm);
}
#[test]
fn review_threads_v5_timeline_keyboard_actual_delete_204_and_node_absence() {
    journey(false, CommentOpV2::Delete, FinishV5::Confirm);
}
#[test]
fn review_threads_v5_timeline_mouse_actual_delete_204_and_node_absence() {
    journey(true, CommentOpV2::Delete, FinishV5::Confirm);
}
#[test]
fn review_threads_v5_timeline_keyboard_edit_prepost_cancel_retains_pending() {
    journey(false, CommentOpV2::Edit, FinishV5::PreCancel);
}
#[test]
fn review_threads_v5_timeline_mouse_edit_prepost_cancel_retains_pending() {
    journey(true, CommentOpV2::Edit, FinishV5::PreCancel);
}
#[test]
fn review_threads_v5_timeline_keyboard_delete_postinvoke_cancel_retains_uncertainty() {
    journey(false, CommentOpV2::Delete, FinishV5::PostCancel);
}
#[test]
fn review_threads_v5_timeline_mouse_delete_postinvoke_cancel_retains_uncertainty() {
    journey(true, CommentOpV2::Delete, FinishV5::PostCancel);
}
#[test]
fn review_threads_v5_timeline_keyboard_edit_changed_numeric_account_refuses_private_form() {
    journey(false, CommentOpV2::Edit, FinishV5::AccountLoss);
}
#[test]
fn review_threads_v5_timeline_mouse_edit_changed_numeric_account_refuses_private_form() {
    journey(true, CommentOpV2::Edit, FinishV5::AccountLoss);
}
#[test]
fn review_threads_v5_timeline_keyboard_selected_comment_pure_back_preserves_original_page() {
    journey(false, CommentOpV2::Edit, FinishV5::Back);
}
#[test]
fn review_threads_v5_timeline_mouse_selected_comment_pure_back_preserves_original_page() {
    journey(true, CommentOpV2::Edit, FinishV5::Back);
}
