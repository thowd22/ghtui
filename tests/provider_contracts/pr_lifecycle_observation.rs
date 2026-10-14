//! SOURCE ONLY: unregistered strict gh-child lifecycle observations. UNRUN.
use super::*;
use ghtui::providers::gh::{
    GithubError, MetadataChoices, PrContext, PrLifecycleBoolean, PrLifecycleReadLimits,
    PrMetadataCatalogueRead, PrMetadataReadLimits, PR_LIFECYCLE_OBSERVATION_QUERY,
};

fn current() -> HostIdentity {
    HostIdentity {
        account_id: Some(42),
        ..identity()
    }
}
fn repo() -> Value {
    json!({"id":99,"node_id":"R_lifecycle_99","full_name":"example/repo","description":"Repo",
        "default_branch":"main","private":true,"archived":false,"permissions":{"pull":true,"push":true,"maintain":false,"admin":false}})
}
fn pull() -> Value {
    json!({"id":701,"node_id":"PR_lifecycle_7","number":7,"title":"Original title","body":"PRIVATE_ORIGINAL_BODY",
        "state":"open","draft":false,"merged":false,"locked":false,"created_at":"2026-10-01T00:00:00Z",
        "updated_at":"2026-10-13T00:00:00Z","user":{"id":51,"login":"author","type":"User"},
        "head":{"sha":"a".repeat(40),"ref":"Feature/雪","repo":repo()},"base":{"sha":"b".repeat(40),"ref":"main","repo":repo()},
        "html_url":"https://github.invalid/example/repo/pull/7","requested_reviewers":[],"requested_teams":[],
        "labels":[],"assignees":[],"milestone":null,"maintainer_can_modify":false})
}
fn issue() -> Value {
    json!({"id":1701,"node_id":"I_lifecycle_1701","number":7,"title":"Original title","body":"PRIVATE_ORIGINAL_BODY",
        "state":"open","locked":false,"user":{"id":51,"login":"author","type":"User"},"labels":[],"assignees":[],"milestone":null,
        "html_url":"https://github.invalid/example/repo/pull/7","pull_request":{"url":"https://github.invalid/api/v3/repos/example/repo/pulls/7","html_url":"https://github.invalid/example/repo/pull/7"}})
}
fn original() -> (PrSummary, PrContext) {
    let target:PrSummary=serde_json::from_value(json!({"pr":{"host":"github.invalid","id":"PR_lifecycle_7","repository":"example/repo","number":7,
        "title":"Original title","body":"PRIVATE_ORIGINAL_BODY","url":"https://github.invalid/example/repo/pull/7",
        "author":{"login":"author","actor_type":"User"},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","queues":[]},
        "head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"Feature/雪","base_ref":"main","merged":false,"auto_merge":false,
        "merge_commit_sha":null,"mergeable":null,"mergeable_state":"unknown","additions":0,"deletions":0,"changed_files":0})).unwrap();
    let r = json!({"host":"github.invalid","id":99,"full_name":"example/repo","description":"Repo","default_branch":"main","private":true,
        "archived":false,"permissions":{"pull":true,"push":true,"maintain":false,"admin":false}});
    let context:PrContext=serde_json::from_value(json!({"repository":r,"base_repository":r,"head_repository":r,
        "is_cross_repository":false,"maintainer_can_modify":false,"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),
        "issue_metadata":issue(),"requested_reviewers":[],"requested_teams":[],"source_urls":[]})).unwrap();
    (target, context)
}
fn get(endpoint: &str, value: Value) -> GhStep {
    step(api_args(endpoint), value)
}
fn user(id: u64) -> GhStep {
    get("user", json!({"id":id,"login":"octocat"}))
}
fn origin_calls() -> Vec<GhStep> {
    vec![
        user(42),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/7", pull()),
        get("repos/example/repo/issues/7", issue()),
        get("repos/example/repo/labels?per_page=100&page=1", json!([])),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/7", pull()),
        user(42),
    ]
}
fn graph() -> Value {
    json!({"data":{"node":{"__typename":"PullRequest","id":"PR_lifecycle_7","fullDatabaseId":"701","number":7,
        "headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"headRefName":"Feature/雪","baseRefName":"main",
        "state":"OPEN","isDraft":false,"merged":false,"repository":{"id":"R_lifecycle_99","nameWithOwner":"example/repo"},
        "viewerCanClose":true,"viewerCanReopen":false,"viewerCanUpdate":true,"viewerCanUpdateBranch":false}}})
}
fn graph_step(value: Value) -> GhStep {
    GhStep {
        args: vec![
            "api".into(),
            "graphql".into(),
            "--hostname".into(),
            "github.invalid".into(),
            "--method".into(),
            "POST".into(),
            "--input".into(),
            "-".into(),
        ],
        stdin: serde_json::to_vec(
            &json!({"query":PR_LIFECYCLE_OBSERVATION_QUERY,"variables":{"id":"PR_lifecycle_7"}}),
        )
        .unwrap(),
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
fn lifecycle_calls(raw: Value) -> Vec<GhStep> {
    vec![
        user(42),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/7", pull()),
        get("repos/example/repo/issues/7", issue()),
        graph_step(raw),
        get("repos/example/repo/pulls/7", pull()),
        get("repos/example/repo", repo()),
        user(42),
    ]
}
fn scripted(fixture: &Fixture, calls: Vec<GhStep>) {
    let mut all = origin_calls();
    all.extend(calls);
    fixture.scripted_gh(&all);
}
fn load(gh: &GithubProvider) -> Arc<PrMetadataCatalogueRead> {
    let (target, context) = original();
    Arc::new(
        gh.metadata_catalogue_read(
            &current(),
            &target,
            &context,
            MetadataChoices::Labels,
            &PrMetadataReadLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap(),
    )
}
fn read_only(fixture: &Fixture, expected: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), expected);
    for row in rows {
        assert_eq!(row["matched"], true);
        if row["args"][1] == "graphql" {
            assert_eq!(row["args"][5], "POST");
            let hex = row["stdin_hex"].as_str().unwrap();
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|n| u8::from_str_radix(&hex[n..n + 2], 16).unwrap())
                .collect();
            let input: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                input,
                json!({"query":PR_LIFECYCLE_OBSERVATION_QUERY,"variables":{"id":"PR_lifecycle_7"}})
            );
            assert!(!input["query"].as_str().unwrap().contains("mutation"));
        } else {
            assert_eq!(row["args"][5], "GET");
            assert_eq!(row["stdin_hex"], "");
        }
    }
}
fn sql_failure() -> GithubError {
    GithubError {
        kind: GithubErrorKind::Json,
        message: "PRIVATE_SQL_GUARD_CANARY".into(),
    }
}

