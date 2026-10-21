use crate::{
    local, local_management,
    support::{Fixture, Session},
};
#[path = "../support/stash_refresh_failure_v2.rs"]
#[allow(dead_code)] // Shared fixture also supplies the isolated component-process test.
mod phase;
use std::{fs, time::Duration};
fn drafts(fixture: &Fixture) -> Vec<(String, String, String, String)> {
    let connection =
        rusqlite::Connection::open(fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    let mut statement = connection
        .prepare("SELECT host,login,resource,body FROM drafts ORDER BY host,login,resource")
        .unwrap();
    statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}
fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file", b"base\n", "base");
    fixture.commit_file("stable", b"untouched\n", "stable");
    fs::write(fixture.repo().join("file"), b"successful stash content\n").unwrap();
    fixture.git(&["stash", "push", "-m", "phase witness"]);
    let sha = fixture.git(&["rev-parse", "refs/stash"]).trim().to_owned();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let config = fs::read(fixture.repo().join(".git/config")).unwrap();
    phase::install(&fixture.root);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    local_management::operation(
        &mut session,
        mouse,
        ghtui::local_actions::LocalCommand::StashPop,
    );
    if mouse {
        let (x, y) = session.locate("Exact stash commit SHA:").unwrap();
        session.click(x, y + 1);
    }
    session.send_key(b"\x15");
    session.paste(&sha);
    session.wait_for_screen(&sha);
    local::preview(&mut session, mouse);
    local::confirm(&mut session, mouse, 1);
    session.wait_for_screen("Stash succeeded • refresh failed");
    session.wait_for_screen("[F3 Inspect stash result]");
    assert!(!session
        .screen()
        .contains("READ-ONLY PREVIEW • Confirm action"));
    phase::assert_success_and_failed_refresh(&fixture.root);
    let original_drafts = drafts(&fixture);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    assert_eq!(
        fs::read(fixture.repo().join("file")).unwrap(),
        b"successful stash content\n"
    );
    assert_eq!(fixture.git(&["stash", "list"]), "");
    let before = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("[F3 Inspect stash result]")
    } else {
        session.send_key(b"\x1bOR")
    }
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("Original stash result")
                && text.contains("Git reported success; view refresh failed")
                && text.contains("absent from stash list")
                && text.contains(&sha)
        },
        "Actual successful stash/failed refresh observation",
        Duration::from_secs(8),
    );
    if mouse {
        session.click_visible_action("[Esc Back]")
    } else {
        session.send_key(b"\x1b")
    };
    session.wait_for_screen("[F3 Inspect stash result]");
    local::close(&mut session, mouse);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("file")).unwrap(),
        b"successful stash content\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("stable")).unwrap(),
        b"untouched\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        config
    );
    assert_eq!(fixture.git(&["stash", "list"]), "");
    phase::assert_success_and_failed_refresh(&fixture.root);
    assert_eq!(drafts(&fixture), original_drafts);
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
fn refused_before_witness(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file", b"base\n", "base");
    fs::write(fixture.repo().join("file"), b"retained stash bytes\n").unwrap();
    fixture.git(&["stash", "push", "-m", "refused target"]);
    let sha = fixture.git(&["rev-parse", "refs/stash"]).trim().to_owned();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    phase::install(&fixture.root);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    local_management::operation(
        &mut session,
        mouse,
        ghtui::local_actions::LocalCommand::StashPop,
    );
    if mouse {
        let (x, y) = session.locate("Exact stash commit SHA:").unwrap();
        session.click(x, y + 1);
    }
    session.send_key(b"\x15");
    session.paste(&sha);
    session.wait_for_screen(&sha);
    local::preview(&mut session, mouse);
    fixture.git(&["stash", "drop", "--", "stash@{0}"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    local::confirm(&mut session, mouse, 1);
    session.wait_for_screen("Stash refused before witness capture");
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("LOCAL INTENT • Preview first")
                    && screen.contains("[Keep draft / Close]")
                    && screen.contains("no result inspection")
                    && !screen.contains("[F3")
                    && crate::support::first_form_field_value(
                        screen,
                        "LOCAL INTENT • Preview first",
                        "Exact stash commit SHA",
                    )
                    .is_some_and(|value| value.contains(&sha))
            },
            "Owned refused form/footer/current exact field has no phantom F3",
            Duration::from_secs(8),
        )
        .unwrap();
    assert!(!fixture.root.join("stash-command-succeeded").exists());
    assert!(!fixture.root.join("stash-refresh-failed").exists());
    let records = phase::records(&fixture.root);
    assert!(!records.iter().any(|r| r
        .windows(2)
        .any(|w| w == [b"stash".to_vec(), b"pop".to_vec()])));
    local::close(&mut session, mouse);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join("file")).unwrap(), b"base\n");
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["stash", "list"]), "");
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
fn recreated_during_successful_refresh(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file", b"base\n", "base");
    fs::write(fixture.repo().join("file"), b"successful stash content\n").unwrap();
    fixture.git(&["stash", "push", "-m", "recreated original"]);
    let sha = fixture.git(&["rev-parse", "refs/stash"]).trim().to_owned();
    let original_head = fixture.git(&["rev-parse", "HEAD"]);
    let original_config = fs::read(fixture.repo().join(".git/config")).unwrap();
    phase::replacement(&fixture.root);
    let replacement = fixture.root.join("replacement");
    let replacement_head = phase::git(&replacement, &["rev-parse", "HEAD"]);
    let replacement_index = fs::read(replacement.join(".git/index")).unwrap();
    let replacement_config = fs::read(replacement.join(".git/config")).unwrap();
    phase::install_held_reload(&fixture.root);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    local_management::operation(
        &mut session,
        mouse,
        ghtui::local_actions::LocalCommand::StashPop,
    );
    if mouse {
        let (x, y) = session.locate("Exact stash commit SHA:").unwrap();
        session.click(x, y + 1);
    }
    session.send_key(b"\x15");
    session.paste(&sha);
    session.wait_for_screen(&sha);
    local::preview(&mut session, mouse);
    local::confirm(&mut session, mouse, 1);
    let original = phase::replace_held_locator(&fixture.root, &fixture.repo());
    session.wait_for_screen("Stash succeeded • refresh failed");
    session.wait_for_screen("[F3 Inspect stash result]");
    assert!(!session
        .screen()
        .contains("replacement must not be accepted"));
    assert!(!session.screen().contains("replacement private body"));
    let draft_before_inspection = drafts(&fixture);
    let commands_before = phase::records(&fixture.root).len();
    let before = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("[F3 Inspect stash result]")
    } else {
        session.send_key(b"\x1bOR")
    };
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("Original stash result")
                && text.contains("Original stash inspection unavailable")
                && !text.contains("replacement private body")
                && !text.contains("replacement must not be accepted")
                && !text.contains("Current HEAD:")
        },
        "Original source loss refuses recreated repository before any stash observer/body",
        Duration::from_secs(8),
    );
    assert_eq!(
        phase::records(&fixture.root).len(),
        commands_before,
        "Metadata-only original source refusal starts zero Git children"
    );
    if mouse {
        session.click_visible_action("[Esc Back]")
    } else {
        session.send_key(b"\x1b")
    };
    session.wait_for_screen("[F3 Inspect stash result]");
    local::close(&mut session, mouse);
    assert_eq!(drafts(&fixture), draft_before_inspection);
    assert_eq!(
        phase::git(&original, &["rev-parse", "HEAD"]),
        original_head.as_bytes()
    );
    assert_eq!(
        fs::read(original.join("file")).unwrap(),
        b"successful stash content\n"
    );
    assert!(phase::git(&original, &["stash", "list"]).is_empty());
    assert_eq!(
        fs::read(original.join(".git/config")).unwrap(),
        original_config
    );
    assert_eq!(
        phase::git(&fixture.repo(), &["rev-parse", "HEAD"]),
        replacement_head
    );
    assert_eq!(
        fs::read(fixture.repo().join("file")).unwrap(),
        b"replacement private body\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        replacement_index
    );
    assert_eq!(
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        replacement_config
    );
    let records = phase::records(&fixture.root);
    assert_eq!(
        records
            .iter()
            .filter(|r| r
                .windows(2)
                .any(|w| w == [b"stash".to_vec(), b"pop".to_vec()]))
            .count(),
        1
    );
    assert!(!records.iter().any(|r| r
        .windows(2)
        .any(|w| w == [b"stash".to_vec(), b"drop".to_vec()])));
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn stash_recovery_keyboard_successful_pop_held_refresh_recreated_locator_refuses_new_workspace() {
    recreated_during_successful_refresh(false)
}
#[test]
fn stash_recovery_mouse_successful_pop_held_refresh_recreated_locator_refuses_new_workspace() {
    recreated_during_successful_refresh(true)
}
#[test]
fn stash_recovery_keyboard_successful_pop_failed_refresh_has_genuine_phase_inspection() {
    journey(false);
    refused_before_witness(false)
}
#[test]
fn stash_recovery_mouse_successful_pop_failed_refresh_has_genuine_phase_inspection() {
    journey(true);
    refused_before_witness(true)
}
