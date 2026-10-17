//! Genuine retained Personal Summary -> SAME original Core/raw parent. These
//! contracts never reconstruct a source or use a JSON fixture as permission.
use super::*;
use crate::providers::gh::{
    GithubResult, OriginalMetadataTextV25, PrMetadataTextFieldV25 as Field,
    METADATA_TEXT_CAPABILITY_QUERY_V25,
};
const CURRENT: &str = "Fresh raw description, not the cached summary body 界";
const REQUESTED: &str = "One explicit changed metadata value";
#[path = "scoped_pr_metadata_context_v51_tests.rs"]
mod metadata_context_v51;
#[path = "scoped_pr_metadata_publication_v25_tests.rs"]
mod publication;

fn capability(allowed: Value) -> Value {
    json!({"data":{"node":{"__typename":"PullRequest","id":"PERSONAL_PR_1007",
        "fullDatabaseId":"1007","number":7,"headRefOid":"a".repeat(40),
        "baseRefOid":"b".repeat(40),"headRefName":"feature/personal","baseRefName":"main",
        "state":"OPEN","isDraft":false,"merged":false,"viewerCanUpdate":allowed,
        "repository":{"id":"R_10000","nameWithOwner":REPO}}}})
}
fn capability_step(value: Value) -> GhStep {
    let mut step = get("graphql", value);
    *step.args.iter_mut().find(|s| s.as_str() == "GET").unwrap() = "POST".into();
    step.args.extend(["--input".into(), "-".into()]);
    step.stdin = serde_json::to_vec(&json!({"query":METADATA_TEXT_CAPABILITY_QUERY_V25,
        "variables":{"id":"PERSONAL_PR_1007"}}))
    .unwrap();
    step
}
fn phase(raw: Value, capability: Value) -> Vec<GhStep> {
    let mut steps = parent_phase();
    steps[3].stdout = serde_json::to_vec(&raw).unwrap();
    steps[4].stdout = serde_json::to_vec(&raw).unwrap();
    steps.insert(4, capability_step(capability));
    assert_eq!(steps.len(), 9, "six original Core + three measured reads");
    steps
}
fn raw(body: Value) -> Value {
    let mut parent = parent();
    parent["body"] = body;
    parent
}
fn bounded(s: &Setup, original: &PrCollaborationOpenIntentV17, cap: usize) -> GithubProvider {
    s.api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), cap)
        .unwrap()
}
type Opened = (
    Arc<source::PrCollaborationSourceV17>,
    OriginalMetadataTextV25,
    (String, bool),
);
fn open(s: &Setup, field: Field) -> GithubResult<Opened> {
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    open_captured(s, original, field)
}
fn open_captured(
    s: &Setup,
    original: Arc<PrCollaborationOpenIntentV17>,
    field: Field,
) -> GithubResult<Opened> {
    source::open_metadata_text_v25(
        original.clone(),
        field,
        &bounded(s, &original, 1024 * 1024),
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
        |actual, _| {
            actual.validate_live_source_v25()?;
            Ok((
                actual.observed().value().to_owned(),
                actual.observed().explicitly_null(),
            ))
        },
        |_| Ok(()),
    )
}
fn unchanged(s: &Setup, count: usize) {
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    for (row, expected) in rows.iter().zip(&s.steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(expected.args));
        assert_eq!(
            row["stdin_hex"],
            expected
                .stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        assert!(!expected
            .args
            .iter()
            .any(|v| matches!(v.as_str(), "PATCH" | "PUT" | "DELETE")));
        assert!(!expected.stdin.windows(8).any(|v| v == b"mutation"));
    }
    assert_eq!(sql(&s.fixture), s.before_sql);
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (path, expected) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(path)).unwrap(), expected);
    }
}

#[test]
fn pr_metadata_original_v25_actual_raw_body_not_stripped_summary_and_exact_nine_children() {
    let s = setup(phase(raw(json!(CURRENT)), capability(json!(true))));
    let original_allocations = allocations(&s.app);
    let (source, actual, prepared) = open(&s, Field::Description).unwrap();
    assert_eq!(prepared, (CURRENT.into(), false));
    assert_ne!(actual.observed().value(), BODY);
    assert!(actual.can_update());
    assert_eq!(actual.identity().account_id, Some(42));
    assert_eq!(source.parent().database_id(), 1007);
    assert_eq!(source.parent().repository_id(), 10000);
    assert_eq!(allocations(&s.app), original_allocations);
    assert_eq!(
        s.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .body,
        BODY
    );
    assert!(!format!("{actual:?}").contains(CURRENT));
    assert!(
        actual.validate_live_source_v25().is_err(),
        "closed initial pool is not a fresh producer seed"
    );
    unchanged(&s, s.steps.len());
}

