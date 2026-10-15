//! DECLARED/UNRUN. Original immutable range -> actual shared Inline Preview.
use super::*;
fn body() -> String {
    (0..160)
        .map(|n| format!("INLINE_ROW_{n:03} {} 界e\u{301}\n", "abcdefghi".repeat(12)))
        .collect::<String>()
        + "EXACT_INLINE_TAIL"
}
fn marker(session: &Session) -> Option<String> {
    let t = session.completed_cursorless_screen()?.contents();
    let start = t.find("INLINE_ROW_")?;
    Some(t[start..].chars().take(14).collect())
}
fn editable(session: &mut Session, tail: &str) {
    session
        .try_wait_for_condition(
            |text| {
                text.contains("PR INTENT • Preview first")
                    && text.contains("Body:")
                    && text.contains(tail)
                    && text.contains("[Preview]")
                    && text.contains("[Confirm action]")
                    && text.contains("[Keep draft / Close]")
            },
            "actual owned long inline editor tail/caret and complete controls",
            Duration::from_secs(8),
        )
        .unwrap();
}
fn scroll(session: &mut Session, mouse: bool) {
    let before = session.completed_cursorless_end();
    let old = session.completed_cursorless_screen().unwrap().contents();
    if mouse {
        session.wheel(30, 12, true)
    } else {
        session.send_key(b"\x1b[6~")
    }
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let t = frame.contents();
            t != old && t.contains("READ-ONLY PREVIEW") && t.contains("[Confirm action]")
        },
        "new actual indexed inline Preview scroll",
        Duration::from_secs(8),
    );
}
fn replace(value: &mut Value, old: &str, new: &str) {
    match value {
        Value::String(text) if text == old => *text = new.into(),
        Value::Array(rows) => {
            for row in rows {
                replace(row, old, new)
            }
        }
        Value::Object(rows) => {
            for row in rows.values_mut() {
                replace(row, old, new)
            }
        }
        _ => {}
    }
}
fn confirm_control(session: &mut Session, mouse: bool, side: &str) {
    let title = format!("Inline comment • {FIRST}#7 • {side} 1–2");
    if mouse {
        let point = session.wait_for_owned_completed_action(&title, "[Confirm action]", &[]);
        session.click(point.0, point.1);
    } else {
        session.focus_completed_owned_action_v1(
            &title,
            "[Confirm action]",
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            &[
                "[Preview]",
                "[External editor]",
                "[Confirm action]",
                "[Keep draft / Close]",
                "[F2 Full anchor details]",
            ],
            &[],
        );
        session.send_key(b"\r");
    }
}
fn preview_ready(session: &mut Session, side: &str, head: &str) {
    let title = format!("Inline comment • {FIRST}#7 • {side} 1–2");
    let expected = format!("{side} lines 1–2 • commit_id H {head}")
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    session.wait_for_cursorless_condition(
        |frame| {
            let Some(body) = crate::support::overlay_body(frame, &title) else {
                return false;
            };
            let exact = body
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>();
            body.contains("READ-ONLY PREVIEW")
                && body.contains("[Confirm action]")
                && exact.contains(&expected)
        },
        "complete actual original H and side inside owned Preview, including physical line wraps",
        Duration::from_secs(8),
    );
}
fn journey(mouse: bool, edit: bool) {
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    let images = steps[steps.len() - 17..].to_vec();
    steps.extend(source_phase(&h, true));
    steps.extend(images.clone());
    if edit {
        steps.extend(images);
    }
    let original = body();
    let final_body = if edit {
        format!("{original}\nEDITED_EXACT_TAIL")
    } else {
        original.clone()
    };
    let side = if mouse { "RIGHT" } else { "LEFT" };
    let mut confirmation = confirm_steps(&h, side);
    for step in &mut confirmation {
        step.delay_ms = 0;
        if !step.stdin.is_empty() {
            let mut v: Value = serde_json::from_slice(&step.stdin).unwrap();
            replace(&mut v, BODY, &final_body);
            step.stdin = serde_json::to_vec(&v).unwrap();
        }
        if let Ok(mut v) = serde_json::from_slice::<Value>(&step.stdout) {
            replace(&mut v, BODY, &final_body);
            step.stdout = serde_json::to_vec(&v).unwrap();
        }
    }
    let confirmation_start = steps.len();
    steps.extend(confirmation);
    fixture.scripted_gh(&steps);
    let git = git_state(&fixture);
    let drafts = stable_drafts(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    choose(&mut session, mouse, "[O Old image]", b"O");
    session.wait_for_command_count(&fixture, 58);
    overlay_ready(
        &mut session,
        "Immutable file images",
        &["[R Range]", "[P Comment]"],
    );
    let before = session.completed_cursorless_end();
    choose(
        &mut session,
        mouse,
        if mouse { "[N New]" } else { "[O Old]" },
        if mouse { b"N" } else { b"O" },
    );
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("Immutable file images")
                && text.contains(&format!("Selection {side} 1–1"))
                && text.contains(&h.h)
                && text.contains("[R Range]")
        },
        "NEW actual chosen image side before using its current row coordinates",
        Duration::from_secs(8),
    );
    let before = session.completed_cursorless_end();
    if mouse {
        let first = session.locate("R      1").unwrap();
        let last = session.locate("R      2").unwrap();
        session.drag(first, last);
    } else {
        session.send_key(b"\x1b[1;2B");
    }
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("Immutable file images")
                && text.contains(&format!("Selection {side} 1–2"))
                && text.contains(&h.h)
                && text.contains("[R Range]")
        },
        "NEW exact two-line selection before capturing canonical range",
        Duration::from_secs(8),
    );
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[R Range]", b"R");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("Immutable file images")
                && text.contains(&format!("READ-ONLY {side} 1–2"))
                && text.contains(&h.h)
                && text.contains("[P Comment]")
        },
        "NEW genuine canonical H-bound range before Inline",
        Duration::from_secs(8),
    );
    let trace = git_commands(&fixture);
    choose(&mut session, mouse, "[P Comment]", b"P");
    session.wait_for_command_count(&fixture, 66);
    editing_ready(&mut session, "");
    if mouse {
        session.click_owned_form_field("PR INTENT • Preview first", "Body");
    }
    session.paste(&original);
    editable(&mut session, "EXACT_INLINE_TAIL");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_command_count(&fixture, 83);
    preview_ready(&mut session, side, &h.h);
    for _ in 0..18 {
        scroll(&mut session, mouse);
    }
    let anchor = marker(&session).expect("actual original inline body deep row");
    for (width, height) in [(80, 24), (120, 40), (80, 24), (120, 40)] {
        let old = session.completed_cursorless_end();
        session.resize(width, height);
        session.wait_for_new_cursorless_condition(
            old,
            |frame| {
                let t = frame.contents();
                t.contains(&anchor) && t.contains("[Confirm action]")
            },
            "exact safe-byte inline anchor across64/88 layouts",
            Duration::from_secs(8),
        );
    }
    assert_eq!(commands(&session, &fixture).len(), 83);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        raw(&fixture)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    if edit {
        choose(&mut session, mouse, "[Preview]", b"\x10");
        editable(&mut session, "EXACT_INLINE_TAIL");
        if mouse {
            session.click_owned_form_field("PR INTENT • Preview first", "Body");
        }
        session.send_key(b"\x01");
        session.paste(&final_body);
        editable(&mut session, "EDITED_EXACT_TAIL");
        // Actual rejected Submit is not a surrogate fresh Preview and sends0GH.
        confirm_control(&mut session, mouse, side);
        ready(&mut session, &["Choose Preview", "[Preview]"]);
        assert_eq!(commands(&session, &fixture).len(), 83);
        choose(&mut session, mouse, "[Preview]", b"\x10");
        session.wait_for_command_count(&fixture, 100);
        preview_ready(&mut session, side, &h.h);
    }
    assert_eq!(commands(&session, &fixture).len(), confirmation_start);
    let before = session.completed_cursorless_end();
    confirm_control(&mut session, mouse, side);
    session.wait_for_command_count(&fixture, steps.len());
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let t = frame.contents();
            t.contains("Comment posted")
                && !t.contains("INLINE_ROW_")
                && !t.contains("old-no-newline")
        },
        "one exact original inline effect then private source retirement",
        Duration::from_secs(8),
    );
    exact_commands(&session, &fixture, &steps, 1);
    let db = raw(&fixture);
    let receipt: (String, String, String) = db
        .query_row(
            "SELECT command,observed_head,state FROM pr_write_attempts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        receipt,
        ("inline-comment".into(), h.h.clone(), "confirmed".into())
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM drafts WHERE resource LIKE 'inline/%'",
            [],
            |row| row.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(stable_drafts(&fixture), drafts);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn form_preview_visible_keyboard_inline_long_original_anchor_exact_post_sql_git_tty() {
    journey(false, false)
}
#[test]
fn form_preview_visible_mouse_inline_long_original_anchor_exact_post_sql_git_tty() {
    journey(true, false)
}
#[test]
fn form_preview_visible_keyboard_inline_edit_retires_old_document_fresh_preview_exact_post() {
    journey(false, true)
}
#[test]
fn form_preview_visible_mouse_inline_edit_retires_old_document_fresh_preview_exact_post() {
    journey(true, true)
}
