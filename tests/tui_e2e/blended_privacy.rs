//! Registered exact-principal/original-receipt/retained-proof real-binary
//! keyboard/mouse journeys. Actual verification evidence lives in TASK-50;
//! focused passes do not imply complete product or platform verification.
use super::*;
use ghtui::search::{model::DiscussionKind, sources::MetadataSources};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

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
    // Actual Open9, Preview9 and Confirm pre9 share the SAME current Core
    // producer, not the retired legacy3+3 protocol or local-only Preview.
    let mut steps = description_phase_v51(original);
    steps.extend(description_phase_v51(original));
    steps.extend(description_phase_v51(original));
    let mut write = gh_fixture::read("repos/example/repo/pulls/7", published.clone());
    write.args[5] = "PATCH".into();
    write.args.extend(["--input".into(), "-".into()]);
    write.stdin = serde_json::to_vec(&json!({"body":body})).unwrap();
    steps.push(write);
    steps.push(gh_fixture::read(
        "repos/example/repo/pulls/7",
        published.clone(),
    ));
    steps.push(gh_fixture::read("user", json!({"id":42,"login":"octocat"})));
    steps.push(gh_fixture::read(
        "repos/example/repo",
        gh_fixture::repository(),
    ));
    steps.extend(pr_details(published));
    steps.push(gh_fixture::read("user", json!({"id":42,"login":"octocat"})));
    // The exact real SQL insertion trigger fails AFTER actual paid visibility
    // and durable confirmation. It never changes the remote effect or resends.
    assert_eq!(steps.len(), 39);
    steps
}
const DESCRIPTION_QUERY_V51:&str="query($id:ID!){node(id:$id){__typename ... on PullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged viewerCanUpdate repository{id nameWithOwner}}}}";
fn description_phase_v51(parent: &Value) -> Vec<GhStep> {
    let mut calls = gh_fixture::account(42);
    calls.push(gh_fixture::read(
        "repos/example/repo",
        gh_fixture::repository(),
    ));
    calls.push(gh_fixture::read(
        "repos/example/repo/pulls/7",
        parent.clone(),
    ));
    let mut gql = gh_fixture::read(
        "graphql",
        json!({"data":{"node":{"__typename":"PullRequest","id":gh_fixture::PR_NODE,"fullDatabaseId":"700","number":7,"headRefOid":gh_fixture::HEAD,"baseRefOid":gh_fixture::BASE,"headRefName":"feature","baseRefName":"main","state":"OPEN","isDraft":false,"merged":false,"viewerCanUpdate":true,"repository":{"id":"R_hydration_410","nameWithOwner":gh_fixture::REPOSITORY}}}}),
    );
    gql.args[5] = "POST".into();
    gql.args.extend(["--input".into(), "-".into()]);
    gql.stdin = serde_json::to_vec(
        &json!({"query":DESCRIPTION_QUERY_V51,"variables":{"id":gh_fixture::PR_NODE}}),
    )
    .unwrap();
    calls.push(gql);
    calls.push(gh_fixture::read(
        "repos/example/repo/pulls/7",
        parent.clone(),
    ));
    calls.push(gh_fixture::read(
        "repos/example/repo",
        gh_fixture::repository(),
    ));
    calls.extend(gh_fixture::account(42));
    assert_eq!(calls.len(), 9);
    calls
}
fn canonical_first_source_fixture_v51(steps: &mut [GhStep]) {
    fn canonical(value: &mut Value) {
        match value {
            Value::Object(map) => {
                if map.get("id") == Some(&json!(410))
                    && map.get("full_name") == Some(&json!(gh_fixture::REPOSITORY))
                {
                    map.insert("node_id".into(), json!("R_hydration_410"));
                    map.insert("owner".into(), json!({"id":71,"node_id":"O_hydration_71","login":"example","type":"Organization"}));
                }
                if map.get("node_id") == Some(&json!(gh_fixture::PR_NODE))
                    && map.contains_key("head")
                    && map.contains_key("base")
                {
                    map.insert("merged".into(), json!(false));
                    let mut repo = gh_fixture::repository();
                    repo["node_id"] = json!("R_hydration_410");
                    map.get_mut("head").unwrap()["repo"] = repo.clone();
                    map.get_mut("base").unwrap()["repo"] = repo;
                }
                for child in map.values_mut() {
                    canonical(child);
                }
            }
            Value::Array(values) => {
                for child in values {
                    canonical(child);
                }
            }
            _ => {}
        }
    }
    for step in steps {
        if step.args == ["--version"] {
            step.stdout = b"gh version 2.99.0 (2026-10-01)\n".to_vec();
            continue;
        }
        if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            canonical(&mut value);
            step.stdout = serde_json::to_vec(&value).unwrap();
        }
    }
}

