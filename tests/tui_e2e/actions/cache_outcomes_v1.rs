//! TASK-58: a cache that changed before deletion is refused without DELETE and a refresh
//! shows the current list; a denied deletion explains the missing permission and keeps
//! the cache. Keyboard and mouse journeys, strict offline fixtures, no Git changes.
use super::*;

fn cache() -> Value {
    json!({"id":71,"key":"cargo/linux/v1","ref":"refs/heads/main","version":"v1","size_in_bytes":2048,"created_at":"2026-10-12T12:00:00Z","last_accessed_at":"2026-10-13T12:00:00Z"})
}
fn other() -> Value {
    json!({"id":72,"key":"docs/index","ref":"refs/heads/docs","version":"v2","size_in_bytes":4096,"created_at":"2026-10-11T12:00:00Z","last_accessed_at":"2026-10-12T12:00:00Z"})
}
fn catalogue(caches: Vec<Value>, bytes: u64) -> [GhStep; 2] {
    let count = caches.len();
    [
        read(
            "repos/example/repo/actions/caches?per_page=100",
            json!([{ "actions_caches": caches }]),
            true,
        ),
        read(
            "repos/example/repo/actions/cache/usage",
            json!({"active_caches_count":count,"active_caches_size_in_bytes":bytes}),
            false,
        ),
    ]
}
/// Open, preview, confirm: the guard re-reads the exact cache before any DELETE.
fn delete_steps(steps: &mut Vec<GhStep>, exact: Vec<Value>) {
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.extend([
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
        read(
            "repos/example/repo/actions/caches?per_page=100&key=cargo%2Flinux%2Fv1&ref=refs%2Fheads%2Fmain",
            json!([{ "actions_caches": exact }]),
            true,
        ),
    ]);
}
fn open_caches(session: &mut Session, mouse: bool) {
    session.wait_for_screen("ghtui");
    choose(session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    choose(session, mouse, "Caches", b"c");
    session.wait_for_screen("#71 • cargo/linux/v1");
    session.wait_for_screen("#72 • docs/index");
}
fn delete_selected(session: &mut Session, mouse: bool) {
    command(session, mouse, "Delete selected cache");
    session.wait_for_screen("ACTIONS INTENT");
    choose(session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    confirm(session, mouse, 1);
}
fn no_delete_and_clean(rows: &[Value], fixture: &Fixture) {
    assert!(rows.iter().all(|row| row["args"][5] != "DELETE"));
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
}

#[test]
fn actions_cache_keyboard_stale_target_refused_without_delete_then_refresh() {
    stale(false)
}
#[test]
fn actions_cache_mouse_stale_target_refused_without_delete_then_refresh() {
    stale(true)
}
fn stale(mouse: bool) {
    let fixture = fixture();
    let mut steps = initial();
    steps.extend(catalogue(vec![cache(), other()], 6144));
    let listed = steps.len();
    // Another client already removed cache 71: the exact re-read is empty.
    delete_steps(&mut steps, vec![]);
    // The principal is re-read after every outcome, refused or not.
    steps.extend(current_account());
    let refused = steps.len();
    steps.extend(current_account());
    steps.extend(catalogue(vec![other()], 4096));
    let refreshed = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_caches(&mut session, mouse);
    journal(&mut session, &fixture, listed);
    delete_selected(&mut session, mouse);
    session.wait_for_screen("Selected cache changed or disappeared");
    let rows = journal(&mut session, &fixture, refused);
    no_delete_and_clean(&rows, &fixture);
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert!(
        durable.iter().all(|row| row.state != "confirmed"),
        "a refused stale deletion is never recorded as confirmed"
    );
    // Refresh reloads the exact current list and usage.
    close_form(&mut session, mouse);
    if mouse {
        session.click_visible_action("[r Refresh]");
    } else {
        session.send_key(b"r");
    }
    session.wait_for_screen("4096 bytes in 1 caches");
    session.wait_for_screen("#72 • docs/index");
    assert!(!session.screen().contains("#71 • cargo/linux/v1"));
    let rows = journal(&mut session, &fixture, refreshed);
    no_delete_and_clean(&rows, &fixture);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actions_cache_keyboard_denied_delete_explains_permission_and_keeps_cache() {
    denied(false)
}
#[test]
fn actions_cache_mouse_denied_delete_explains_permission_and_keeps_cache() {
    denied(true)
}
fn denied(mouse: bool) {
    let fixture = fixture();
    let mut steps = initial();
    steps.extend(catalogue(vec![cache(), other()], 6144));
    let listed = steps.len();
    delete_steps(&mut steps, vec![cache()]);
    let mut deletion = write("repos/example/repo/actions/caches/71", None, Value::Null);
    deletion.args[5] = "DELETE".into();
    deletion.status = 1;
    deletion.stdout.clear();
    deletion.stderr = b"gh: Resource not accessible by integration (HTTP 403)".to_vec();
    steps.push(deletion);
    steps.extend(current_account());
    let denied = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_caches(&mut session, mouse);
    journal(&mut session, &fixture, listed);
    delete_selected(&mut session, mouse);
    session.wait_for_screen("Cache deletion denied");
    let rows = journal(&mut session, &fixture, denied);
    let writes = rows
        .iter()
        .filter(|row| row["args"][5] == "DELETE")
        .collect::<Vec<_>>();
    assert_eq!(
        writes.len(),
        1,
        "the denied DELETE is sent once, never retried"
    );
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert!(durable.iter().all(|row| row.state != "confirmed"));
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    // The unknown outcome keeps its ORIGINAL ATTEMPT form open; close it before quitting.
    close_form(&mut session, mouse);
    session.quit_and_assert_restored_terminal(mouse);
}