#[test]
fn pr_metadata_original_v25_explicit_null_and_empty_are_observed_missing_or_wrong_type_refuses() {
    for (value, explicitly_null) in [(Value::Null, true), (json!(""), false)] {
        let s = setup(phase(raw(value), capability(json!(true))));
        let (_, actual, prepared) = open(&s, Field::Description).unwrap();
        assert_eq!(prepared, (String::new(), explicitly_null));
        assert_eq!(actual.observed().value(), "");
        unchanged(&s, s.steps.len());
    }
    for malformed in [json!(42), json!({}), json!(["body"])] {
        let s = setup(phase(raw(malformed), capability(json!(true))));
        assert!(open(&s, Field::Description).is_err());
        unchanged(&s, opening().len() + 4);
    }
    let mut missing = parent();
    missing.as_object_mut().unwrap().remove("body");
    let s = setup(phase(missing, capability(json!(true))));
    assert!(open(&s, Field::Description).is_err());
    unchanged(&s, opening().len() + 4);
}

fn nullable_closing_v26(before: Value, closing: Value, expected: Option<bool>) {
    let mut tail = phase(raw(before), capability(json!(true)));
    tail[5].stdout = serde_json::to_vec(&raw(closing)).unwrap();
    let s = setup(tail);
    let page = allocations(&s.app);
    let retained_body = s
        .app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .details
        .summary
        .pr
        .body
        .clone();
    let actual = open(&s, Field::Description);
    let count = if let Some(explicitly_null) = expected {
        let (_, actual, prepared) = actual.unwrap();
        assert_eq!(prepared, (String::new(), explicitly_null));
        assert_eq!(actual.observed().value(), "");
        assert_eq!(actual.observed().explicitly_null(), explicitly_null);
        s.steps.len()
    } else {
        assert!(
            actual.is_err(),
            "A closing null/string change is not the same raw observation"
        );
        opening().len() + 6
    };
    assert_eq!(allocations(&s.app), page);
    assert_eq!(
        s.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .body,
        retained_body
    );
    assert!(s.app.form.is_none());
    assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    // Complete private SQL includes drafts and publication attempts. Exact
    // scripted argv also prove no PATCH, claim, or final UID after mismatch.
    unchanged(&s, count);
}

#[test]
fn pr_metadata_original_v25_closing_null_to_empty_refuses_without_source_or_private_effects() {
    nullable_closing_v26(Value::Null, json!(""), None);
}

#[test]
fn pr_metadata_original_v25_closing_empty_to_null_refuses_without_source_or_private_effects() {
    nullable_closing_v26(json!(""), Value::Null, None);
}

#[test]
fn pr_metadata_original_v25_closing_unchanged_null_preserves_actual_null_fact() {
    nullable_closing_v26(Value::Null, Value::Null, Some(true));
}

#[test]
fn pr_metadata_original_v25_closing_unchanged_empty_preserves_actual_string_fact() {
    nullable_closing_v26(json!(""), json!(""), Some(false));
}

#[test]
fn pr_metadata_original_v25_actual_false_permission_readable_null_or_missing_capability_refuses() {
    let s = setup(phase(raw(json!(CURRENT)), capability(json!(false))));
    let (_, actual, _) = open(&s, Field::Description).unwrap();
    assert!(!actual.can_update());
    assert!(actual.prepare_requested(REQUESTED).is_err());
    unchanged(&s, s.steps.len());
    for missing in [false, true] {
        let mut cap = capability(Value::Null);
        if missing {
            cap["data"]["node"]
                .as_object_mut()
                .unwrap()
                .remove("viewerCanUpdate");
        }
        let s = setup(phase(raw(json!(CURRENT)), cap));
        assert!(open(&s, Field::Description).is_err());
        unchanged(&s, opening().len() + 5);
    }
}