fn publication_commands(
    session: &mut Session,
    fixture: &Fixture,
    count: usize,
    published_body: &str,
    expected: &[GhStep],
) {
    session.wait_for_command_count(fixture, count);
    let rows = session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    assert!(rows.iter().all(|row| row["matched"] == true));
    assert_eq!(expected.len(), count);
    for (n, (row, step)) in rows.iter().zip(expected).enumerate() {
        assert_eq!(row["step"], n);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
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

fn open_description(session: &mut Session, fixture: &Fixture, mouse: bool, end: usize) {
    let before = session.completed_cells_end();
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
                break;
            }
            session.send_key(b"\x1b[B");
        }
    }
    session.wait_for_command_count(fixture, end);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_end() > before
            && session.completed_cells_screen().is_some_and(|frame| {
                crate::support::overlay_body(frame, "Edit PR Description • example/repo#7")
                    .is_some_and(|body| {
                        [
                            "Description:",
                            "[Preview]",
                            "[Confirm action]",
                            "[Keep draft / Close]",
                        ]
                        .iter()
                        .all(|label| body.contains(label))
                    })
            })
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "Actual completed Metadata Form after current Open9\n{}",
            session.screen()
        );
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MetadataDatabaseV51 {
    path: Vec<u8>,
    device: u64,
    inode: u64,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MetadataSeedV51 {
    version: u8,
    database: MetadataDatabaseV51,
    scope: Scope,
    repository_id: u64,
    repository_node: String,
    repository: String,
    pr_id: u64,
    pr_node: String,
    number: u64,
    head: String,
    base: String,
    field: ghtui::providers::gh::PrMetadataTextFieldV25,
    before_sha256: String,
    requested_sha256: String,
    requester: String,
    author_uid: Option<u64>,
}
fn confirmed_attempt(fixture: &Fixture) -> i64 {
    let sql = Connection::open(database(fixture)).unwrap();
    let (id, state, receipt, head):(i64,String,String,String) = sql.query_row(
        "SELECT attempt_id,state,confirmed_json,observed_head FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND repository=?4 AND pr_number=7 AND command='metadata-description-v25' AND selected_resource LIKE 'metadata-text-v25/%' ORDER BY attempt_id DESC LIMIT 1",
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
    let origin: String = sql
        .query_row(
            "SELECT origin_json FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    let origin: Value = serde_json::from_str(&origin).unwrap();
    assert_eq!(origin["seed"]["version"], 25);
    assert_eq!(origin["seed"]["field"], "description");
    assert_eq!(origin["seed"]["repository_id"], 410);
    assert_eq!(origin["seed"]["pr_id"], 700);
    assert_eq!(origin["seed"]["author_uid"], 42);
    let expected_body = format!("{NEEDLE} FirstSourceConfirmedPublishedCanary");
    let digest = format!("{:x}", Sha256::digest(expected_body.as_bytes()));
    assert_eq!(origin["seed"]["requested_sha256"], digest);
    assert_eq!(
        origin["seed"]["before_sha256"],
        format!(
            "{:x}",
            Sha256::digest(b"Original remote body before any source hydration")
        )
    );
    let key = format!(
        "metadata-text-v25/{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                scope(),
                410u64,
                "R_hydration_410",
                700u64,
                gh_fixture::PR_NODE,
                7u64,
                gh_fixture::HEAD,
                gh_fixture::BASE,
                ghtui::providers::gh::PrMetadataTextFieldV25::Description
            ))
            .unwrap()
        )
    );
    let (selected, payload): (String, String) = sql
        .query_row(
            "SELECT selected_resource,payload_sha256 FROM pr_write_attempts WHERE attempt_id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(selected, key);
    let seed: MetadataSeedV51 = serde_json::from_value(origin["seed"].clone()).unwrap();
    let origin_digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&seed).unwrap()));
    assert_eq!(
        payload,
        ghtui::pr_write_journal::PrWriteIntent {
            command: "metadata-description-v25".into(),
            selected_resource: key,
            observed_head: gh_fixture::HEAD.into(),
            values: BTreeMap::from([
                ("body".into(), expected_body),
                ("origin-v25".into(), origin_digest)
            ])
        }
        .digest()
        .unwrap()
    );
    assert_eq!(
        receipt.resource_id,
        format!(
            "metadata-text-v25/description/{}",
            origin["seed"]["requested_sha256"].as_str().unwrap()
        )
    );
    assert_eq!(receipt.node_id.as_deref(), Some(gh_fixture::PR_NODE));
    let count:i64=sql.query_row(
        "SELECT COUNT(*) FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND repository=?4 AND pr_number=7 AND command='metadata-description-v25' AND selected_resource LIKE 'metadata-text-v25/%'",
        params![scope().host,scope().login,gh_fixture::PR_NODE,gh_fixture::REPOSITORY],
        |row|row.get(0),
    ).unwrap();
    assert_eq!(
        count, 1,
        "All recovery reads preserve the one original attempt"
    );
    id
}
fn confirm_description_v51(session: &mut Session, mouse: bool) {
    const TITLE: &str = "Edit PR Description • example/repo#7";
    const CONTROLS: &[&str] = &[
        "[Preview]",
        "[External editor]",
        "[Confirm action]",
        "[Keep draft / Close]",
    ];
    if !mouse {
        session.focus_completed_owned_action_v1(
            TITLE,
            "[Confirm action]",
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            CONTROLS,
            &[],
        );
    }
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        let point = session.completed_cells_screen().and_then(|frame| {
            if !crate::support::overlay_body(frame, TITLE).is_some_and(|body| {
                CONTROLS.iter().all(|label| body.contains(label))
                    && body.contains("READ-ONLY PREVIEW")
            }) {
                return None;
            }
            crate::support::completed_navigation_controls_v1::owned_action_position(
                frame,
                TITLE,
                "[Confirm action]",
                &[],
            )
            .filter(|(x, y)| {
                frame
                    .cell(*y, *x)
                    .is_some_and(|cell| cell.underline() && (mouse || cell.inverse()))
            })
        });
        if let Some((x, y)) = point {
            if mouse {
                session.click(x, y);
            } else {
                session.send_key(b"\r");
            }
            return;
        }
        assert!(
            Instant::now() < until,
            "Actual enabled topmost Metadata Confirm\n{}",
            session.screen()
        );
    }
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
    assert_eq!(steps.len(), 144);
    canonical_first_source_fixture_v51(&mut steps);
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
    open_description(&mut session, &fixture, mouse, count + 9);
    count += 9;
    session.send_key(b"\x01");
    session.paste(&body);
    session.wait_for_screen("FirstSourceConfirmedPublishedCanary");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    count += 9;
    session.wait_for_command_count(&fixture, count);
    session.wait_for_screen("READ-ONLY PREVIEW • Confirm action");
    session.wait_for_screen("PR metadata preview");
    journal(&mut session, &fixture, count);
    assert_eq!(
        session.inspect_command_journal(&fixture).lines().count(),
        count
    );
    confirm_description_v51(&mut session, mouse);
    count += 21;
    publication_commands(&mut session, &fixture, count, &body, &steps[..count]);
    // The gh journal records invocation/readback, not completion of the worker's
    // durable receipt transaction. This unique prepared-result prefix proves
    // the confirmed worker and local persistence finished before inspecting SQL.
    session.wait_for_screen("Confirmed; visibility unavailable; no resend");
    let attempt = confirmed_attempt(&fixture);
    assert_eq!(scoped_body_bindings(&sql), 0);
    session.wait_for_screen("Original Conversation retired");
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
    session.wait_for_screen("Confirmed; visibility unavailable; no resend");
    assert!(!session.screen().contains("example/repo#7"));
    assert!(!session.screen().contains("[Submit]"));
    assert!(!session.screen().contains("[Confirm action]"));
    publication_commands(&mut session, &fixture, count, &body, &steps[..count]);
    session.resize(120, 40);
    session.wait_for_screen("Confirmed; visibility unavailable; no resend");
    let before_search_return = session.completed_cells_end();
    choose(&mut session, mouse, "3 Search", b"3");
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_end() > before_search_return
            && session.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, "Search • focused view")
                        .is_some_and(|body| {
                            [
                                "[e Exact]", "[x Regex]", "[n Ranked]", "[h GitHub]", "[Enter Search]",
                                "[w Working files]", "[c Corpus]", "[b Blend GitHub: on]",
                                "2 results • Ranked • searched source snapshot",
                                "COMPLETE within selected scope • 1 documents across 1 repositories",
                                "GitHub PRs • COMPLETE • 1 admitted", "[o Open result]",
                            ].iter().all(|label| body.contains(label))
                                && body.contains(&format!("[/ Query] {NEEDLE}"))
                                && body.contains(&format!("> {NEEDLE} remote parent 7"))
                        })
                    && text.contains("Search • focused view")
            })
        {
            break;
        }
        assert!(Instant::now() < until, "NEW completed original ranked Search source/query/selected remote parent after confirmed privacy retirement\n{}", session.screen());
    }
    choose(&mut session, mouse, "[h GitHub]", b"h");
    count += 4;
    publication_commands(&mut session, &fixture, count, &body, &steps[..count]);
    session.wait_for_screen("GitHub scope verified");
    choose(&mut session, mouse, "[n Ranked]", b"n");
    session.wait_for_screen("Ranked mode");
    query(&mut session, mouse, NEEDLE);
    count += 8;
    publication_commands(&mut session, &fixture, count, &body, &steps[..count]);
    session.wait_for_screen("Search failed • prior results hidden");
    assert!(!session.screen().contains("stale indexed fragment"));
    assert!(!session.screen().contains("2 results • Ranked"));
    assert_eq!(confirmed_attempt(&fixture), attempt);
    assert_eq!(scoped_body_bindings(&sql), 0);
    sql.execute_batch("DROP TRIGGER fixture_fail_first_source_fence;")
        .unwrap();
    query(&mut session, mouse, NEEDLE);
    count += 11;
    publication_commands(&mut session, &fixture, count, &body, &steps[..count]);
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
    publication_commands(&mut session, &fixture, count, &body, &steps[..count]);
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
    publication_commands(&mut session, &fixture, count, &body, &steps[..count]);
    session.wait_for_screen("2 results • Ranked");
    session.wait_for_screen("FirstSourceConfirmedPublishedCanary");
    assert!(!session.screen().contains("stale indexed fragment"));
    canonical(&mut session, mouse);
    session.resize(80, 24);
    choose(&mut session, mouse, "[o Open result]", b"o");
    count += 6;
    publication_commands(&mut session, &fixture, count, &body, &steps[..count]);
    session.wait_for_screen("Exact search source • read-only");
    session.wait_for_screen("FirstSourceConfirmedPublishedCanary");
    session.wait_for_screen(gh_fixture::PR_NODE);
    close(&mut session, mouse, "Exact search source • read-only");
    assert_eq!(confirmed_attempt(&fixture), attempt);
    assert_eq!(count, steps.len());
    preserve(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
}
