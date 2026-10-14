//! Bounded eligible catalogues. Never bare-Vec completeness/auth.
use super::*;
use ghtui::providers::gh::{MetadataChoices, MetricsReadStatus, PrContext, PrMetadataReadLimits};

fn current() -> HostIdentity {
    HostIdentity {
        account_id: Some(42),
        ..identity()
    }
}
fn repo() -> Value {
    json!({"id":99,"node_id":"R_catalogue_99","full_name":"example/repo","description":"Repo","default_branch":"main","private":true,"archived":false,"permissions":{"pull":true,"push":true,"admin":false,"maintain":false}})
}
fn author() -> Value {
    json!({"id":51,"login":"author","type":"User","avatar_url":"https://avatars.invalid/51"})
}
fn pull() -> Value {
    json!({"id":701,"node_id":"PR_catalogue_7","number":7,"title":"Original title","body":"Original description","state":"open","draft":false,"merged":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","user":author(),"head":{"sha":"a".repeat(40),"ref":"feature","repo":repo()},"base":{"sha":"b".repeat(40),"ref":"main","repo":repo()},"html_url":"https://github.invalid/example/repo/pull/7","requested_reviewers":[],"requested_teams":[],"labels":[],"assignees":[],"milestone":null,"maintainer_can_modify":false,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0})
}
fn issue() -> Value {
    json!({"id":1701,"node_id":"I_issue_catalogue_7","number":7,"title":"Original title","body":"Original description","state":"open","user":author(),"labels":[],"assignees":[],"milestone":null,"html_url":"https://github.invalid/example/repo/pull/7","pull_request":{"url":"https://github.invalid/api/v3/repos/example/repo/pulls/7","html_url":"https://github.invalid/example/repo/pull/7"}})
}
fn original() -> (PrSummary, PrContext) {
    let target:PrSummary=serde_json::from_value(json!({"pr":{"host":"github.invalid","id":"PR_catalogue_7","repository":"example/repo","number":7,"title":"Original title","body":"Original description","url":"https://github.invalid/example/repo/pull/7","author":{"login":"author","actor_type":"User"},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","queues":[]},"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0})).unwrap();
    let r = json!({"host":"github.invalid","id":99,"full_name":"example/repo","description":"Repo","default_branch":"main","private":true,"archived":false,"permissions":{"pull":true,"push":true,"maintain":false,"admin":false}});
    let context:PrContext=serde_json::from_value(json!({"repository":r,"base_repository":r,"head_repository":r,"is_cross_repository":false,"maintainer_can_modify":false,"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"issue_metadata":issue(),"requested_reviewers":[],"requested_teams":[],"source_urls":[]})).unwrap();
    (target, context)
}
fn limits() -> PrMetadataReadLimits {
    PrMetadataReadLimits {
        page_size: 2,
        max_rows: 10,
        max_pages: 3,
        ..Default::default()
    }
}
fn get(endpoint: &str, value: Value) -> GhStep {
    step(api_args(endpoint), value)
}
fn user(id: u64, login: &str) -> GhStep {
    get("user", json!({"id":id,"login":login}))
}
fn page(path: &str, index: usize, rows: Vec<Value>) -> GhStep {
    get(
        &format!(
            "repos/example/repo/{path}{}per_page=2&page={index}",
            if path.contains('?') { '&' } else { '?' }
        ),
        json!(rows),
    )
}
fn prefix() -> Vec<GhStep> {
    vec![
        user(42, "octocat"),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/7", pull()),
        get("repos/example/repo/issues/7", issue()),
    ]
}
fn suffix() -> Vec<GhStep> {
    vec![
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/7", pull()),
        user(42, "octocat"),
    ]
}
fn complete(pages: Vec<GhStep>) -> Vec<GhStep> {
    let mut calls = prefix();
    calls.extend(pages);
    calls.extend(suffix());
    calls
}
fn read(
    fixture: &Fixture,
    kind: MetadataChoices,
    limits: &PrMetadataReadLimits,
) -> Result<ghtui::providers::gh::PrMetadataCatalogueRead, ghtui::providers::gh::GithubError> {
    let (target, context) = original();
    provider(fixture).metadata_catalogue_read(
        &current(),
        &target,
        &context,
        kind,
        limits,
        &CancellationToken::default(),
    )
}
fn reads(fixture: &Fixture, count: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["args"][5] == "GET" && row["stdin_hex"] == ""));
    assert!(rows.iter().all(|row| !row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "--paginate")));
}

#[test]
fn pr_metadata_catalogue_provider_exact_eight_reads_six_kinds_preserve_real_ids_context_and_counts()
{
    for (kind, path, row) in [
        (
            MetadataChoices::Labels,
            "labels",
            json!({"id":81,"node_id":"L_81","name":"bug 🐛"}),
        ),
        (
            MetadataChoices::Assignees,
            "assignees",
            json!({"id":43,"login":"alice","type":"User"}),
        ),
        (
            MetadataChoices::Collaborators,
            "collaborators",
            json!({"id":44,"login":"reviewer","permissions":{"pull":true,"push":false}}),
        ),
        (
            MetadataChoices::Teams,
            "teams",
            json!({"id":55,"slug":"read-team","permission":"pull"}),
        ),
        (
            MetadataChoices::Milestones,
            "milestones?state=open",
            json!({"id":85,"number":5,"title":"October","state":"open"}),
        ),
        (
            MetadataChoices::Branches,
            "branches",
            json!({"name":"release/雪","commit":{"sha":"c".repeat(40)}}),
        ),
    ] {
        let fixture = Fixture::new();
        let calls = complete(vec![page(path, 1, vec![row.clone()])]);
        let bytes: usize = calls.iter().map(|step| step.stdout.len()).sum();
        fixture.scripted_gh(&calls);
        let observed = read(&fixture, kind, &limits()).unwrap();
        assert_eq!(observed.identity().account_id, Some(42));
        assert_eq!(observed.target().pr.id, "PR_catalogue_7");
        assert_eq!(observed.context().issue_metadata["id"], 1701);
        assert_eq!(
            observed.context().issue_metadata["node_id"],
            "I_issue_catalogue_7"
        );
        assert_eq!(observed.rows(), &[row]);
        assert_eq!(observed.requests(), 8);
        assert_eq!(observed.requested_pages(), 1);
        assert_eq!(observed.fetched_pages(), 1);
        assert_eq!(observed.raw_rows(), 1);
        assert_eq!(observed.response_bytes(), bytes);
        assert_eq!(observed.accounting().charged_bytes(), bytes);
        assert!(
            observed.coverage().enumeration_complete
                && matches!(observed.coverage().status, MetricsReadStatus::Complete)
        );
        assert!(observed.context().repository.permissions.push);
        assert!(!format!("{observed:?}").contains("Original description"));
        reads(&fixture, 8);
    }
}

#[test]
fn pr_metadata_catalogue_provider_fixed_paging_caps_and_genuine_complete_empty_are_distinct() {
    let first = vec![json!({"id":1,"name":"one"}), json!({"id":2,"name":"two"})];
    for capped in [false, true] {
        let fixture = Fixture::new();
        let mut pages = vec![page("labels", 1, first.clone())];
        if !capped {
            pages.push(page("labels", 2, vec![]));
        }
        fixture.scripted_gh(&complete(pages));
        let observed = read(
            &fixture,
            MetadataChoices::Labels,
            &PrMetadataReadLimits {
                max_pages: if capped { 1 } else { 3 },
                ..limits()
            },
        )
        .unwrap();
        assert_eq!(observed.rows().len(), 2);
        assert_eq!(observed.fetched_pages(), if capped { 1 } else { 2 });
        assert_eq!(observed.requests(), if capped { 8 } else { 9 });
        assert_eq!(observed.coverage().enumeration_complete, !capped);
        assert!(!observed.coverage().complete_empty);
        reads(&fixture, if capped { 8 } else { 9 });
    }
    let fixture = Fixture::new();
    fixture.scripted_gh(&complete(vec![page("labels", 1, vec![])]));
    let observed = read(&fixture, MetadataChoices::Labels, &limits()).unwrap();
    assert!(observed.rows().is_empty() && observed.coverage().complete_empty);
    assert!(matches!(
        observed.coverage().status,
        MetricsReadStatus::Complete
    ));
    reads(&fixture, 8);
}

#[test]
fn pr_metadata_catalogue_provider_null_candidate_identity_is_retained_without_position_alias_or_fake_eligibility(
) {
    let fixture = Fixture::new();
    let raw = json!({"id":null,"login":"unknown-id","type":null,"permissions":null});
    let mut calls = complete(vec![page("collaborators", 1, vec![raw.clone()])]);
    calls[4].stdout = serde_json::to_vec(&json!([raw.clone()])).unwrap();
    fixture.scripted_gh(&calls);
    let observed = read(&fixture, MetadataChoices::Collaborators, &limits()).unwrap();
    assert_eq!(observed.rows(), &[raw]);
    assert_eq!(observed.raw_rows(), 1);
    assert_eq!(observed.coverage().excluded_rows, 0);
    assert!(observed.coverage().enumeration_complete);
    assert!(matches!(
        observed.coverage().status,
        MetricsReadStatus::Complete
    ));
    assert!(observed.rows()[0]["id"].is_null());
    reads(&fixture, 8);
}

#[test]
fn pr_metadata_catalogue_provider_final_requester_permissions_replace_old_true_and_rename_keeps_uid(
) {
    for missing in [false, true] {
        let fixture = Fixture::new();
        let mut calls = complete(vec![page("labels", 1, vec![json!({"id":81,"name":"bug"})])]);
        let mut final_repo = repo();
        if missing {
            final_repo.as_object_mut().unwrap().remove("permissions");
        } else {
            final_repo["permissions"]["push"] = json!(false);
        }
        calls[5].stdout = serde_json::to_vec(&final_repo).unwrap();
        calls[7] = user(42, "renamed-user");
        fixture.scripted_gh(&calls);
        let observed = read(&fixture, MetadataChoices::Labels, &limits()).unwrap();
        assert_eq!(observed.identity().login, "renamed-user");
        assert_eq!(observed.identity().account_id, Some(42));
        assert!(!observed.context().repository.permissions.push);
        assert_eq!(
            observed.repository_raw()["permissions"]["push"].as_bool(),
            if missing { None } else { Some(false) }
        );
        assert!(original().1.repository.permissions.push);
        reads(&fixture, 8);
    }
}

#[test]
fn pr_metadata_catalogue_provider_selected_endpoint403_is_unavailable_only_after_scope_closes() {
    let fixture = Fixture::new();
    let mut failed = page("teams", 1, vec![]);
    failed.status = 1;
    failed.stderr = b"HTTP 403: selected team list denied".to_vec();
    fixture.scripted_gh(&complete(vec![failed]));
    let observed = read(&fixture, MetadataChoices::Teams, &limits()).unwrap();
    assert!(observed.rows().is_empty());
    assert!(
        matches!(&observed.coverage().status,MetricsReadStatus::Unavailable(failure) if matches!(failure.kind,GithubErrorKind::Permission))
    );
    assert!(!observed.coverage().complete_empty);
    assert_eq!(
        observed.failed_reserved_bytes(),
        limits().max_document_bytes
    );
    assert_eq!(observed.requests(), 8);
    reads(&fixture, 8);
    let fixture = Fixture::new();
    let mut calls = prefix();
    calls[1].status = 1;
    calls[1].stderr = b"HTTP 403: canonical repository revoked".to_vec();
    calls.truncate(2);
    fixture.scripted_gh(&calls);
    assert!(matches!(
        read(&fixture, MetadataChoices::Teams, &limits())
            .unwrap_err()
            .kind,
        GithubErrorKind::Permission
    ));
    reads(&fixture, 2);
}

#[test]
fn pr_metadata_catalogue_provider_final_uid_replaced_repo_node_head_base_and_body_discard_all_rows()
{
    for change in ["uid", "repo", "node", "head", "base", "body"] {
        let fixture = Fixture::new();
        let mut calls = complete(vec![page("labels", 1, vec![json!({"id":81,"name":"bug"})])]);
        let at = match change {
            "uid" => {
                calls[7] = user(43, "octocat");
                7
            }
            "repo" => {
                let mut raw = repo();
                raw["node_id"] = json!("R_recreated");
                calls[5].stdout = serde_json::to_vec(&raw).unwrap();
                5
            }
            _ => {
                let mut raw = pull();
                match change {
                    "node" => raw["node_id"] = json!("PR_replaced"),
                    "head" => raw["head"]["sha"] = json!("c".repeat(40)),
                    "base" => raw["base"]["sha"] = json!("c".repeat(40)),
                    "body" => raw["body"] = json!("Concurrent description"),
                    _ => unreachable!(),
                };
                calls[6].stdout = serde_json::to_vec(&raw).unwrap();
                6
            }
        };
        calls.truncate(at + 1);
        fixture.scripted_gh(&calls);
        assert!(matches!(
            read(&fixture, MetadataChoices::Labels, &limits())
                .unwrap_err()
                .kind,
            GithubErrorKind::IdentityChanged | GithubErrorKind::StaleHead
        ));
        reads(&fixture, at + 1);
    }
}

#[test]
fn pr_metadata_catalogue_provider_independent_issue_ids_need_actual_canonical_pr_linkage() {
    for field in ["url", "html_url", "node", "number"] {
        let fixture = Fixture::new();
        let mut calls = prefix();
        let mut raw = issue();
        match field {
            "url" => {
                raw["pull_request"]["url"] =
                    json!("https://evil.invalid/api/v3/repos/example/repo/pulls/7")
            }
            "html_url" => {
                raw["pull_request"]["html_url"] =
                    json!("https://github.invalid/example/repo/pull/8")
            }
            "node" => raw["node_id"] = json!(null),
            "number" => raw["number"] = json!(8),
            _ => unreachable!(),
        };
        calls[3].stdout = serde_json::to_vec(&raw).unwrap();
        fixture.scripted_gh(&calls);
        assert!(read(&fixture, MetadataChoices::Labels, &limits()).is_err());
        reads(&fixture, 4);
    }
}

#[test]
fn pr_metadata_catalogue_provider_invalid_original_limits_and_precancel_do_not_launch() {
    let fixture = Fixture::new();
    let gh = provider(&fixture);
    let (target, context) = original();
    let token = CancellationToken::default();
    let valid = limits();
    for age in [Duration::ZERO, Duration::from_secs(61)] {
        assert!(gh
            .metadata_catalogue_read(
                &current(),
                &target,
                &context,
                MetadataChoices::Labels,
                &PrMetadataReadLimits {
                    max_duration: age,
                    ..valid.clone()
                },
                &token
            )
            .is_err());
    }
    for bytes in [0, 2 * 1024 * 1024 + 1] {
        assert!(gh
            .metadata_catalogue_read(
                &current(),
                &target,
                &context,
                MetadataChoices::Labels,
                &PrMetadataReadLimits {
                    max_response_bytes: bytes,
                    ..valid.clone()
                },
                &token
            )
            .is_err());
    }
    let mut bad = target.clone();
    bad.head_sha = "a".repeat(7);
    assert!(gh
        .metadata_catalogue_read(
            &current(),
            &bad,
            &context,
            MetadataChoices::Labels,
            &valid,
            &token
        )
        .is_err());
    let mut bad = context.clone();
    bad.repository.id = 0;
    assert!(gh
        .metadata_catalogue_read(
            &current(),
            &target,
            &bad,
            MetadataChoices::Labels,
            &valid,
            &token
        )
        .is_err());
    assert!(gh
        .metadata_catalogue_read(
            &HostIdentity {
                account_id: Some(0),
                ..current()
            },
            &target,
            &context,
            MetadataChoices::Labels,
            &valid,
            &token
        )
        .is_err());
    token.cancel();
    assert!(gh
        .metadata_catalogue_read(
            &current(),
            &target,
            &context,
            MetadataChoices::Labels,
            &valid,
            &token
        )
        .is_err());
    assert!(journal(&fixture).is_empty());
}

#[test]
fn pr_metadata_catalogue_provider_raw_success_controls_and_failed_capture_budget_are_not_display_lengths(
) {
    let fixture = Fixture::new();
    let mut calls = complete(vec![page("labels", 1, vec![])]);
    let diagnostics = b"\x1b[31m\x1b[0m\x01\x02".to_vec();
    calls[0].stderr = diagnostics.clone();
    let bytes: usize = calls.iter().map(|step| step.stdout.len()).sum();
    fixture.scripted_gh(&calls);
    let observed = read(&fixture, MetadataChoices::Labels, &limits()).unwrap();
    assert_eq!(observed.response_bytes(), bytes);
    assert_eq!(observed.successful_diagnostic_bytes(), diagnostics.len());
    assert_eq!(
        observed.accounting().charged_bytes(),
        bytes + diagnostics.len()
    );
    reads(&fixture, 8);
    let fixture = Fixture::new();
    let mut calls = prefix();
    let mut failed = page("labels", 1, vec![]);
    failed.status = 1;
    failed.stdout = "é".repeat(1000).into_bytes();
    failed.stderr = format!("HTTP 502 {}", "界".repeat(1000)).into_bytes();
    calls.push(failed);
    let cap: usize = calls[..4]
        .iter()
        .map(|step| step.stdout.len())
        .sum::<usize>()
        + 6000;
    fixture.scripted_gh(&calls);
    assert!(read(
        &fixture,
        MetadataChoices::Labels,
        &PrMetadataReadLimits {
            max_response_bytes: cap,
            ..limits()
        }
    )
    .is_err());
    reads(&fixture, 5);
}

#[test]
fn pr_metadata_catalogue_provider_row_caps_repeated_identity_and_oversized_pages_are_honest() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&complete(vec![page(
        "labels",
        1,
        vec![json!({"id":81,"name":"huge","description":"x".repeat(200)})],
    )]));
    let observed = read(
        &fixture,
        MetadataChoices::Labels,
        &PrMetadataReadLimits {
            max_row_bytes: 64,
            ..limits()
        },
    )
    .unwrap();
    assert!(observed.rows().is_empty());
    assert_eq!(observed.coverage().excluded_rows, 1);
    assert!(observed.coverage().enumeration_complete && !observed.coverage().complete_empty);
    assert!(matches!(
        observed.coverage().status,
        MetricsReadStatus::Partial
    ));
    reads(&fixture, 8);
    let fixture = Fixture::new();
    let mut calls = prefix();
    calls.push(page(
        "labels",
        1,
        vec![
            json!({"id":81,"name":"one"}),
            json!({"id":81,"name":"changed"}),
        ],
    ));
    fixture.scripted_gh(&calls);
    assert!(matches!(
        read(&fixture, MetadataChoices::Labels, &limits())
            .unwrap_err()
            .kind,
        GithubErrorKind::IdentityChanged
    ));
    reads(&fixture, 5);
    let fixture = Fixture::new();
    let mut calls = prefix();
    calls.push(page(
        "labels",
        1,
        vec![json!({"id":1}), json!({"id":2}), json!({"id":3})],
    ));
    fixture.scripted_gh(&calls);
    assert!(matches!(
        read(&fixture, MetadataChoices::Labels, &limits())
            .unwrap_err()
            .kind,
        GithubErrorKind::Json
    ));
    reads(&fixture, 5);
}

#[test]
fn pr_metadata_catalogue_provider_read_only_context_does_not_touch_real_git_or_original_fields() {
    let fixture = Fixture::new();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(fixture.repo())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
    git(&["init", "--initial-branch=main"]);
    git(&["config", "user.name", "Fixture"]);
    git(&["config", "user.email", "fixture@example.invalid"]);
    fs::write(fixture.repo().join("original"), b"original\n").unwrap();
    git(&["add", "original"]);
    git(&["commit", "-m", "baseline"]);
    fs::write(fixture.repo().join("dirty"), b"untracked\n").unwrap();
    let baseline = git(&["rev-parse", "HEAD"]);
    let status = git(&["status", "--porcelain=v1", "-z"]);
    let staged = git(&["diff", "--cached", "--binary"]);
    fixture.scripted_gh(&complete(vec![page(
        "branches",
        1,
        vec![json!({"name":"release/雪","commit":{"sha":"c".repeat(40)}})],
    )]));
    let (target, context) = original();
    let observed = provider(&fixture)
        .metadata_catalogue_read(
            &current(),
            &target,
            &context,
            MetadataChoices::Branches,
            &limits(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(observed.target().pr.title, "Original title");
    assert_eq!(observed.target().pr.body, "Original description");
    assert_eq!(observed.target().head_sha, target.head_sha);
    assert_eq!(observed.target().base_sha, target.base_sha);
    assert_eq!(observed.context().issue_metadata, context.issue_metadata);
    assert_eq!(observed.rows()[0]["name"], "release/雪");
    assert_eq!(observed.rows()[0]["commit"]["sha"], "c".repeat(40));
    assert_eq!(git(&["rev-parse", "HEAD"]), baseline);
    assert_eq!(git(&["status", "--porcelain=v1", "-z"]), status);
    assert_eq!(git(&["diff", "--cached", "--binary"]), staged);
    assert_eq!(
        fs::read(fixture.repo().join("dirty")).unwrap(),
        b"untracked\n"
    );
    reads(&fixture, 8);
}
