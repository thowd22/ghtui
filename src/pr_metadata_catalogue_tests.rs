//! Strict adapter contracts. Real read protocols and fixed judgement
//! facts, not current-output-derived snapshots; native coverage remains separate.
use super::*;
use crate::{pr_metadata_choices::PrMetadataValue, test_support as support};
use serde_json::{json, Value};
use std::{fs, process::Command};
use support::{Fixture, GhStep};
#[path = "pr_metadata_base_tests.rs"]
mod base;

fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "requestor".into(),
        account_id: Some(42),
        gh_version: "2.101.0".into(),
    }
}
fn repository() -> Value {
    json!({"id":410,"node_id":"R_catalogue_410","full_name":"Example/Repo",
        "description":"Private repo","default_branch":"main","private":true,"archived":false,
        "permissions":{"pull":true,"push":true,"admin":false,"maintain":false}})
}
fn author() -> Value {
    json!({"id":99,"login":"author","type":"User"})
}
fn parent() -> Value {
    json!({"id":1007,"node_id":"PR_catalogue_7","number":7,"title":"Original title",
        "body":"Private unrelated description","state":"open","draft":false,"merged":false,
        "created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z",
        "user":author(),"head":{"sha":"a".repeat(40),"ref":"feature","repo":repository()},
        "base":{"sha":"b".repeat(40),"ref":"main","repo":repository()},
        "html_url":"https://github.invalid/Example/Repo/pull/7","requested_reviewers":[],
        "requested_teams":[],"labels":[],"assignees":[],"milestone":null,
        "maintainer_can_modify":false,"mergeable":true,"mergeable_state":"clean",
        "additions":0,"deletions":0,"changed_files":0})
}
fn issue() -> Value {
    json!({"id":2007,"node_id":"I_catalogue_7","number":7,"title":"Original title",
        "body":"Private unrelated description","state":"open","user":author(),"labels":[],
        "assignees":[],"milestone":null,"html_url":"https://github.invalid/Example/Repo/pull/7",
        "pull_request":{"url":"https://github.invalid/api/v3/repos/Example/Repo/pulls/7",
            "html_url":"https://github.invalid/Example/Repo/pull/7"}})
}
fn original() -> (HostIdentity, PrSummary, PrContext) {
    let target:PrSummary=serde_json::from_value(json!({"pr":{"host":"github.invalid","id":"PR_catalogue_7",
        "repository":"Example/Repo","number":7,"title":"Original title","body":"Private unrelated description",
        "url":"https://github.invalid/Example/Repo/pull/7","author":{"login":"author","actor_type":"User"},
        "state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","queues":[]},
        "head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"feature","base_ref":"main",
        "merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean",
        "additions":0,"deletions":0,"changed_files":0})).unwrap();
    let repo = json!({"host":"github.invalid","id":410,"full_name":"Example/Repo","description":"Private repo",
        "default_branch":"main","private":true,"archived":false,
        "permissions":{"pull":true,"push":true,"maintain":false,"admin":false}});
    let context:PrContext=serde_json::from_value(json!({"repository":repo,"base_repository":repo,"head_repository":repo,
        "is_cross_repository":false,"maintainer_can_modify":false,"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),
        "issue_metadata":issue(),"requested_reviewers":[],"requested_teams":[],"source_urls":[]})).unwrap();
    (identity(), target, context)
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep {
        args: [
            "api",
            endpoint,
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ]
        .map(str::to_owned)
        .to_vec(),
        stdin: vec![],
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
fn page(path: &str, number: usize, rows: Vec<Value>) -> GhStep {
    get(
        &format!(
            "repos/example/repo/{path}{}per_page=2&page={number}",
            if path.contains('?') { '&' } else { '?' }
        ),
        json!(rows),
    )
}
fn protocol(pages: Vec<GhStep>) -> Vec<GhStep> {
    let mut steps = vec![
        get("user", json!({"id":42,"login":"requestor"})),
        get("repos/example/repo", repository()),
        get("repos/example/repo/pulls/7", parent()),
        get("repos/example/repo/issues/7", issue()),
    ];
    steps.extend(pages);
    steps.extend([
        get("repos/example/repo", repository()),
        get("repos/example/repo/pulls/7", parent()),
        get("user", json!({"id":42,"login":"requestor"})),
    ]);
    steps
}
fn limits() -> PrMetadataReadLimits {
    PrMetadataReadLimits {
        page_size: 2,
        max_pages: 3,
        max_rows: 10,
        ..Default::default()
    }
}
fn load_fixture(
    fixture: &Fixture,
    kind: MetadataChoices,
    limits: &PrMetadataReadLimits,
) -> GithubResult<LoadedPrMetadataChoices> {
    let (identity, target, context) = original();
    load(
        &provider(fixture),
        PrMetadataOriginal {
            identity: &identity,
            target: &target,
            context: &context,
        },
        kind,
        limits,
        PrMetadataLimits::default(),
        &CancellationToken::default(),
    )
}
fn commands(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn read_only(fixture: &Fixture, count: usize) {
    let rows = commands(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["args"][5] == "GET" && row["stdin_hex"] == ""));
    assert!(rows.iter().all(|row| {
        !row["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arg| arg == "--paginate")
    }));
}
fn git(fixture: &Fixture, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .args(args)
        .current_dir(fixture.repo())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn setup_git(fixture: &Fixture) -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    git(fixture, &["init", "--initial-branch=main"]);
    git(fixture, &["config", "user.name", "Metadata contract"]);
    git(
        fixture,
        &["config", "user.email", "metadata@example.invalid"],
    );
    fs::write(fixture.repo().join("tracked.txt"), b"committed\n").unwrap();
    git(fixture, &["add", "tracked.txt"]);
    git(fixture, &["commit", "-m", "baseline"]);
    fs::write(fixture.repo().join("tracked.txt"), b"user dirty bytes\n").unwrap();
    fs::write(
        fixture.repo().join("untracked.txt"),
        b"user untracked bytes\n",
    )
    .unwrap();
    (
        git(fixture, &["rev-parse", "HEAD"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        git(fixture, &["status", "--porcelain=v1", "-z"]),
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
    )
}

#[test]
fn pr_metadata_catalogue_adapter_six_real_endpoint_reads_keep_typed_indices_id_provenance_and_git_untouched(
) {
    for (kind, path, row, expected) in [
        (
            MetadataChoices::Branches,
            "branches",
            json!({"name":"release/雪","commit":{"sha":"c".repeat(40)}}),
            PrMetadataValue::BaseBranch {
                name: "release/雪".into(),
                commit_sha: "c".repeat(40),
            },
        ),
        (
            MetadataChoices::Labels,
            "labels",
            json!({"id":81,"name":"bug 🐛"}),
            PrMetadataValue::Label {
                id: 81,
                name: "bug 🐛".into(),
            },
        ),
        (
            MetadataChoices::Assignees,
            "assignees",
            json!({"id":43,"login":"alice","type":"User"}),
            PrMetadataValue::Assignee {
                account_id: 43,
                login: "alice".into(),
            },
        ),
        (
            MetadataChoices::Collaborators,
            "collaborators",
            json!({"id":44,"login":"read-only","type":"User","permissions":{"pull":true,"push":false}}),
            PrMetadataValue::Reviewer {
                account_id: 44,
                login: "read-only".into(),
            },
        ),
        (
            MetadataChoices::Teams,
            "teams",
            json!({"id":55,"name":"Read team","slug":"read-team","permission":"pull"}),
            PrMetadataValue::ReviewTeam {
                team_id: 55,
                slug: "read-team".into(),
            },
        ),
        (
            MetadataChoices::Milestones,
            "milestones?state=open",
            json!({"number":5,"title":"October","state":"open"}),
            PrMetadataValue::Milestone { number: 5 },
        ),
    ] {
        let fixture = Fixture::new();
        let baseline = setup_git(&fixture);
        let steps = protocol(vec![page(path, 1, vec![row])]);
        let bytes: usize = steps.iter().map(|step| step.stdout.len()).sum();
        let page_bytes = steps[4].stdout.len();
        fixture.scripted_gh(&steps);
        let loaded = load_fixture(&fixture, kind, &limits()).unwrap();
        assert!(loaded.prepared().complete());
        assert_eq!(
            std::mem::discriminant(&loaded.prepared().kind()),
            std::mem::discriminant(&kind)
        );
        assert_eq!(loaded.choose(0).unwrap().value(), &expected);
        assert_eq!(
            loaded.choose(0).unwrap().target().scope().login,
            "user-id:42"
        );
        assert_eq!(
            loaded.choose(0).unwrap().target().repository(),
            "example/repo"
        );
        assert_eq!(loaded.canonical_repository().id, 410);
        assert_eq!(loaded.canonical_repository().node_id, "R_catalogue_410");
        assert_eq!(loaded.target().pr.id, "PR_catalogue_7");
        assert_eq!(loaded.context().issue_metadata["id"], 2007);
        assert_eq!(loaded.context().issue_metadata["node_id"], "I_catalogue_7");
        assert_eq!(loaded.coverage().response_bytes, page_bytes);
        assert_eq!(loaded.accounting().successful_stdout_bytes(), bytes);
        assert_eq!(loaded.accounting().charged_bytes(), bytes);
        assert_eq!(loaded.limits().max_pages, 3);
        assert_eq!(loaded.limits().page_size, 2);
        assert!(!format!("{loaded:?}").contains("Private unrelated description"));
        assert!(loaded.choose(1).is_err());
        read_only(&fixture, 8);
        assert_eq!(git(&fixture, &["rev-parse", "HEAD"]), baseline.0);
        assert_eq!(
            fs::read(fixture.repo().join(".git/index")).unwrap(),
            baseline.1
        );
        assert_eq!(
            git(&fixture, &["status", "--porcelain=v1", "-z"]),
            baseline.2
        );
        assert_eq!(
            fs::read(fixture.repo().join("tracked.txt")).unwrap(),
            baseline.3
        );
        assert_eq!(
            fs::read(fixture.repo().join("untracked.txt")).unwrap(),
            b"user untracked bytes\n"
        );
    }
}

#[test]
fn pr_metadata_catalogue_adapter_known_partial_base_member_is_usable_without_unseen_absence_or_removal_claim(
) {
    for capped in [false, true] {
        let fixture = Fixture::new();
        let first = vec![
            json!({"name":"main","commit":{"sha":"b".repeat(40)}}),
            json!({"name":"release","commit":{"sha":"c".repeat(40)}}),
        ];
        let mut pages = vec![page("branches", 1, first)];
        if !capped {
            pages.push(page("branches", 2, vec![]));
        }
        fixture.scripted_gh(&protocol(pages));
        let limits = PrMetadataReadLimits {
            max_pages: if capped { 1 } else { 3 },
            ..limits()
        };
        let loaded = load_fixture(&fixture, MetadataChoices::Branches, &limits).unwrap();
        assert_eq!(loaded.prepared().complete(), !capped);
        assert_eq!(loaded.coverage().enumeration_complete, !capped);
        assert_eq!(loaded.prepared().coverage_notice().is_some(), capped);
        assert!(loaded.prepared().unavailable_reason().is_none());
        assert_eq!(
            loaded.choose(1).unwrap().value(),
            &PrMetadataValue::BaseBranch {
                name: "release".into(),
                commit_sha: "c".repeat(40)
            }
        );
        loaded
            .choose(1)
            .unwrap()
            .validate_catalogue(loaded.prepared())
            .unwrap();
        assert!(
            loaded.choose(2).is_err(),
            "Unobserved index cannot infer or substitute an unseen member"
        );
        read_only(&fixture, if capped { 8 } else { 9 });
    }
}

#[test]
fn pr_metadata_catalogue_adapter_nullable_unrelated_context_and_unknown_candidate_do_not_grant_or_globally_deny_base(
) {
    let fixture = Fixture::new();
    let mut steps = protocol(vec![page(
        "branches",
        1,
        vec![json!({"name":"release","commit":{"sha":"c".repeat(40)}})],
    )]);
    for index in [2, 6] {
        let mut raw = parent();
        raw["requested_teams"] = Value::Null;
        steps[index].stdout = serde_json::to_vec(&raw).unwrap();
    }
    fixture.scripted_gh(&steps);
    let loaded = load_fixture(&fixture, MetadataChoices::Branches, &limits()).unwrap();
    assert!(loaded.coverage().enumeration_complete);
    assert!(!loaded.coverage().metric_evidence_complete);
    assert!(loaded
        .coverage()
        .reasons
        .iter()
        .any(|reason| reason.contains("requested_teams")));
    assert!(loaded.prepared().complete());
    assert!(
        loaded.choose(0).is_ok(),
        "Unrelated missing reviewer metadata cannot deny an observed eligible Base branch"
    );
    read_only(&fixture, 8);
    let fixture = Fixture::new();
    fixture.scripted_gh(&protocol(vec![page(
        "assignees",
        1,
        vec![json!({"id":null,"login":"unknown-id","type":null})],
    )]));
    let loaded = load_fixture(&fixture, MetadataChoices::Assignees, &limits()).unwrap();
    assert!(loaded.coverage().enumeration_complete);
    assert_eq!(loaded.coverage().excluded_rows, 0);
    assert!(loaded.prepared().choices()[0].value().is_none());
    assert!(
        loaded.choose(0).is_err(),
        "No row index is a substitute numeric account identity"
    );
    read_only(&fixture, 8);
}

#[test]
fn pr_metadata_catalogue_adapter_final_permission_and_same_uid_rename_use_observed_context_not_old_write_flags(
) {
    for missing in [false, true] {
        let fixture = Fixture::new();
        let mut steps = protocol(vec![page(
            "branches",
            1,
            vec![json!({"name":"release","commit":{"sha":"c".repeat(40)}})],
        )]);
        let mut repo = repository();
        if missing {
            repo.as_object_mut().unwrap().remove("permissions");
        } else {
            repo["permissions"]["push"] = json!(false);
        }
        steps[5].stdout = serde_json::to_vec(&repo).unwrap();
        steps[7] = get("user", json!({"id":42,"login":"renamed-requestor"}));
        fixture.scripted_gh(&steps);
        let loaded = load_fixture(&fixture, MetadataChoices::Branches, &limits()).unwrap();
        assert_eq!(loaded.identity().login, "renamed-requestor");
        assert_eq!(loaded.identity().account_id, Some(42));
        assert!(!loaded.context().repository.permissions.push);
        assert!(loaded
            .choose(0)
            .unwrap_err()
            .message
            .contains("permission is unverified"));
        read_only(&fixture, 8);
    }
}

#[test]
fn pr_metadata_catalogue_adapter_account_repository_and_ref_replacement_never_publish_an_old_original_selection(
) {
    for cause in ["account", "repository", "ref"] {
        let fixture = Fixture::new();
        let mut steps = protocol(vec![page(
            "branches",
            1,
            vec![json!({"name":"release","commit":{"sha":"c".repeat(40)}})],
        )]);
        match cause {
            "account" => steps[7] = get("user", json!({"id":43,"login":"requestor"})),
            "repository" => {
                let mut raw = repository();
                raw["id"] = json!(411);
                steps[5].stdout = serde_json::to_vec(&raw).unwrap();
            }
            "ref" => {
                for index in [2, 6] {
                    let mut raw = parent();
                    raw["base"]["ref"] = json!("renamed-base");
                    steps[index].stdout = serde_json::to_vec(&raw).unwrap();
                }
            }
            _ => unreachable!(),
        }
        fixture.scripted_gh(&steps);
        let error = load_fixture(&fixture, MetadataChoices::Branches, &limits()).unwrap_err();
        assert!(matches!(
            error.kind,
            GithubErrorKind::IdentityChanged | GithubErrorKind::StaleHead
        ));
        read_only(&fixture, if cause == "repository" { 6 } else { 8 });
    }
}

#[test]
fn pr_metadata_catalogue_adapter_original_selected_branch_tip_must_exist_unchanged_in_a_fresh_read()
{
    let fixture = Fixture::new();
    fixture.scripted_gh(&protocol(vec![page(
        "branches",
        1,
        vec![json!({"name":"release","commit":{"sha":"c".repeat(40)}})],
    )]));
    let original = load_fixture(&fixture, MetadataChoices::Branches, &limits()).unwrap();
    let selection = original.choose(0).unwrap();
    read_only(&fixture, 8);
    for rows in [
        vec![json!({"name":"release","commit":{"sha":"d".repeat(40)}})],
        vec![],
    ] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&protocol(vec![page("branches", 1, rows)]));
        let fresh = load_fixture(&fixture, MetadataChoices::Branches, &limits()).unwrap();
        assert!(
            selection.validate_catalogue(fresh.prepared()).is_err(),
            "Neither changed tip nor disappearance may choose a replacement"
        );
        assert_eq!(
            selection.value(),
            &PrMetadataValue::BaseBranch {
                name: "release".into(),
                commit_sha: "c".repeat(40)
            }
        );
        read_only(&fixture, 8);
    }
}

#[test]
fn pr_metadata_catalogue_adapter_invalid_preparation_limits_and_pre_cancel_refuse_before_any_transport(
) {
    let fixture = Fixture::new();
    let (identity, target, context) = original();
    let original = PrMetadataOriginal {
        identity: &identity,
        target: &target,
        context: &context,
    };
    let error = load(
        &provider(&fixture),
        original,
        MetadataChoices::Branches,
        &limits(),
        PrMetadataLimits {
            max_rows: 0,
            ..Default::default()
        },
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::InvalidTarget));
    let cancel = CancellationToken::default();
    cancel.cancel();
    let error = load(
        &provider(&fixture),
        original,
        MetadataChoices::Branches,
        &limits(),
        PrMetadataLimits::default(),
        &cancel,
    )
    .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::Transport));
    read_only(&fixture, 0);
}