#[test]
fn pr_metadata_original_v25_strict_actual_node_numeric_repo_head_base_lifecycle_and_errors_refuse()
{
    for key in [
        "fullDatabaseId",
        "id",
        "number",
        "headRefOid",
        "baseRefOid",
        "headRefName",
        "baseRefName",
        "state",
        "isDraft",
        "merged",
        "repository",
        "errors",
    ] {
        let mut cap = capability(json!(true));
        let node = &mut cap["data"]["node"];
        match key {
            "fullDatabaseId" => node[key] = json!("1008"),
            "number" => node[key] = json!(8),
            "isDraft" | "merged" => node[key] = json!(true),
            "repository" => node[key]["id"] = json!("R_RECREATED"),
            "errors" => cap["errors"] = json!([{"message":"refused alongside data"}]),
            _ => node[key] = json!("wrong"),
        }
        let s = setup(phase(raw(json!(CURRENT)), cap));
        assert!(open(&s, Field::Description).is_err(), "strict actual {key}");
        unchanged(&s, opening().len() + 5);
    }
}

#[test]
fn pr_metadata_original_v25_changed_closing_field_or_parent_hides_prepared_value_before_final_uid()
{
    for field_change in [false, true] {
        let mut tail = phase(raw(json!(CURRENT)), capability(json!(true)));
        let mut closing: Value = serde_json::from_slice(&tail[5].stdout).unwrap();
        if field_change {
            closing["body"] = json!("independently newer description");
        } else {
            closing["head"]["sha"] = json!("c".repeat(40));
        }
        tail[5].stdout = serde_json::to_vec(&closing).unwrap();
        let s = setup(tail);
        assert!(open(&s, Field::Description).is_err());
        unchanged(&s, opening().len() + 6);
    }
}

#[test]
fn pr_metadata_original_v25_title_is_selected_actual_field_not_description_and_no_change_refuses() {
    let s = setup(phase(raw(json!(CURRENT)), capability(json!(true))));
    let (_, actual, prepared) = open(&s, Field::Title).unwrap();
    assert_eq!(prepared, ("Actual personal source".into(), false));
    assert_eq!(actual.observed().field(), Field::Title);
    assert!(actual.prepare_requested("Actual personal source").is_err());
    assert!(actual.prepare_requested("  ").is_err());
    assert!(actual.prepare_requested(&"x".repeat(65_537)).is_err());
    assert!(actual.prepare_requested(REQUESTED).is_ok());
    unchanged(&s, s.steps.len());
}

#[test]
fn pr_metadata_original_v25_original_token_deadline_and_inherited_capture_cap_cannot_be_renewed() {
    let s = setup(Vec::new());
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let provider = bounded(&s, &original, 1024 * 1024);
    for (cancel, deadline) in [
        (CancellationToken::default(), original.deadline()),
        (
            original.cancellation().clone(),
            Instant::now()
                .checked_sub(Duration::from_millis(1))
                .unwrap(),
        ),
    ] {
        assert!(source::open_metadata_text_v25(
            original.clone(),
            Field::Title,
            &provider,
            &database(&s.fixture),
            &cancel,
            deadline,
            |_, _| Ok(()),
            |_| Ok(())
        )
        .is_err());
    }
    unchanged(&s, s.steps.len());
    let mut parent = raw(json!(CURRENT));
    parent["unrelated_bounded_capture"] = json!("z".repeat(70_000));
    let s = setup(phase(parent, capability(json!(true))));
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    assert!(source::open_metadata_text_v25(
        original.clone(),
        Field::Description,
        &bounded(&s, &original, 64 * 1024),
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
        |_, _| Ok(()),
        |_| Ok(())
    )
    .is_err());
    unchanged(&s, opening().len() + 4);
}

