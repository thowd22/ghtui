//! Registered exact-principal/original-receipt/retained-proof real-binary
//! keyboard/mouse journeys. Actual verification evidence lives in TASK-50;
//! focused passes do not imply complete product or platform verification.
use super::*;
use ghtui::search::{model::DiscussionKind, sources::MetadataSources};
use rusqlite::{params, Connection};
use std::time::Duration;

fn database(fixture: &Fixture) -> std::path::PathBuf {
    fixture.root.join("config/ghtui/cache.sqlite3")
}
fn selected(fixture: &Fixture) -> MetadataSources {
    let saved = cache(fixture)
        .saved_view(&scope(), "search-source-selection:v1")
        .unwrap()
        .unwrap();
    serde_json::from_value(saved["sources"].clone()).unwrap()
}
fn body_target(fixture: &Fixture) -> ghtui::search::collaboration::DiscussionTarget {
    selected(fixture)
        .discussions
        .into_iter()
        .find(|target| target.category == DiscussionKind::PullRequestBody)
        .unwrap()
}
fn canonical(session: &mut Session, mouse: bool) {
    let title = "Native hydration PR";
    if mouse {
        session.click_visible_action(title);
        session.wait_for_screen(&format!("Preview {title}"));
    } else {
        for _ in 0..3 {
            if session
                .try_wait_for(&format!("Preview {title}"), Duration::from_millis(100))
                .is_ok()
            {
                return;
            }
            session.send_key(b"\x1b[B");
        }
        session.wait_for_screen(&format!("Preview {title}"));
    }
}
fn preserve(fixture: &Fixture, head: &str, index: &[u8], status: &str) {
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        format!("{NEEDLE} local dirty working bytes\n").as_bytes()
    );
}
fn private_steps(body: &str, failure: Option<Vec<GhStep>>) -> Vec<GhStep> {
    let mut steps = gh_fixture::pr_load(&gh_fixture::pr(body), vec![], vec![], vec![], 42);
    steps.extend(gh_fixture::private_source_read(42));
    steps.extend(policy_steps(false));
    steps.extend(blend_steps(search_step(
        NEEDLE,
        vec![indexed_parent(gh_fixture::PR_NODE, 7)],
        1,
        100,
    )));
    if let Some(failure) = failure {
        steps.extend(failure);
    }
    steps
}
fn prepare_private(session: &mut Session, fixture: &Fixture, mouse: bool, body: &str) -> usize {
    start(session, mouse);
    load_parent(session, fixture, mouse, 29);
    current_parent(fixture, body);
    let mut count = 29;
    rank_off(session, fixture, mouse, &mut count);
    enable_pr_only(session, fixture, mouse, &mut count, false);
    query(session, mouse, NEEDLE);
    count += 12;
    journal(session, fixture, count);
    session.wait_for_screen("2 results • Ranked");
    session.wait_for_screen(body);
    assert_eq!(count, 59);
    count
}

