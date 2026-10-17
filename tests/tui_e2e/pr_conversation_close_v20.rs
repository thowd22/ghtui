//! Real installed binary, actual accepted/failed SQLite Save, strict offline gh.
//! No durability inference from dirty=false, relabelled result or handler call.
use super::*;

const UNSAVED: &str = "V20_ORIGINAL_ACCEPTED_BODY_界";
const EMPTY: &str = "{\"version\":17,\"values\":{\"body\":\"\"},\"publication\":null}";
fn open(s: &mut Session, f: &Fixture, mouse: bool, target: usize) {
    let before = s.completed_cells_end();
    crate::pr_actions::menu(
        s,
        mouse,
        "New comment • eligibility UNKNOWN/server-enforced",
    );
    s.wait_for_command_count(f, target);
    form_frame(s, before, &title(PrCommand::NewComment), EDITABLE);
}
fn close(s: &mut Session, mouse: bool) {
    let before = s.completed_cursorless_end();
    let p = s.wait_for_owned_completed_action(
        &title(PrCommand::NewComment),
        "[Keep draft / Close]",
        &[],
    );
    if mouse {
        s.click(p.0, p.1);
    } else {
        s.send_key(b"\x1b");
    }
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            crate::support::main_footer_visible(frame)
                && !frame.contents().contains("[Confirm action]")
        },
        "Actual native page after inert Close, no renewed purpose",
        Duration::from_secs(8),
    );
}
fn journey(mouse: bool, fail: bool) {
    let command = PrCommand::NewComment;
    let mut steps = initial_steps(true);
    let initial = steps.len();
    steps.extend(phase(command, true, Some(true)));
    let first = steps.len();
    if !fail {
        steps.extend(phase(command, true, Some(true)));
    }
    let f = resource_configured(&steps);
    for uid in [42, 43] {
        Cache::open(&database(&f))
            .unwrap()
            .put_draft(
                &Scope::github(HOST, Some(uid)).unwrap(),
                "independent-kept-v20",
                &format!("KEPT-{uid}"),
            )
            .unwrap();
    }
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    enter(&mut s, &f, true, mouse, initial, false, &steps[..initial]);
    let stable = stable_sql(&f);
    open(&mut s, &f, mouse, first);
    let db = rusqlite::Connection::open_with_flags(
        database(&f),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    // The genuine Open registers its initial empty autosave. Observe its
    // ACTUAL SQL completion before injecting the later held/failed Save;
    // absence is not silently treated as a completed seed or expected zero.
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        let saved = db
            .query_row(
                "SELECT body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .unwrap();
        if let Some(saved) = saved {
            assert_eq!(saved, EMPTY);
            break;
        }
        assert!(
            Instant::now() < until,
            "Actual initial empty autosave did not commit"
        );
        s.poll_output(Duration::from_millis(2));
    }
    if fail {
        db.execute_batch("CREATE TRIGGER actual_v20_save_failure BEFORE INSERT ON drafts WHEN NEW.resource LIKE 'conversation-draft-v17/%' BEGIN SELECT RAISE(ABORT,'actual local draft write refusal'); END;").unwrap();
    } else {
        // Genuine database lock, not a fake Save or a synthetic ACK. Release
        // promptly after observing queued Open; the writer retains its own
        // existing two-second busy bound, which this test never widens.
        db.execute_batch(
            "BEGIN IMMEDIATE; UPDATE drafts SET body=body WHERE resource='independent-kept-v20';",
        )
        .unwrap();
    }
    if mouse {
        s.click_owned_form_field("PR INTENT • Preview first", "Comment text");
    }
    s.paste(UNSAVED);
    s.wait_for_screen(UNSAVED);
    // A completed edited body is not an ACK. Close must retain it privately,
    // and the accepted ordinary Save remains owned by the actual DraftWriter.
    close(&mut s, mouse);
    exact(&s, &f, &steps[..first]);
    let before = s.completed_cells_end();
    crate::pr_actions::menu(
        &mut s,
        mouse,
        "New comment • eligibility UNKNOWN/server-enforced",
    );
    if fail {
        form_frame(
            &mut s,
            before,
            &title(command),
            &[UNSAVED, "Local draft save failed.", "[Keep draft / Close]"],
        );
        assert!(!s.screen().contains("READ-ONLY PREVIEW"));
        assert!(s.screen().contains("edit explicitly"));
        exact(&s, &f, &steps);
        assert_eq!(
            db.query_row(
                "SELECT body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            EMPTY
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            0
        );
    } else {
        s.wait_for_new_cursorless_condition(
            before,
            |frame| {
                frame
                    .contents()
                    .contains("Queued behind accepted local draft saves")
                    && !frame.contents().contains("[Confirm action]")
            },
            "Actual pending read cutoff before any private GH child",
            Duration::from_secs(8),
        );
        exact(&s, &f, &steps[..first]);
        // External observer sees ONLY the genuine old empty body, never the
        // accepted new bytes, while the real Save waits.
        assert_eq!(
            db.query_row(
                "SELECT body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            EMPTY
        );
        db.execute_batch("COMMIT;").unwrap();
        s.wait_for_command_count(&f, steps.len());
        form_frame(
            &mut s,
            before,
            &title(command),
            &[
                UNSAVED,
                "[Preview]",
                "[Confirm action]",
                "[Keep draft / Close]",
            ],
        );
        let encoded: String = db
            .query_row(
                "SELECT body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let saved: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(saved["values"]["body"], UNSAVED);
        assert!(saved["publication"].is_null() && saved.get("approval").is_none());
        assert!(!s.screen().contains("READ-ONLY PREVIEW"));
        exact(&s, &f, &steps);
        assert_eq!(
            db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            0
        );
    }
    assert_eq!(stable_sql(&f), stable);
    assert_eq!(git_state(&f), git);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_read_only_git(&f);
    close(&mut s, mouse);
    s.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn conversation_comments_v20_held_accepted_save_close_immediate_open_keyboard() {
    journey(false, false);
}
#[test]
fn conversation_comments_v20_held_accepted_save_close_immediate_open_mouse() {
    journey(true, false);
}
#[test]
fn conversation_comments_v20_failed_save_close_immediate_open_retains_editable_body_keyboard() {
    journey(false, true);
}
#[test]
fn conversation_comments_v20_failed_save_close_immediate_open_retains_editable_body_mouse() {
    journey(true, true);
}