#[test]
fn pr_metadata_original_v25_closed_initial_cannot_seed_but_fresh_same_source_phase_can() {
    let mut tail = phase(raw(json!(CURRENT)), capability(json!(true)));
    tail.extend(phase(raw(json!(CURRENT)), capability(json!(true))));
    let s = setup(tail);
    let (source, initial, _) = open(&s, Field::Description).unwrap();
    use crate::pr_write_journal::metadata_origin_v25::MetadataOriginSeedV25;
    assert!(MetadataOriginSeedV25::from_producer(
        &source,
        &initial,
        REQUESTED,
        &database(&s.fixture)
    )
    .is_err());
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    source
        .with_metadata_text_phase_v25(
            &api,
            &database(&s.fixture),
            &cancel,
            deadline,
            Field::Description,
            false,
            |phase| {
                let local = phase.local(&database(&s.fixture))?;
                let seed = MetadataOriginSeedV25::from_producer(
                    &source,
                    phase.actual(),
                    REQUESTED,
                    &database(&s.fixture),
                )
                .map_err(|_| crate::providers::gh::GithubError {
                    kind: crate::providers::gh::GithubErrorKind::InvalidTarget,
                    message: "Expected actual live metadata seed".into(),
                })?;
                let values = BTreeMap::from([("body".into(), REQUESTED.into())]);
                assert_eq!(
                    seed.write(&values).unwrap().command,
                    "metadata-description-v25"
                );
                phase.finish(|| {
                    local.validate_boundary_v13(&database(&s.fixture), source.local_boundary())
                })?;
                Ok(())
            },
        )
        .unwrap();
    unchanged(&s, s.steps.len());
}

#[test]
fn pr_metadata_original_v25_actual_final_uid_newer_same_runner_writer_refuses_source_issuance() {
    let mut calls = phase(raw(json!(CURRENT)), capability(json!(true)));
    calls[8].delay_ms = 2000;
    calls.extend(principal());
    calls.push(get(&format!("repos/{REPO}/pulls/7"), parent()));
    let mut write = get(
        &format!("repos/{REPO}/issues/7/comments"),
        json!({"id":900,"node_id":"IC_900","body":"independent actual writer","user":{"id":42,"login":"octocat","type":"User"}}),
    );
    *write.args.iter_mut().find(|a| a.as_str() == "GET").unwrap() = "POST".into();
    write.args.extend(["--input".into(), "-".into()]);
    write.stdin = serde_json::to_vec(&json!({"body":"independent actual writer"})).unwrap();
    calls.push(write);
    let s = setup(calls);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| open(&s, Field::Description));
        let held = super::protocol::publication::refresh_v19::StoppedV19::final_uid(&s);
        // Existing genuine writer on the SAME runner/resource, not a test
        // generation setter or new reconciliation capability.
        let identity = s.app.github.as_ref().unwrap().identity.clone();
        let written = s
            .api
            .comment(
                &identity,
                REPO,
                7,
                &"a".repeat(40),
                "independent actual writer",
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(written.id, 900);
        drop(held);
        assert!(worker.join().unwrap().is_err());
    });
    publication::exact(&s, s.steps.len());
    assert_eq!(sql(&s.fixture), s.before_sql);
}

#[test]
fn pr_metadata_original_v25_actual_raw_read_cannot_cross_sibling_pool_runner_or_token() {
    let mut calls = principal();
    calls.push(get(&format!("repos/{REPO}"), repository()));
    calls.push(get(&format!("repos/{REPO}/pulls/7"), raw(json!(CURRENT))));
    calls.extend(principal());
    calls.push(get(&format!("repos/{REPO}"), repository()));
    let s = setup(calls);
    let opening = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let api = bounded(&s, &opening, 128 * 1024);
    let cancel = opening.cancellation();
    let limits = crate::providers::gh::RepositoryCatalogueAccessLimits {
        transport: crate::providers::gh::RepositoryCatalogueLimits {
            max_document_bytes: 128 * 1024,
            max_response_bytes: 8 * 1024 * 1024,
            ..Default::default()
        },
        max_repository_reads: 2,
        ..Default::default()
    };
    let mut first = api
        .begin_personal_pr_access_v13(opening.parent(), &limits, cancel)
        .unwrap();
    let window = first
        .reserve_measured_external_reads(3, 128 * 1024, 8 * 1024 * 1024, cancel)
        .unwrap();
    let first_api = api.clone().with_measured_read_window(window).unwrap();
    let observed = first_api
        .pr_original_metadata_text_v25(
            first.observation(),
            opening.parent(),
            Field::Description,
            cancel,
        )
        .unwrap();
    let mut sibling = api
        .begin_personal_pr_access_v13(opening.parent(), &limits, cancel)
        .unwrap();
    let window = sibling
        .reserve_measured_external_reads(3, 128 * 1024, 8 * 1024 * 1024, cancel)
        .unwrap();
    let sibling_api = api.clone().with_measured_read_window(window).unwrap();
    assert!(sibling_api
        .observe_original_metadata_text_v25(observed.clone(), sibling.identity(), cancel)
        .is_err());
    assert!(first_api
        .clone()
        .with_runner(crate::providers::runner::CommandRunner::default())
        .observe_original_metadata_text_v25(observed.clone(), first.identity(), cancel)
        .is_err());
    assert!(first_api
        .observe_original_metadata_text_v25(
            observed.clone(),
            first.identity(),
            &CancellationToken::default()
        )
        .is_err());
    let identity = first.identity().clone();
    // Deliberately drop both incomplete READ sessions: neither issued a Source,
    // closed permission or write allowance. All three grafts started 0 children.
    drop(sibling);
    drop(first);
    assert!(
        first_api
            .observe_original_metadata_text_v25(observed, &identity, cancel)
            .is_err(),
        "An aborted original pool cannot observe a new capability"
    );
    unchanged(&s, s.steps.len());
}

