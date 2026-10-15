//! SOURCE ONLY / UNRUN. Actual Timeline -> All threads -> selected reaction,
//! same fresh numeric-UID factory/one writer; never a timeline-event guess.
use super::super::timeline_management_v5 as timeline;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FinishV5 {
    Confirm,
    PreCancel,
    PostCancel,
    AccountLoss,
    Back,
}

fn panel(session: &mut Session, fixture: &Fixture, mouse: bool) -> Vec<String> {
    let original = timeline::open_timeline_comments(session, fixture, mouse);
    control_v2(session, mouse, PANEL, "[R Reactions]", b"R");
    session.wait_for_command_count(fixture, 80);
    reaction_ready(
        session,
        &["COMPLETE", "same-login-not-ownership", "reaction #7001"],
    );
    for (cols, rows) in [(80, 24), (120, 40)] {
        session.resize_owned_header(cols, rows, REACTIONS, &["[5 heart]", "[Esc Back]"]);
        reaction_ready(session, &["COMPLETE", "same-login-not-ownership"]);
    }
    original
}
fn journey(mouse: bool, operation: Op, finish: FinishV5) {
    let mut steps = initial();
    steps.extend(browse(Operation::Reply));
    steps.extend(observation(Operation::Reply));
    steps.extend(fresh());
    if finish != FinishV5::Back {
        steps.extend(fresh());
        steps.extend(fresh());
        if finish == FinishV5::AccountLoss {
            steps.extend(principal(43));
        } else {
            steps.extend(confirm_reaction(
                operation,
                match finish {
                    FinishV5::PreCancel => Finish::PreCancel,
                    FinishV5::PostCancel => Finish::PostCancel,
                    _ => Finish::Happy,
                },
            ));
        }
    }
    let fixture = resource_configured(&steps);
    Cache::open(&database(&fixture))
        .unwrap()
        .put_draft(
            &Scope::github(HOST, Some(43)).unwrap(),
            "foreign-timeline-reaction",
            FOREIGN,
        )
        .unwrap();
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let original = panel(&mut session, &fixture, mouse);
    let other = unrelated(&fixture);
    let texts = text_projection(&fixture);
    let trace = git_commands(&fixture);
    if finish == FinishV5::Back {
        let sql = sql_state(&fixture);
        control_v2(&mut session, mouse, REACTIONS, "[Esc Back]", b"\x1b");
        panel_ready_v2(&mut session, &["> Comment #9009"]);
        timeline::pure_timeline_back(&mut session, mouse, &original);
        assert_eq!(sql_state(&fixture), sql);
        timeline::no_timeline_attempts(&fixture, "foreign-timeline-reaction");
    } else {
        control_v2(
            &mut session,
            mouse,
            REACTIONS,
            operation.control(),
            operation.key(),
        );
        session.wait_for_command_count(&fixture, 101);
        form_ready_reaction(&mut session, operation, false);
        form_action(&mut session, mouse, operation, "[Preview]");
        session.wait_for_command_count(&fixture, 122);
        form_ready_reaction(&mut session, operation, true);
        let endpoint = session.completed_cursorless_end();
        form_action(&mut session, mouse, operation, "[Confirm action]");
        if finish == FinishV5::AccountLoss {
            session.wait_for_command_count(&fixture, 124);
            session.wait_for_new_cursorless_condition(endpoint, |frame| {
                let text = frame.contents();
                text.contains("refresh explicitly") && !text.contains(THREAD_BODY) && !text.contains("same-login-not-ownership") && !text.contains("review_requested") && !text.contains(&operation.title())
            }, "Changed numeric action account hides original Timeline and private reaction actors before journal/POST", Duration::from_secs(8));
            timeline::no_timeline_attempts(&fixture, "foreign-timeline-reaction");
        } else {
            let held = if finish == FinishV5::PostCancel {
                26
            } else {
                18
            };
            let child = held_child(&mut session, &fixture, 122 + held);
            let encoded: String = raw(&fixture)
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'thread-reaction-draft-v3/%'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let value: Value = serde_json::from_str(&encoded).unwrap();
            assert_eq!(value["version"], 3);
            assert_eq!(value["values"], json!({"body":""}));
            assert!(value.get("approved").is_none());
            let (attempt,command,state,resource,payload): (i64,String,String,String,String) = raw(&fixture).query_row("SELECT attempt_id,command,state,selected_resource,payload_sha256 FROM pr_write_attempts", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).unwrap();
            assert_eq!(
                attempt,
                value["publication"]["attempt_id"].as_i64().unwrap()
            );
            assert_eq!(command, operation.command());
            assert_eq!(state, "pending");
            assert!(resource.starts_with("thread-reaction-v3/"));
            assert_eq!(payload, value["publication"]["payload_sha256"]);
            let origin: String = raw(&fixture)
                .query_row(
                    "SELECT origin_json FROM pr_write_thread_reaction_origins_v3",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let origin: Value = serde_json::from_str(&origin).unwrap();
            assert_eq!(origin["reference"], value["publication"]);
            assert_eq!(origin["seed"]["scope"]["login"], "user-id:42");
            assert_eq!(origin["seed"]["comment_id"], 9009);
            assert_eq!(origin["seed"]["comment_node"], "NATIVE_COMMENT_9");
            assert_eq!(origin["seed"]["author_account_id"], 77);
            assert_eq!(origin["seed"]["comment_commit"], "c".repeat(40));
            assert_eq!(origin["seed"]["original_commit"], "d".repeat(40));
            assert_eq!(origin["seed"]["head"], "a".repeat(40));
            assert_eq!(origin["seed"]["base"], "b".repeat(40));
            assert_eq!(origin["seed"]["observed_path"], "src/old.rs");
            if operation == Op::Remove {
                assert_eq!(origin["seed"]["selected"]["id"], 7001);
                assert_eq!(origin["seed"]["selected"]["account_id"], 42);
                assert_eq!(origin["seed"]["selected"]["node"], "NATIVE_REACTION_7001");
            } else {
                assert!(origin["seed"]["selected"].is_null());
            }
            timeline::assert_live_original(&child);
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
            session.wait_for_new_cursorless_condition(endpoint, |frame| {
                let text = frame.contents();
                text.contains(if finish == FinishV5::Confirm { "refresh explicitly" } else { "publication canceled" }) && !text.contains(THREAD_BODY) && !text.contains("same-login-not-ownership") && !text.contains("review_requested") && !text.contains(&operation.title())
            }, "Single observed/unknown Timeline reaction retires original private presentation, not a body-text mutation", Duration::from_secs(8));
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
                assert_eq!(
                    effect["kind"],
                    if operation == Op::Add {
                        "reaction"
                    } else {
                        "reaction_removed"
                    }
                );
                assert_eq!(
                    effect["resource_id"],
                    if operation == Op::Add { "7004" } else { "7001" }
                );
                assert_eq!(
                    effect["node_id"],
                    if operation == Op::Add {
                        "NATIVE_REACTION_7004"
                    } else {
                        "NATIVE_REACTION_7001"
                    }
                );
                assert_eq!(raw(&fixture).query_row("SELECT count(*) FROM drafts WHERE resource LIKE 'thread-reaction-draft-v3/%'", [], |row| row.get::<_,u64>(0)).unwrap(), 0);
            } else {
                timeline::wait_state(
                    &mut session,
                    &fixture,
                    if finish == FinishV5::PreCancel {
                        "pending"
                    } else {
                        "uncertain"
                    },
                );
                assert_eq!(raw(&fixture).query_row("SELECT body FROM drafts WHERE resource LIKE 'thread-reaction-draft-v3/%'", [], |row| row.get::<_,String>(0)).unwrap(), encoded);
            }
        }
    }
    let writes = usize::from(matches!(finish, FinishV5::Confirm | FinishV5::PostCancel));
    exact_reactions(&session, &fixture, &steps, writes);
    assert_eq!(
        text_projection(&fixture),
        texts,
        "Closed reaction effect has EMPTY discussion-text impact, not fake Inline/Review fences"
    );
    assert_eq!(unrelated(&fixture), other);
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
fn review_threads_v5_timeline_keyboard_actual_add_reaction_201_actor_receipt_empty_text_impact() {
    journey(false, Op::Add, FinishV5::Confirm);
}
#[test]
fn review_threads_v5_timeline_mouse_actual_add_reaction_201_actor_receipt_empty_text_impact() {
    journey(true, Op::Add, FinishV5::Confirm);
}
#[test]
fn review_threads_v5_timeline_keyboard_actual_remove_owned_reaction_204_complete_absence() {
    journey(false, Op::Remove, FinishV5::Confirm);
}
#[test]
fn review_threads_v5_timeline_mouse_actual_remove_owned_reaction_204_complete_absence() {
    journey(true, Op::Remove, FinishV5::Confirm);
}
#[test]
fn review_threads_v5_timeline_keyboard_reaction_prepost_cancel_protected_pending() {
    journey(false, Op::Add, FinishV5::PreCancel);
}
#[test]
fn review_threads_v5_timeline_mouse_reaction_prepost_cancel_protected_pending() {
    journey(true, Op::Add, FinishV5::PreCancel);
}
#[test]
fn review_threads_v5_timeline_keyboard_reaction_postinvoke_cancel_protected_uncertainty() {
    journey(false, Op::Remove, FinishV5::PostCancel);
}
#[test]
fn review_threads_v5_timeline_mouse_reaction_postinvoke_cancel_protected_uncertainty() {
    journey(true, Op::Remove, FinishV5::PostCancel);
}
#[test]
fn review_threads_v5_timeline_keyboard_reaction_changed_numeric_account_no_private_return() {
    journey(false, Op::Add, FinishV5::AccountLoss);
}
#[test]
fn review_threads_v5_timeline_mouse_reaction_changed_numeric_account_no_private_return() {
    journey(true, Op::Add, FinishV5::AccountLoss);
}
#[test]
fn review_threads_v5_timeline_keyboard_reaction_pure_nested_back_exact_original_page() {
    journey(false, Op::Add, FinishV5::Back);
}
#[test]
fn review_threads_v5_timeline_mouse_reaction_pure_nested_back_exact_original_page() {
    journey(true, Op::Add, FinishV5::Back);
}
