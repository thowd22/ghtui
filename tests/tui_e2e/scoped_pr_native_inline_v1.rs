//! Actual real-binary declarations for Native canonical range -> SAME inline
//! composer. Strict offline gh, installed divergent Git, actual SQL and terminal.
use super::super::inline_comments as composer;
use super::*;
const PENDING: &str = "Opening inline comment";
fn bytes() -> (String, String) {
    let old = (1..=24)
        .map(|n| format!("line{n:03} original context\n"))
        .collect::<String>();
    let new = old
        .replace("line002 original context", "line002 changed context")
        .replace("line003 original context", "line003 changed context");
    (old, new)
}
fn fixture() -> (Fixture, history::GitImagesV1, Vec<GhStep>) {
    let (old, new) = bytes();
    fixture_with_history(
        |repo| {
            history::GitImagesV1::create_renamed_with_bytes(
                repo,
                false,
                old.as_bytes(),
                new.as_bytes(),
            )
        },
        |_| {},
    )
}
fn confirm(h: &history::GitImagesV1, side: &str) -> Vec<GhStep> {
    let mut steps = composer::confirm_steps(h, side);
    for step in &mut steps {
        step.delay_ms = 0;
        if !step.stdin.is_empty() {
            let mut value: Value = serde_json::from_slice(&step.stdin).unwrap();
            value["line"] = json!(3);
            value["start_line"] = json!(2);
            step.stdin = serde_json::to_vec(&value).unwrap();
        }
        if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            if value.get("path").is_some() && value.get("commit_id").is_some() {
                value["line"] = json!(3);
                value["start_line"] = json!(2);
                step.stdout = serde_json::to_vec(&value).unwrap();
            }
        }
    }
    steps
}
fn underlined(frame: &vt100::Screen, text: &str) -> bool {
    crate::support::locate_screen(frame, text)
        .is_some_and(|(x, y)| frame.cell(y, x).is_some_and(|cell| cell.underline()))
}
fn visible_selected_line(session: &mut Session, text: &str, mouse: bool) {
    if mouse {
        let point =
            crate::support::locate_screen(session.completed_cursorless_screen().unwrap(), text)
                .unwrap();
        let after = session.completed_cursorless_end();
        session.click(point.0, point.1);
        session.wait_for_new_cursorless_condition(
            after,
            |frame| underlined(frame, text),
            "actual intended canonical side is focused",
            Duration::from_secs(8),
        );
    } else {
        // Existing exact Home/predicted-row helper locates the OLD member.
        // Focused-side control then moves the SAME row to RIGHT if requested.
        focus(session, false, text);
    }
}
fn range(session: &mut Session, fixture: &Fixture, mouse: bool, side: &str) {
    files_selected(session, fixture, mouse);
    choose(session, mouse, "[v Native diff]", b"v");
    session.wait_for_command_count(fixture, 58);
    nready(session, "line002 changed context");
    session.resize_owned_modal(120, 40, NATIVE, &["[Esc Close]", "[o Options]"]);
    nready(session, "line002 changed context");
    act(session, mouse, NATIVE, "[o Options]", b"o");
    act(session, mouse, NATIVE, "[l Layout]", b"l");
    act(session, mouse, NATIVE, "[o Back to diff]", b"o");
    nready(session, "line002 original context");
    visible_selected_line(session, "line002 original context", mouse);
    act(session, mouse, NATIVE, "[o Options]", b"o");
    act(
        session,
        mouse,
        NATIVE,
        if side == "LEFT" {
            "[O Old side]"
        } else {
            "[N New side]"
        },
        if side == "LEFT" { b"O" } else { b"N" },
    );
    act(session, mouse, NATIVE, "[o Back to diff]", b"o");
    let first = if side == "LEFT" {
        "line002 original context"
    } else {
        "line002 changed context"
    };
    let last = if side == "LEFT" {
        "line003 original context"
    } else {
        "line003 changed context"
    };
    session.wait_for_cursorless_condition(
        |frame| underlined(frame, first) && frame.contents().contains("[o Options]"),
        "exact focused original side before range input",
        Duration::from_secs(8),
    );
    if mouse {
        let frame = session.completed_cursorless_screen().unwrap();
        let a = crate::support::locate_screen(frame, first).unwrap();
        let b = crate::support::locate_screen(frame, last).unwrap();
        session.drag(a, b);
    } else {
        session.send_key(b" ");
        nready(session, "Selected 1");
        session.send_key(b"\x1b[1;2B");
    }
    nready(session, "Selected 2");
    for text in [first, last] {
        let frame = session.completed_cursorless_screen().unwrap();
        let (x, y) = crate::support::locate_screen(frame, text).unwrap();
        assert!(
            frame.cell(y, x).unwrap().inverse(),
            "actual selected side cell, not just summary"
        );
    }
}
fn comment(session: &mut Session, mouse: bool) {
    act(session, mouse, NATIVE, "[P Comment]", b"P");
}
#[expect(
    clippy::too_many_arguments,
    reason = "Keep each independently captured command, Git, draft, and configuration witness explicit"
)]
fn effect_checks(
    session: &Session,
    f: &Fixture,
    steps: &[GhStep],
    posts: usize,
    git: &GitState,
    trace: &[Value],
    drafts: &[String],
    config: &[u8],
) {
    composer::exact_commands(session, f, steps, posts);
    assert_eq!(&readonly_git(f), git);
    assert_eq!(git_commands(f).as_slice(), trace);
    assert_eq!(composer::stable_drafts(f).as_slice(), drafts);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml"))
            .unwrap()
            .as_slice(),
        config
    );
    let foreign:u64=raw(f).query_row("SELECT (SELECT count(*) FROM records WHERE login='user-id:43')+(SELECT count(*) FROM drafts WHERE login='user-id:43')+(SELECT count(*) FROM pr_write_attempts WHERE principal='user-id:43')+(SELECT count(*) FROM actions_write_attempts)",[],|row|row.get(0)).unwrap();
    assert_eq!(foreign, 0);
}
fn post(mouse: bool, side: &str) {
    let (f, h, mut steps) = fixture();
    let fresh = steps[steps.len() - 17..].to_vec();
    steps.extend(composer::source_phase(&h, true));
    steps.extend(fresh);
    steps.extend(confirm(&h, side));
    f.scripted_gh(&steps);
    let git = readonly_git(&f);
    let drafts = composer::stable_drafts(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&f, mouse);
    range(&mut session, &f, mouse, side);
    let trace = git_commands(&f);
    comment(&mut session, mouse);
    session.wait_for_command_count(&f, 66);
    composer::editing_ready(&mut session, "");
    if mouse {
        session.click_owned_form_field("PR INTENT • Preview first", "Body");
    }
    session.paste(composer::BODY);
    composer::editing_ready(&mut session, composer::BODY);
    choose(&mut session, mouse, "[F2 Full anchor details]", b"\x1bOQ");
    overlay_ready(
        &mut session,
        "Original inline anchor",
        &[
            "Canonical API filename:",
            "Selected image path:",
            history::PATH,
            if side == "LEFT" {
                history::PREVIOUS_PATH
            } else {
                history::PATH
            },
            &h.h,
            &h.m,
            &format!("{side} lines 2–3"),
        ],
    );
    choose(&mut session, mouse, "[Esc Back to draft]", b"\x1b");
    composer::editing_ready(&mut session, composer::BODY);
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_command_count(&f, 83);
    ready(
        &mut session,
        &[
            "READ-ONLY PREVIEW • Confirm action",
            &format!("{side} lines 2–3"),
            &h.h,
            "[Confirm action]",
        ],
    );
    assert!(!commands(&session, &f).iter().any(|row| row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "POST")));
    let before = session.completed_cursorless_end();
    crate::local::confirm(&mut session, mouse, 1);
    session.wait_for_command_count(&f, steps.len());
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let s = frame.contents();
            s.contains("Comment posted")
                && !s.contains(NATIVE)
                && !s.contains("line002 original context")
                && !s.contains(composer::BODY.split('\n').next().unwrap())
        },
        "actual receipt retires original Native/private form before return",
        Duration::from_secs(8),
    );
    let row: (String, String, String, String) = raw(&f)
        .query_row(
            "SELECT command,observed_head,state,confirmed_json FROM pr_write_attempts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (row.0, row.1, row.2),
        ("inline-comment".into(), h.h.clone(), "confirmed".into())
    );
    assert_eq!(
        serde_json::from_str::<Value>(&row.3).unwrap()["resource_id"],
        "901"
    );
    let post = commands(&session, &f)
        .into_iter()
        .find(|row| {
            row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST")
        })
        .unwrap();
    let payload = json!({"path":history::PATH,"side":side,"line":3,"start_line":2,"start_side":side,"commit_id":h.h,"body":composer::BODY});
    let exact = serde_json::to_vec(&payload)
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(post["stdin_hex"], exact);
    effect_checks(&session, &f, &steps, 1, &git, &trace, &drafts, &config);
    session.quit_and_assert_restored_terminal(mouse);
}
fn readonly_back(mouse: bool) {
    let (f, h, mut steps) = fixture();
    steps.extend(composer::source_phase(&h, true));
    f.scripted_gh(&steps);
    let git = readonly_git(&f);
    let drafts = composer::stable_drafts(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&f, mouse);
    range(&mut session, &f, mouse, "RIGHT");
    let trace = git_commands(&f);
    // Full-side true context can be searched and viewed, but P cannot produce
    // review anchors even when the original patch selection remains retained.
    act(&mut session, mouse, NATIVE, "[o Options]", b"o");
    act(&mut session, mouse, NATIVE, "[V Full New]", b"V");
    full(&mut session, "New", "line001 original context");
    act(&mut session, mouse, NATIVE, "[P Comment disabled]", b"P");
    overlay_ready(
        &mut session,
        "Action unavailable",
        &["Full-file rows", "not canonical comment anchors"],
    );
    session.send_key(b"\x1b");
    full(&mut session, "New", "line001 original context");
    assert_eq!(commands(&session, &f).len(), 58);
    act(&mut session, mouse, NATIVE, "[b Back patch]", b"b");
    nready(&mut session, "Selected 2");
    let before = session.completed_cursorless_screen().unwrap().clone();
    let first = if mouse {
        "line002 changed context"
    } else {
        "line003 changed context"
    };
    assert!(underlined(&before, first));
    comment(&mut session, mouse);
    session.wait_for_command_count(&f, 66);
    composer::editing_ready(&mut session, "");
    session.paste("Retained Native draft 界");
    composer::editing_ready(&mut session, "Retained Native draft 界");
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    nready(&mut session, "Selected 2");
    assert!(underlined(
        session.completed_cursorless_screen().unwrap(),
        first
    ));
    assert_eq!(commands(&session, &f).len(), 66);
    // Pure Back must return identical canonical focus, not compensate with
    // Home/Down. Resize only after that independent exact-position assertion.
    session.resize_owned_modal(80, 24, NATIVE, &["[Esc Close]", "[o Options]"]);
    nready(&mut session, "Selected 2");
    assert!(session.screen().contains("line003"));
    assert_eq!(commands(&session, &f).len(), 66);
    effect_checks(&session, &f, &steps, 0, &git, &trace, &drafts, &config);
    session.quit_and_assert_restored_terminal(mouse);
}
fn privacy(mouse: bool) {
    // Both exact numeric-principal and moving-head closures, independently.
    for changed_uid in [true, false] {
        let (f, h, mut steps) = fixture();
        let fresh = steps[steps.len() - 17..].to_vec();
        if changed_uid {
            let mut phase = composer::source_phase(&h, true);
            phase.last_mut().unwrap().stdout =
                json!({"id":43,"login":"octocat"}).to_string().into_bytes();
            steps.extend(phase);
        } else {
            steps.extend(composer::source_phase(&h, true));
            let mut phase = fresh;
            let at = phase
                .iter()
                .rposition(|step| {
                    step.args
                        .get(1)
                        .is_some_and(|arg| arg.ends_with("/pulls/7"))
                })
                .unwrap();
            let mut parent: Value = serde_json::from_slice(&phase[at].stdout).unwrap();
            parent["head"]["sha"] = json!("f".repeat(40));
            phase[at].stdout = parent.to_string().into_bytes();
            phase.truncate(at + 1);
            steps.extend(phase);
        }
        f.scripted_gh(&steps);
        let git = readonly_git(&f);
        let drafts = composer::stable_drafts(&f);
        let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = launch(&f, mouse);
        range(&mut session, &f, mouse, "LEFT");
        let trace = git_commands(&f);
        comment(&mut session, mouse);
        if !changed_uid {
            session.wait_for_command_count(&f, 66);
            composer::editing_ready(&mut session, "");
            session.paste(composer::BODY);
            composer::editing_ready(&mut session, composer::BODY);
            choose(&mut session, mouse, "[Preview]", b"\x10");
        }
        session.wait_for_command_count(&f, steps.len());
        session.wait_for_cursorless_condition(
            |frame| {
                let s = frame.contents();
                !s.contains(NATIVE)
                    && !s.contains("Inline comment •")
                    && !s.contains("line002 original context")
                    && !s.contains(composer::BODY.split('\n').next().unwrap())
                    && !s.contains("Immutable file images")
                    && s.contains("private")
            },
            "exact source/account failure clears original private Native/images/form without POST",
            Duration::from_secs(8),
        );
        if !changed_uid {
            let body: String = raw(&f)
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'inline/%'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let draft: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(draft["values"]["body"], composer::BODY);
            assert!(draft.get("approved").is_none());
        }
        let writes: u64 = raw(&f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(writes, 0);
        effect_checks(&session, &f, &steps, 0, &git, &trace, &drafts, &config);
        session.quit_and_assert_restored_terminal(mouse);
    }
}
fn pending_cancel(mouse: bool) {
    for resize in [false, true] {
        let (f, h, mut steps) = fixture();
        let mut held = composer::source_phase(&h, true);
        held.truncate(1);
        held[0].delay_ms = 6000;
        steps.extend(held);
        f.scripted_gh(&steps);
        let git = readonly_git(&f);
        let drafts = composer::stable_drafts(&f);
        let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = launch(&f, mouse);
        range(&mut session, &f, mouse, "LEFT");
        let trace = git_commands(&f);
        let sql = sql_state(&f);
        comment(&mut session, mouse);
        session.wait_for_command_count(&f, 59);
        overlay_ready(&mut session, PENDING, &["[Esc Cancel inline]", "[q Quit]"]);
        let pid = commands(&session, &f)[58]["pid"].as_i64().unwrap() as i32;
        let identity = process_identity(pid)
            .unwrap()
            .expect("actual delayed Open child is alive");
        let owned = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
        assert!(owned.owns(&identity) && !identity.zombie);
        session.paste("POISON_UNDERLAY_BODY");
        session.send_key(b"aP");
        overlay_ready(&mut session, PENDING, &["[Esc Cancel inline]"]);
        assert_eq!(commands(&session, &f).len(), 59);
        assert_eq!(sql_state(&f), sql);
        if resize {
            session.resize_owned_modal(80, 24, NATIVE, &["[Esc Close]", "[o Options]"]);
        } else {
            act(&mut session, mouse, PENDING, "[Esc Cancel inline]", b"\x1b");
        }
        nready(&mut session, "Selected 2");
        let deadline = Instant::now() + Duration::from_secs(5);
        while let Some(now) = process_identity(pid).unwrap() {
            assert_eq!(
                (now.uid, now.birth, now.session),
                (identity.uid, identity.birth, identity.session)
            );
            assert!(
                Instant::now() < deadline,
                "actual canceled original child must be gone"
            );
            session.poll_output(Duration::from_millis(10));
        }
        assert!(!session.screen().contains("POISON_UNDERLAY_BODY"));
        assert_eq!(sql_state(&f), sql);
        effect_checks(&session, &f, &steps, 0, &git, &trace, &drafts, &config);
        session.quit_and_assert_restored_terminal(mouse);
    }
}
#[test]
fn native_inline_comments_keyboard_left_multiline_renamed_original_preview_confirm_receipt() {
    post(false, "LEFT");
}
#[test]
fn native_inline_comments_mouse_left_drag_renamed_original_preview_confirm_receipt() {
    post(true, "LEFT");
}
#[test]
fn native_inline_comments_keyboard_right_multiline_original_preview_confirm_receipt() {
    post(false, "RIGHT");
}
#[test]
fn native_inline_comments_mouse_right_drag_original_preview_confirm_receipt() {
    post(true, "RIGHT");
}
#[test]
fn native_inline_comments_keyboard_full_file_disabled_pure_back_exact_native_focus_and_resize() {
    readonly_back(false);
}
#[test]
fn native_inline_comments_mouse_full_file_disabled_pure_back_exact_native_focus_and_resize() {
    readonly_back(true);
}
#[test]
fn native_inline_comments_keyboard_changed_uid_and_head_reject_without_wrong_anchor_or_post() {
    privacy(false);
}
#[test]
fn native_inline_comments_mouse_changed_uid_and_head_reject_without_wrong_anchor_or_post() {
    privacy(true);
}
#[test]
fn native_inline_comments_keyboard_pending_open_capture_cancel_resize_and_actual_child_cleanup() {
    pending_cancel(false);
}
#[test]
fn native_inline_comments_mouse_pending_open_capture_cancel_resize_and_actual_child_cleanup() {
    pending_cancel(true);
}