#[test]
fn pr_metadata_original_v25_successfully_closed_generation_lineage_is_not_live_read_permission() {
    let s = setup(phase(raw(json!(CURRENT)), capability(json!(true))));
    let opening = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let (_, actual, _) = open_captured(&s, opening.clone(), Field::Description).unwrap();
    let api = bounded(&s, &opening, 1024 * 1024);
    api.validate_original_metadata_generation_v25(
        &actual,
        opening.cancellation(),
        opening.deadline(),
    )
    .unwrap();
    assert!(actual.validate_live_source_v25().is_err());
    assert!(
        api.inspect_original_metadata_v25(actual, opening.cancellation())
            .is_err(),
        "A successfully closed original is not a new live inspection producer"
    );
    unchanged(&s, s.steps.len());
}

#[test]
fn pr_metadata_original_v25_postclose_generation_refuses_foreign_issuer_token_and_expired_clock() {
    let s = setup(phase(raw(json!(CURRENT)), capability(json!(true))));
    let opening = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let (_, actual, _) = open_captured(&s, opening.clone(), Field::Description).unwrap();
    let api = bounded(&s, &opening, 1024 * 1024);
    assert!(api
        .clone()
        .with_runner(crate::providers::runner::CommandRunner::default())
        .validate_original_metadata_generation_v25(
            &actual,
            opening.cancellation(),
            opening.deadline()
        )
        .is_err());
    assert!(api
        .clone()
        .with_env("GHTUI_METADATA_FOREIGN_ISSUER_V35", "foreign")
        .validate_original_metadata_generation_v25(
            &actual,
            opening.cancellation(),
            opening.deadline()
        )
        .is_err());
    assert!(api
        .validate_original_metadata_generation_v25(
            &actual,
            &CancellationToken::default(),
            opening.deadline(),
        )
        .is_err());
    assert!(api
        .validate_original_metadata_generation_v25(
            &actual,
            opening.cancellation(),
            Instant::now()
                .checked_sub(Duration::from_millis(1))
                .unwrap(),
        )
        .is_err());
    opening.cancellation().cancel();
    assert!(api
        .validate_original_metadata_generation_v25(
            &actual,
            opening.cancellation(),
            Instant::now() + Duration::from_secs(30),
        )
        .is_err());
    unchanged(&s, s.steps.len());
}

