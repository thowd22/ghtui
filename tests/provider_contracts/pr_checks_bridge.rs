//! Strict real gh-child and shared projection contracts for Task14.
use super::*;
#[path = "pr_checks_guarded_read.rs"]
mod guarded_read;
use ghtui::providers::gh::{
    Check, PrCheckBridgeUnavailable, PrCheckJobAttemptOrigin, PrCheckRunReadLimits,
};

fn current() -> HostIdentity {
    HostIdentity {
        account_id: Some(42),
        ..identity()
    }
}
fn api_url(path: &str) -> String {
    format!("https://github.invalid/api/v3/{path}")
}
fn repo() -> Value {
    json!({"id":410,"node_id":"R_410","full_name":"example/repo","url":api_url("repos/example/repo"),"html_url":"https://github.invalid/example/repo","permissions":{"pull":true}})
}
fn pull() -> Value {
    json!({"id":1007,"node_id":"PR_7","number":7,"state":"open","draft":false,"merged":false,"url":api_url("repos/example/repo/pulls/7"),"html_url":"https://github.invalid/example/repo/pull/7","head":{"sha":"a".repeat(40),"ref":"feature","repo":repo()},"base":{"sha":"b".repeat(40),"ref":"main","repo":repo()}})
}
fn original() -> PrSummary {
    serde_json::from_value(json!({"pr":{"host":"github.invalid","id":"PR_7","repository":"example/repo","number":7,"title":"Original PR","body":"PRIVATE_PR_CANARY","url":"https://github.invalid/example/repo/pull/7","author":{"login":"octocat","actor_type":"User"},"state":"open","draft":false,"created_at":"","updated_at":"","queues":[]},"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0})).unwrap()
}
fn selected() -> Check {
    Check {
        id: 600,
        name: "Misleading run999 attempt1".into(),
        status: "completed".into(),
        conclusion: "failure".into(),
        url: "https://foreign.invalid/other/repo/actions/runs/999".into(),
        head_sha: "a".repeat(40),
        kind: "check".into(),
    }
}
fn check() -> Value {
    json!({"id":600,"node_id":"CR_600","head_sha":"a".repeat(40),"name":"build","status":"completed","conclusion":"failure","url":api_url("repos/example/repo/check-runs/600"),"details_url":"https://foreign.invalid/actions/runs/999","check_suite":{"id":77}})
}
fn run() -> Value {
    json!({"id":123,"node_id":"WR_123","workflow_id":7,"check_suite_id":77,"run_attempt":2,"head_sha":"a".repeat(40),"head_branch":"feature","name":"CI","display_title":"PRIVATE_RUN_CANARY","event":"pull_request","status":"completed","conclusion":"failure","url":api_url("repos/example/repo/actions/runs/123"),"html_url":"https://github.invalid/example/repo/actions/runs/123","repository":repo(),"actor":{"id":51,"login":"alice","type":"User"},"triggering_actor":{"id":42,"login":"octocat","type":"User"}})
}
fn job() -> Value {
    json!({"id":501,"node_id":"J_501","run_id":123,"head_sha":"a".repeat(40),"name":"test (stable)","status":"completed","conclusion":"failure","url":api_url("repos/example/repo/actions/jobs/501"),"check_run_url":api_url("repos/example/repo/check-runs/600"),"steps":[{"number":1,"name":"Build","status":"completed","conclusion":"failure"}]})
}
fn get(path: &str, value: Value) -> GhStep {
    step(api_args(path), value)
}
fn user(id: u64) -> GhStep {
    get("user", json!({"id":id,"login":"octocat","type":"User"}))
}
fn runs_path(size: usize, page: usize) -> String {
    format!(
        "repos/example/repo/actions/runs?check_suite_id=77&head_sha={}&per_page={size}&page={page}",
        "a".repeat(40)
    )
}
fn jobs_path(size: usize, page: usize) -> String {
    format!("repos/example/repo/actions/runs/123/attempts/2/jobs?per_page={size}&page={page}")
}
fn prefix() -> Vec<GhStep> {
    vec![
        user(42),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/7", pull()),
        get("repos/example/repo/check-runs/600", check()),
    ]
}
fn suffix() -> Vec<GhStep> {
    vec![
        get("repos/example/repo/check-runs/600", check()),
        get("repos/example/repo/pulls/7", pull()),
        get("repos/example/repo", repo()),
        user(42),
    ]
}
fn happy(raw_job: Value) -> Vec<GhStep> {
    let mut calls = prefix();
    calls.extend([
        get(
            &runs_path(100, 1),
            json!({"total_count":1,"workflow_runs":[run()]}),
        ),
        get("repos/example/repo/actions/runs/123/attempts/2", run()),
        get(
            &jobs_path(100, 1),
            json!({"total_count":1,"jobs":[raw_job]}),
        ),
        get("repos/example/repo/actions/runs/123", run()),
    ]);
    calls.extend(suffix());
    calls
}
fn read(
    fixture: &Fixture,
    limits: &PrCheckRunReadLimits,
) -> Result<ghtui::providers::gh::PrCheckRunObservation, ghtui::providers::gh::GithubError> {
    provider(fixture).pr_check_run(
        &current(),
        &original(),
        &selected(),
        limits,
        &CancellationToken::default(),
    )
}
fn reads(fixture: &Fixture, expected: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), expected);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["args"][5] == "GET" && row["stdin_hex"] == ""));
}
fn git(fixture: &Fixture, args: &[&str]) -> Vec<u8> {
    let output = std::process::Command::new("git")
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .current_dir(fixture.repo())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn pr_checks_bridge_provider_actual_distinct_check_suite_run_job_attempt_no_url_guess_preserves_git(
) {
    for supplied in [false, true] {
        let fixture = Fixture::new();
        let mut raw = job();
        if supplied {
            raw["run_attempt"] = json!(2);
        }
        let calls = happy(raw);
        let bytes: usize = calls.iter().map(|step| step.stdout.len()).sum();
        fixture.scripted_gh(&calls);
        git(&fixture, &["init", "--initial-branch=main"]);
        git(&fixture, &["config", "user.name", "Bridge fixture"]);
        git(
            &fixture,
            &["config", "user.email", "bridge@example.invalid"],
        );
        fs::write(fixture.repo().join("tracked.txt"), b"baseline\n").unwrap();
        git(&fixture, &["add", "tracked.txt"]);
        git(&fixture, &["commit", "-m", "baseline"]);
        fs::write(fixture.repo().join("tracked.txt"), b"dirty exact bytes\n").unwrap();
        let head = git(&fixture, &["rev-parse", "HEAD"]);
        let index = fs::read(fixture.repo().join(".git/index")).unwrap();
        let observed = read(&fixture, &Default::default()).unwrap();
        assert_eq!(observed.requests(), 12);
        assert_eq!(observed.successful_stdout_bytes(), bytes);
        assert_eq!(observed.pr_database_id(), 1007);
        assert_eq!(observed.check_id(), 600);
        assert_eq!(observed.suite_id(), 77);
        let link = observed.link().unwrap();
        assert_eq!(link.attempt().run.id, 123);
        assert_eq!(link.attempt().run.attempt, 2);
        assert_eq!(link.selected_job(), 501);
        assert_eq!(link.attempt().run.event, "pull_request");
        assert_eq!(link.attempt().run.actor.login, "alice");
        assert_eq!(
            link.job_attempt_origin(501),
            Some(if supplied {
                PrCheckJobAttemptOrigin::JsonObserved
            } else {
                PrCheckJobAttemptOrigin::ExactEndpointBound
            })
        );
        assert!(!format!("{observed:?}").contains("PRIVATE"));
        reads(&fixture, 12);
        assert_eq!(git(&fixture, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read(fixture.repo().join("tracked.txt")).unwrap(),
            b"dirty exact bytes\n"
        );
    }
}

#[test]
fn pr_checks_bridge_provider_optional_attempt_absent_not_null_zero_malformed_or_drift() {
    for value in [Value::Null, json!(0), json!(1), json!("2"), json!(false)] {
        let fixture = Fixture::new();
        let mut raw = job();
        raw["run_attempt"] = value;
        fixture.scripted_gh(&happy(raw));
        assert!(read(&fixture, &Default::default()).is_err());
        reads(&fixture, 7);
    }
}

#[test]
fn pr_checks_bridge_provider_status_context_short_sha_invalid_principal_and_limits_never_launch() {
    let fixture = Fixture::new();
    let p = provider(&fixture);
    for (who, target, check, limits) in [
        (
            identity(),
            original(),
            selected(),
            PrCheckRunReadLimits::default(),
        ),
        (
            current(),
            original(),
            Check {
                kind: "status".into(),
                ..selected()
            },
            Default::default(),
        ),
        (
            current(),
            PrSummary {
                head_sha: "a".repeat(8),
                ..original()
            },
            Check {
                head_sha: "a".repeat(8),
                ..selected()
            },
            Default::default(),
        ),
        (
            current(),
            original(),
            selected(),
            PrCheckRunReadLimits {
                max_row_bytes: 0,
                ..Default::default()
            },
        ),
    ] {
        assert!(p
            .pr_check_run(&who, &target, &check, &limits, &Default::default())
            .is_err());
    }
    assert!(journal(&fixture).is_empty());
}

#[test]
fn pr_checks_bridge_provider_canonical_check_suite_run_job_namespace_and_head_cannot_alias() {
    for index in [3usize, 4, 5, 6, 7] {
        let fixture = Fixture::new();
        let mut calls = happy(job());
        let mut raw: Value = serde_json::from_slice(&calls[index].stdout).unwrap();
        match index {
            3 => raw["head_sha"] = json!("c".repeat(40)),
            4 => raw["workflow_runs"][0]["check_suite_id"] = json!(78),
            5 => raw["run_attempt"] = json!(3),
            6 => raw["jobs"][0]["run_id"] = json!(999),
            7 => raw["repository"]["id"] = json!(411),
            _ => unreachable!(),
        }
        calls[index].stdout = serde_json::to_vec(&raw).unwrap();
        fixture.scripted_gh(&calls);
        assert!(read(&fixture, &Default::default()).is_err());
        reads(&fixture, index + 1);
    }
}

#[test]
fn pr_checks_bridge_provider_original_and_closing_pr_repo_account_actual_id_replacement_discards() {
    for index in [1usize, 2, 8, 9, 10, 11] {
        let fixture = Fixture::new();
        let mut calls = happy(job());
        let mut raw: Value = serde_json::from_slice(&calls[index].stdout).unwrap();
        match index {
            1 => raw["permissions"]["pull"] = Value::Null,
            2 => raw["head"]["sha"] = json!("c".repeat(40)),
            8 => raw["node_id"] = json!("CR_replaced"),
            9 => raw["id"] = json!(1008),
            10 => raw["id"] = json!(411),
            11 => raw["id"] = json!(43),
            _ => unreachable!(),
        }
        calls[index].stdout = serde_json::to_vec(&raw).unwrap();
        fixture.scripted_gh(&calls);
        assert!(read(&fixture, &Default::default()).is_err());
        reads(&fixture, index + 1);
    }
}

#[test]
fn pr_checks_bridge_provider_same_numeric_uid_login_rename_is_not_account_reassignment() {
    let fixture = Fixture::new();
    let mut calls = happy(job());
    calls[11] = get("user", json!({"id":42,"login":"renamed","type":"User"}));
    fixture.scripted_gh(&calls);
    let observed = read(&fixture, &Default::default()).unwrap();
    assert_eq!(observed.identity().account_id, Some(42));
    assert_eq!(observed.identity().login, "renamed");
    assert!(observed.matches_original(&current(), &original(), &selected()));
    let mut wrong = current();
    wrong.account_id = Some(43);
    assert!(!observed.matches_original(&wrong, &original(), &selected()));
    wrong.account_id = None;
    assert!(!observed.matches_original(&wrong, &original(), &selected()));
    reads(&fixture, 12);
}

#[test]
fn pr_checks_bridge_provider_known_partial_positive_link_never_claims_complete_or_absence() {
    let fixture = Fixture::new();
    let limits = PrCheckRunReadLimits {
        max_run_pages: 1,
        max_job_pages: 1,
        page_size: 1,
        max_rows: 2,
        ..Default::default()
    };
    let mut calls = happy(job());
    calls[4] = get(
        &runs_path(1, 1),
        json!({"total_count":2,"workflow_runs":[run()]}),
    );
    calls[6] = get(&jobs_path(1, 1), json!({"total_count":2,"jobs":[job()]}));
    fixture.scripted_gh(&calls);
    let observed = read(&fixture, &limits).unwrap();
    assert!(observed.link().is_some());
    assert!(
        !observed.run_coverage().enumeration_complete
            && !observed.job_coverage().enumeration_complete
    );
    assert_eq!(observed.run_coverage().total_matches, Some(2));
    assert_eq!(observed.job_coverage().total_matches, Some(2));
    reads(&fixture, 12);
}

#[test]
fn pr_checks_bridge_provider_missing_partial_or_empty_run_does_not_infer_default_run_attempt() {
    for total in [0, 1] {
        let fixture = Fixture::new();
        let mut calls = prefix();
        calls.push(get(
            &runs_path(100, 1),
            json!({"total_count":total,"workflow_runs":[]}),
        ));
        calls.extend(suffix());
        fixture.scripted_gh(&calls);
        let observed = read(&fixture, &Default::default()).unwrap();
        assert!(observed.link().is_none());
        assert_eq!(
            observed.unavailable(),
            Some(if total == 0 {
                PrCheckBridgeUnavailable::NoCorrelatedRunObserved
            } else {
                PrCheckBridgeUnavailable::PartialRunCatalogue
            })
        );
        reads(&fixture, 9);
    }
}

#[test]
fn pr_checks_bridge_provider_unrelated_job_check_url_old_attempt_and_ambiguity_are_unavailable() {
    for ambiguous in [false, true] {
        let fixture = Fixture::new();
        let mut calls = happy(job());
        let mut other = job();
        other["id"] = json!(502);
        other["node_id"] = json!("J_502");
        other["url"] = json!(api_url("repos/example/repo/actions/jobs/502"));
        let raw = if ambiguous {
            json!({"total_count":2,"jobs":[job(),other]})
        } else {
            other["check_run_url"] = json!(api_url("repos/example/repo/check-runs/601"));
            json!({"total_count":1,"jobs":[other]})
        };
        calls[6] = get(&jobs_path(100, 1), raw);
        fixture.scripted_gh(&calls);
        let observed = read(&fixture, &Default::default()).unwrap();
        assert!(observed.link().is_none());
        assert_eq!(
            observed.unavailable(),
            Some(if ambiguous {
                PrCheckBridgeUnavailable::AmbiguousJobCorrelation
            } else {
                PrCheckBridgeUnavailable::NoCorrelatedJobObserved
            })
        );
        reads(&fixture, 12);
    }
}

#[test]
fn pr_checks_bridge_provider_ambiguous_runs_no_arbitrary_candidate_or_job_fetch() {
    let fixture = Fixture::new();
    let mut second = run();
    second["id"] = json!(124);
    second["node_id"] = json!("WR_124");
    second["url"] = json!(api_url("repos/example/repo/actions/runs/124"));
    let mut calls = prefix();
    calls.push(get(
        &runs_path(100, 1),
        json!({"total_count":2,"workflow_runs":[run(),second]}),
    ));
    calls.extend(suffix());
    fixture.scripted_gh(&calls);
    let observed = read(&fixture, &Default::default()).unwrap();
    assert_eq!(
        observed.unavailable(),
        Some(PrCheckBridgeUnavailable::AmbiguousRunCorrelation)
    );
    assert!(observed.link().is_none());
    reads(&fixture, 9);
}

#[test]
fn pr_checks_bridge_provider_raw_control_capture_and_cancellation_never_continue_or_mutate() {
    let fixture = Fixture::new();
    let mut calls = happy(job());
    calls[4].status = 1;
    calls[4].stdout.clear();
    calls[4].stderr = vec![0x1b; 129 * 1024];
    fixture.scripted_gh(&calls);
    assert!(read(&fixture, &Default::default()).is_err());
    reads(&fixture, 5);
    let fixture = Fixture::new();
    let token = CancellationToken::default();
    token.cancel();
    assert!(provider(&fixture)
        .pr_check_run(
            &current(),
            &original(),
            &selected(),
            &Default::default(),
            &token
        )
        .is_err());
    assert!(journal(&fixture).is_empty());
}

#[test]
fn pr_checks_bridge_provider_shared_native_projection_retains_exact_origin_and_no_extra_reads() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&happy(job()));
    let observed = Arc::new(read(&fixture, &Default::default()).unwrap());
    let prepared = ghtui::pr_checks_actions::PreparedPrCheckActions::new(observed.clone()).unwrap();
    assert!(Arc::ptr_eq(prepared.observation(), &observed));
    assert!(prepared.matches_original(&current(), &original(), &selected()));
    assert!(!prepared.matches_original(
        &HostIdentity {
            account_id: Some(43),
            ..current()
        },
        &original(),
        &selected()
    ));
    assert!(!prepared.matches_original(
        &current(),
        &PrSummary {
            head_sha: "c".repeat(40),
            ..original()
        },
        &selected()
    ));
    let workspace = prepared.workspace();
    assert_eq!(workspace.repositories, vec!["example/repo"]);
    assert_eq!(workspace.repository().unwrap(), "example/repo");
    assert_eq!(workspace.run.as_ref().unwrap().selected_job, Some(501));
    assert_eq!(workspace.run.as_ref().unwrap().attempt.run.attempt, 2);
    assert!(workspace
        .warnings
        .iter()
        .any(|line| line.contains("exact-attempt endpoint")));
    assert!(workspace.rows.iter().any(|row| row
        .actor
        .as_ref()
        .is_some_and(|actor| actor.login == "alice")));
    assert!(!format!("{prepared:?}").contains("PRIVATE"));
    reads(&fixture, 12);
}

