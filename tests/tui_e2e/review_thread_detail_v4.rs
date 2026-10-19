//! UNRUN genuine scoped Summary/Timeline browser and SAME durable fresh thread
//! writer. No fake file association, login/legacy write fallback or new queue.
use super::*;
fn detail_ready(session: &mut Session, timeline: bool) {
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            crate::support::main_footer_visible(frame)
                && crate::support::controls::control_painted(frame, "[T All threads]")
                && text.contains(DETAIL_TITLE)
                && if timeline {
                    text.contains("review_requested") && !text.contains(DETAIL_BODY)
                } else {
                    text.contains(DETAIL_BODY)
                }
        },
        "actual completed original Summary/Timeline section and All threads control",
        Duration::from_secs(8),
    );
}
fn page_rows(session: &Session) -> Vec<String> {
    let frame = session
        .completed_cursorless_screen()
        .expect("Actual completed page frame");
    let (height, width) = frame.size();
    // Empty VT cells and explicitly painted spaces have identical visible
    // content. Compare every displayed cell at fixed geometry, not vt100's
    // variable trailing-padding serialization after a resize/redraw.
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
fn result_ready(session: &mut Session) {
    owned(
        session,
        &[
            "ALL PR",
            "NATIVE_THREAD_9",
            "native-thread-actor",
            "[a All PR only]",
        ],
    );
}
fn derivative_rows(fixture: &Fixture) -> Vec<String> {
    let db = raw(fixture);
    ["SELECT json_array('records',host,login,kind,id,repository,payload,fetched_at,source_updated_at,complete) FROM records ORDER BY host,login,kind,id",
     "SELECT json_array('checkpoints',host,login,source,generation,fetched_at,complete) FROM checkpoints ORDER BY host,login,source",
     "SELECT json_array('members',host,login,source,kind,id) FROM checkpoint_records ORDER BY host,login,source,kind,id"]
        .into_iter().flat_map(|query|db.prepare(query).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn prepared_detail(session: &mut Session, fixture: &Fixture, mouse: bool, operation: Operation) {
    choose(session, mouse, "[T All threads]", b"T");
    session.wait_for_command_count(fixture, 48);
    result_ready(session);
    for (width, height) in [(80, 24), (120, 40)] {
        session.resize_owned_header(
            width,
            height,
            THREAD_TITLE,
            &["[a All PR only]", "[Esc Back]"],
        );
        result_ready(session);
    }
    choose(session, mouse, operation.control(), operation.key());
    session.wait_for_command_count(fixture, 59);
    form_ready(session, operation, false);
    if operation == Operation::Reply {
        if mouse {
            session.click_owned_form_field("PR INTENT • Preview first", "Body");
        }
        session.paste(REPLY);
        let title = format!("{} • {FIRST}#7", operation.label());
        session.focus_completed_owned_action_v1(
            &title,
            "[Preview]",
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            &[
                "[Preview]",
                "[External editor]",
                "[Confirm action]",
                "[Keep draft / Close]",
            ],
            &[],
        );
        session.wait_for_cursorless_condition(
            |frame| {
                let text = frame.contents();
                text.contains("Original native reply 界")
                    && text.contains("second line")
                    && text.contains("[Keep draft / Close]")
            },
            "actual entered Timeline thread reply body before Preview",
            Duration::from_secs(8),
        );
    }
    form_control(session, mouse, operation, "[Preview]");
    session.wait_for_command_count(fixture, 70);
    form_ready(session, operation, true);
}
fn journey(mouse: bool, mode: u8) {
    let timeline = mode != 0;
    let operation = if mode == 3 {
        Operation::Resolve
    } else if mode == 4 {
        Operation::Reopen
    } else {
        Operation::Reply
    };
    let mut steps = initial();
    let mut read = if mode == 5 {
        principal(43)
    } else {
        browse(operation)
    };
    if mode == 6 {
        read[9].delay_ms = 6000;
        read.truncate(10);
    }
    steps.extend(read);
    if (2..=4).contains(&mode) {
        steps.extend(observation(operation));
        steps.extend(observation(operation));
        let mut write = confirm(operation);
        write[7].delay_ms = 6000;
        steps.extend(write);
    }
    let fixture = resource_configured(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    selected(&mut session, &fixture, mouse);
    session.resize(120, 40);
    if timeline {
        choose(&mut session, mouse, "[t Timeline]", b"t");
    }
    detail_ready(&mut session, timeline);
    let before = sql_state(&fixture);
    let unaffected = untouched_sql(&fixture);
    let derivatives = derivative_rows(&fixture);
    let trace = git_commands(&fixture);
    let original = page_rows(&session);
    if (2..=4).contains(&mode) {
        prepared_detail(&mut session, &fixture, mouse, operation);
        let endpoint = session.completed_cursorless_end();
        form_control(&mut session, mouse, operation, "[Confirm action]");
        let child = held_child(&mut session, &fixture, 77);
        let pending = draft_and_pending(&fixture, operation);
        assert_eq!(pending["values"]["body"], operation.body());
        let live=process_identity(child.pid).unwrap().expect("Exact prewrite live child while original DB ACK/body/ref/journal is independently observed");
        assert_eq!(
            (live.uid, live.birth, live.session),
            (child.uid, child.birth, child.session)
        );
        assert!(!live.zombie);
        session.wait_for_command_count(&fixture, 90);
        session.wait_for_new_cursorless_condition(endpoint,|frame|{let text=frame.contents();text.contains("original snapshot retired")&&text.contains("refresh explicitly")&&!text.contains(THREAD_BODY)&&!text.contains(DETAIL_BODY)&&!text.contains(REPLY)&&!text.contains("native-thread-actor")},"typed actual thread receipt retires only original Timeline private source/presentation",Duration::from_secs(8));
        wait_original_child_gone(&mut session, &child);
        let db = raw(&fixture);
        let (command, state, selected, effect): (String, String, String, String) = db
            .query_row(
                "SELECT command,state,selected_resource,confirmed_json FROM pr_write_attempts",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(command, operation.command());
        assert_eq!(state, "confirmed");
        assert!(selected.starts_with(if operation == Operation::Reply {
            "thread-reply/"
        } else {
            "thread-state/"
        }));
        let effect: Value = serde_json::from_str(&effect).unwrap();
        assert_eq!(
            effect["kind"],
            if operation == Operation::Reply {
                "comment"
            } else {
                "thread"
            }
        );
        assert_eq!(
            effect["resource_id"],
            if operation == Operation::Reply {
                "901"
            } else if operation == Operation::Resolve {
                "resolved"
            } else {
                "unresolved"
            }
        );
        assert_eq!(
            effect["node_id"],
            if operation == Operation::Reply {
                "NATIVE_REPLY_901"
            } else {
                "NATIVE_THREAD_9"
            }
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM drafts WHERE resource LIKE 'thread-draft/%'",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM pr_write_thread_origins_v1",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(db.query_row("SELECT generation FROM pr_write_scopes WHERE host=?1 AND principal='user-id:42' AND pr_node='GENUINE_CANONICAL_PR_NODE_1007'",[HOST],|row|row.get::<_,u64>(0)).unwrap(),3);
        assert_eq!(untouched_sql(&fixture), unaffected);
        exact(&session, &fixture, &steps, 1);
    } else {
        choose(&mut session, mouse, "[T All threads]", b"T");
        if mode == 5 {
            session.wait_for_command_count(&fixture, 33);
            session.wait_for_cursorless_condition(|frame|{let text=frame.contents();text.contains("private views hidden")&&!text.contains(DETAIL_BODY)&&!text.contains(THREAD_BODY)&&!text.contains("native-thread-actor")},"fresh numeric-account failure suppresses old Summary/Timeline private body before return",Duration::from_secs(8));
        } else if mode == 6 {
            let child = held_child(&mut session, &fixture, 40);
            owned(&mut session, &["Loading original threads"]);
            choose(&mut session, mouse, "[Esc Back]", b"\x1b");
            wait_original_child_gone(&mut session, &child);
            detail_ready(&mut session, timeline);
            assert_eq!(page_rows(&session), original);
            assert_eq!(commands(&session, &fixture).len(), 41);
        } else {
            session.wait_for_command_count(&fixture, 48);
            result_ready(&mut session);
            for (width, height) in [(80, 24), (120, 40)] {
                session.resize_owned_header(
                    width,
                    height,
                    THREAD_TITLE,
                    &["[a All PR only]", "[Esc Back]"],
                );
                result_ready(&mut session);
            }
            choose(&mut session, mouse, "[a All PR only]", b"a");
            result_ready(&mut session);
            assert_eq!(commands(&session, &fixture).len(), 48);
            choose(&mut session, mouse, "[Esc Back]", b"\x1b");
            detail_ready(&mut session, timeline);
            assert_eq!(page_rows(&session), original);
            assert_eq!(commands(&session, &fixture).len(), 48);
        }
        assert_eq!(sql_state(&fixture), before);
        no_attempts(&fixture);
        exact(&session, &fixture, &steps, 0);
    }
    assert_eq!(derivative_rows(&fixture), derivatives);
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
fn review_threads_v4_summary_keyboard_all_threads_original_pure_back() {
    journey(false, 0);
}
#[test]
fn review_threads_v4_summary_mouse_all_threads_original_pure_back() {
    journey(true, 0);
}
#[test]
fn review_threads_v4_timeline_keyboard_all_threads_original_pure_back() {
    journey(false, 1);
}
#[test]
fn review_threads_v4_timeline_mouse_all_threads_original_pure_back() {
    journey(true, 1);
}
#[test]
fn review_threads_v4_timeline_keyboard_actual_reply_durable_single_effect() {
    journey(false, 2);
}
#[test]
fn review_threads_v4_timeline_mouse_actual_reply_durable_single_effect() {
    journey(true, 2);
}
#[test]
fn review_threads_v4_timeline_keyboard_actual_resolve_durable_single_effect() {
    journey(false, 3);
}
#[test]
fn review_threads_v4_timeline_mouse_actual_resolve_durable_single_effect() {
    journey(true, 3);
}
#[test]
fn review_threads_v4_timeline_keyboard_actual_reopen_durable_single_effect() {
    journey(false, 4);
}
#[test]
fn review_threads_v4_timeline_mouse_actual_reopen_durable_single_effect() {
    journey(true, 4);
}
#[test]
fn review_threads_v4_timeline_keyboard_account_loss_no_private_return() {
    journey(false, 5);
}
#[test]
fn review_threads_v4_timeline_mouse_account_loss_no_private_return() {
    journey(true, 5);
}
#[test]
fn review_threads_v4_timeline_keyboard_actual_held_read_cancel_pure_back() {
    journey(false, 6);
}
#[test]
fn review_threads_v4_timeline_mouse_actual_held_read_cancel_pure_back() {
    journey(true, 6);
}
