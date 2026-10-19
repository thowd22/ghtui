//! Local keyboard/mouse selection must retain exact durable disabled targets.
use super::*;
use ghtui::{
    actions_write_journal::{
        ActionsImmutableContext, ActionsSafeOptions, ActionsWriteIntent, ActionsWriteJournal,
        ActionsWriteKind, ActionsWriteTarget,
    },
    providers::gh::ActionsTarget,
    storage::Scope,
};

#[test]
fn actions_run_control_keyboard_retained_cache_lease_survives_selection_and_key_bar() {
    journey(false);
}
#[test]
fn actions_run_control_mouse_retained_cache_lease_survives_selection_and_key_bar() {
    journey(true);
}
fn journey(mouse: bool) {
    let fixture = fixture();
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    let mut publication = ActionsWriteJournal::open(&database).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let target = ActionsWriteTarget::from_scope(
        &scope,
        &ActionsTarget::Cache {
            repository: "example/repo".into(),
            id: 71,
        },
    )
    .unwrap();
    let intent = ActionsWriteIntent::new(
        ActionsWriteKind::DeleteCache,
        ActionsImmutableContext::default(),
        ActionsSafeOptions::default(),
        b"",
    )
    .unwrap();
    let permit = publication.begin(&target, &intent).unwrap();
    assert_eq!(permit.marker().attempt_id, 1);
    drop(publication);
    let original_head = fixture.git(&["rev-parse", "HEAD"]);
    let original_index = fixture.git(&["ls-files", "--stage"]);
    let cache = |id, key| json!({"id":id,"key":key,"ref":"refs/heads/main","version":"v1","size_in_bytes":1024,"created_at":"2026-10-13T12:00:00Z","last_accessed_at":"2026-10-13T12:01:00Z"});
    let mut steps = initial();
    steps.extend([
        read(
            "repos/example/repo/actions/caches?per_page=100",
            json!([{"actions_caches":[cache(71,"protected-cache"),cache(72,"unrelated-cache")]}]),
            true,
        ),
        read(
            "repos/example/repo/actions/cache/usage",
            json!({"active_caches_count":2,"active_caches_size_in_bytes":2048}),
            false,
        ),
    ]);
    let catalogue_count = steps.len();
    steps.extend(compose_identity_steps());
    let final_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen("example/repo run 123");
    choose(&mut session, mouse, "Caches", b"c");
    session.wait_for_screen("#71 • protected-cache");
    session.wait_for_screen("#72 • unrelated-cache");
    journal(&mut session, &fixture, catalogue_count);
    // Select another resource, rebuild the key bar, and compose only
    // that unblocked target. A cancelled composer must not claim any publication.
    if mouse {
        session.click_visible_action("#72 • unrelated-cache");
    } else {
        session.send_key(b"\x1b[H\x1b[B");
    }
    session
        .try_wait_for_condition(
            |screen| {
                screen
                    .lines()
                    .any(|line| line.contains("#72 • unrelated-cache") && line.contains('>'))
            },
            "Unrelated cache72 visibly selected with refreshed control layout",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    command(&mut session, mouse, "Delete selected cache");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("72");
    close_form(&mut session, mouse);
    journal(&mut session, &fixture, final_count);
    if mouse {
        session.click_visible_action("#71 • protected-cache");
    } else {
        session.send_key(b"\x1b[H");
    }
    session
        .try_wait_for_condition(
            |screen| {
                screen
                    .lines()
                    .any(|line| line.contains("#71 • protected-cache") && line.contains('>'))
            },
            "Original protected cache71 visibly selected with refreshed disabled controls",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    command(&mut session, mouse, "Delete selected cache");
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("Original attempt #1");
    assert!(!session.screen().contains("ACTIONS INTENT"));
    session.resize(80, 24);
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("Original attempt #1");
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Action unavailable"),
            "Exact durable lease explanation closed",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    let calls = journal(&mut session, &fixture, final_count);
    assert!(calls
        .iter()
        .all(|row| row["args"][0] == "--version" || row["args"][5] == "GET"));
    let markers = publication_support::durable_rows(&database);
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].attempt_id, 1);
    assert_eq!(markers[0].state, "pending");
    assert_eq!(markers[0].target["Cache"]["id"], 71);
    assert!(markers[0].receipt.is_none());
    let connection = rusqlite::Connection::open_with_flags(
        &database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let claimed: i64 = connection
        .query_row(
            "SELECT claimed FROM actions_write_attempts WHERE attempt_id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(claimed, 0);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), original_head);
    assert_eq!(fixture.git(&["ls-files", "--stage"]), original_index);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.quit_and_assert_restored_terminal(mouse);
}