#[test]
fn collaboration_index_blended_privacy_keyboard_genuine_unselected_body_exact_open() {
    unselected(false);
}
#[test]
fn collaboration_index_blended_privacy_mouse_genuine_unselected_body_exact_open() {
    unselected(true);
}
fn unselected(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let body = format!("{NEEDLE} GenuineOutsideIntentCanary");
    let mut steps = gh_fixture::pr_load(&gh_fixture::pr(&body), vec![], vec![], vec![], 42);
    steps.extend(policy_steps(false));
    steps.extend(blend_steps_selected(
        search_step(NEEDLE, vec![indexed_parent(gh_fixture::PR_NODE, 7)], 1, 100),
        false,
    ));
    steps.extend(gh_fixture::private_source_read(42));
    assert_eq!(steps.len(), 58);
    fixture.scripted_gh(&steps);
    let mut first = Session::launch(&fixture, 120, 40);
    start(&mut first, mouse);
    load_parent(&mut first, &fixture, mouse, 29);
    current_parent(&fixture, &body);
    let targets = selected(&fixture);
    assert_eq!(targets.discussions.len(), 4);
    first.quit_and_assert_restored_terminal(mouse);
    // Explicit fixture setup changes only saved selection intent. Unlike the
    // actual Remove journey, it does NOT revoke any genuine hydrated authority.
    let cache = cache(&fixture);
    let mut intent = cache
        .saved_view(&scope(), "search-source-selection:v1")
        .unwrap()
        .unwrap();
    intent["sources"] = serde_json::to_value(MetadataSources::default()).unwrap();
    cache
        .save_view(&scope(), "search-source-selection:v1", &intent)
        .unwrap();
    for target in &targets.discussions {
        assert!(cache
            .binding_status(&scope(), &target.source())
            .unwrap()
            .unwrap()
            .visible_generation
            .is_some());
    }
    drop(cache);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    choose(&mut session, mouse, "[n Ranked]", b"n");
    session.wait_for_screen("Ranked mode");
    let mut count = 29;
    enable_pr_only(&mut session, &fixture, mouse, &mut count, false);
    assert!(selected(&fixture).is_empty());
    query(&mut session, mouse, NEEDLE);
    count += 11;
    journal(&mut session, &fixture, count);
    session.wait_for_screen("2 results • Ranked");
    session.wait_for_screen("GenuineOutsideIntentCanary");
    canonical(&mut session, mouse);
    session.resize(80, 24);
    choose(&mut session, mouse, "[o Open result]", b"o");
    count += 6;
    journal(&mut session, &fixture, count);
    session.wait_for_screen("Exact search source • read-only");
    session.wait_for_screen(gh_fixture::PR_NODE);
    session.wait_for_screen("GenuineOutsideIntentCanary");
    assert!(!session.screen().contains("stale indexed fragment"));
    assert!(
        selected(&fixture).is_empty(),
        "Open does not fabricate selected source intent"
    );
    close(&mut session, mouse, "Exact search source • read-only");
    assert_eq!(count, steps.len());
    preserve(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn collaboration_index_blended_privacy_keyboard_final_uid_reuse_hides_prior_private() {
    identity_failure(false, false);
}
#[test]
fn collaboration_index_blended_privacy_mouse_final_uid_reuse_hides_prior_private() {
    identity_failure(true, false);
}
#[test]
fn collaboration_index_blended_privacy_keyboard_final_401_hides_prior_private() {
    identity_failure(false, true);
}
#[test]
fn collaboration_index_blended_privacy_mouse_final_401_hides_prior_private() {
    identity_failure(true, true);
}
fn failed_identity(authentication: bool) -> Vec<GhStep> {
    if !authentication {
        return gh_fixture::account(43);
    }
    let mut steps = gh_fixture::account(42);
    steps[1].status = 1;
    steps[1].stdout.clear();
    steps[1].stderr = b"gh: Bad credentials (HTTP 401)\n".to_vec();
    steps
}
fn identity_failure(mouse: bool, authentication: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let body = format!("{NEEDLE} PriorPrivateAccountCanary");
    let mut failed = gh_fixture::private_read(42);
    failed.push(gh_fixture::read(
        "repos/example/repo",
        gh_fixture::repository(),
    ));
    failed.push(search_step(
        NEEDLE,
        vec![indexed_parent(gh_fixture::PR_NODE, 7)],
        1,
        100,
    ));
    failed.extend([
        gh_fixture::read("repos/example/repo", gh_fixture::repository()),
        gh_fixture::read("repos/example/repo", gh_fixture::repository()),
    ]);
    failed.extend(failed_identity(authentication)); // actual worker final account.
    failed.extend(failed_identity(authentication)); // honest main failure privacy bracket.
    assert_eq!(failed.len(), 12);
    let steps = private_steps(&body, Some(failed));
    assert_eq!(steps.len(), 71);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    let mut count = prepare_private(&mut session, &fixture, mouse, &body);
    let retained = cache(&fixture)
        .list(&scope(), Some("collaboration-document-v1"))
        .unwrap();
    query(&mut session, mouse, NEEDLE);
    count += 12;
    journal(&mut session, &fixture, count);
    if authentication {
        session.wait_for_screen("Identity verification failed");
    } else {
        session.wait_for_screen("Account changed");
    }
    session.wait_for_screen("private");
    assert!(!session.screen().contains("PriorPrivateAccountCanary"));
    assert!(!session.screen().contains("stale indexed fragment"));
    assert!(!session.screen().contains("2 results • Ranked"));
    assert_eq!(cache(&fixture).list(&scope(),Some("collaboration-document-v1")).unwrap(),retained,"authentication drift hides but does not erase another original account's recoverable bytes");
    assert!(cache(&fixture)
        .list(
            &Scope::github(gh_fixture::HOST, Some(43)).unwrap(),
            Some("collaboration-document-v1")
        )
        .unwrap()
        .is_empty());
    assert_eq!(count, steps.len());
    preserve(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn collaboration_index_blended_privacy_keyboard_retained_metadata_change_before_open_hides_counts()
{
    changed_open(false);
}
#[test]
fn collaboration_index_blended_privacy_mouse_retained_metadata_change_before_open_hides_counts() {
    changed_open(true);
}
fn changed_open(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let body = format!("{NEEDLE} RetainedProofOpenCanary");
    let mut failed_open = gh_fixture::account(42);
    failed_open.push(gh_fixture::read(
        "repos/example/repo",
        gh_fixture::repository(),
    ));
    failed_open.extend(gh_fixture::account(42));
    let steps = private_steps(&body, Some(failed_open));
    assert_eq!(steps.len(), 64);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    let mut count = prepare_private(&mut session, &fixture, mouse, &body);
    canonical(&mut session, mouse);
    let target = body_target(&fixture);
    let retained = cache(&fixture)
        .list(&scope(), Some("collaboration-document-v1"))
        .unwrap();
    let sql = Connection::open(database(&fixture)).unwrap();
    let metadata:String=sql.query_row("SELECT payload FROM records WHERE host=?1 AND login=?2 AND kind='collaboration-state-v1' AND id=?3",params![scope().host,scope().login,target.source()],|row|row.get(0)).unwrap();
    let mut metadata: Value = serde_json::from_str(&metadata).unwrap();
    metadata["access"] = json!({"Unavailable":"deliberate metadata-only proof mutation"});
    metadata["complete"] = json!(false);
    metadata["enumeration_complete"] = json!(false);
    sql.execute("UPDATE records SET payload=?1,complete=0 WHERE host=?2 AND login=?3 AND kind='collaboration-state-v1' AND id=?4",params![serde_json::to_string(&metadata).unwrap(),scope().host,scope().login,target.source()]).unwrap();
    session.resize(80, 24);
    choose(&mut session, mouse, "[o Open result]", b"o");
    count += 5;
    journal(&mut session, &fixture, count);
    session.wait_for_screen("Search visibility changed");
    session.wait_for_screen("prior results hidden");
    assert!(!session.screen().contains("Exact search source • read-only"));
    assert!(!session.screen().contains("RetainedProofOpenCanary"));
    assert!(!session.screen().contains("2 results • Ranked"));
    assert_eq!(
        cache(&fixture)
            .list(&scope(), Some("collaboration-document-v1"))
            .unwrap(),
        retained
    );
    assert_eq!(count, steps.len());
    preserve(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn collaboration_index_blended_privacy_keyboard_first_source_confirmed_sql_failure_then_fresh_load()
{
    first_source_receipt(false);
}
#[test]
fn collaboration_index_blended_privacy_mouse_first_source_confirmed_sql_failure_then_fresh_load() {
    first_source_receipt(true);
}

fn pr_details(parent: &Value) -> Vec<GhStep> {
    let mut steps = vec![gh_fixture::read(
        "repos/example/repo/pulls/7",
        parent.clone(),
    )];
    for (endpoint, values) in [
        (
            "repos/example/repo/issues/7/comments?per_page=100",
            json!([]),
        ),
        ("repos/example/repo/pulls/7/commits?per_page=100", json!([])),
        (
            "repos/example/repo/pulls/7/files?per_page=100",
            json!([{"filename":"tracked.txt","status":"modified","additions":3,"deletions":1,"changes":4,"patch":"@@ -1 +1 @@\n-old\n+new"}]),
        ),
        ("repos/example/repo/pulls/7/reviews?per_page=100", json!([])),
        (
            "repos/example/repo/issues/7/timeline?per_page=100",
            json!([]),
        ),
    ] {
        let mut step = gh_fixture::read(endpoint, json!([values]));
        step.args.extend(["--paginate".into(), "--slurp".into()]);
        steps.push(step);
    }
    steps.push(gh_fixture::read(
        "repos/example/repo/pulls/7",
        parent.clone(),
    ));
    assert_eq!(steps.len(), 7);
    steps
}

fn description_publication(original: &Value, published: &Value, body: &str) -> Vec<GhStep> {
    let mut steps = gh_fixture::account(42); // durable legacy controller preflight.
    steps.push(gh_fixture::read(
        "repos/example/repo/pulls/7",
        original.clone(),
    ));
    steps.extend(gh_fixture::account(42)); // provider's exact original-target guard.
    steps.push(gh_fixture::read(
        "repos/example/repo/pulls/7",
        original.clone(),
    ));
    let mut write = gh_fixture::read("repos/example/repo/pulls/7", published.clone());
    write.args[5] = "PATCH".into();
    write.args.extend(["--input".into(), "-".into()]);
    write.stdin = serde_json::to_vec(&json!({"body":body})).unwrap();
    steps.push(write);
    steps.push(gh_fixture::read(
        "repos/example/repo/pulls/7",
        published.clone(),
    ));
    // The visibility failure happens after durable confirmation and before
    // optional PR refresh. Neither a failed refresh nor a read resends PATCH.
    assert_eq!(steps.len(), 8);
    steps
}

fn publication_commands(
    session: &mut Session,
    fixture: &Fixture,
    count: usize,
    published_body: &str,
) {
    session.wait_for_command_count(fixture, count);
    let rows = session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    assert!(rows.iter().all(|row| row["matched"] == true));
    let mutations = rows
        .iter()
        .filter(|row| {
            row["args"][0] != "--version"
                && row["args"][5] != "GET"
                && !(row["args"][1] == "graphql" && row["args"][5] == "POST")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        mutations.len(),
        1,
        "The original confirmed request is never resent"
    );
    assert_eq!(mutations[0]["args"][1], "repos/example/repo/pulls/7");
    assert_eq!(mutations[0]["args"][5], "PATCH");
    assert_eq!(mutations[0]["args"][6], "--input");
    assert_eq!(mutations[0]["args"][7], "-");
    let input = serde_json::to_vec(&json!({"body":published_body})).unwrap();
    let hex = input
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(mutations[0]["stdin_hex"], hex);
}

fn select_remote(session: &mut Session, mouse: bool) {
    let title = format!("{NEEDLE} remote parent 7");
    if mouse {
        session.click_visible_action(&title);
    } else {
        for _ in 0..3 {
            if session
                .try_wait_for(&format!("Preview {title}"), Duration::from_millis(100))
                .is_ok()
            {
                return;
            }
            session.send_key(b"\x1b[B");
        }
    }
    session.wait_for_screen(&format!("Preview {title}"));
}

fn open_description(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[p PR actions]", b"p");
    session.wait_for_screen("PR actions • native workflows");
    if mouse {
        session.click_visible_action("Edit PR description");
    } else {
        for _ in 0..7 {
            if session
                .try_wait_for("> Edit PR description", Duration::from_millis(100))
                .is_ok()
            {
                session.send_key(b"\r");
                session.wait_for_screen("Description");
                return;
            }
            session.send_key(b"\x1b[B");
        }
        panic!(
            "Exact PR description action never became selected\n{}",
            session.screen()
        );
    }
    session.wait_for_screen("Description");
}

fn confirmed_attempt(fixture: &Fixture) -> i64 {
    let sql = Connection::open(database(fixture)).unwrap();
    let (id, state, receipt, head):(i64,String,String,String) = sql.query_row(
        "SELECT attempt_id,state,confirmed_json,observed_head FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND repository=?4 AND pr_number=7 AND command='legacy-description' AND selected_resource='pr' ORDER BY attempt_id DESC LIMIT 1",
        params![scope().host,scope().login,gh_fixture::PR_NODE,gh_fixture::REPOSITORY],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
    ).unwrap();
    assert!(id > 0);
    assert_eq!(state, "confirmed");
    assert_eq!(head, gh_fixture::HEAD);
    let receipt: ghtui::pr_write_journal::PrConfirmedEffect =
        serde_json::from_str(&receipt).unwrap();
    assert_eq!(
        receipt.kind,
        ghtui::pr_write_journal::PrEffectKind::Metadata
    );
    assert_eq!(receipt.resource_id, gh_fixture::PR_NODE);
    assert_eq!(receipt.node_id.as_deref(), Some(gh_fixture::PR_NODE));
    let count:i64=sql.query_row(
        "SELECT COUNT(*) FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND repository=?4 AND pr_number=7 AND command='legacy-description' AND selected_resource='pr'",
        params![scope().host,scope().login,gh_fixture::PR_NODE,gh_fixture::REPOSITORY],
        |row|row.get(0),
    ).unwrap();
    assert_eq!(
        count, 1,
        "All recovery reads preserve the one original attempt"
    );
    id
}

fn scoped_body_bindings(sql: &Connection) -> i64 {
    sql.query_row(
        "SELECT COUNT(*) FROM sync_resource_bindings WHERE host=?1 AND login=?2 AND namespace='collaboration' AND repository=?3 AND resource=?4 AND kind='pull_request_body'",
        params![scope().host,scope().login,gh_fixture::REPOSITORY,gh_fixture::PR_NODE],
        |row|row.get(0),
    ).unwrap()
}

fn first_source_receipt(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let original = gh_fixture::pr("Original remote body before any source hydration");
    let body = format!("{NEEDLE} FirstSourceConfirmedPublishedCanary");
    let published = gh_fixture::pr(&body);
    let remote = || search_step(NEEDLE, vec![indexed_parent(gh_fixture::PR_NODE, 7)], 1, 100);
    let mut steps = policy_steps(false); // Local -> verified principal, no source Load.
    steps.extend(blend_steps_selected(remote(), false));
    steps.extend(gh_fixture::account(42));
    steps.push(gh_fixture::read(
        "repos/example/repo",
        gh_fixture::repository(),
    ));
    steps.extend(pr_details(&original));
    steps.extend(gh_fixture::account(42));
    steps.extend(description_publication(&original, &published, &body));
    steps.extend(gh_fixture::private_read(42)); // explicit GitHub scope restore, no query.
    let mut failed_blend = gh_fixture::private_read(42);
    failed_blend.push(remote());
    failed_blend.push(gh_fixture::read(
        "repos/example/repo",
        gh_fixture::repository(),
    ));
    failed_blend.extend(gh_fixture::account(42)); // main failure privacy bracket only.
    assert_eq!(failed_blend.len(), 8);
    steps.extend(failed_blend);
    steps.extend(blend_steps_selected(remote(), false));
    steps.extend(gh_fixture::pr_load(&published, vec![], vec![], vec![], 42));
    steps.extend(blend_steps(remote()));
    steps.extend(gh_fixture::private_source_read(42));
    assert_eq!(steps.len(), 113);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    choose(&mut session, mouse, "[n Ranked]", b"n");
    session.wait_for_screen("Ranked mode");
    let mut count = 0;
    enable_pr_only(&mut session, &fixture, mouse, &mut count, false);
    query(&mut session, mouse, NEEDLE);
    count += 11;
    journal(&mut session, &fixture, count);
    session.wait_for_screen("2 results • Ranked");
    select_remote(&mut session, mouse);
    choose(&mut session, mouse, "[o Open result]", b"o");
    count += 12;
    journal(&mut session, &fixture, count);
    session.wait_for_screen("Original remote body before any source hydration");
    assert!(cache(&fixture)
        .saved_view(&scope(), "search-source-selection:v1")
        .unwrap()
        .is_none());
    let sql = Connection::open(database(&fixture)).unwrap();
    assert_eq!(scoped_body_bindings(&sql), 0);
    // This is a real native PATCH below, not a fabricated confirmed marker.
    // Inject a narrowly scoped local post-receipt failure: metadata visibility
    // cannot be committed, but the original durable remote receipt survives.
    sql.execute_batch("CREATE TRIGGER fixture_fail_first_source_fence BEFORE INSERT ON sync_mutation_fences BEGIN SELECT RAISE(ABORT,'fixture first-source visibility failure'); END;").unwrap();
    open_description(&mut session, mouse);
    session.send_key(b"\x01");
    session.paste(&body);
    session.wait_for_screen("FirstSourceConfirmedPublishedCanary");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW • Confirm action");
    session.wait_for_screen("PR metadata preview");
    journal(&mut session, &fixture, count);
    assert_eq!(
        session.inspect_command_journal(&fixture).lines().count(),
        count
    );
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.focus_visible_form_action("[Confirm action]");
        session.send_key(b"\r");
    }
    count += 8;
    publication_commands(&mut session, &fixture, count, &body);
    // The gh journal records invocation/readback, not completion of the worker's
    // durable receipt transaction. This unique prepared-result prefix proves
    // the confirmed worker and local persistence finished before inspecting SQL.
    session.wait_for_screen("Published • private pages hidden • do not resend");
    let attempt = confirmed_attempt(&fixture);
    assert_eq!(scoped_body_bindings(&sql), 0);
    session.wait_for_screen("Edit PR description");
    session.wait_for_screen("refresh failed");
    session.wait_for_screen("private pages hidden");
    assert!(!session.screen().contains("[Submit]"));
    assert!(!session.screen().contains("[Confirm action]"));
    assert!(!session.screen().contains("example/repo#7"));
    assert!(!session
        .screen()
        .contains("Original remote body before any source hydration"));
    assert!(!session
        .screen()
        .contains("FirstSourceConfirmedPublishedCanary"));
    session.resize(80, 24);
    session.wait_for_screen("Published • private pages hidden • do not resend");
    assert!(!session.screen().contains("example/repo#7"));
    assert!(!session.screen().contains("[Submit]"));
    assert!(!session.screen().contains("[Confirm action]"));
    publication_commands(&mut session, &fixture, count, &body);
    session.resize(120, 40);
    session.wait_for_screen("Published • private pages hidden • do not resend");
    choose(&mut session, mouse, "3 Search", b"3");
    session.wait_for_screen("Selected local search corpus ready");
    choose(&mut session, mouse, "[h GitHub]", b"h");
    count += 4;
    publication_commands(&mut session, &fixture, count, &body);
    session.wait_for_screen("GitHub scope verified");
    choose(&mut session, mouse, "[n Ranked]", b"n");
    session.wait_for_screen("Ranked mode");
    query(&mut session, mouse, NEEDLE);
    count += 8;
    publication_commands(&mut session, &fixture, count, &body);
    session.wait_for_screen("Search failed • prior results hidden");
    assert!(!session.screen().contains("stale indexed fragment"));
    assert!(!session.screen().contains("2 results • Ranked"));
    assert_eq!(confirmed_attempt(&fixture), attempt);
    assert_eq!(scoped_body_bindings(&sql), 0);
    sql.execute_batch("DROP TRIGGER fixture_fail_first_source_fence;")
        .unwrap();
    query(&mut session, mouse, NEEDLE);
    count += 11;
    publication_commands(&mut session, &fixture, count, &body);
    session.wait_for_screen("1 results • Ranked");
    assert!(!session.screen().contains("stale indexed fragment"));
    assert_eq!(confirmed_attempt(&fixture), attempt);
    let ordinal:i64=sql.query_row(
        "SELECT mutation_ordinal FROM sync_mutation_fences WHERE host=?1 AND login=?2 AND namespace='collaboration' AND repository=?3 AND resource=?4 AND kind='pull_request_body'",
        params![scope().host,scope().login,gh_fixture::REPOSITORY,gh_fixture::PR_NODE],
        |row|row.get(0),
    ).unwrap();
    assert_eq!(ordinal, attempt);
    assert_eq!(
        scoped_body_bindings(&sql),
        0,
        "First-source tombstone works without a prior source binding"
    );
    choose(&mut session, mouse, "[H Search sources]", b"H");
    session.wait_for_screen("Search sources • explicit read-only hydration");
    choose(&mut session, mouse, "[p PR]", b"p");
    choose(&mut session, mouse, "[v PR/issue number]", b"v");
    session.send_key(b"\x01");
    session.paste("7");
    session.send_key(b"\r");
    choose(&mut session, mouse, "[l Load source]", b"l");
    count += 29;
    publication_commands(&mut session, &fixture, count, &body);
    session.wait_for_screen("Loaded 1 cached discussion records");
    close(
        &mut session,
        mouse,
        "Search sources • explicit read-only hydration",
    );
    current_parent(&fixture, &body);
    assert_eq!(confirmed_attempt(&fixture), attempt);
    query(&mut session, mouse, NEEDLE);
    count += 12;
    publication_commands(&mut session, &fixture, count, &body);
    session.wait_for_screen("2 results • Ranked");
    session.wait_for_screen("FirstSourceConfirmedPublishedCanary");
    assert!(!session.screen().contains("stale indexed fragment"));
    canonical(&mut session, mouse);
    session.resize(80, 24);
    choose(&mut session, mouse, "[o Open result]", b"o");
    count += 6;
    publication_commands(&mut session, &fixture, count, &body);
    session.wait_for_screen("Exact search source • read-only");
    session.wait_for_screen("FirstSourceConfirmedPublishedCanary");
    session.wait_for_screen(gh_fixture::PR_NODE);
    close(&mut session, mouse, "Exact search source • read-only");
    assert_eq!(confirmed_attempt(&fixture), attempt);
    assert_eq!(count, steps.len());
    preserve(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
}
