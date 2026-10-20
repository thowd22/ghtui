//! TASK-23 `suggestions`: S Suggest on the original immutable RIGHT range opens
//! the shared inline composer seeded with the selected code, the Preview shows
//! the resulting change, and Confirm publishes exactly that body once.
use super::*;

/// NEW image lines 1–2 (`context\r\n`, `new-no-newline`) without terminators.
const SUGGESTION: &str = "```suggestion\ncontext\nnew-no-newline\n```";

fn draft_body(fixture: &Fixture) -> Option<String> {
    let encoded: String = raw(fixture)
        .query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'inline/%'",
            [],
            |row| row.get(0),
        )
        .ok()?;
    let draft: Value = serde_json::from_str(&encoded).ok()?;
    draft["values"]["body"].as_str().map(str::to_owned)
}

fn journey(mouse: bool) {
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    let fresh_images = steps[steps.len() - 17..].to_vec();
    steps.extend(source_phase(&h, true));
    steps.extend(fresh_images);
    let mut confirm = confirm_steps_with(&h, "RIGHT", SUGGESTION);
    confirm[7].delay_ms = 0;
    steps.extend(confirm);
    fixture.scripted_gh(&steps);
    let before_git = git_state(&fixture);
    let before_drafts = stable_drafts(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    choose(&mut session, mouse, "[O Old image]", b"O");
    session.wait_for_command_count(&fixture, 58);
    overlay_ready(
        &mut session,
        "Immutable file images",
        &["[R Range]", "[P Comment]", "[S Suggest]"],
    );
    let git_journal = git_commands(&fixture);
    choose(&mut session, mouse, "[N New]", b"N");
    ready(&mut session, &["Selection RIGHT 1–1"]);
    if mouse {
        let first = session.locate("R      1").unwrap();
        let last = session.locate("R      2").unwrap();
        session.drag(first, last);
    } else {
        session.send_key(b"\x1b[1;2B");
    }
    choose(&mut session, mouse, "[R Range]", b"R");
    ready(&mut session, &["READ-ONLY RIGHT 1–2", &h.h]);
    // The control stays on the overlay's bar at 80×24 as well.
    session.resize_owned_modal(
        80,
        24,
        "Immutable file images",
        &["[S Suggest]", "[q Quit]"],
    );
    overlay_ready(&mut session, "Immutable file images", &["[S Suggest]"]);
    session.resize_owned_modal(
        120,
        40,
        "Immutable file images",
        &["[S Suggest]", "[q Quit]"],
    );
    overlay_ready(&mut session, "Immutable file images", &["[S Suggest]"]);
    assert_eq!(commands(&session, &fixture).len(), 58, "Selection is local");
    choose(&mut session, mouse, "[S Suggest]", b"S");
    session.wait_for_command_count(&fixture, 66);
    editing_ready(&mut session, SUGGESTION);
    ready(&mut session, &["Suggested change •", "new-no-newline"]);
    // The seeded body is an ordinary draft: it autosaves before any write.
    session
        .try_wait_for_condition(
            |_| draft_body(&fixture).as_deref() == Some(SUGGESTION),
            "Seeded suggestion body autosaved as the inline draft",
            Duration::from_secs(5),
        )
        .unwrap();
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_command_count(&fixture, 83);
    ready(
        &mut session,
        &[
            "READ-ONLY PREVIEW • Confirm action",
            "RIGHT lines 1–2",
            &h.h,
            "[Confirm action]",
        ],
    );
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("Suggested change • 1 block(s) replace RIGHT lines 1–2 at H:")
                    && screen.contains("+new-no-newline")
                    && screen.contains("-new-no-newline")
            },
            "Preview shows the resulting change of the suggestion",
            Duration::from_secs(5),
        )
        .unwrap();
    assert!(commands(&session, &fixture).iter().all(|row| !row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "POST")));
    let before = session.completed_cursorless_end();
    crate::local::confirm(&mut session, mouse, 1);
    session.wait_for_command_count(&fixture, steps.len());
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("Comment posted")
                && !text.contains("Suggested change •")
                && !text.contains("new-no-newline")
        },
        "Confirmed suggestion published once; private images/form retired",
        Duration::from_secs(8),
    );
    let db = raw(&fixture);
    let row: (String, String, String) = db
        .query_row(
            "SELECT command,state,confirmed_json FROM pr_write_attempts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (row.0.as_str(), row.1.as_str()),
        ("inline-comment", "confirmed")
    );
    let receipt: Value = serde_json::from_str(&row.2).unwrap();
    assert_eq!(receipt["resource_id"], "901");
    assert_eq!(draft_body(&fixture), None, "Published draft removed");
    assert_eq!(stable_drafts(&fixture), before_drafts);
    assert_eq!(git_state(&fixture), before_git);
    assert_eq!(git_commands(&fixture), git_journal);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    // The single POST carries the exact seeded suggestion body (stdin checked).
    exact_commands(&session, &fixture, &steps, 1);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn suggestions_keyboard_suggest_right_range_seeds_previews_and_publishes_once() {
    journey(false)
}
#[test]
fn suggestions_mouse_suggest_right_range_seeds_previews_and_publishes_once() {
    journey(true)
}
