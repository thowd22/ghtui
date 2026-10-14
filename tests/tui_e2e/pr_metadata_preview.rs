//! Real compiled-binary preview approval, editor and restart journeys.
use super::*;

const RETAINED_TITLE: &str = "Editor retained metadata title café 雪";

fn close(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[Keep draft / Close]", b"\x1b");
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("[u Metadata]")
                    && !screen.contains("[Confirm action]")
                    && !screen.contains("[Keep draft / Close]")
            },
            "complete native Metadata frame after composer close",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}; artifacts: {}", session.artifact_dir.display()));
}
fn no_publication(session: &mut Session, fixture: &Fixture, count: usize) {
    let actual = observed_requests(session, fixture, count);
    assert_eq!(actual.len(), count);
    assert_eq!(mutation_count(&actual), 0);
    assert!(receipts(fixture).is_empty());
}
fn require_preview(session: &mut Session, fixture: &Fixture, mouse: bool, count: usize) {
    submit(session, mouse);
    session.wait_for_screen("Choose Preview and inspect the exact effect");
    session.wait_for_screen("Tab: controls • Esc: keep draft");
    no_publication(session, fixture, count);
}
fn preview_text(session: &mut Session, mouse: bool, text: &str) {
    for _ in 0..16 {
        if session
            .try_wait_for(text, Duration::from_millis(30))
            .is_ok()
        {
            return;
        }
        if mouse {
            session.wheel(15, 12, true);
        } else {
            session.send_key(b"\x1b[B");
        }
    }
    session.wait_for_screen(text);
}
fn edit_again(session: &mut Session, mouse: bool) {
    if !mouse {
        session.focus_visible_form_action("[Preview]");
    }
    choose(session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("PR INTENT • Preview first");
    session.wait_for_screen("Title:");
    if !mouse {
        // Preview is the first visibly focused footer after the one field.
        // Shift-Tab returns to that field; replace_field independently proves
        // that the actual requested text changed, not just button focus.
        session.send_key(b"\x1b[Z");
    }
}

#[test]
fn pr_metadata_preview_keyboard_no_change_edit_editor_close_restart_and_fresh_confirmation() {
    journey(false);
}
#[test]
fn pr_metadata_preview_mouse_no_change_edit_editor_close_restart_and_fresh_confirmation() {
    journey(true);
}
fn journey(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let original = target("Native collaboration", "A fixture description", "main");
    let updated = target(RETAINED_TITLE, "A fixture description", "main");
    let mut reads = at_target(initial(), &original);
    reads.extend(at_target(details(vec![starter()]), &original));
    reads.extend(metadata(&original));
    assert_eq!(reads.len(), 18);
    let mut steps = reads.clone();
    steps.extend(reads);
    steps.extend(publication(&original, &updated, "title", RETAINED_TITLE));
    assert_eq!(steps.len(), 61);
    fixture.scripted_gh(&steps);
    let editor = fixture.root.join("metadata fixture editor.sh");
    fs::write(&editor,format!("#!/bin/sh\nstty -a > \"$GH_FIXTURE_DIR/metadata-editor-termios.txt\"\nprintf '%s' '{RETAINED_TITLE}' > \"$1\"\n")).unwrap();
    let editor_command = format!("sh '{}'", editor.display());
    let mut session = Session::launch_binary(
        std::path::Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        120,
        40,
        &[("VISUAL", ""), ("EDITOR", &editor_command)],
    );
    session.wait_for_screen("ghtui");
    session.send_key(b"1");
    open(&mut session, mouse);
    choose(&mut session, mouse, "[u Metadata]", b"u");
    unchanged_metadata(&mut session, mouse);

    for action in ["Edit PR title", "Edit PR description", "Change base branch"] {
        menu(&mut session, mouse, action);
        local_preview(&mut session, &fixture, mouse, 18);
        session.wait_for_screen("No change • requested");
        submit(&mut session, mouse);
        session.wait_for_screen("Tab: controls • Esc: keep draft");
        no_publication(&mut session, &fixture, 18);
        close(&mut session, mouse);
        preserve_git(&fixture, &head, &index, &status);
    }

    menu(&mut session, mouse, "Edit PR title");
    replace_field(&mut session, mouse, "Title", "Unapproved draft title");
    require_preview(&mut session, &fixture, mouse, 18);
    local_preview(&mut session, &fixture, mouse, 18);
    preview_text(&mut session, mouse, "Current Title");
    preview_text(&mut session, mouse, "Native collaboration");
    preview_text(&mut session, mouse, "Requested Title");
    preview_text(&mut session, mouse, "Unapproved draft title");
    session.resize(80, 24);
    session.wait_for_screen("[Confirm action]");
    preview_text(&mut session, mouse, "Unapproved draft title");
    edit_again(&mut session, mouse);
    replace_field(&mut session, mouse, "Title", "Edited after preview title");
    require_preview(&mut session, &fixture, mouse, 18);
    local_preview(&mut session, &fixture, mouse, 18);
    choose(&mut session, mouse, "[External editor]", b"\x05");
    session.wait_for_screen(RETAINED_TITLE);
    session.wait_for_screen("PR INTENT • Preview first");
    let termios =
        fs::read_to_string(fixture.root.join("gh-fixture/metadata-editor-termios.txt")).unwrap();
    assert!(termios
        .split_whitespace()
        .any(|flag| flag == "icanon" || flag == "icanon;"));
    assert!(!termios.contains("-icanon"));
    require_preview(&mut session, &fixture, mouse, 18);
    local_preview(&mut session, &fixture, mouse, 18);
    preview_text(&mut session, mouse, RETAINED_TITLE);
    close(&mut session, mouse);
    wait_local_draft(&mut session, &fixture, "title", RETAINED_TITLE);
    let saved = Cache::open(&cache_path(&fixture))
        .unwrap()
        .draft(&scope(), &format!("{REPOSITORY}/pull/7/composer/title"))
        .unwrap()
        .unwrap();
    let draft: Value = serde_json::from_str(&saved).unwrap();
    assert!(draft.get("metadata_preview").is_none());
    assert!(draft.get("fingerprint").is_none());
    no_publication(&mut session, &fixture, 18);
    preserve_git(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);

    let mut restarted = Session::launch(&fixture, 80, 24);
    restarted.wait_for_screen("ghtui");
    restarted.send_key(b"1");
    open(&mut restarted, mouse);
    choose(&mut restarted, mouse, "[u Metadata]", b"u");
    unchanged_metadata(&mut restarted, mouse);
    menu(&mut restarted, mouse, "Edit PR title");
    restarted.wait_for_screen(RETAINED_TITLE);
    restarted.wait_for_screen("PR INTENT • Preview first");
    require_preview(&mut restarted, &fixture, mouse, 36);
    local_preview(&mut restarted, &fixture, mouse, 36);
    preview_text(&mut restarted, mouse, "Current Title");
    preview_text(&mut restarted, mouse, "Native collaboration");
    preview_text(&mut restarted, mouse, "Requested Title");
    preview_text(&mut restarted, mouse, RETAINED_TITLE);
    submit(&mut restarted, mouse);
    restarted.wait_for_screen("Published • Edit PR title");
    restarted.wait_for_overlay_close("┌ Edit PR title ");
    restarted.wait_for_screen(RETAINED_TITLE);
    unchanged_metadata(&mut restarted, mouse);
    let actual = observed_requests(&mut restarted, &fixture, 61);
    assert_eq!(mutation_count(&actual), 1);
    assert_confirmed(&fixture, &[("legacy-title", RETAINED_TITLE)], &updated);
    assert!(Cache::open(&cache_path(&fixture))
        .unwrap()
        .draft(&scope(), &format!("{REPOSITORY}/pull/7/composer/title"))
        .unwrap()
        .is_none());
    preserve_git(&fixture, &head, &index, &status);
    restarted.quit_and_assert_restored_terminal(mouse);
    assert_eq!(observed_requests(&mut restarted, &fixture, 61), actual);
    assert_confirmed(&fixture, &[("legacy-title", RETAINED_TITLE)], &updated);
    preserve_git(&fixture, &head, &index, &status);
}