fn log_reads(body: &str) -> Vec<GhStep> {
    vec![
        user(42),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/7", pull()),
        get("repos/example/repo/actions/runs/123", run()),
        GhStep::read(
            &[
                "run",
                "view",
                "123",
                "--repo",
                "github.invalid/example/repo",
                "--attempt",
                "2",
                "--log",
                "--job",
                "501",
            ],
            body,
        ),
        get("repos/example/repo/actions/runs/123", run()),
        get("repos/example/repo/pulls/7", pull()),
        get("repos/example/repo", repo()),
        user(42),
    ]
}

#[test]
fn pr_checks_bridge_provider_projected_empty_object_vectors_are_bounded_before_job_cloning() {
    let fixture = Fixture::new();
    let mut raw = job();
    raw["steps"] = json!(vec![json!({}); 1000]);
    fixture.scripted_gh(&happy(raw));
    let limits = PrCheckRunReadLimits {
        max_projected_items: 10,
        ..Default::default()
    };
    let error = read(&fixture, &limits).unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::Unsupported));
    reads(&fixture, 7);
}

#[test]
fn pr_checks_bridge_provider_each_private_log_has_fresh_exact_scope_nine_and_actual_capture() {
    let fixture = Fixture::new();
    let mut calls = happy(job());
    let later = log_reads("matrix\tStep3\terror: genuine log failure\nnext line\n");
    let bytes: usize = later.iter().map(|step| step.stdout.len()).sum();
    calls.extend(later);
    fixture.scripted_gh(&calls);
    let p = provider(&fixture);
    let origin = Arc::new(
        p.pr_check_run(
            &current(),
            &original(),
            &selected(),
            &Default::default(),
            &Default::default(),
        )
        .unwrap(),
    );
    let observed = p
        .pr_check_logs(origin.clone(), &Default::default(), &Default::default())
        .unwrap();
    assert!(Arc::ptr_eq(observed.origin(), &origin));
    assert_eq!(observed.requests(), 9);
    assert_eq!(observed.successful_stdout_bytes(), bytes);
    assert!(observed.logs().contains("genuine log failure"));
    assert_eq!(observed.identity().account_id, Some(42));
    let rows = journal(&fixture);
    assert_eq!(rows.len(), 21);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["stdin_hex"] == ""));
    assert_eq!(
        rows[16]["args"],
        json!([
            "run",
            "view",
            "123",
            "--repo",
            "github.invalid/example/repo",
            "--attempt",
            "2",
            "--log",
            "--job",
            "501"
        ])
    );
}

