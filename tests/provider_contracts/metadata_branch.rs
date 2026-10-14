//! Source-only exact-name validation. Known member is never complete catalogue.
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
    json!({"id":51,"login":"author","type":"User"})
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
fn get(endpoint: &str, value: Value) -> GhStep {
    step(api_args(endpoint), value)
}
fn user(id: u64) -> GhStep {
    get("user", json!({"id":id,"login":"octocat"}))
}
fn lookup(encoded: &str, value: Value) -> GhStep {
    get(&format!("repos/example/repo/branches/{encoded}"), value)
}
fn complete(branch: GhStep) -> Vec<GhStep> {
    vec![
        user(42),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/7", pull()),
        get("repos/example/repo/issues/7", issue()),
        branch,
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/7", pull()),
        user(42),
    ]
}
fn read(
    fixture: &Fixture,
    name: &str,
    limits: &PrMetadataReadLimits,
) -> Result<ghtui::providers::gh::PrMetadataCatalogueRead, ghtui::providers::gh::GithubError> {
    let (target, context) = original();
    provider(fixture).metadata_branch_read(
        &current(),
        &target,
        &context,
        name,
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
}

#[test]
fn pr_metadata_branch_provider_exact_eight_unicode_slash_percent_and_case_args_preserve_actual_tip()
{
    for (name, path, length) in [
        ("release/雪", "release%2F%E9%9B%AA", 40),
        ("Case/Sensitive", "Case%2FSensitive", 64),
        ("topic/%2F", "topic%2F%252F", 40),
    ] {
        let fixture = Fixture::new();
        let row = json!({"name":name,"commit":{"sha":"c".repeat(length)},"protected":false});
        let calls = complete(lookup(path, row.clone()));
        let bytes: usize = calls.iter().map(|step| step.stdout.len()).sum();
        fixture.scripted_gh(&calls);
        let observed = read(&fixture, name, &Default::default()).unwrap();
        assert!(matches!(observed.kind(), MetadataChoices::Branches));
        assert_eq!(observed.rows(), &[row]);
        assert_eq!(observed.requests(), 8);
        assert_eq!(observed.raw_rows(), 1);
        assert_eq!(observed.fetched_pages(), 1);
        assert_eq!(observed.requested_pages(), 1);
        assert_eq!(observed.response_bytes(), bytes);
        assert!(!observed.coverage().enumeration_complete && !observed.coverage().complete_empty);
        assert!(matches!(
            observed.coverage().status,
            MetricsReadStatus::Partial
        ));
        assert_eq!(observed.context().issue_metadata["id"], 1701);
        assert_eq!(observed.target().pr.id, "PR_catalogue_7");
        assert!(observed
            .coverage()
            .reasons
            .iter()
            .any(|reason| reason.contains("not enumerated")));
        reads(&fixture, 8);
    }
}

#[test]
fn pr_metadata_branch_provider_invalid_grammar_control_caps_and_shorthand_never_launch() {
    let fixture = Fixture::new();
    for name in [
        "",
        "@",
        "HEAD",
        "-dash",
        "/leading",
        "trailing/",
        "trailing.",
        "a//b",
        "a..b",
        "@{-1}",
        "a/.hidden",
        "a/foo.lock",
        "a b",
        "bad\nname",
        "bad\u{1b}name",
        "foo?bar",
        "foo*bar",
        "foo[bar",
        "foo\\bar",
        "foo~bar",
        "foo^bar",
        "foo:bar",
    ] {
        assert!(
            read(&fixture, name, &Default::default()).is_err(),
            "{name:?}"
        );
    }
    assert!(read(&fixture, &"雪".repeat(342), &Default::default()).is_err());
    assert!(journal(&fixture).is_empty());
}

#[test]
fn pr_metadata_branch_provider_404_is_unknown_not_absent_or_complete_empty_after_final_scope() {
    let fixture = Fixture::new();
    let mut failure = lookup("unseen%2Fbranch", Value::Null);
    failure.status = 1;
    failure.stderr = b"HTTP 404: branch not found".to_vec();
    fixture.scripted_gh(&complete(failure));
    let observed = read(&fixture, "unseen/branch", &Default::default()).unwrap();
    assert!(observed.rows().is_empty());
    assert!(
        matches!(&observed.coverage().status,MetricsReadStatus::Unavailable(failure)if matches!(failure.kind,GithubErrorKind::NotFound))
    );
    assert!(!observed.coverage().complete_empty && !observed.coverage().enumeration_complete);
    assert_eq!(
        observed.failed_reserved_bytes(),
        PrMetadataReadLimits::default().max_document_bytes
    );
    reads(&fixture, 8);
}

#[test]
fn pr_metadata_branch_provider_malformed_nullable_or_short_tip_stays_raw_ineligible_no_expansion() {
    for sha in [
        Value::Null,
        json!("a".repeat(7)),
        json!("z".repeat(40)),
        json!(17),
    ] {
        let fixture = Fixture::new();
        let row = json!({"name":"release","commit":{"sha":sha}});
        fixture.scripted_gh(&complete(lookup("release", row.clone())));
        let observed = read(&fixture, "release", &Default::default()).unwrap();
        assert_eq!(observed.rows(), &[row]);
        assert!(matches!(
            observed.coverage().status,
            MetricsReadStatus::Partial
        ));
        assert!(observed
            .coverage()
            .reasons
            .iter()
            .any(|reason| reason.contains("missing/malformed")));
        assert!(!observed.coverage().complete_empty);
        reads(&fixture, 8);
    }
    let fixture = Fixture::new();
    fixture.scripted_gh(&complete(lookup(
        "release",
        json!({"commit":{"sha":"c".repeat(40)}}),
    )));
    let observed = read(&fixture, "release", &Default::default()).unwrap();
    assert!(observed.rows().is_empty());
    assert_eq!(observed.coverage().excluded_rows, 1);
    assert!(!observed.coverage().complete_empty);
    reads(&fixture, 8);
}

#[test]
fn pr_metadata_branch_provider_redirected_name_refuses_and_changed_actual_tip_is_not_expected_tip_authority(
) {
    let fixture = Fixture::new();
    let mut calls = complete(lookup(
        "Case%2FSensitive",
        json!({"name":"case/sensitive","commit":{"sha":"c".repeat(40)}}),
    ));
    calls.truncate(5);
    fixture.scripted_gh(&calls);
    assert!(matches!(
        read(&fixture, "Case/Sensitive", &Default::default())
            .unwrap_err()
            .kind,
        GithubErrorKind::IdentityChanged
    ));
    reads(&fixture, 5);
    let fixture = Fixture::new();
    fixture.scripted_gh(&complete(lookup(
        "release",
        json!({"name":"release","commit":{"sha":"d".repeat(40)}}),
    )));
    let observed = read(&fixture, "release", &Default::default()).unwrap();
    assert_eq!(observed.rows()[0]["commit"]["sha"], "d".repeat(40));
    assert_ne!(observed.rows()[0]["commit"]["sha"], "c".repeat(40));
    assert_eq!(observed.target().head_sha, "a".repeat(40));
    reads(&fixture, 8);
}

#[test]
fn pr_metadata_branch_provider_final_permission_uid_head_and_base_cannot_reuse_old_scope() {
    for change in ["permission", "uid", "head", "base"] {
        let fixture = Fixture::new();
        let mut calls = complete(lookup(
            "release",
            json!({"name":"release","commit":{"sha":"c".repeat(40)}}),
        ));
        let at = match change {
            "permission" => {
                let mut raw = repo();
                raw.as_object_mut().unwrap().remove("permissions");
                calls[5].stdout = serde_json::to_vec(&raw).unwrap();
                7
            }
            "uid" => {
                calls[7] = user(43);
                7
            }
            _ => {
                let mut raw = pull();
                raw[if change == "head" { "head" } else { "base" }]["sha"] = json!("d".repeat(40));
                calls[6].stdout = serde_json::to_vec(&raw).unwrap();
                6
            }
        };
        calls.truncate(at + 1);
        fixture.scripted_gh(&calls);
        let result = read(&fixture, "release", &Default::default());
        if change == "permission" {
            let observed = result.unwrap();
            assert!(!observed.context().repository.permissions.push);
            assert!(observed.repository_raw()["permissions"].is_null());
        } else {
            assert!(matches!(
                result.unwrap_err().kind,
                GithubErrorKind::IdentityChanged | GithubErrorKind::StaleHead
            ));
        }
        reads(&fixture, at + 1);
    }
}

#[test]
fn pr_metadata_branch_provider_oversized_rows_raw_capture_deadline_and_precancel_remain_bounded() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&complete(lookup(
        "release",
        json!({"name":"release","commit":{"sha":"c".repeat(40)},"description":"x".repeat(200)}),
    )));
    let observed = read(
        &fixture,
        "release",
        &PrMetadataReadLimits {
            max_row_bytes: 64,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(observed.rows().is_empty());
    assert_eq!(observed.coverage().excluded_rows, 1);
    assert!(!observed.coverage().complete_empty);
    reads(&fixture, 8);
    let fixture = Fixture::new();
    let mut calls = complete(lookup("release", Value::Null));
    calls[4].status = 1;
    calls[4].stdout = "é".repeat(1000).into_bytes();
    calls[4].stderr = format!("HTTP 502 {}", "界".repeat(1000)).into_bytes();
    let cap: usize = calls[..4]
        .iter()
        .map(|step| step.stdout.len())
        .sum::<usize>()
        + 6000;
    calls.truncate(5);
    fixture.scripted_gh(&calls);
    assert!(read(
        &fixture,
        "release",
        &PrMetadataReadLimits {
            max_response_bytes: cap,
            ..Default::default()
        }
    )
    .is_err());
    reads(&fixture, 5);
    let fixture = Fixture::new();
    let mut slow = user(42);
    slow.delay_ms = 1000;
    fixture.scripted_gh(&[slow]);
    assert!(read(
        &fixture,
        "release",
        &PrMetadataReadLimits {
            max_duration: Duration::from_millis(250),
            ..Default::default()
        }
    )
    .is_err());
    reads(&fixture, 1);
    let fixture = Fixture::new();
    let (target, context) = original();
    let token = CancellationToken::default();
    token.cancel();
    assert!(provider(&fixture)
        .metadata_branch_read(
            &current(),
            &target,
            &context,
            "release",
            &Default::default(),
            &token
        )
        .is_err());
    assert!(journal(&fixture).is_empty());
}