#[test]
fn pr_lifecycle_observation_eight_actual_reads_retained_origin_ids_sql_order_and_no_barrier_release(
) {
    let fixture = Fixture::new();
    scripted(&fixture, lifecycle_calls(graph()));
    let runner = CommandRunner::default();
    let resource = "github.invalid/example/repo/pull/7";
    runner.mark_ambiguous(resource);
    let generation = runner.resource_generation(resource);
    let gh = provider(&fixture).with_runner(runner.clone());
    let origin = load(&gh);
    let mut journal_sql =
        ghtui::pr_write_journal::PrWriteJournal::open(&fixture.root.join("private-cache.sqlite3"))
            .unwrap();
    let target = ghtui::pr_write_journal::PrWriteTarget::new(&current(), origin.target()).unwrap();
    let intent = ghtui::pr_write_journal::PrWriteIntent {
        command: "state-close".into(),
        selected_resource: "pr".into(),
        observed_head: "a".repeat(40),
        values: BTreeMap::new(),
    };
    let _pending = journal_sql.begin(&target, &intent).unwrap();
    let mut seen = 0;
    let observed = gh
        .pr_lifecycle_observation(
            origin.clone(),
            &PrLifecycleReadLimits::default(),
            |ctx| {
                seen += 1;
                assert_eq!(journal(&fixture).len(), 14);
                assert_eq!(ctx.identity().account_id, Some(42));
                assert_eq!(ctx.repository().id, 99);
                assert_eq!(ctx.repository().node_id, "R_lifecycle_99");
                assert_eq!(ctx.pr_id(), 701);
                assert_eq!(ctx.pr_node(), "PR_lifecycle_7");
                assert_eq!(ctx.pr_number(), 7);
                assert_eq!(ctx.issue_id(), 1701);
                assert_eq!(ctx.issue_node(), "I_lifecycle_1701");
                assert_ne!(ctx.issue_node(), ctx.pr_node());
                assert_eq!(ctx.head(), "a".repeat(40));
                assert_eq!(ctx.base(), "b".repeat(40));
                assert!(ctx.deadline() > Instant::now());
                assert!(journal_sql.blocking(&target).unwrap().is_some());
                Ok(())
            },
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(seen, 1);
    assert!(Arc::ptr_eq(observed.origin(), &origin));
    assert_eq!(observed.accounting().requests(), 8);
    assert_eq!(observed.pr_id(), 701);
    assert_eq!(observed.issue_id(), 1701);
    assert_eq!(
        observed.capabilities().close(),
        PrLifecycleBoolean::Observed(true)
    );
    assert_eq!(observed.locked(), PrLifecycleBoolean::Observed(false));
    assert!(observed.observed_interval().1 >= observed.observed_interval().0);
    assert_eq!(runner.resource_generation(resource), generation);
    assert!(journal_sql.blocking(&target).unwrap().is_some());
    assert!(!format!("{observed:?}").contains("PRIVATE_ORIGINAL_BODY"));
    read_only(&fixture, 16);
}

#[test]
fn pr_lifecycle_observation_capabilities_keep_null_missing_malformed_and_generic_update_not_specific_eligibility(
) {
    for (value, expected) in [
        (Value::Null, PrLifecycleBoolean::Null),
        (json!(false), PrLifecycleBoolean::Observed(false)),
        (json!("true"), PrLifecycleBoolean::Malformed),
    ] {
        let fixture = Fixture::new();
        let mut raw = graph();
        raw["data"]["node"]["viewerCanClose"] = value;
        raw["data"]["node"]
            .as_object_mut()
            .unwrap()
            .remove("viewerCanReopen");
        scripted(&fixture, lifecycle_calls(raw));
        let gh = provider(&fixture);
        let observed = gh
            .pr_lifecycle_observation(
                load(&gh),
                &PrLifecycleReadLimits::default(),
                |_| Ok(()),
                &CancellationToken::default(),
            )
            .unwrap();
        let caps = observed.capabilities();
        assert_eq!(caps.close(), expected);
        assert_eq!(caps.reopen(), PrLifecycleBoolean::Missing);
        assert_eq!(caps.update_object(), PrLifecycleBoolean::Observed(true));
        for state in [
            caps.draft_ready_eligibility(),
            caps.lock_eligibility(),
            caps.merge_eligibility(),
            caps.repository_merge_settings(),
            caps.queue_state(),
            caps.auto_merge_state(),
            caps.required_checks_and_rules(),
        ] {
            assert_eq!(state, PrLifecycleBoolean::NotLoaded);
            assert!(state.label().contains("unknown"));
        }
        read_only(&fixture, 16);
    }
}

#[test]
fn pr_lifecycle_observation_wrong_original_repo_pr_full_head_base_ref_or_lifecycle_stops_before_sql(
) {
    for (position, path, value) in [
        (1, vec!["id"], json!(100)),
        (1, vec!["node_id"], json!("R_replaced")),
        (2, vec!["id"], json!(702)),
        (2, vec!["node_id"], json!("PR_other")),
        (2, vec!["head", "sha"], json!("c".repeat(40))),
        (2, vec!["base", "sha"], json!("d".repeat(40))),
        (2, vec!["head", "ref"], json!("feature/雪")),
        (2, vec!["state"], json!("closed")),
        (2, vec!["draft"], Value::Null),
    ] {
        let fixture = Fixture::new();
        let mut calls = lifecycle_calls(graph());
        let mut raw: Value = serde_json::from_slice(&calls[position].stdout).unwrap();
        let mut selected = &mut raw;
        for key in path {
            selected = &mut selected[key];
        }
        *selected = value;
        calls[position].stdout = serde_json::to_vec(&raw).unwrap();
        calls.truncate(position + 1);
        scripted(&fixture, calls);
        let gh = provider(&fixture);
        assert!(gh
            .pr_lifecycle_observation(
                load(&gh),
                &PrLifecycleReadLimits::default(),
                |_| panic!("no SQL after scope drift"),
                &CancellationToken::default()
            )
            .is_err());
        read_only(&fixture, 8 + position + 1);
    }
}

#[test]
fn pr_lifecycle_observation_independent_issue_id_node_number_link_and_lock_cannot_alias_selected_pr(
) {
    for (key, value) in [
        ("id", json!(701)),
        ("node_id", json!("PR_lifecycle_7")),
        ("number", json!(8)),
        (
            "html_url",
            json!("https://github.invalid/example/other/pull/7"),
        ),
        ("locked", json!(true)),
    ] {
        let fixture = Fixture::new();
        let mut calls = lifecycle_calls(graph());
        let mut raw = issue();
        raw[key] = value;
        calls[3] = get("repos/example/repo/issues/7", raw);
        calls.truncate(4);
        scripted(&fixture, calls);
        let gh = provider(&fixture);
        assert!(gh
            .pr_lifecycle_observation(
                load(&gh),
                &PrLifecycleReadLimits::default(),
                |_| panic!("no SQL on issue contradiction"),
                &CancellationToken::default()
            )
            .is_err());
        read_only(&fixture, 12);
    }
    let fixture = Fixture::new();
    let mut calls = lifecycle_calls(graph());
    let mut raw = issue();
    raw["pull_request"]["url"] = json!("https://github.invalid/api/v3/repos/example/repo/pulls/8");
    calls[3] = get("repos/example/repo/issues/7", raw);
    calls.truncate(4);
    scripted(&fixture, calls);
    let gh = provider(&fixture);
    assert!(gh
        .pr_lifecycle_observation(
            load(&gh),
            &PrLifecycleReadLimits::default(),
            |_| panic!("no SQL without genuine link"),
            &CancellationToken::default()
        )
        .is_err());
    read_only(&fixture, 12);
}

#[test]
fn pr_lifecycle_observation_graphql_true_properties_for_wrong_id_comparison_or_repository_never_admit_sql(
) {
    for (path, value) in [
        (vec!["fullDatabaseId"], json!("702")),
        (vec!["id"], json!("PR_wrong")),
        (vec!["number"], json!(8)),
        (vec!["repository", "id"], json!("R_wrong")),
        (vec!["headRefOid"], json!("c".repeat(40))),
        (vec!["baseRefName"], json!("release")),
        (vec!["state"], json!("CLOSED")),
        (vec!["__typename"], json!("Issue")),
    ] {
        let fixture = Fixture::new();
        let mut raw = graph();
        let mut node = &mut raw["data"]["node"];
        for key in path {
            node = &mut node[key];
        }
        *node = value;
        scripted(&fixture, lifecycle_calls(raw));
        let gh = provider(&fixture);
        let failure = gh
            .pr_lifecycle_observation(
                load(&gh),
                &PrLifecycleReadLimits::default(),
                |_| panic!("foreign GraphQL cannot authorize SQL guard"),
                &CancellationToken::default(),
            )
            .unwrap_err();
        assert!(matches!(failure.kind, GithubErrorKind::IdentityChanged));
        read_only(&fixture, 16);
    }
}

#[test]
fn pr_lifecycle_observation_missing_bigint_or_graphql_schema_errors_keep_capabilities_unavailable_without_fallback(
) {
    for raw in [
        json!({"errors":[{"message":"UNKNOWN_FIELD_PRIVATE_CANARY"}],"data":null}),
        {
            let mut raw = graph();
            raw["data"]["node"]["fullDatabaseId"] = Value::Null;
            raw
        },
        {
            let mut raw = graph();
            raw["data"]["node"]["fullDatabaseId"] = json!(701);
            raw
        },
        {
            let mut raw = graph();
            raw["data"]["node"]
                .as_object_mut()
                .unwrap()
                .remove("fullDatabaseId");
            raw
        },
    ] {
        let fixture = Fixture::new();
        scripted(&fixture, lifecycle_calls(raw));
        let gh = provider(&fixture);
        let observed = gh
            .pr_lifecycle_observation(
                load(&gh),
                &PrLifecycleReadLimits::default(),
                |_| Ok(()),
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(
            observed.capabilities().close(),
            PrLifecycleBoolean::NotLoaded
        );
        assert!(observed
            .capabilities()
            .unavailable_reason()
            .unwrap()
            .contains("no eligibility"));
        assert!(!format!("{:?}", observed.capabilities()).contains("UNKNOWN_FIELD_PRIVATE_CANARY"));
        read_only(&fixture, 16);
    }
}

#[test]
fn pr_lifecycle_observation_closing_uid_auth_and_repo_failures_override_private_sql_error() {
    for outcome in 0..6 {
        let fixture = Fixture::new();
        let mut calls = lifecycle_calls(graph());
        if outcome == 0 || outcome == 2 {
            calls[7] = user(43);
        }
        if outcome == 1 {
            calls[7].status = 1;
            calls[7].stdout.clear();
            calls[7].stderr = b"HTTP 401: PRIVATE_AUTH_CANARY".to_vec();
        }
        if outcome == 2 {
            calls[6].status = 1;
            calls[6].stdout.clear();
            calls[6].stderr = b"HTTP 403: PRIVATE_REPO_CANARY".to_vec();
        }
        if outcome >= 3 {
            let mut raw = json!({"id":42,"login":"octocat"});
            match outcome {
                3 => raw["id"] = json!(0),
                4 => raw["id"] = Value::Null,
                5 => {
                    raw.as_object_mut().unwrap().remove("id");
                }
                _ => unreachable!(),
            }
            calls[7] = get("user", raw);
        }
        scripted(&fixture, calls);
        let gh = provider(&fixture);
        let mut guarded = 0;
        let failure = gh
            .pr_lifecycle_observation(
                load(&gh),
                &PrLifecycleReadLimits::default(),
                |_| {
                    guarded += 1;
                    Err(sql_failure())
                },
                &CancellationToken::default(),
            )
            .unwrap_err();
        assert_eq!(guarded, 1);
        assert!(matches!(
            failure.kind,
            GithubErrorKind::IdentityChanged | GithubErrorKind::Authentication
        ));
        if outcome >= 3 {
            assert_eq!(failure.kind, GithubErrorKind::IdentityChanged);
            assert!(failure.message.contains("numeric principal is unverified"));
        }
        assert!(!failure.message.contains("PRIVATE_"));
        read_only(&fixture, 16);
    }
}

#[test]
fn pr_lifecycle_observation_local_sql_error_still_closes_exact_scope_and_never_returns_partial_proof(
) {
    let fixture = Fixture::new();
    scripted(&fixture, lifecycle_calls(graph()));
    let gh = provider(&fixture);
    let failure = gh
        .pr_lifecycle_observation(
            load(&gh),
            &PrLifecycleReadLimits::default(),
            |ctx| {
                assert_eq!(journal(&fixture).len(), 14);
                assert_eq!(ctx.identity().account_id, Some(42));
                Err(sql_failure())
            },
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::Json));
    assert_eq!(failure.message, "PRIVATE_SQL_GUARD_CANARY");
    read_only(&fixture, 16);
}

#[test]
fn pr_lifecycle_observation_sql_cancel_or_original_deadline_prevents_final_children_without_new_budget(
) {
    for expire in [false, true] {
        let fixture = Fixture::new();
        let mut calls = lifecycle_calls(graph());
        calls.truncate(6);
        scripted(&fixture, calls);
        let gh = provider(&fixture);
        let token = CancellationToken::default();
        let limits = PrLifecycleReadLimits {
            max_duration: Duration::from_secs(2),
            ..Default::default()
        };
        let failure = gh
            .pr_lifecycle_observation(
                load(&gh),
                &limits,
                |ctx| {
                    assert_eq!(journal(&fixture).len(), 14);
                    if expire {
                        while Instant::now() < ctx.deadline() {
                            thread::yield_now();
                        }
                    } else {
                        token.cancel();
                    }
                    Err(sql_failure())
                },
                &token,
            )
            .unwrap_err();
        assert!(matches!(failure.kind, GithubErrorKind::Transport));
        assert!(!failure.message.contains("PRIVATE_SQL"));
        read_only(&fixture, 14);
    }
}

#[test]
fn pr_lifecycle_observation_raw_control_diagnostics_are_charged_and_failed_calls_reserve_original_cap(
) {
    for fail in [false, true] {
        let fixture = Fixture::new();
        let mut calls = lifecycle_calls(graph());
        if fail {
            calls[4].status = 1;
            calls[4].stdout.clear();
            calls[4].stderr = b"HTTP 429: rate limit".to_vec();
        } else {
            calls[4].stderr = vec![0x07; 64];
            calls[6].stderr = vec![0x07; 17];
        }
        let expected_stdout: usize = calls.iter().map(|call| call.stdout.len()).sum();
        scripted(&fixture, calls);
        let gh = provider(&fixture);
        let observed = gh
            .pr_lifecycle_observation(
                load(&gh),
                &PrLifecycleReadLimits::default(),
                |_| Ok(()),
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(
            observed.accounting().successful_stdout_bytes(),
            expected_stdout
        );
        assert_eq!(
            observed.accounting().successful_diagnostic_bytes(),
            if fail { 0 } else { 81 }
        );
        assert_eq!(
            observed.accounting().failed_reserved_bytes(),
            if fail { 128 * 1024 } else { 0 }
        );
        assert_eq!(
            observed.accounting().charged_bytes(),
            expected_stdout + if fail { 128 * 1024 } else { 81 }
        );
        if fail {
            assert_eq!(
                observed.capabilities().close(),
                PrLifecycleBoolean::NotLoaded
            );
        }
        read_only(&fixture, 16);
    }
}

#[test]
fn pr_lifecycle_observation_capture_exhaustion_has_no_closing_call_sql_or_private_partial_result() {
    let fixture = Fixture::new();
    let mut calls = lifecycle_calls(graph());
    calls[4].stderr = vec![0x07; 128 * 1024 + 1];
    calls.truncate(5);
    scripted(&fixture, calls);
    let gh = provider(&fixture);
    let failure = gh
        .pr_lifecycle_observation(
            load(&gh),
            &PrLifecycleReadLimits::default(),
            |_| panic!("no SQL after capture exhaustion"),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::Transport));
    read_only(&fixture, 13);
}

#[test]
fn pr_lifecycle_observation_same_uid_rename_and_exact_terminal_capture_limit_are_not_extra_authority_or_new_reads(
) {
    let fixture = Fixture::new();
    let mut calls = lifecycle_calls(graph());
    calls[7] = get("user", json!({"id":42,"login":"renamed_user"}));
    let exact_bytes: usize = calls.iter().map(|step| step.stdout.len()).sum();
    scripted(&fixture, calls);
    let gh = provider(&fixture);
    let observed = gh
        .pr_lifecycle_observation(
            load(&gh),
            &PrLifecycleReadLimits {
                max_response_bytes: exact_bytes,
                ..Default::default()
            },
            |_| Ok(()),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(observed.identity().account_id, Some(42));
    assert_eq!(observed.identity().login, "renamed_user");
    assert_eq!(observed.accounting().charged_bytes(), exact_bytes);
    assert_eq!(observed.accounting().requests(), 8);
    read_only(&fixture, 16);
}

#[test]
fn pr_lifecycle_observation_invalid_limits_and_pre_cancel_stop_before_any_new_child() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&origin_calls());
    let gh = provider(&fixture);
    let origin = load(&gh);
    for limits in [
        PrLifecycleReadLimits {
            max_duration: Duration::ZERO,
            ..Default::default()
        },
        PrLifecycleReadLimits {
            max_document_bytes: 0,
            ..Default::default()
        },
        PrLifecycleReadLimits {
            max_response_bytes: 0,
            ..Default::default()
        },
    ] {
        assert!(gh
            .pr_lifecycle_observation(
                origin.clone(),
                &limits,
                |_| panic!("no SQL on invalid limits"),
                &CancellationToken::default()
            )
            .is_err());
    }
    let token = CancellationToken::default();
    token.cancel();
    assert!(gh
        .pr_lifecycle_observation(
            origin,
            &PrLifecycleReadLimits::default(),
            |_| panic!("no SQL on cancelled source"),
            &token
        )
        .is_err());
    read_only(&fixture, 8);
}

