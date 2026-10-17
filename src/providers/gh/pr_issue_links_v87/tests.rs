//! Real original Personal factory/strict installed-gh contracts. Pure decoder
//! negatives below supplement, not replace, these genuine Core producers.
use super::*;
use crate::{
    personal_pr_entry_v13::tests as personal,
    pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
    scoped_pr_collaboration_source_v17::issue_links_phase_v87::{self, IssueLinkOpenV87},
    test_support::GhStep,
};
use std::{collections::BTreeMap, fs};
#[path = "publication_tests.rs"]
mod publication_v87;
#[path = "recovery_tests.rs"]
mod recovery_v87;
#[path = "ui_tests.rs"]
mod ui_v87;

fn scalar(name: &str, required: bool) -> Value {
    let scalar = json!({"kind":"SCALAR","name":name,"ofType":null});
    if required {
        json!({"kind":"NON_NULL","name":null,"ofType":scalar})
    } else {
        scalar
    }
}
fn schema() -> Value {
    let fields = json!([
        {"name":"issueId","type":scalar("ID",true)},
        {"name":"pullRequestIds","type":{"kind":"NON_NULL","name":null,"ofType":{"kind":"LIST","name":null,"ofType":scalar("ID",true)}}},
        {"name":"clientMutationId","type":scalar("String",false)}
    ]);
    let args = |reverse: bool| {
        let mut args = vec![
            json!({"name":"first","type":scalar("Int",false)}),
            json!({"name":"userLinkedOnly","type":scalar("Boolean",false)}),
            json!({"name":"excludeUserLinked","type":scalar("Boolean",false)}),
        ];
        if reverse {
            args.push(json!({"name":"includeClosedPrs","type":scalar("Boolean",false)}));
        }
        args
    };
    json!({"data":{"__schema":{"mutationType":{"fields":[{"name":"addCloseIssueReferences"},{"name":"removeCloseIssueReferences"}]}},
        "add":{"kind":"INPUT_OBJECT","inputFields":fields},"remove":{"kind":"INPUT_OBJECT","inputFields":fields},
        "pr":{"fields":[{"name":"closingIssuesReferences","args":args(false)}]},
        "issue":{"fields":[{"name":"closedByPullRequestsReferences","args":args(true)}]}}})
}
fn gql_repository() -> Value {
    json!({"__typename":"Repository","id":"R_10000","databaseId":10000,"nameWithOwner":personal::REPO,"isArchived":false,"viewerPermission":"WRITE"})
}
fn issue(number: u64) -> Value {
    json!({"__typename":"Issue","id":format!("ISSUE_{}",20000+number),"fullDatabaseId":20000+number,"number":number,
        "url":format!("https://github.invalid/{}/issues/{number}",personal::REPO),"title":format!("Actual Issue {number}"),"state":"OPEN","viewerCanUpdate":true,"repository":gql_repository()})
}
fn connection(rows: Vec<Value>) -> Value {
    json!({"totalCount":rows.len(),"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":rows})
}
fn pr_node() -> Value {
    json!({"__typename":"PullRequest","id":"PERSONAL_PR_1007","fullDatabaseId":1007,"number":7,
        "url":format!("https://github.invalid/{}/pull/7",personal::REPO),"title":"Actual personal source","body":personal::BODY,
        "headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"headRefName":"feature/personal","baseRefName":"main",
        "state":"OPEN","isDraft":false,"merged":false,"viewerCanUpdate":true,"repository":gql_repository(),
        "all":connection(vec![]),"manual":connection(vec![]),"nonmanual":connection(vec![])})
}
fn catalogue(rows: Vec<Value>, more: bool, cursor: Option<&str>) -> Value {
    let mut repository = gql_repository();
    repository["issues"] = json!({"totalCount":if more{200}else{rows.len()},"pageInfo":{"hasNextPage":more,"endCursor":cursor},"nodes":rows});
    json!({"data":{"repository":repository}})
}
fn graphql(query: &str, variables: Value, response: Value) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            "graphql",
            "--hostname",
            "github.invalid",
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &response.to_string(),
    );
    step.stdin = serde_json::to_vec(&json!({"query":query,"variables":variables})).unwrap();
    step
}
fn open_steps(page: Value, pr: Value, closing: Value) -> Vec<GhStep> {
    let mut calls = personal::principal();
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        personal::repository(),
    ));
    calls.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        personal::parent(),
    ));
    calls.push(graphql(protocol::CAPABILITY_QUERY_V87, json!({}), schema()));
    calls.push(graphql(
        &protocol::catalogue_query(),
        json!({"owner":"example","name":"repo0000","after":null}),
        page,
    ));
    calls.push(graphql(
        &protocol::pr_links_query(),
        json!({"pr":"PERSONAL_PR_1007"}),
        json!({"data":{"pr":pr}}),
    ));
    calls.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        closing,
    ));
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        personal::repository(),
    ));
    calls.extend(personal::principal());
    assert_eq!(calls.len(), 11);
    calls
}
fn writable_repository() -> Value {
    let mut repository = personal::repository();
    repository["permissions"] = json!({"pull":true,"push":true,"maintain":false,"admin":false});
    repository
}
fn pair(mut pr: Value, mut selected: Value) -> Value {
    for key in ["all", "manual", "nonmanual"] {
        if pr.get(key).is_none() {
            pr[key] = connection(vec![]);
        }
        if selected.get(key).is_none() {
            selected[key] = connection(vec![]);
        }
    }
    json!({"data":{"pr":pr,"issue":selected}})
}
fn selected_steps(observation: Value) -> Vec<GhStep> {
    let mut calls = personal::principal();
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    calls.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        personal::parent(),
    ));
    calls.push(graphql(protocol::CAPABILITY_QUERY_V87, json!({}), schema()));
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    calls.push(graphql(
        &protocol::pair_query(),
        json!({"pr":"PERSONAL_PR_1007","issue":"ISSUE_20009"}),
        observation.clone(),
    ));
    calls.push(graphql(
        &protocol::issue_query(),
        json!({"issue":"ISSUE_20009"}),
        json!({"data":{"issue":issue(9)}}),
    ));
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    calls.push(graphql(
        &protocol::pair_query(),
        json!({"pr":"PERSONAL_PR_1007","issue":"ISSUE_20009"}),
        observation,
    ));
    calls.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        personal::parent(),
    ));
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    calls.extend(personal::principal());
    assert_eq!(calls.len(), 14);
    calls
}
fn reservations(s: &personal::Setup) -> BTreeMap<(String, String, String), (i64, i64)> {
    let db = rusqlite::Connection::open_with_flags(
        personal::database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement=db.prepare("SELECT host,login,source,generation,active FROM sync_reservations ORDER BY host,login,source").unwrap();
    let rows = statement
        .query_map([], |r| {
            Ok(((r.get(0)?, r.get(1)?, r.get(2)?), (r.get(3)?, r.get(4)?)))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    drop(statement);
    rows
}
fn schema_sql(s: &personal::Setup) -> Vec<(String, String, String)> {
    let db = rusqlite::Connection::open_with_flags(
        personal::database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement = db
        .prepare("SELECT type,name,coalesce(sql,'') FROM sqlite_master ORDER BY type,name")
        .unwrap();
    let rows = statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    drop(statement);
    rows
}
fn effects(s: &personal::Setup) {
    transport_and_git(s);
    assert_eq!(personal::sql(&s.fixture), s.before_sql);
}
fn transport_and_git(s: &personal::Setup) {
    transport_and_git_protocol(s, None);
}
fn transport_and_git_protocol(s: &personal::Setup, mutation: Option<IssueLinkCommandV87>) {
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), s.steps.len());
    let mut mutations = 0;
    for (row, step) in rows.iter().zip(&s.steps) {
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["matched"], true);
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        assert!(row["pid"].as_u64().is_some_and(|id| id > 0));
        if !step.stdin.is_empty() {
            let payload: Value = serde_json::from_slice(&step.stdin).unwrap();
            let query = payload["query"].as_str().unwrap();
            if query.starts_with("mutation ") {
                let expected = mutation.expect("Read-only contract must not contain a mutation");
                assert_eq!(query, protocol::mutation_query(expected));
                assert_eq!(
                    payload["variables"]["input"],
                    json!({"issueId":"ISSUE_20009","pullRequestIds":["PERSONAL_PR_1007"]})
                );
                mutations += 1;
            } else {
                assert!(query.starts_with("query "));
            }
        }
    }
    assert_eq!(mutations, usize::from(mutation.is_some()));
    for (name, expected) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(name)).unwrap(), expected);
    }
    let head = std::process::Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(s.fixture.repo())
        .args(["rev-parse", "HEAD"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(head.stdout, s.git[0]);
}

fn selected_seed_v87(
    s: &personal::Setup,
) -> (
    Arc<crate::scoped_pr_collaboration_source_v17::PrCollaborationSourceV17>,
    Arc<OriginalIssueLinksV87>,
    IssueLinkPlanV87,
    crate::pr_write_journal::issue_link_origin_v87::IssueLinkOriginSeedV87,
) {
    let PreparedV87 {
        source,
        actual,
        plan,
        seed,
        ..
    } = selected_bundle_v87(s);
    (source, actual, plan, seed)
}
struct PreparedV87 {
    _catalogue: Arc<IssueLinkCatalogueV87>,
    source: Arc<crate::scoped_pr_collaboration_source_v17::PrCollaborationSourceV17>,
    actual: Arc<OriginalIssueLinksV87>,
    plan: IssueLinkPlanV87,
    seed: crate::pr_write_journal::issue_link_origin_v87::IssueLinkOriginSeedV87,
    choice: IssueLinkChoiceV87,
}
fn selected_bundle_v87(s: &personal::Setup) -> PreparedV87 {
    use crate::pr_write_journal::issue_link_origin_v87::IssueLinkOriginSeedV87;
    let (source, catalogue, ()) = open(s).unwrap();
    let choice = catalogue.choose(0).unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + std::time::Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let ((actual, plan, _), seed) = source
        .with_issue_link_phase_v87(
            issue_links_phase_v87::IssueLinkPhaseRequestV87 {
                command: IssueLinkCommandV87::LinkIssue,
                choice: &choice,
                retained: None,
                purpose: issue_links_phase_v87::IssueLinkPhasePurposeV87::Preview,
            },
            &provider,
            &personal::database(&s.fixture),
            &cancel,
            deadline,
            |phase| {
                let seed = IssueLinkOriginSeedV87::from_producer(
                    &source,
                    phase.actual(),
                    phase.plan(),
                    &personal::database(&s.fixture),
                )
                .map_err(|_| refused(GithubErrorKind::InvalidTarget))?;
                let local = phase.local(&personal::database(&s.fixture))?;
                let closed = phase.finish(|| {
                    local.validate_boundary_v13(
                        &personal::database(&s.fixture),
                        source.local_boundary(),
                    )
                })?;
                Ok((closed, seed))
            },
        )
        .unwrap();
    PreparedV87 {
        _catalogue: catalogue,
        source,
        actual,
        plan,
        seed,
        choice,
    }
}
fn seed_setup_v87() -> personal::Setup {
    let mut calls = open_steps(
        catalogue(vec![issue(9)], false, None),
        pr_node(),
        personal::parent(),
    );
    calls.extend(selected_steps(pair(pr_node(), issue(9))));
    personal::setup(calls)
}

#[test]
fn pr_metadata_issue_link_v87_actual_live_origin_and_same_transaction_claim_preserve_foreign_drafts_and_git(
) {
    use crate::pr_write_journal::{PrWriteJournal, PrWriteState, PrWriteTarget};
    let s = seed_setup_v87();
    let (source, actual, plan, seed) = selected_seed_v87(&s);
    let before_reservations = reservations(&s);
    let values = seed.values().unwrap();
    let write = seed.write(&values).unwrap();
    let target = PrWriteTarget::for_source(
        source.opening().scope(),
        actual.parent().repository(),
        actual.parent().node_id(),
        actual.parent().number(),
    )
    .unwrap();
    let mut journal = PrWriteJournal::open(&personal::database(&s.fixture)).unwrap();
    let permit = journal
        .begin_issue_link_v87(&target, &write, &seed)
        .unwrap();
    let pending = journal.blocking(&target).unwrap().unwrap();
    assert_eq!(pending.command, "issue-link-v87");
    assert_eq!(pending.selected_resource, seed.draft_key());
    assert_eq!(pending.payload_sha256, write.digest().unwrap());
    assert_eq!(pending.state, PrWriteState::Pending);
    let record = journal.issue_link_origin_v87(&target, &pending).unwrap();
    let raw = serde_json::to_value(&record).unwrap();
    assert_eq!(raw["seed"]["issue_id"], 20009);
    assert_eq!(raw["seed"]["issue_node"], "ISSUE_20009");
    assert_eq!(raw["seed"]["pr_id"], 1007);
    assert_eq!(raw["seed"]["repository_id"], 10000);
    assert_eq!(raw["seed"]["issue_repository_id"], 10000);
    assert_eq!(raw["seed"]["plan_sha256"], plan.digest());
    assert_eq!(raw["seed"]["issue_latest"], Value::Null);
    journal.claim(&permit, &target, &write).unwrap();
    assert!(
        journal.claim(&permit, &target, &write).is_err(),
        "Actual permit is one-take"
    );
    journal.mark_uncertain(&permit).unwrap();
    let current = journal.blocking(&target).unwrap().unwrap();
    assert_eq!(current.state, PrWriteState::Uncertain);
    journal.issue_link_origin_v87(&target, &current).unwrap();
    let db = rusqlite::Connection::open_with_flags(
        personal::database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM drafts", [], |r| r.get::<_, usize>(0))
            .unwrap(),
        2
    );
    for uid in [42, 43] {
        assert_eq!(db.query_row("SELECT body FROM drafts WHERE host='github.invalid' AND login=?1 AND resource='kept'",[format!("user-id:{uid}")],|r|r.get::<_,String>(0)).unwrap(),format!("PRIVATE_{uid}"));
    }
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, usize>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_issue_link_origins_v87",
            [],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(reservations(&s), before_reservations);
    transport_and_git(&s);
}

#[test]
fn pr_metadata_issue_link_v87_failed_origin_insert_rolls_back_whole_begin_and_keeps_full_sql() {
    use crate::pr_write_journal::{PrWriteJournal, PrWriteTarget};
    let s = seed_setup_v87();
    let (source, actual, _, seed) = selected_seed_v87(&s);
    let target = PrWriteTarget::for_source(
        source.opening().scope(),
        actual.parent().repository(),
        actual.parent().node_id(),
        actual.parent().number(),
    )
    .unwrap();
    let mut journal = PrWriteJournal::open(&personal::database(&s.fixture)).unwrap();
    let db = rusqlite::Connection::open(personal::database(&s.fixture)).unwrap();
    db.execute_batch("CREATE TRIGGER fail_owned_issue_origin BEFORE INSERT ON pr_write_issue_link_origins_v87 BEGIN SELECT RAISE(ABORT,'owned insert fault');END;").unwrap();
    let rows = personal::sql(&s.fixture);
    let schema = schema_sql(&s);
    let reservation = reservations(&s);
    let write = seed.write(&seed.values().unwrap()).unwrap();
    assert!(journal
        .begin_issue_link_v87(&target, &write, &seed)
        .is_err());
    assert!(journal.blocking(&target).unwrap().is_none());
    assert_eq!(personal::sql(&s.fixture), rows);
    assert_eq!(schema_sql(&s), schema);
    assert_eq!(reservations(&s), reservation);
    transport_and_git(&s);
}

#[test]
fn pr_metadata_issue_link_v87_known_purpose_has_no_originless_or_foreign_database_fallback() {
    use crate::pr_write_journal::{PrWriteJournal, PrWriteTarget};
    let s = seed_setup_v87();
    let (source, actual, _, seed) = selected_seed_v87(&s);
    let target = PrWriteTarget::for_source(
        source.opening().scope(),
        actual.parent().repository(),
        actual.parent().node_id(),
        actual.parent().number(),
    )
    .unwrap();
    let write = seed.write(&seed.values().unwrap()).unwrap();
    let mut journal = PrWriteJournal::open(&personal::database(&s.fixture)).unwrap();
    let before = personal::sql(&s.fixture);
    assert!(journal.begin(&target, &write).is_err());
    assert_eq!(personal::sql(&s.fixture), before);
    let foreign = s.fixture.root.join("foreign-cache.sqlite3");
    let mut other = PrWriteJournal::open(&foreign).unwrap();
    assert!(other.begin_issue_link_v87(&target, &write, &seed).is_err());
    assert!(other.blocking(&target).unwrap().is_none());
    assert_eq!(personal::sql(&s.fixture), before);
    transport_and_git(&s);
}

#[test]
fn pr_metadata_issue_link_v87_completed_original_cannot_mint_another_live_seed_or_read() {
    use crate::pr_write_journal::issue_link_origin_v87::IssueLinkOriginSeedV87;
    let s = seed_setup_v87();
    let (source, actual, plan, _) = selected_seed_v87(&s);
    assert!(actual.binding.measured.accounting().unwrap().sealed());
    let before = personal::sql(&s.fixture);
    assert!(IssueLinkOriginSeedV87::from_producer(
        &source,
        &actual,
        &plan,
        &personal::database(&s.fixture)
    )
    .is_err());
    assert_eq!(personal::sql(&s.fixture), before);
    effects(&s);
}

#[test]
fn pr_metadata_issue_link_v87_origin_is_immutable_and_changed_values_cannot_claim_a_real_permit() {
    use crate::pr_write_journal::{PrWriteJournal, PrWriteTarget};
    let s = seed_setup_v87();
    let (source, actual, _, seed) = selected_seed_v87(&s);
    let target = PrWriteTarget::for_source(
        source.opening().scope(),
        actual.parent().repository(),
        actual.parent().node_id(),
        actual.parent().number(),
    )
    .unwrap();
    let write = seed.write(&seed.values().unwrap()).unwrap();
    let mut journal = PrWriteJournal::open(&personal::database(&s.fixture)).unwrap();
    let permit = journal
        .begin_issue_link_v87(&target, &write, &seed)
        .unwrap();
    let before = personal::sql(&s.fixture);
    let schema = schema_sql(&s);
    let db = rusqlite::Connection::open(personal::database(&s.fixture)).unwrap();
    let error = db
        .execute(
            "UPDATE pr_write_issue_link_origins_v87 SET issue_id=20010 WHERE attempt_id=?1",
            [permit.marker().attempt_id],
        )
        .unwrap_err();
    assert_eq!(error.sqlite_error().unwrap().extended_code, 1811);
    assert_eq!(personal::sql(&s.fixture), before);
    assert_eq!(schema_sql(&s), schema);
    let mut changed = write.clone();
    changed.values.insert(
        "body".into(),
        "same number is not the selected actual Issue".into(),
    );
    assert!(journal.claim(&permit, &target, &changed).is_err());
    assert_eq!(personal::sql(&s.fixture), before);
    journal.claim(&permit, &target, &write).unwrap();
    transport_and_git(&s);
}
fn open(
    s: &personal::Setup,
) -> GithubResult<(
    Arc<crate::scoped_pr_collaboration_source_v17::PrCollaborationSourceV17>,
    Arc<IssueLinkCatalogueV87>,
    (),
)> {
    let original = PrCollaborationOpenIntentV17::capture_issue_links_v87(
        &s.app,
        &personal::database(&s.fixture),
        IssueLinkCommandV87::LinkIssue,
    )
    .unwrap();
    let bounded = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    issue_links_phase_v87::open_issue_links_v87(
        IssueLinkOpenV87 {
            opening: Arc::clone(&original),
            command: IssueLinkCommandV87::LinkIssue,
            repository: None,
            max_pages: 1,
        },
        &bounded,
        &personal::database(&s.fixture),
        original.cancellation(),
        original.deadline(),
        |catalogue, _| {
            assert!(catalogue.supported());
            assert!(catalogue.retained_bound_v87() < MODEL_CAP_V87);
            Ok(())
        },
    )
}

#[test]
fn pr_metadata_issue_link_v87_actual_personal_core_chooser_keeps_original_page_and_exact_read_only_effects(
) {
    let s = personal::setup(open_steps(
        catalogue(vec![issue(9)], false, None),
        pr_node(),
        personal::parent(),
    ));
    let before_schema = schema_sql(&s);
    let before_reservations = reservations(&s);
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    let body = page.details.summary.pr.body.as_ptr();
    let rows = page.rows.as_ptr();
    let (source, catalogue, _) = open(&s).unwrap();
    assert_eq!(source.parent().database_id(), 1007);
    assert_eq!(source.identity().account_id, Some(42));
    assert!(source.owns_presentation(&s.app));
    assert_eq!(catalogue.len(), 1);
    assert!(catalogue.complete());
    let row = catalogue.row_v87(0).unwrap();
    assert_eq!(row.id, 20009);
    assert_eq!(row.node, "ISSUE_20009");
    assert!(row.enabled);
    assert_eq!(row.selected_observations, Some([false; 3]));
    assert!(catalogue.choose(0).is_ok());
    assert!(catalogue.choose(1).is_err());
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    assert_eq!(page.details.summary.pr.body.as_ptr(), body);
    assert_eq!(page.rows.as_ptr(), rows);
    let mut expected = before_reservations;
    let scope = crate::storage::Scope::github("github.invalid", Some(42)).unwrap();
    let encoded = serde_json::to_vec(&(
        &scope,
        10000u64,
        "R_10000",
        1007u64,
        "PERSONAL_PR_1007",
        7u64,
    ))
    .unwrap();
    let key = (
        "github.invalid".into(),
        "user-id:42".into(),
        format!("personal-pr-source-v13/{:x}", Sha256::digest(encoded)),
    );
    let generation = expected.get(&key).map_or(1, |(old, _)| old + 1);
    expected.insert(key, (generation, 1));
    assert_eq!(
        reservations(&s),
        expected,
        "Only the genuine exact Personal reservation is accepted; no omitted SQL table"
    );
    assert_eq!(schema_sql(&s), before_schema);
    effects(&s);
}
#[test]
fn pr_metadata_issue_link_v87_catalogue_choice_rejects_foreign_repository_without_new_effects() {
    let s = personal::setup(open_steps(
        catalogue(vec![issue(9)], false, None),
        pr_node(),
        personal::parent(),
    ));
    let (_, mut catalogue, ()) = open(&s).unwrap();
    let sql = personal::sql(&s.fixture);
    let reservations = reservations(&s);
    let schema = schema_sql(&s);
    assert!(catalogue.choose(0).is_ok());
    for field in 0..3 {
        {
            let actual = Arc::get_mut(&mut catalogue).unwrap();
            match field {
                0 => actual.repository.id += 1,
                1 => actual.repository.node.push_str("_foreign"),
                _ => actual.repository.name.push_str("-foreign"),
            }
        }
        assert!(catalogue.choose(0).is_err());
        {
            let actual = Arc::get_mut(&mut catalogue).unwrap();
            match field {
                0 => actual.repository.id -= 1,
                1 => actual.repository.node.truncate("R_10000".len()),
                _ => actual.repository.name.truncate(personal::REPO.len()),
            }
        }
        assert!(catalogue.choose(0).is_ok());
    }
    assert_eq!(personal::sql(&s.fixture), sql);
    assert_eq!(self::reservations(&s), reservations);
    assert_eq!(schema_sql(&s), schema);
    transport_and_git(&s);
}

#[test]
fn pr_metadata_issue_link_v87_partial_chooser_keeps_known_rows_but_does_not_claim_complete_membership(
) {
    let s = personal::setup(open_steps(
        catalogue(vec![issue(9)], true, Some("next-page-2")),
        pr_node(),
        personal::parent(),
    ));
    let (_, catalogue, _) = open(&s).unwrap();
    assert!(!catalogue.complete());
    assert_eq!(catalogue.len(), 1);
    assert!(catalogue.choose(0).is_ok());
    effects(&s);
}
#[test]
fn pr_metadata_issue_link_v87_actual_closing_body_null_and_head_drift_refuse_before_final_principal(
) {
    for null_body in [true, false] {
        let mut closing = personal::parent();
        if null_body {
            closing["body"] = Value::Null;
        } else {
            closing["head"]["sha"] = json!("c".repeat(40));
        }
        let mut calls = open_steps(catalogue(vec![issue(9)], false, None), pr_node(), closing);
        calls.truncate(8);
        let s = personal::setup(calls);
        assert!(open(&s).is_err());
        effects(&s);
    }
}
#[test]
fn pr_metadata_issue_link_v87_fixed_ledgers_preserve_original_eight_mib_and_exact_role_counts() {
    assert_eq!(ISSUE_LINK_PRE_CAPS_V87.iter().sum::<usize>(), 2208 * 1024);
    assert_eq!(ISSUE_LINK_POST_CAPS_V87.iter().sum::<usize>(), 4864 * 1024);
    assert_eq!(
        ISSUE_LINK_METADATA_POST_CAPS_V87.iter().sum::<usize>(),
        5376 * 1024
    );
    assert_eq!(
        ISSUE_LINK_REFRESH_CAPS_V87.iter().sum::<usize>(),
        4768 * 1024
    );
    assert_eq!(
        ISSUE_LINK_METADATA_REFRESH_CAPS_V87.iter().sum::<usize>(),
        5280 * 1024
    );
    assert_eq!(
        ISSUE_LINK_PRE_CAPS_V87.len() + ISSUE_LINK_POST_CAPS_V87.len(),
        29
    );
    assert_eq!(
        ISSUE_LINK_PRE_CAPS_V87.len() + ISSUE_LINK_METADATA_POST_CAPS_V87.len(),
        33
    );
    let combined = ISSUE_LINK_PRE_CAPS_V87
        .iter()
        .chain(ISSUE_LINK_METADATA_POST_CAPS_V87.iter())
        .sum::<usize>();
    assert_eq!(combined, 7584 * 1024);
    assert!(combined < 8 * 1024 * 1024);
}
#[test]
fn pr_metadata_issue_link_v87_decoder_charges_live_keys_nodes_arrays_and_retains_inherited_without_reset(
) {
    for bytes in [
        br#"{"same":1,"same":2}"#.as_slice(),
        br#"{"errors":null,"data":{}}"#.as_slice(),
    ] {
        let mut heap = IssueLinkHeapV87::new(1234).unwrap();
        let reservation = heap.reserve_document(SCALAR_CAP_V87).unwrap();
        match heap.decode(reservation, bytes) {
            Ok(doc) => {
                assert!(protocol::error_free(doc.value()).is_err());
            }
            Err(error) => assert!(matches!(error.kind, GithubErrorKind::Json)),
        }
    }
    let mut heap = IssueLinkHeapV87::new(1234).unwrap();
    let reservation = heap.reserve_document(SCALAR_CAP_V87).unwrap();
    let doc = heap
        .decode(reservation, br#"{"data":{"values":[1,2,3]}}"#)
        .unwrap();
    heap.charge(2048).unwrap();
    heap.release_document(doc).unwrap();
    assert_eq!(heap.retained().unwrap(), 1234 + 2048);
    let mut heap = IssueLinkHeapV87::new(MODEL_CAP_V87 - 3 * SCALAR_CAP_V87).unwrap();
    let reservation = heap.reserve_document(SCALAR_CAP_V87).unwrap();
    assert!(
        heap.decode(reservation, br#"{}"#).is_err(),
        "Raw scratch fit alone does NOT admit a Value node"
    );
}
#[test]
fn pr_metadata_issue_link_v87_capability_requires_both_exact_inputs_and_all_fixed_read_arguments() {
    let mut heap = IssueLinkHeapV87::new(0).unwrap();
    assert!(IssueLinkCapabilityV87::parse(&schema(), &mut heap)
        .unwrap()
        .supported());
    for fault in 0..5 {
        let mut raw = schema();
        match fault {
            0 => raw["data"]["add"]["inputFields"][1]["type"] = scalar("ID", true),
            1 => raw["data"]["remove"]["inputFields"]
                .as_array_mut()
                .unwrap()
                .push(json!({"name":"requiredUnknown","type":scalar("String",true)})),
            2 => {
                raw["data"]["issue"]["fields"][0]["args"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            3 => {
                raw["data"]["__schema"]["mutationType"]["fields"] =
                    json!([{"name":"addCloseIssueReferences"}])
            }
            _ => raw["errors"] = Value::Null,
        }
        let mut heap = IssueLinkHeapV87::new(0).unwrap();
        assert!(match IssueLinkCapabilityV87::parse(&raw, &mut heap) {
            Ok(support) => !support.supported(),
            Err(_) => true,
        });
    }
}
#[test]
fn pr_metadata_issue_link_v87_selected_preview_uses_exact_fourteen_children_and_keeps_actual_source_and_null_facts(
) {
    let mut calls = open_steps(
        catalogue(vec![issue(9)], false, None),
        pr_node(),
        personal::parent(),
    );
    calls.extend(selected_steps(pair(pr_node(), issue(9))));
    let s = personal::setup(calls);
    let (source, catalogue, _) = open(&s).unwrap();
    let before = reservations(&s);
    let choice = catalogue.choose(0).unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + std::time::Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let (actual, plan, identity) = source
        .with_issue_link_phase_v87(
            issue_links_phase_v87::IssueLinkPhaseRequestV87 {
                command: IssueLinkCommandV87::LinkIssue,
                choice: &choice,
                retained: None,
                purpose: issue_links_phase_v87::IssueLinkPhasePurposeV87::Preview,
            },
            &provider,
            &personal::database(&s.fixture),
            &cancel,
            deadline,
            |phase| {
                assert_eq!(phase.actual().parent().database_id(), 1007);
                assert_eq!(phase.actual().pair.issue.id, 20009);
                assert!(!phase.actual().opening.body_null());
                let local = phase.local(&personal::database(&s.fixture))?;
                phase.finish(|| {
                    local.validate_boundary_v13(
                        &personal::database(&s.fixture),
                        source.local_boundary(),
                    )
                })
            },
        )
        .unwrap();
    assert_eq!(identity.account_id, Some(42));
    assert_eq!(plan.command(), IssueLinkCommandV87::LinkIssue);
    assert_eq!(plan.digest().len(), 64);
    assert!(
        actual.binding.measured.accounting().unwrap().sealed(),
        "Successful same Core closes/aborts original READ pool"
    );
    assert!(
        actual.prepare_requested().is_err(),
        "Closed observation cannot issue another live plan/read"
    );
    provider
        .validate_original_issue_link_generations_v87(&actual, &cancel, deadline)
        .unwrap();
    assert_eq!(
        reservations(&s),
        before,
        "Fresh selected phase retains SAME source reservation, no rebase"
    );
    effects(&s);
}
#[test]
fn pr_metadata_issue_link_v87_six_sets_are_not_a_partition_and_unrelated_row_drift_is_not_a_receipt(
) {
    let mut calls = open_steps(
        catalogue(vec![issue(9)], false, None),
        pr_node(),
        personal::parent(),
    );
    calls.extend(selected_steps(pair(pr_node(), issue(9))));
    let s = personal::setup(calls);
    let (source, catalogue, _) = open(&s).unwrap();
    let choice = catalogue.choose(0).unwrap();
    let token = CancellationToken::default();
    let deadline = Instant::now() + std::time::Duration::from_secs(30);
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    source
        .with_issue_link_phase_v87(
            issue_links_phase_v87::IssueLinkPhaseRequestV87 {
                command: IssueLinkCommandV87::LinkIssue,
                choice: &choice,
                retained: None,
                purpose: issue_links_phase_v87::IssueLinkPhasePurposeV87::Preview,
            },
            &api,
            &personal::database(&s.fixture),
            &token,
            deadline,
            |phase| {
                let actual = phase.actual();
                let mut next = pair(pr_node(), issue(9));
                // Same selected relation can appear in ALL THREE actual sets. This is
                // observed overlap, not a guessed disjoint manual/keyword partition.
                for key in ["all", "manual", "nonmanual"] {
                    next["data"]["pr"][key] = connection(vec![issue(9)]);
                    next["data"]["issue"][key] = connection(vec![pr_node()]);
                }
                let mut heap = IssueLinkHeapV87::new(actual.retained).unwrap();
                let observed = IssueLinkPairV87::parse(
                    &next,
                    &actual.opening,
                    &actual.pair.issue,
                    &api.host,
                    &mut heap,
                )?;
                actual
                    .pair
                    .confirms(&observed, actual.parent(), IssueLinkCommandV87::LinkIssue)?;
                let mut other = next;
                other["data"]["pr"]["nonmanual"] = connection(vec![issue(9), issue(10)]);
                let mut heap = IssueLinkHeapV87::new(actual.retained).unwrap();
                let observed = IssueLinkPairV87::parse(
                    &other,
                    &actual.opening,
                    &actual.pair.issue,
                    &api.host,
                    &mut heap,
                )?;
                assert!(actual
                    .pair
                    .confirms(&observed, actual.parent(), IssueLinkCommandV87::LinkIssue)
                    .is_err());
                phase.finish(|| Ok(()))
            },
        )
        .unwrap();
    effects(&s);
}