#[test]
fn pr_metadata_original_v25_postclose_actual_same_runner_writer_changes_generation_without_new_read(
) {
    let mut calls = phase(raw(json!(CURRENT)), capability(json!(true)));
    calls.extend(principal());
    calls.push(get(&format!("repos/{REPO}/pulls/7"), parent()));
    let mut write = get(
        &format!("repos/{REPO}/issues/7/comments"),
        json!({"id":901,"node_id":"IC_901","body":"Actual postclose independent writer","user":{"id":42,"login":"octocat","type":"User"}}),
    );
    *write.args.iter_mut().find(|a| a.as_str() == "GET").unwrap() = "POST".into();
    write.args.extend(["--input".into(), "-".into()]);
    write.stdin =
        serde_json::to_vec(&json!({"body":"Actual postclose independent writer"})).unwrap();
    calls.push(write);
    let s = setup(calls);
    let opening = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let (_, actual, _) = open_captured(&s, opening.clone(), Field::Description).unwrap();
    s.api
        .validate_original_metadata_generation_v25(
            &actual,
            opening.cancellation(),
            opening.deadline(),
        )
        .unwrap();
    let written = s
        .api
        .comment(
            actual.identity(),
            REPO,
            7,
            &"a".repeat(40),
            "Actual postclose independent writer",
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(written.id, 901);
    assert!(s
        .api
        .validate_original_metadata_generation_v25(
            &actual,
            opening.cancellation(),
            opening.deadline(),
        )
        .is_err());
    assert!(actual.validate_live_source_v25().is_err());
    publication::exact(&s, s.steps.len());
    assert_eq!(sql(&s.fixture), s.before_sql);
}

#[test]
fn pr_metadata_original_v25_failed_final_actual_uid_never_admits_source_or_inspection() {
    for history in [false, true] {
        let mut calls = phase(raw(json!(CURRENT)), capability(json!(true)));
        calls[8].stdout = serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
        let s = setup(calls);
        let page = allocations(&s.app);
        let opening = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
        let api = bounded(&s, &opening, 1024 * 1024);
        let prepared = std::cell::Cell::new(0usize);
        if history {
            assert!(source::with_metadata_history_v25(
                opening.clone(),
                Field::Description,
                &api,
                &database(&s.fixture),
                opening.cancellation(),
                opening.deadline(),
                |actual, _| {
                    prepared.set(prepared.get() + 1);
                    Ok(actual)
                },
                |_, _| Ok(()),
            )
            .is_err());
        } else {
            assert!(source::open_metadata_text_v25(
                opening.clone(),
                Field::Description,
                &api,
                &database(&s.fixture),
                opening.cancellation(),
                opening.deadline(),
                |actual, _| {
                    actual.validate_live_source_v25()?;
                    prepared.set(prepared.get() + 1);
                    Ok(())
                },
                |_| Ok(()),
            )
            .is_err());
        }
        assert_eq!(
            prepared.get(),
            1,
            "Actual live preparation preceded failed final UID"
        );
        assert_eq!(allocations(&s.app), page);
        assert!(s.app.form.is_none());
        unchanged(&s, s.steps.len());
    }
}

#[test]
fn pr_metadata_original_v25_actual_sql_guard_lock_failure_never_admits_source_or_inspection() {
    for history in [false, true] {
        let s = setup(phase(raw(json!(CURRENT)), capability(json!(true))));
        let page = allocations(&s.app);
        let opening = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
        let api = bounded(&s, &opening, 1024 * 1024);
        let held = std::cell::RefCell::new(None);
        let prepared = std::cell::Cell::new(0usize);
        let lock = || {
            let db = rusqlite::Connection::open_with_flags(
                database(&s.fixture),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
            )
            .unwrap();
            db.execute_batch("BEGIN IMMEDIATE").unwrap();
            // No rows/schema changed: Cache's actual close-time IMMEDIATE
            // transaction refuses this real writer lock under its own bound.
            assert!(held.replace(Some(db)).is_none());
            prepared.set(prepared.get() + 1);
        };
        if history {
            assert!(source::with_metadata_history_v25(
                opening.clone(), Field::Description, &api, &database(&s.fixture),
                opening.cancellation(), opening.deadline(),
                |actual, boundary| {
                    let local = crate::scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard::capture_boundary_v13(
                        &database(&s.fixture), boundary, actual.actual().identity(),
                        actual.actual().observed().parent().summary(),
                    )?;
                    lock();
                    Ok((actual, local))
                },
                |(_, local), boundary| local.validate_boundary_v13(&database(&s.fixture), boundary),
            ).is_err());
        } else {
            assert!(source::open_metadata_text_v25(
                opening.clone(),
                Field::Description,
                &api,
                &database(&s.fixture),
                opening.cancellation(),
                opening.deadline(),
                |actual, _| {
                    actual.validate_live_source_v25()?;
                    lock();
                    Ok(())
                },
                |_| Ok(()),
            )
            .is_err());
        }
        assert_eq!(prepared.get(), 1);
        let db = held
            .take()
            .expect("Actual lock was acquired after live preparation");
        db.execute_batch("ROLLBACK").unwrap();
        drop(db);
        assert_eq!(allocations(&s.app), page);
        assert!(s.app.form.is_none());
        // Closing raw parent matched, but failed SQL prevents closing repo
        // and both final principal children. No closed material can escape.
        unchanged(&s, super::opening().len() + 6);
    }
}