#[test]
fn pr_checks_bridge_provider_private_log_late_uid_or_attempt_drift_discards_body_canary() {
    for drift in [false, true] {
        let fixture = Fixture::new();
        let mut calls = happy(job());
        let mut later = log_reads("PRIVATE_LOG_CANARY\n");
        if drift {
            let mut raw = run();
            raw["run_attempt"] = json!(3);
            later[5] = get("repos/example/repo/actions/runs/123", raw);
            later.truncate(6);
            later.push(user(42));
        } else {
            later[8] = user(43);
        }
        let expected = 12 + later.len();
        calls.extend(later);
        fixture.scripted_gh(&calls);
        let p = provider(&fixture);
        let origin = Arc::new(
            p.pr_check_run(
                &current(),
                &original(),
                &selected(),
                &Default::default(),
                &Default::default(),
            )
            .unwrap(),
        );
        let failure = p
            .pr_check_logs(origin, &Default::default(), &Default::default())
            .unwrap_err();
        assert!(matches!(
            failure.kind,
            GithubErrorKind::IdentityChanged | GithubErrorKind::StaleHead
        ));
        assert!(!format!("{failure:?}").contains("PRIVATE_LOG_CANARY"));
        let rows = journal(&fixture);
        assert_eq!(rows.len(), expected);
        assert!(rows.iter().all(|row| row["matched"] == true));
    }
}