#[test]
fn pr_lifecycle_observation_nullable_current_read_permission_is_unknown_not_old_true_or_inferred_false(
) {
    for (value, kind) in [
        (Some(json!(false)), GithubErrorKind::Permission),
        (Some(Value::Null), GithubErrorKind::Unsupported),
        (None, GithubErrorKind::Unsupported),
        (Some(json!("true")), GithubErrorKind::Unsupported),
    ] {
        let fixture = Fixture::new();
        let mut raw = repo();
        if let Some(value) = value {
            raw["permissions"]["pull"] = value;
        } else {
            raw["permissions"].as_object_mut().unwrap().remove("pull");
        }
        let mut calls = lifecycle_calls(graph());
        calls[1] = get("repos/example/repo", raw);
        calls.truncate(2);
        scripted(&fixture, calls);
        let gh = provider(&fixture);
        let source = load(&gh);
        assert_eq!(source.repository_raw()["permissions"]["pull"], true);
        let error = gh
            .pr_lifecycle_observation(
                source,
                &PrLifecycleReadLimits::default(),
                |_| panic!("no private SQL when current read capability is denied/unverified"),
                &CancellationToken::default(),
            )
            .unwrap_err();
        assert_eq!(
            std::mem::discriminant(&error.kind),
            std::mem::discriminant(&kind)
        );
        read_only(&fixture, 10);
    }
}

