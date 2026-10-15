//! Genuine renamed catalogue->Inbox->PR source; all tab effects remain pure.
use super::*;
use crate::github::timeline_actor_v1::observed;

fn journey(mouse: bool) {
    let mut steps = protocol(0);
    steps[26].stdout = json!([observed::events()]).to_string().into_bytes();
    let fixture = configured(&steps);
    observed::forbid_http(&fixture);
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&database(&fixture)).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&database(&fixture)).unwrap());
    let original_git = git_state(&fixture);
    let config_path = fixture.root.join("config/ghtui/config.toml");
    let config = fs::read(&config_path).unwrap();
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    session.wait_for_command_count(&fixture, 7);
    ready(&mut session, &[TITLE, FIRST]);
    if !mouse {
        session.send_key(b"\t");
    }
    choose(&mut session, mouse, "[Open My PRs]", b"\r");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_command_count(&fixture, 18);
    ready(&mut session, &[ROW_TITLE]);
    choose(&mut session, mouse, "1 Inbox", b"1");
    choose(&mut session, mouse, ROW_TITLE, b"\r");
    session.wait_for_command_count(&fixture, 31);
    ready(
        &mut session,
        &[RENAMED, DETAIL_TITLE, "Head aaaaaaaaa", "[t Timeline]"],
    );
    let original_sql = sql_state(&fixture);
    let gh = session.inspect_command_journal(&fixture);
    let git = git_commands(&fixture);
    session.resize(120, 40);
    ready(&mut session, &[RENAMED, DETAIL_TITLE, "[t Timeline]"]);
    choose(&mut session, mouse, "[t Timeline]", b"t");
    session.wait_for_cursorless_condition(
        |f| {
            let s = f.contents();
            observed::EXPECTED.iter().all(|e| s.contains(e))
                && observed::ABSENT.iter().all(|e| !s.contains(e))
        },
        "Completed original scoped timeline explicit observed roles",
        Duration::from_secs(8),
    );
    session.resize(80, 24);
    ready(
        &mut session,
        &["comment author", "[t Timeline]", "[b Back]"],
    );
    session.resize(120, 40);
    ready(&mut session, &["comment author", "review author"]);
    choose(&mut session, mouse, "[o Summary]", b"o");
    ready(&mut session, &[DETAIL_TITLE, DETAIL_BODY, "Head aaaaaaaaa"]);
    choose(&mut session, mouse, "[b Back]", b"b");
    session.wait_for_cursorless_condition(
        |f| {
            let s = f.contents();
            s.contains(ROW_TITLE)
                && s.contains("Cached • fetched")
                && !s.contains(DETAIL_TITLE)
                && !s.contains("Exact timeline comment body")
        },
        "Completed pure scoped Back hides original private timeline",
        Duration::from_secs(8),
    );
    assert_eq!(session.inspect_command_journal(&fixture), gh);
    assert_eq!(git_commands(&fixture), git);
    assert_eq!(sql_state(&fixture), original_sql);
    assert_eq!(git_state(&fixture), original_git);
    assert_eq!(fs::read(&config_path).unwrap(), config);
    no_attempts(&fixture);
    observed::assert_http_zero(&fixture);
    assert_read_only_git(&fixture);
    exact_reads(&session, &fixture, &steps);
    session.quit_and_assert_restored_terminal(mouse);
    exact_reads(&session, &fixture, &steps);
    assert_eq!(sql_state(&fixture), original_sql);
    assert_eq!(git_state(&fixture), original_git);
    observed::assert_http_zero(&fixture);
}
#[test]
fn timeline_actor_v1_scoped_keyboard_renamed_original_roles_resize_and_pure_back() {
    journey(false)
}
#[test]
fn timeline_actor_v1_scoped_mouse_renamed_original_roles_resize_and_pure_back() {
    journey(true)
}
