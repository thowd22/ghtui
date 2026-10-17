//! DECLARED / UNRUN. Existing closed reaction publisher on the actual created
//! FILE comment; own reaction ID is observed numerically, never by login.
use super::super::super::file_controls_v75 as file_flow;
use super::*;
fn journey(mouse: bool, operation: Op) {
    let mut extra = browse(Operation::Reply);
    extra.extend(observation(Operation::Reply));
    extra.extend(fresh());
    extra.extend(fresh());
    extra.extend(fresh());
    let mut write = confirm_reaction(operation, Finish::Happy);
    // This positive lifecycle does not need a held child; held pre/post
    // uncertainty remains the independent existing reaction-native matrix.
    for step in &mut write {
        step.delay_ms = 0;
    }
    extra.extend(write);
    let extra = file_flow::adapted(extra);
    assert_eq!(extra.len(), 122);
    file_flow::with_created_file(mouse, extra, |session, fixture, base| {
        choose(session, mouse, "[T Threads]", b"T");
        session.wait_for_command_count(fixture, base + 17);
        owned(session, &["FILE", "actual-file-author", "[c Comments]"]);
        control_v2(session, mouse, THREAD_TITLE, "[c Comments]", b"c");
        session.wait_for_command_count(fixture, base + 28);
        panel_ready_v2(session, &["Comment #9181", "actual-file-author"]);
        control_v2(session, mouse, PANEL, "Comment #9181", b"\x1b[B");
        panel_ready_v2(session, &["> Comment #9181"]);
        control_v2(session, mouse, PANEL, "[R Reactions]", b"R");
        session.wait_for_command_count(fixture, base + 49);
        reaction_ready(
            session,
            &["COMPLETE", "same-login-not-ownership", "reaction #7001"],
        );
        let texts = text_projection(fixture);
        for (cols, rows) in [(80, 24), (120, 40)] {
            session.resize_owned_header(
                cols,
                rows,
                REACTIONS,
                &[operation.control(), "[Esc Back]"],
            );
            reaction_ready(session, &["COMPLETE", "same-login-not-ownership"]);
        }
        control_v2(
            session,
            mouse,
            REACTIONS,
            operation.control(),
            operation.key(),
        );
        session.wait_for_command_count(fixture, base + 70);
        form_ready_reaction(session, operation, false);
        form_action(session, mouse, operation, "[Preview]");
        session.wait_for_command_count(fixture, base + 91);
        form_ready_reaction(session, operation, true);
        let at = session.completed_cursorless_end();
        form_action(session, mouse, operation, "[Confirm action]");
        session.wait_for_command_count(fixture, base + 122);
        session.wait_for_new_cursorless_condition(
            at,
            |frame| {
                let s = frame.contents();
                s.contains("refresh explicitly")
                    && !s.contains("PRIVATE_NATIVE_WHOLE_FILE_BODY")
                    && !s.contains("same-login-not-ownership")
                    && !s.contains(REACTIONS)
                    && !s.contains(PANEL)
                    && !s.contains(THREAD_TITLE)
                    && !s.contains(&operation.title())
            },
            "Actual created FILE reaction observed and original private model retired",
            Duration::from_secs(8),
        );
        let(state,effect,origin):(String,String,String)=raw(fixture).query_row("SELECT a.state,a.confirmed_json,o.origin_json FROM pr_write_attempts a JOIN pr_write_thread_reaction_origins_v3 o USING(attempt_id) WHERE a.command=?1",[operation.command()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
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
        let origin: Value = serde_json::from_str(&origin).unwrap();
        assert_eq!(origin["seed"]["comment_id"], 9181);
        assert_eq!(origin["seed"]["comment_node"], "NATIVE_FILE_COMMENT_9181");
        assert_eq!(origin["seed"]["author_account_id"], 42);
        assert_eq!(origin["seed"]["head"], "a".repeat(40));
        assert_eq!(origin["seed"]["base"], "b".repeat(40));
        assert_eq!(origin["seed"]["comment_commit"], "a".repeat(40));
        assert_eq!(origin["seed"]["original_commit"], "a".repeat(40));
        assert_eq!(origin["seed"]["observed_path"], "src/界.rs");
        if operation == Op::Remove {
            assert_eq!(origin["seed"]["selected"]["id"], 7001);
            assert_eq!(origin["seed"]["selected"]["account_id"], 42);
        } else {
            assert!(origin["seed"]["selected"].is_null());
        }
        assert_eq!(
            text_projection(fixture),
            texts,
            "Closed reaction has EMPTY discussion-text impact, not a fake File/Inline/Review fence"
        );
        assert_eq!(
            raw(fixture)
                .query_row(
                    "SELECT count(*) FROM drafts WHERE resource LIKE 'thread-reaction-draft-v3/%'",
                    [],
                    |row| row.get::<_, u64>(0)
                )
                .unwrap(),
            0
        );
    });
}
#[test]
fn file_comments_v75_keyboard_actual_created_file_comment_reaction_add_exact_actor_and_empty_text_impact(
) {
    journey(false, Op::Add)
}
#[test]
fn file_comments_v75_mouse_actual_created_file_comment_reaction_add_exact_actor_and_empty_text_impact(
) {
    journey(true, Op::Add)
}
#[test]
fn file_comments_v75_keyboard_actual_created_file_comment_reaction_remove_observed_own_id_not_login(
) {
    journey(false, Op::Remove)
}
#[test]
fn file_comments_v75_mouse_actual_created_file_comment_reaction_remove_observed_own_id_not_login() {
    journey(true, Op::Remove)
}