#[test]
fn pr_lifecycle_observation_actual_readonly_transport_preserves_real_git_head_index_and_dirty_bytes(
) {
    let fixture = Fixture::new();
    scripted(&fixture, lifecycle_calls(graph()));
    let root = fixture.repo();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(&root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out.stdout
    };
    git(&["init", "--initial-branch=main"]);
    fs::write(root.join("tracked"), b"before\n").unwrap();
    git(&["add", "tracked"]);
    git(&["commit", "-m", "baseline"]);
    fs::write(root.join("tracked"), b"dirty\n").unwrap();
    fs::write(root.join("unrelated"), b"keep\0bytes").unwrap();
    let head = git(&["rev-parse", "HEAD"]);
    let status = git(&["status", "--porcelain=v1", "-z"]);
    let index = fs::read(root.join(".git/index")).unwrap();
    let gh = provider(&fixture);
    gh.pr_lifecycle_observation(
        load(&gh),
        &PrLifecycleReadLimits::default(),
        |_| Ok(()),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(git(&["rev-parse", "HEAD"]), head);
    assert_eq!(git(&["status", "--porcelain=v1", "-z"]), status);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(root.join("tracked")).unwrap(), b"dirty\n");
    assert_eq!(fs::read(root.join("unrelated")).unwrap(), b"keep\0bytes");
    read_only(&fixture, 16);
}
