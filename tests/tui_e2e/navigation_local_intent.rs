//! Registered real-binary keyboard/mouse intent pair. Preview and cancellation
//! use the same guarded local handlers as the contextual command palette.
use super::*;

#[test]
fn navigation_local_palette_keyboard_disabled_then_exact_file_intent_preview_and_cancel() {
    intent(false);
}
#[test]
fn navigation_local_palette_mouse_disabled_then_exact_file_intent_preview_and_cancel() {
    intent(true);
}
fn intent(mouse: bool) {
    let local = fixture();
    let trace = local.fixture.root.join("navigation-child-git-trace.jsonl");
    let mut session = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &local.fixture,
        80,
        24,
        &[
            ("GIT_TRACE2_EVENT", trace.to_str().unwrap()),
            ("NO_COLOR", "1"),
        ],
    );
    session.wait_for_screen("Recent commits");
    command(&mut session, mouse, "Show History");
    session.wait_for_screen("loaded (limit 10000)");
    session.open_saved_graph_subject(mouse, "Original navigation ancestry", &local.original);
    session.wait_for_screen(&local.original);
    // The Working mode tab, located before the overlay covers it.
    let underlying = session.wait_for_control("[w Working]");
    let no_read = starts(&local.fixture);
    command(&mut session, mouse, "Stage selected lines");
    session.wait_for_screen("Action unavailable • explanation only");
    session.wait_for_screen("canonical added or removed");
    if mouse {
        session.click(underlying.0, underlying.1);
    } else {
        session.send_key(b"w");
    }
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close("Action unavailable • explanation only");
    session.wait_for_screen("loaded (limit 10000)");
    assert_eq!(
        starts(&local.fixture),
        no_read,
        "Explain/filter/background clicks must not execute Git"
    );
    assert!(session.inspect_command_journal(&local.fixture).is_empty());
    assert!(!session.screen().contains("LOCAL INTENT"));
    command(&mut session, mouse, "Show Working diff");
    session.wait_for_screen("working navigation needle β");
    // Working comparison rows carry true DiffLine anchors, not WorkingFile
    // status rows. Select that actual rendered changed line; the existing
    // LocalContext derives its exact new/old RawPath from the canonical anchor.
    selected_row(&mut session, mouse, "working navigation needle β");
    session.wait_for_screen("Selected change line");
    let before_intent = starts(&local.fixture);
    command(&mut session, mouse, "Stage selected file");
    body_ready(
        &mut session,
        "Stage selected file",
        &[
            "LOCAL INTENT",
            "Preview first",
            "[Preview]",
            "[Confirm action]",
        ],
    );
    assert!(!session.screen().contains("READ-ONLY PREVIEW"));
    check_effects(&local, &session);
    // Preparing a fresh exact intent/Preview legitimately performs bounded
    // read-only Git calls. Only filtering and unavailable choice promise0.
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW • Confirm action");
    session.wait_for_screen("Read-only preview ready");
    session.wait_for_screen("nested/日本語.txt");
    assert!(
        starts(&local.fixture) > before_intent,
        "Actual Preview must run its real Git target/snapshot checks"
    );
    check_effects(&local, &session);
    assert!(!session.screen().contains("Completed • Stage selected file"));
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_overlay_close("Stage selected file");
    session
        .try_wait_for_condition(
            |screen| !screen.contains("READ-ONLY PREVIEW") && !screen.contains("LOCAL INTENT"),
            "real composer/preview closure before next input",
            Duration::from_secs(8),
        )
        .unwrap();
    session.assert_focused_view("Repository");
    session.wait_for_screen("working navigation needle β");
    check_effects(&local, &session);
    // No Confirm input was sent. Draft retention is allowed local intent and
    // is NOT an approved plan or a reason to send the operation on reopening.
    session.quit_and_assert_restored_terminal(mouse);
}
