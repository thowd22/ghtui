//! SOURCE ONLY: strict unregistered contracts, not executed coverage.
use super::*;
use ghtui::{
    actions_workspace::{
        ActionsAction, ActionsCommand, ActionsPublicationLeaseSnapshot, ActionsWorkspace,
    },
    actions_write_journal::{
        ActionsImmutableContext, ActionsSafeOptions, ActionsWriteIntent, ActionsWriteJournal,
        ActionsWriteKind, ActionsWriteState, ActionsWriteTarget,
    },
    storage::Scope,
};
use std::process::Command;

fn job() -> Value {
    json!({"id":501,"run_id":123,"head_sha":"a".repeat(40),"name":"test (linux, 1.94)","status":"completed","conclusion":"failure","started_at":"2026-10-14T00:00:00Z","completed_at":"2026-10-14T00:01:00Z","runner_name":"hosted","labels":["ubuntu-latest"],"check_run_url":"https://github.invalid/api/v3/repos/example/repo/check-runs/600","html_url":"https://foreign.invalid/actions/runs/999/attempts/1","steps":[{"number":2,"name":"tests","status":"completed","conclusion":"failure"}]})
}
fn attempt_step(run: Value) -> GhStep {
    step(
        api_args("repos/example/repo/actions/runs/123/attempts/2"),
        run,
    )
}
fn jobs_step(pages: Value) -> GhStep {
    page_step(
        "repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100",
        pages,
    )
}
fn reads(fixture: &Fixture, count: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows.iter().all(|row| row["matched"] == true));
    assert!(rows.iter().all(|row| row["stdin_hex"] == ""));
    assert!(!rows.iter().any(|row| row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| matches!(arg.as_str(), Some("POST" | "PUT" | "PATCH" | "DELETE")))));
}
fn git(fixture: &Fixture, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(fixture.repo())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
type GitSnapshot = (String, String, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);
fn snapshot(fixture: &Fixture) -> GitSnapshot {
    (
        git(fixture, &["rev-parse", "HEAD"]),
        git(fixture, &["status", "--porcelain"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fs::read(fixture.repo().join("kept.txt")).unwrap(),
        fs::read(fixture.repo().join("staged.txt")).unwrap(),
        fs::read(fixture.repo().join("untracked.txt")).unwrap(),
    )
}
fn dirty_git(fixture: &Fixture) -> GitSnapshot {
    git(fixture, &["init", "-q", "--initial-branch", "main"]);
    git(fixture, &["config", "user.name", "Fixture"]);
    git(
        fixture,
        &["config", "user.email", "fixture@example.invalid"],
    );
    fs::write(fixture.repo().join("kept.txt"), b"initial\n").unwrap();
    git(fixture, &["add", "kept.txt"]);
    git(fixture, &["commit", "-qm", "baseline"]);
    fs::write(fixture.repo().join("kept.txt"), b"dirty retained bytes\n").unwrap();
    fs::write(
        fixture.repo().join("staged.txt"),
        b"staged retained bytes\n",
    )
    .unwrap();
    git(fixture, &["add", "staged.txt"]);
    fs::write(fixture.repo().join("untracked.txt"), "untracked α\n").unwrap();
    snapshot(fixture)
}

#[test]
fn actions_run_view_optional_job_attempt_actual_endpoint2_drives_exact_logs_without_url_default_or_git_effects(
) {
    let fixture = Fixture::new();
    let before = dirty_git(&fixture);
    let raw = job();
    let saved_raw = raw.clone();
    fixture.scripted_gh(&[
        attempt_step(run_value()),
        jobs_step(json!([{"jobs":[raw]}])),
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
            "actual attempt two logs\n",
        ),
    ]);
    let client = provider(&fixture);
    let observed = client
        .run_attempt("example/repo", 123, 2, &CancellationToken::default())
        .unwrap();
    assert_eq!(observed.run.id, 123);
    assert_eq!(observed.run.attempt, 2);
    assert_eq!(observed.jobs[0].attempt, 2);
    assert_eq!(observed.jobs[0].steps[0].number, 2);
    assert_eq!(observed.run.actor.login, "alice");
    assert_eq!(observed.run.triggering_actor.login, "octocat");
    assert!(observed
        .definition_note
        .contains("1 job(s) omitted run_attempt"));
    assert!(observed.definition_note.contains("run 123 / attempt 2"));
    assert!(observed.definition_note.contains("not explicit"));
    let log = client
        .run_logs_at(
            "example/repo",
            &observed.run,
            Some(&observed.jobs[0]),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(log, "actual attempt two logs\n");
    let retained_raw: Value =
        serde_json::from_slice(&fs::read(fixture.root.join("gh-fixture/001.stdout")).unwrap())
            .unwrap();
    assert_eq!(retained_raw[0]["jobs"][0], saved_raw);
    assert!(retained_raw[0]["jobs"][0].get("run_attempt").is_none());
    reads(&fixture, 3);
    assert_eq!(snapshot(&fixture), before);
}

#[test]
fn actions_run_view_optional_job_attempt_matching_supplied_and_omitted_across_real_slurped_pages_keep_each_origin(
) {
    let fixture = Fixture::new();
    let mut supplied = job();
    supplied["run_attempt"] = json!(2);
    let mut missing = job();
    missing["id"] = json!(502);
    missing["name"] = json!("matrix macOS");
    fixture.scripted_gh(&[
        attempt_step(run_value()),
        jobs_step(json!([{"jobs":[supplied]},{"jobs":[missing]}])),
    ]);
    let observed = provider(&fixture)
        .run_attempt("example/repo", 123, 2, &CancellationToken::default())
        .unwrap();
    assert_eq!(
        observed
            .jobs
            .iter()
            .map(|job| (job.id, job.attempt))
            .collect::<Vec<_>>(),
        vec![(501, 2), (502, 2)]
    );
    assert!(observed
        .definition_note
        .contains("1 job(s) supplied run_attempt"));
    assert!(observed
        .definition_note
        .contains("1 job(s) omitted run_attempt"));
    assert_eq!(observed.jobs[1].name, "matrix macOS");
    reads(&fixture, 2);
}

#[test]
fn actions_run_view_optional_job_attempt_null_zero_malformed_and_explicit_other_attempt_are_not_omission(
) {
    for supplied in [
        Value::Null,
        json!(0),
        json!(-1),
        json!(2.0),
        json!("2"),
        json!(true),
        json!([]),
        json!({}),
        json!(1),
        json!(3),
        json!(u64::MAX),
    ] {
        let fixture = Fixture::new();
        let mut raw = job();
        raw["run_attempt"] = supplied;
        fixture.scripted_gh(&[
            attempt_step(run_value()),
            jobs_step(json!([{"jobs":[raw]}])),
        ]);
        let failure = provider(&fixture)
            .run_attempt("example/repo", 123, 2, &CancellationToken::default())
            .unwrap_err();
        assert!(matches!(failure.kind, GithubErrorKind::Json));
        reads(&fixture, 2);
    }
}

#[test]
fn actions_run_view_optional_job_attempt_actual_attempt_mismatch_refuses_before_jobs_and_zero_target_before_transport(
) {
    for case in 0..5 {
        let fixture = Fixture::new();
        let mut run = run_value();
        match case {
            0 => run["id"] = json!(999),
            1 => run["run_attempt"] = json!(0),
            2 => run["run_attempt"] = json!(3),
            3 => {
                run.as_object_mut().unwrap().remove("run_attempt");
            }
            _ => run["head_sha"] = json!("not-a-sha"),
        }
        fixture.scripted_gh(&[attempt_step(run)]);
        assert!(matches!(
            provider(&fixture)
                .run_attempt("example/repo", 123, 2, &CancellationToken::default())
                .unwrap_err()
                .kind,
            GithubErrorKind::Json
        ));
        reads(&fixture, 1);
    }
    let fixture = Fixture::new();
    for (repo, id, attempt) in [
        ("example/repo", 123, 0),
        ("example/repo", 0, 2),
        ("invalid", 123, 2),
    ] {
        assert!(provider(&fixture)
            .run_attempt(repo, id, attempt, &CancellationToken::default())
            .is_err());
    }
    reads(&fixture, 0);
}

#[test]
fn actions_run_view_optional_job_attempt_no_supplied_field_does_not_override_cross_run_commit_or_missing_positive_job(
) {
    for case in 0..5 {
        let fixture = Fixture::new();
        let mut raw = job();
        match case {
            0 => raw["run_id"] = json!(999),
            1 => raw["head_sha"] = json!("b".repeat(40)),
            2 => raw["id"] = json!(0),
            3 => {
                raw.as_object_mut().unwrap().remove("run_id");
            }
            _ => {
                raw.as_object_mut().unwrap().remove("head_sha");
            }
        }
        fixture.scripted_gh(&[
            attempt_step(run_value()),
            jobs_step(json!([{"jobs":[raw]}])),
        ]);
        assert!(matches!(
            provider(&fixture)
                .run_attempt("example/repo", 123, 2, &CancellationToken::default())
                .unwrap_err()
                .kind,
            GithubErrorKind::Json
        ));
        reads(&fixture, 2);
    }
}

#[test]
fn actions_run_view_optional_job_attempt_empty_observed_jobs_does_not_invent_job_or_json_attempt() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[attempt_step(run_value()), jobs_step(json!([{"jobs":[]}]))]);
    let observed = provider(&fixture)
        .run_attempt("example/repo", 123, 2, &CancellationToken::default())
        .unwrap();
    assert_eq!(observed.run.attempt, 2);
    assert!(observed.jobs.is_empty());
    assert!(observed.definition_note.contains("no observed jobs"));
    assert!(observed
        .definition_note
        .contains("no JSON attempt or job allocation is inferred"));
    reads(&fixture, 2);
}

#[test]
fn actions_run_view_optional_job_attempt_refresh_rebuild_and_pending_sql_lease_keep_actor_provenance_and_no_resend(
) {
    let fixture = Fixture::new();
    let before_git = dirty_git(&fixture);
    let mut refreshed = job();
    refreshed["name"] = json!("fresh exact attempt two");
    fixture.scripted_gh(&[
        attempt_step(run_value()),
        jobs_step(json!([{"jobs":[job()]}])),
        attempt_step(run_value()),
        jobs_step(json!([{"jobs":[refreshed]}])),
    ]);
    let client = provider(&fixture);
    let identity = verified_identity();
    let scope = Scope::github(&identity.host, identity.account_id).unwrap();
    let database = fixture.root.join("cache.sqlite3");
    let mut sql = ActionsWriteJournal::open(&database).unwrap();
    let target = ActionsWriteTarget::from_scope(
        &scope,
        &ActionsTarget::Run {
            repository: "example/repo".into(),
            id: 123,
        },
    )
    .unwrap();
    let intent = ActionsWriteIntent::new(
        ActionsWriteKind::Cancel,
        ActionsImmutableContext {
            run_id: Some(123),
            attempt: Some(2),
            head_sha: Some("a".repeat(40)),
            ..Default::default()
        },
        ActionsSafeOptions::default(),
        b"",
    )
    .unwrap();
    let pending = sql.begin(&target, &intent).unwrap().marker().clone();
    let page = sql.list_unresolved(&scope, 256).unwrap();
    let before_sql = fs::read(&database).unwrap();
    let mut workspace = ActionsWorkspace::new(identity, vec!["example/repo".into()]);
    workspace
        .apply(
            &client,
            &ActionsAction::OpenRun {
                repository: "example/repo".into(),
                id: 123,
                attempt: 2,
            },
            &CancellationToken::default(),
        )
        .unwrap();
    workspace.publication_leases = Some(Arc::new(
        ActionsPublicationLeaseSnapshot::prepare(&workspace.identity, &page).unwrap(),
    ));
    workspace.prepare_rows();
    workspace
        .apply(
            &client,
            &ActionsAction::Refresh,
            &CancellationToken::default(),
        )
        .unwrap();
    for _ in 0..3 {
        workspace.select(0);
        workspace.prepare_rows();
        assert!(workspace
            .rows
            .iter()
            .any(|row| row.label.contains("1 job(s) omitted run_attempt")
                && row.label.contains("run 123 / attempt 2")));
        assert!(workspace.rows.iter().any(|row| row
            .actor
            .as_ref()
            .is_some_and(|actor| actor.login == "alice")));
        assert!(workspace
            .controls
            .iter()
            .find(|control| control.action == ActionsAction::Compose(ActionsCommand::RerunAll))
            .unwrap()
            .disabled_reason
            .as_deref()
            .unwrap()
            .contains("Original attempt"));
    }
    assert_eq!(
        workspace.run.as_ref().unwrap().attempt.jobs[0].name,
        "fresh exact attempt two"
    );
    let after = sql.list_unresolved(&scope, 256).unwrap();
    assert_eq!(after.total, 1);
    assert_eq!(after.markers[0].state, ActionsWriteState::Pending);
    assert_eq!(after.markers[0].attempt_id, pending.attempt_id);
    assert_eq!(
        after.markers[0].fingerprint_sha256,
        pending.fingerprint_sha256
    );
    assert_eq!(after.markers[0].context, pending.context);
    assert!(after.markers[0].receipt.is_none());
    assert_eq!(fs::read(&database).unwrap(), before_sql);
    assert_eq!(snapshot(&fixture), before_git);
    reads(&fixture, 4);
}

#[test]
fn actions_run_control_optional_job_attempt_reader_does_not_weaken_unbound_direct_job_rerun_guard()
{
    let fixture = Fixture::new();
    let before = dirty_git(&fixture);
    let mut steps = account_steps(42);
    steps.push(step(
        api_args("repos/example/repo/actions/runs/123"),
        run_value(),
    ));
    steps.push(step(api_args("repos/example/repo/actions/jobs/501"), job()));
    fixture.scripted_gh(&steps);
    let failure = provider(&fixture)
        .mutate_run(
            &verified_identity(),
            "example/repo",
            &run_target(),
            RunMutation::RerunJob {
                job_id: 501,
                debug: true,
            },
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::StaleHead));
    reads(&fixture, 4);
    assert_eq!(snapshot(&fixture), before);
}

#[test]
fn actions_run_view_optional_job_attempt_cancel_or_failed_actual_attempt_read_never_loads_jobs_or_returns_context(
) {
    let fixture = Fixture::new();
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(provider(&fixture)
        .run_attempt("example/repo", 123, 2, &cancel)
        .is_err());
    reads(&fixture, 0);
    for timeout in [false, true] {
        let fixture = Fixture::new();
        let mut failed = attempt_step(run_value());
        if timeout {
            failed.delay_ms = 200;
        } else {
            failed.status = 1;
            failed.stdout.clear();
            failed.stderr = b"gh: API unavailable (HTTP 403)".to_vec();
        }
        fixture.scripted_gh(&[failed]);
        let client = provider(&fixture).with_timeout(Duration::from_millis(80));
        assert!(client
            .run_attempt("example/repo", 123, 2, &CancellationToken::default())
            .is_err());
        reads(&fixture, 1);
    }
}
