//! DECLARED / UNRUN. Original created FILE comment, not a line-comment DTO.
use super::super::file_controls_v75 as file_flow;
use super::*;
fn journey(mouse: bool, operation: CommentOpV2) {
    let mut extra = browse(Operation::Reply);
    extra.extend(observation(Operation::Reply));
    extra.extend(fresh_v2());
    extra.extend(fresh_v2());
    extra.extend(confirm_v2(operation));
    let extra = file_flow::adapted(extra);
    assert_eq!(extra.len(), 86);
    file_flow::with_created_file(mouse, extra, |session, fixture, base| {
        choose(session, mouse, "[T Threads]", b"T");
        session.wait_for_command_count(fixture, base + 17);
        owned(session, &["FILE", "actual-file-author", "[c Comments]"]);
        control_v2(session, mouse, THREAD_TITLE, "[c Comments]", b"c");
        session.wait_for_command_count(fixture, base + 28);
        panel_ready_v2(session, &["Comment #9181", "actual-file-author"]);
        control_v2(session, mouse, PANEL, "Comment #9181", b"\x1b[B");
        panel_ready_v2(session, &["> Comment #9181"]);
        for (cols, rows) in [(80, 24), (120, 40)] {
            session.resize_owned_header(cols, rows, PANEL, &[operation.control(), "[Esc Back]"]);
            panel_ready_v2(session, &["> Comment #9181"]);
        }
        control_v2(session, mouse, PANEL, operation.control(), operation.key());
        session.wait_for_command_count(fixture, base + 44);
        form_ready_v2(session, operation, false);
        if operation == CommentOpV2::Edit {
            if mouse {
                session.click_owned_form_field("PR INTENT • Preview first", "Body");
            }
            session.send_key(b"\x01");
            session.paste(EDITED);
        }
        form_control_v2(session, mouse, operation, "[Preview]");
        session.wait_for_command_count(fixture, base + 60);
        form_ready_v2(session, operation, true);
        let at = session.completed_cursorless_end();
        form_control_v2(session, mouse, operation, "[Confirm action]");
        session.wait_for_command_count(fixture, base + 86);
        session.wait_for_new_cursorless_condition(at,|frame|{let s=frame.contents();s.contains("refresh explicitly")&&!s.contains("PRIVATE_NATIVE_WHOLE_FILE_BODY")&&!s.contains(EDITED)&&!s.contains(THREAD_TITLE)&&!s.contains(PANEL)&&!s.contains(&operation.title())},"Actual created FILE comment edit/delete receipt retires exact original private presentation",Duration::from_secs(8));
        let(state,effect,origin):(String,String,String)=raw(fixture).query_row("SELECT a.state,a.confirmed_json,o.origin_json FROM pr_write_attempts a JOIN pr_write_thread_comment_origins_v2 o USING(attempt_id) WHERE a.command=?1",[operation.command()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!(state, "confirmed");
        let effect: Value = serde_json::from_str(&effect).unwrap();
        assert_eq!(effect["resource_id"], "9181");
        assert_eq!(effect["node_id"], "NATIVE_FILE_COMMENT_9181");
        assert_eq!(
            effect["kind"],
            if operation == CommentOpV2::Edit {
                "comment"
            } else {
                "comment_deleted"
            }
        );
        let origin: Value = serde_json::from_str(&origin).unwrap();
        assert_eq!(origin["seed"]["comment_id"], 9181);
        assert_eq!(origin["seed"]["comment_node"], "NATIVE_FILE_COMMENT_9181");
        assert_eq!(origin["seed"]["author_account_id"], 42);
        assert_eq!(origin["seed"]["scope"]["login"], "user-id:42");
        assert_eq!(origin["seed"]["observed_path"], "src/界.rs");
        assert_eq!(origin["seed"]["comment_commit"], "a".repeat(40));
        assert_eq!(origin["seed"]["original_commit"], "a".repeat(40));
        assert_eq!(origin["seed"]["head"], "a".repeat(40));
        assert_eq!(origin["seed"]["base"], "b".repeat(40));
        assert_eq!(
            raw(fixture)
                .query_row(
                    "SELECT count(*) FROM drafts WHERE resource LIKE 'thread-comment-draft-v2/%'",
                    [],
                    |row| row.get::<_, u64>(0)
                )
                .unwrap(),
            0
        );
    });
}
#[test]
fn file_comments_v75_keyboard_actual_created_file_comment_edit_same_numeric_id_no_line_anchor() {
    journey(false, CommentOpV2::Edit)
}
#[test]
fn file_comments_v75_mouse_actual_created_file_comment_edit_same_numeric_id_no_line_anchor() {
    journey(true, CommentOpV2::Edit)
}
#[test]
fn file_comments_v75_keyboard_actual_created_file_comment_delete_exact_204_and_node_null() {
    journey(false, CommentOpV2::Delete)
}
#[test]
fn file_comments_v75_mouse_actual_created_file_comment_delete_exact_204_and_node_null() {
    journey(true, CommentOpV2::Delete)
}
