//! Real native label publication followed by SQL visibility failure. Cached
//! queries repair original receipts before any original viewer is opened.
use super::*;
use crate::hydration_fixture as hydration;

fn hydrate(p: &Value) -> Vec<GhStep> {
    let mut out = bound(hydration::pr_load(p, vec![], vec![], vec![], 42), p);
    for step in &mut out {
        if step.args.get(1).is_some_and(|e| e == "graphql") {
            let mut input: Value = serde_json::from_slice(&step.stdin).unwrap();
            input["variables"]["id"] = p["node_id"].clone();
            step.stdin = serde_json::to_vec(&input).unwrap();
            let mut data: Value = serde_json::from_slice(&step.stdout).unwrap();
            data["data"]["node"]["id"] = p["node_id"].clone();
            step.stdout = serde_json::to_vec(&data).unwrap();
        }
    }
    out
}
fn account(p: &Value) -> Vec<GhStep> {
    bound(identity(), p)
}
fn query_steps(p: &Value, success: bool) -> Vec<GhStep> {
    let mut out = account(p);
    out.push(values::get("repos/example/repo", values::repository()));
    if success {
        out.push(values::get("repos/example/repo", values::repository()));
    }
    out.extend(account(p));
    out
}
fn query(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[/ Query]", b"/");
    session.send_key(b"\x01");
    session.paste("LabelQueryCanary type:pr label:\"existing ☃\"");
    choose(session, mouse, "[Enter Search]", b"\r");
}
fn load(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[H Search sources]", b"H");
    session.wait_for_screen("Search sources • explicit read-only hydration");
    choose(session, mouse, "[p PR]", b"p");
    choose(session, mouse, "[r Repository]", b"r");
    session.send_key(b"\x01");
    session.paste("example/repo");
    session.send_key(b"\r");
    choose(session, mouse, "[v PR/issue number]", b"v");
    session.send_key(b"\x01");
    session.paste("7");
    session.send_key(b"\r");
    choose(session, mouse, "[l Load source]", b"l");
    session.wait_for_screen("Loaded 1 cached discussion records");
    choose(session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close("Search sources • explicit read-only hydration");
}
fn bindings(f: &Fixture) -> Vec<(String, Option<u64>)> {
    let sql = Connection::open(cache_path(f)).unwrap();
    let mut stmt=sql.prepare("SELECT kind,visible_generation FROM sync_resource_bindings WHERE host='github.invalid' AND login='user-id:42' AND repository='example/repo' AND resource='PR_7' ORDER BY kind").unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

#[test]
fn pr_metadata_actions_label_filter_repair_keyboard_query_before_original_view_repairs_sql_failure_without_resend(
) {
    label_query(false);
}
#[test]
fn pr_metadata_actions_label_filter_repair_mouse_query_before_original_view_repairs_sql_failure_without_resend(
) {
    label_query(true);
}
fn label_query(mouse: bool) {
    let f = fixture();
    let command = PrMetadataCommand::RemoveLabel;
    let mut p = current_parent();
    p["body"] = json!("LabelQueryCanary unchanged retained canonical discussion body");
    p["url"] = json!("https://github.invalid/api/v3/repos/example/repo/pulls/7");
    let mut changed = after(command);
    changed["body"] = p["body"].clone();
    changed["url"] = p["url"].clone();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let mut s = startup(&p);
    s.extend(hydrate(&p));
    s.extend(query_steps(&p, true));
    s.extend(account(&p));
    s.extend(account(&p));
    s.extend(source(command, &p));
    s.extend(account(&p));
    s.extend(values::preparation_from(command, &p));
    s.extend(values::publication_from(
        command,
        &changed,
        values::mutation_input(command),
    ));
    s.extend(account(&p));
    s.extend(query_steps(&p, false));
    s.extend(query_steps(&p, true));
    assert_eq!(s.len(), 100);
    f.scripted_gh(&s);
    let mut session = Session::launch(&f, 120, 40);
    open_pr(&mut session, mouse);
    choose(&mut session, mouse, "3 Search", b"3");
    session.wait_for_screen("Selected local search corpus ready");
    load(&mut session, mouse);
    records(&mut session, &f, 47);
    query(&mut session, mouse);
    records(&mut session, &f, 53);
    session.wait_for_screen("1 results • Exact");
    session.wait_for_screen("LabelQueryCanary");
    let stored = Cache::open(&cache_path(&f))
        .unwrap()
        .list(&scope(), Some("collaboration-document-v1"))
        .unwrap();
    assert!(stored.iter().any(|r| r.payload["body"] == p["body"]));
    let before = bindings(&f);
    assert_eq!(before.len(), 4);
    assert!(before.iter().all(|(_, generation)| generation.is_some()));
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Your repository permissions:");
    open_form(&mut session, mouse, command);
    choose_value(&mut session, &f, mouse, command, 67);
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("Exact effect: Remove label");
    Connection::open(cache_path(&f)).unwrap().execute_batch("CREATE TRIGGER metadata_label_fence_fails BEFORE INSERT ON sync_mutation_fences BEGIN SELECT RAISE(ABORT,'fixture actual label visibility failure'); END;").unwrap();
    confirm(&mut session, mouse);
    records(&mut session, &f, 89);
    session.wait_for_screen("Published • private pages hidden • do not resend");
    session.wait_for_screen("[View original]");
    assert_receipt(&f, command, &changed);
    assert_eq!(bindings(&f), before);
    assert!(!session.screen().contains("LabelQueryCanary"));
    // Do not open View original: Search itself must reconcile the durable
    // receipt/fence and fail closed while the real SQLite trigger remains.
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_overlay_close("┌ Remove label ");
    choose(&mut session, mouse, "3 Search", b"3");
    session.wait_for_screen("[H Search sources]");
    query(&mut session, mouse);
    records(&mut session, &f, 94);
    session.wait_for_screen("prior results hidden");
    assert!(
        !session.screen().contains("LabelQueryCanary unchanged")
            && !session.screen().contains("1 results • Exact")
    );
    assert_eq!(bindings(&f), before);
    assert_eq!(
        Cache::open(&cache_path(&f))
            .unwrap()
            .list(&scope(), Some("collaboration-document-v1"))
            .unwrap(),
        stored
    );
    Connection::open(cache_path(&f))
        .unwrap()
        .execute_batch("DROP TRIGGER metadata_label_fence_fails")
        .unwrap();
    query(&mut session, mouse);
    records(&mut session, &f, 100);
    session.wait_for_screen("0 results • Exact");
    session.wait_for_screen("PARTIAL");
    assert!(bindings(&f)
        .iter()
        .all(|(_, generation)| generation.is_none()));
    assert_eq!(
        Cache::open(&cache_path(&f))
            .unwrap()
            .list(&scope(), Some("collaboration-document-v1"))
            .unwrap(),
        stored
    );
    assert_eq!(mutation_count(&records(&mut session, &f, 100)), 1);
    assert_receipt(&f, command, &changed);
    preserve_git(&f, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
}
