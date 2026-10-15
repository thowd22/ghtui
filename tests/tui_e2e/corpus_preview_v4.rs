//! Actual binary: held real SQL autosaves, bounded wait, cancellation and restart.
use super::*;
use std::os::unix::fs::MetadataExt;

#[test]
fn cache_storage_corpus_fence_v4_keyboard_held_sql_typing_preview_wait_release_cancel_restart() {
    journey(false);
}
#[test]
fn cache_storage_corpus_fence_v4_mouse_held_sql_typing_preview_wait_release_cancel_restart() {
    journey(true);
}

fn readonly(fixture: &Fixture, arguments: &[&str]) -> Vec<u8> {
    let result = Command::new("git")
        .args(["--no-optional-locks", "-c", "diff.autoRefreshIndex=false"])
        .args(arguments)
        .current_dir(fixture.repo())
        .output()
        .unwrap();
    assert!(result.status.success());
    result.stdout
}
fn snapshot(fixture: &Fixture) -> Vec<Vec<u8>> {
    vec![
        readonly(fixture, &["rev-parse", "HEAD"]),
        readonly(fixture, &["show-ref"]),
        readonly(fixture, &["status", "--porcelain=v2", "-z"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        fs::read(fixture.repo().join("nested/日本語.txt")).unwrap(),
        fs::read(fixture.repo().join("untracked.env")).unwrap(),
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
    ]
}
fn wait_pending(session: &mut Session, fixture: &Fixture, mouse: bool, expected_reads: usize) {
    choose(session, mouse, "[Preview]", b"\x10");
    // A busy ordinary form still has a real editor caret. Require the pinned
    // backend's completed CELL frame, not an impossible cursorless marker.
    session.wait_for_owned_completed_action(
        "Manage selected repositories",
        "[Esc Cancel]",
        &["REPOSITORY PREVIEW"],
    );
    session.wait_for_screen("Waiting for accepted local draft saves");
    journal(session, fixture, expected_reads);
    session.resize(80, 24);
    session.wait_for_screen("[Esc Cancel]");
    // Busy forms must not accept newer body text or a second preview.
    session.paste("FORBIDDEN-BUSY-EDIT");
    session.send_key(b"\x10");
    session.resize(120, 40);
    session.wait_for_screen("Waiting for accepted local draft saves");
    journal(session, fixture, expected_reads);
    assert!(!session.screen().contains("FORBIDDEN-BUSY-EDIT"));
}
fn held(sql: &rusqlite::Connection, value: &str) {
    sql.execute_batch("BEGIN IMMEDIATE;").unwrap();
    assert_eq!(sql.execute("UPDATE drafts SET body=?1 WHERE host='github.invalid' AND login='user-id:42' AND resource='held-probe'", [value]).unwrap(), 1);
}
fn journey(mouse: bool) {
    let fixture = fixture();
    fs::write(
        fixture.repo().join("nested/日本語.txt"),
        b"dirty preserved original\n",
    )
    .unwrap();
    fs::write(
        fixture.repo().join("untracked.env"),
        b"private untracked canary\n",
    )
    .unwrap();
    let before = snapshot(&fixture);
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    let seeded = cache(&fixture);
    let original = scope();
    let foreign = Scope::github("github.invalid", Some(43)).unwrap();
    seeded.put_draft(&original, "held-probe", "seed").unwrap();
    seeded
        .put_draft(&foreign, "held-probe", "foreign body unchanged")
        .unwrap();
    seeded
        .save_view(
            &original,
            "Original saved view",
            &json!({"query":"original"}),
        )
        .unwrap();
    drop(seeded);
    let inode = fs::metadata(&database).unwrap();
    let mut steps = identities(8); // each menu closes its principal:4reads; ready preview:4reads
    steps.push(GhStep::read(&["--version"], "gh version 2.101.0\n"));
    steps.push(GhStep::read(
        &[
            "api",
            "user",
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ],
        &json!({"login":"alice","id":43}).to_string(),
    ));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    search(&mut session, mouse);
    manage(&mut session, mouse);
    journal(&mut session, &fixture, 4);
    let sql = rusqlite::Connection::open(&database).unwrap();
    sql.busy_timeout(Duration::from_secs(3)).unwrap();
    held(&sql, "first held transaction");
    set_field(
        &mut session,
        mouse,
        2,
        "Existing directory",
        fixture.repo().to_str().unwrap(),
    );
    wait_pending(&mut session, &fixture, mouse, 4);
    assert!(
        database
            .with_file_name("cache.sqlite3-journal")
            .metadata()
            .unwrap()
            .len()
            > 0
    );
    sql.execute_batch("COMMIT;").unwrap();
    session.wait_for_screen("REPOSITORY PREVIEW");
    session.wait_for_screen("[Confirm action]");
    journal(&mut session, &fixture, 8);
    assert!(manager(&fixture)
        .catalog(&cache(&fixture), &CancellationToken::default())
        .unwrap()
        .is_empty());
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_screen("Composer closed");
    // Reopen the same original saved body, then cancel a SECOND real held wait.
    manage(&mut session, mouse);
    journal(&mut session, &fixture, 12);
    held(&sql, "second held transaction");
    set_field(
        &mut session,
        mouse,
        2,
        "Existing directory",
        fixture.repo().to_str().unwrap(),
    );
    wait_pending(&mut session, &fixture, mouse, 12);
    choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
    session.wait_for_screen("Repository preview cancelled; no plan published");
    sql.execute_batch("COMMIT;").unwrap();
    session.wait_for_screen("[Preview]");
    session.resize(80, 24);
    session.resize(120, 40);
    session.wait_for_screen("REPOSITORY INTENT");
    assert!(!session.screen().contains("REPOSITORY PREVIEW"));
    journal(&mut session, &fixture, 12);
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_screen("Composer closed");
    drop(sql);
    let mut restarted = session.restart_with_same_cache(&fixture, 80, 24);
    restarted.wait_for_screen("Recent commits");
    search(&mut restarted, mouse);
    manage(&mut restarted, mouse);
    journal(&mut restarted, &fixture, 16);
    // Body restoration does not restore approval or a stale prepared plan.
    assert!(!restarted.screen().contains("REPOSITORY PREVIEW"));
    if mouse {
        restarted.click_visible_action("[Confirm action]");
    } else {
        restarted.focus_visible_form_action("[Confirm action]");
        restarted.send_key(b"\r");
    }
    restarted.wait_for_screen("Choose Preview and inspect the exact effect before confirmation.");
    journal(&mut restarted, &fixture, 16);
    restarted.resize(120, 40);
    field(&mut restarted, mouse, 2, "Existing directory");
    restarted.wait_for_screen(fixture.repo().to_str().unwrap());
    // A different numeric account with the SAME login may not publish the old
    // private plan, even after its original draft has genuinely become durable.
    let sql = rusqlite::Connection::open(&database).unwrap();
    held(&sql, "third held transaction");
    set_field(
        &mut restarted,
        mouse,
        2,
        "Existing directory",
        fixture.repo().to_str().unwrap(),
    );
    wait_pending(&mut restarted, &fixture, mouse, 16);
    sql.execute_batch("COMMIT;").unwrap();
    restarted.wait_for_screen("private selections hidden");
    journal(&mut restarted, &fixture, 18);
    assert!(!restarted.screen().contains("REPOSITORY PREVIEW"));
    assert!(!restarted.screen().contains("[Confirm action]"));
    assert!(!restarted.screen().contains("Manage selected repositories"));
    drop(sql);
    let cache = cache(&fixture);
    assert_eq!(
        cache.draft(&foreign, "held-probe").unwrap().as_deref(),
        Some("foreign body unchanged")
    );
    assert_eq!(
        cache.draft(&original, "held-probe").unwrap().as_deref(),
        Some("third held transaction")
    );
    assert_eq!(
        cache.saved_views(&original).unwrap()[0].0,
        "Original saved view"
    );
    assert!(manager(&fixture)
        .catalog(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
    assert_eq!(snapshot(&fixture), before);
    let after = fs::metadata(&database).unwrap();
    assert_eq!((after.dev(), after.ino()), (inode.dev(), inode.ino()));
    assert_eq!(after.mode() & 0o777, 0o600);
    journal(&mut restarted, &fixture, 18);
    restarted.quit_and_assert_restored_terminal(mouse);
}
fn scope() -> Scope {
    Scope::github("github.invalid", Some(42)).unwrap()
}