#[test]
fn pr_checks_bridge_provider_log_error_closes_scope_but_capture_exhaustion_never_launches_closing()
{
    for exhausted in [false, true] {
        let fixture = Fixture::new();
        let mut calls = happy(job());
        let mut later = log_reads("");
        later[4].status = 1;
        later[4].stderr = if exhausted {
            vec![0x1b; 513 * 1024]
        } else {
            b"HTTP 403 PRIVATE_DENIED_CANARY".to_vec()
        };
        if exhausted {
            later.truncate(5);
        }
        let expected = 12 + later.len();
        calls.extend(later);
        fixture.scripted_gh(&calls);
        let p = provider(&fixture);
        let origin = Arc::new(
            p.pr_check_run(
                &current(),
                &original(),
                &selected(),
                &Default::default(),
                &Default::default(),
            )
            .unwrap(),
        );
        let failure = p
            .pr_check_logs(origin, &Default::default(), &Default::default())
            .unwrap_err();
        assert!(!failure.message.contains("PRIVATE_DENIED_CANARY"));
        let rows = journal(&fixture);
        assert_eq!(rows.len(), expected);
        assert!(rows.iter().all(|row| row["matched"] == true));
    }
}

#[test]
fn pr_checks_bridge_provider_opaque_log_origin_cannot_rebase_onto_another_transport_or_zero_caps() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&happy(job()));
    let p = provider(&fixture);
    let origin = Arc::new(
        p.pr_check_run(
            &current(),
            &original(),
            &selected(),
            &Default::default(),
            &Default::default(),
        )
        .unwrap(),
    );
    assert!(provider(&fixture)
        .pr_check_logs(origin.clone(), &Default::default(), &Default::default())
        .is_err());
    assert!(p
        .pr_check_logs(
            origin,
            &ghtui::providers::gh::PrCheckLogReadLimits {
                max_log_bytes: 0,
                ..Default::default()
            },
            &Default::default()
        )
        .is_err());
    reads(&fixture, 12);
}