#[test]
fn pr_metadata_catalogue_adapter_real_empty_source_unavailable_excluded_rows_and_capture_accounting_stay_distinct(
) {
    let fixture = Fixture::new();
    let mut steps = protocol(vec![page("branches", 1, vec![])]);
    steps[4].stderr = b"bounded success diagnostic".to_vec();
    let stdout: usize = steps.iter().map(|step| step.stdout.len()).sum();
    fixture.scripted_gh(&steps);
    let loaded = load_fixture(&fixture, MetadataChoices::Branches, &limits()).unwrap();
    assert!(loaded.prepared().complete() && loaded.coverage().complete_empty);
    assert!(loaded.choose(0).is_err());
    assert_eq!(loaded.accounting().successful_stdout_bytes(), stdout);
    assert_eq!(
        loaded.accounting().successful_diagnostic_bytes(),
        b"bounded success diagnostic".len()
    );
    assert_eq!(
        loaded.accounting().charged_bytes(),
        stdout + b"bounded success diagnostic".len()
    );
    read_only(&fixture, 8);
    let fixture = Fixture::new();
    let mut failed = page("branches", 1, vec![]);
    failed.status = 1;
    failed.stderr = b"gh: Forbidden (HTTP 403)".to_vec();
    fixture.scripted_gh(&protocol(vec![failed]));
    let loaded = load_fixture(&fixture, MetadataChoices::Branches, &limits()).unwrap();
    assert!(!loaded.prepared().complete() && !loaded.coverage().complete_empty);
    assert!(matches!(
        loaded.coverage().status,
        MetricsReadStatus::Unavailable(_)
    ));
    assert!(loaded.choose(0).is_err());
    assert!(loaded.accounting().failed_reserved_bytes() > 0);
    read_only(&fixture, 8);
    let fixture = Fixture::new();
    fixture.scripted_gh(&protocol(vec![page(
        "branches",
        1,
        vec![json!({"name":"large","commit":{"sha":"c".repeat(40)},"extra":"x".repeat(512)})],
    )]));
    let limited = PrMetadataReadLimits {
        max_row_bytes: 128,
        ..limits()
    };
    let loaded = load_fixture(&fixture, MetadataChoices::Branches, &limited).unwrap();
    assert_eq!(loaded.coverage().excluded_rows, 1);
    assert!(loaded.coverage().enumeration_complete);
    assert!(!loaded.coverage().complete_empty && !loaded.prepared().complete());
    assert!(loaded.prepared().coverage_notice().is_some());
    assert!(loaded.choose(0).is_err());
    read_only(&fixture, 8);
}
