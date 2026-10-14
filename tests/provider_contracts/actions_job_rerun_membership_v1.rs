//! SOURCE ONLY: unregistered selected-job writer precondition contracts.
use super::*;
use ghtui::{
    actions_controller::{self, ActionsExecution},
    actions_workspace::{ActionsCommand, ActionsPlan, ActionsWorkspace, RunPage},
    actions_write_journal::{ActionsWriteJournal, ActionsWriteKind, ActionsWriteState},
    providers::gh::RunAttempt,
    storage::{Cache, Scope},
};

fn direct_job() -> Value {
    json!({"id":501,"run_id":123,"head_sha":"a".repeat(40),"name":"Selected job","status":"completed","conclusion":"failure","html_url":"https://foreign.invalid/actions/runs/999/attempts/1"})
}
fn attempt() -> GhStep {
    step(
        api_args("repos/example/repo/actions/runs/123/attempts/2"),
        run_value(),
    )
}
fn members(values: Vec<Value>) -> GhStep {
    page_step(
        "repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100",
        json!([{"jobs":values}]),
    )
}
fn prefix(raw: Value) -> Vec<GhStep> {
    let mut calls = account_steps(42);
    calls.push(step(
        api_args("repos/example/repo/actions/runs/123"),
        run_value(),
    ));
    calls.push(step(api_args("repos/example/repo/actions/jobs/501"), raw));
    calls
}
fn post(debug: bool) -> GhStep {
    write_step(
        "repos/example/repo/actions/jobs/501/rerun",
        "POST",
        Some(json!({"enable_debug_logging":debug})),
        Value::Null,
    )
}
fn omission(debug: bool) -> Vec<GhStep> {
    let mut calls = prefix(direct_job());
    calls.extend([attempt(), members(vec![direct_job()])]);
    calls.extend(account_steps(42));
    calls.extend([
        step(api_args("repos/example/repo/actions/runs/123"), run_value()),
        post(debug),
        step(api_args("repos/example/repo/actions/runs/123"), run_value()),
    ]);
    calls
}
fn call(
    client: &GithubProvider,
) -> Result<ghtui::providers::gh::RunMutationOutcome, ghtui::providers::gh::GithubError> {
    client.mutate_run(
        &verified_identity(),
        "example/repo",
        &run_target(),
        RunMutation::RerunJob {
            job_id: 501,
            debug: true,
        },
        &CancellationToken::default(),
    )
}
fn git_bytes(fixture: &Fixture, args: &[&str]) -> Vec<u8> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(fixture.repo())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn kept_git(fixture: &Fixture) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    git_bytes(fixture, &["init", "--initial-branch=main"]);
    git_bytes(fixture, &["config", "user.name", "Rerun fixture"]);
    git_bytes(fixture, &["config", "user.email", "rerun@example.invalid"]);
    fs::write(fixture.repo().join("tracked.txt"), b"baseline\n").unwrap();
    git_bytes(fixture, &["add", "tracked.txt"]);
    git_bytes(fixture, &["commit", "-m", "baseline"]);
    fs::write(fixture.repo().join("tracked.txt"), b"retained dirty\n").unwrap();
    fs::write(fixture.repo().join("staged.txt"), b"retained staged\n").unwrap();
    git_bytes(fixture, &["add", "staged.txt"]);
    fs::write(fixture.repo().join("untracked.txt"), "retained α\n").unwrap();
    git_state(fixture)
}
fn git_state(fixture: &Fixture) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let status = git_bytes(fixture, &["status", "--porcelain"]);
    (
        git_bytes(fixture, &["rev-parse", "HEAD"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        status,
    )
}
fn retained_git(fixture: &Fixture, before: &(Vec<u8>, Vec<u8>, Vec<u8>)) {
    assert_eq!(&git_state(fixture), before);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        b"retained dirty\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("staged.txt")).unwrap(),
        b"retained staged\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("untracked.txt")).unwrap(),
        "retained α\n".as_bytes()
    );
}
fn effects(fixture: &Fixture, count: usize, writes: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows.iter().all(|row| row["matched"] == true));
    assert_eq!(
        rows.iter().filter(|row| row["args"][5] == "POST").count(),
        writes
    );
    assert!(rows
        .iter()
        .filter(|row| row["args"][0] != "--version" && row["args"][5] != "POST")
        .all(|row| row["args"][5] == "GET" && row["stdin_hex"] == ""));
}
#[test]
fn actions_run_control_membership_supplied_positive_attempt_retains_exact_six_and_debug_payload() {
    for debug in [false, true] {
        let fixture = Fixture::new();
        let mut raw = direct_job();
        raw["run_attempt"] = json!(2);
        let mut calls = prefix(raw);
        calls.extend([
            post(debug),
            step(api_args("repos/example/repo/actions/runs/123"), run_value()),
        ]);
        fixture.scripted_gh(&calls);
        let observed = provider(&fixture)
            .mutate_run(
                &verified_identity(),
                "example/repo",
                &run_target(),
                RunMutation::RerunJob { job_id: 501, debug },
                &Default::default(),
            )
            .unwrap();
        assert!(observed.accepted && !observed.completed);
        assert_eq!(observed.run.unwrap().attempt, 2);
        effects(&fixture, 6, 1);
        assert_eq!(
            journal(&fixture)[4]["args"][1],
            "repos/example/repo/actions/jobs/501/rerun"
        );
    }
}
#[test]
fn actions_run_control_membership_genuine_omission_observed_exact_endpoint_and_closing_guards_eleven_not_url_default(
) {
    for supplied in [false, true] {
        let fixture = Fixture::new();
        let before = kept_git(&fixture);
        let mut calls = omission(true);
        if supplied {
            let mut member = direct_job();
            member["run_attempt"] = json!(2);
            calls[5] = members(vec![member]);
        }
        fixture.scripted_gh(&calls);
        let observed = call(&provider(&fixture)).unwrap();
        assert!(observed.accepted && !observed.completed);
        assert_eq!(observed.run.unwrap().id, 123);
        effects(&fixture, 11, 1);
        let rows = journal(&fixture);
        assert_eq!(
            rows[4]["args"][1],
            "repos/example/repo/actions/runs/123/attempts/2"
        );
        assert_eq!(
            rows[5]["args"][1],
            "repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100"
        );
        assert_eq!(rows[6]["args"], json!(["--version"]));
        assert_eq!(rows[7]["args"][1], "user");
        assert_eq!(rows[8]["args"][1], "repos/example/repo/actions/runs/123");
        assert_eq!(
            rows[9]["args"][1],
            "repos/example/repo/actions/jobs/501/rerun"
        );
        retained_git(&fixture, &before);
    }
}
#[test]
fn actions_run_control_membership_present_null_malformed_mismatch_never_falls_back_or_posts() {
    for value in [
        Value::Null,
        json!(0),
        json!(-1),
        json!(2.0),
        json!("2"),
        json!(false),
        json!(1),
        json!(3),
    ] {
        let fixture = Fixture::new();
        let mut raw = direct_job();
        raw["run_attempt"] = value;
        fixture.scripted_gh(&prefix(raw));
        assert!(matches!(
            call(&provider(&fixture)).unwrap_err().kind,
            GithubErrorKind::StaleHead
        ));
        effects(&fixture, 4, 0);
    }
    for case in 0..3 {
        let fixture = Fixture::new();
        let mut raw = direct_job();
        match case {
            0 => raw["id"] = json!(999),
            1 => raw["run_id"] = json!(999),
            _ => raw["head_sha"] = json!("b".repeat(40)),
        };
        fixture.scripted_gh(&prefix(raw));
        assert!(matches!(
            call(&provider(&fixture)).unwrap_err().kind,
            GithubErrorKind::StaleHead
        ));
        effects(&fixture, 4, 0);
    }
}
#[test]
fn actions_run_control_membership_unobserved_duplicate_or_contradictory_selected_member_never_authorizes_post(
) {
    for case in 0..6 {
        let fixture = Fixture::new();
        let mut raw = direct_job();
        let mut calls = prefix(direct_job());
        calls.push(attempt());
        let values = match case {
            0 => vec![],
            1 => vec![raw.clone(), raw],
            2 => {
                raw["run_id"] = json!(999);
                vec![raw]
            }
            3 => {
                raw["head_sha"] = json!("b".repeat(40));
                vec![raw]
            }
            4 => {
                raw["run_attempt"] = Value::Null;
                vec![raw]
            }
            _ => {
                raw["run_attempt"] = json!(3);
                vec![raw]
            }
        };
        calls.push(members(values));
        fixture.scripted_gh(&calls);
        let failure = call(&provider(&fixture)).unwrap_err();
        if case < 2 {
            assert!(matches!(failure.kind, GithubErrorKind::StaleHead));
        } else {
            assert!(matches!(failure.kind, GithubErrorKind::Json));
        }
        effects(&fixture, 6, 0);
    }
}
#[test]
fn actions_run_control_membership_actual_attempt_changed_id_workflow_head_status_or_conclusion_stops_before_job_list(
) {
    for case in 0..6 {
        let fixture = Fixture::new();
        let mut changed = run_value();
        match case {
            0 => changed["id"] = json!(999),
            1 => changed["run_attempt"] = json!(3),
            2 => changed["workflow_id"] = json!(999),
            3 => changed["head_sha"] = json!("b".repeat(40)),
            4 => changed["status"] = json!("in_progress"),
            _ => changed["conclusion"] = json!("success"),
        };
        let mut calls = prefix(direct_job());
        calls.push(step(
            api_args("repos/example/repo/actions/runs/123/attempts/2"),
            changed,
        ));
        fixture.scripted_gh(&calls);
        let failure = call(&provider(&fixture)).unwrap_err();
        if case < 2 {
            assert!(matches!(failure.kind, GithubErrorKind::Json));
        } else {
            assert!(matches!(failure.kind, GithubErrorKind::StaleHead));
        }
        effects(&fixture, 5, 0);
    }
}
#[test]
fn actions_run_control_membership_closing_uid_auth_or_latest_run_drift_refuses_before_same_original_post(
) {
    for case in 0..5 {
        let fixture = Fixture::new();
        let mut calls = omission(true);
        let expected = if case < 2 { 8 } else { 9 };
        match case {
            0 => calls[7] = step(api_args("user"), json!({"id":43,"login":"octocat"})),
            1 => {
                calls[7].stdout.clear();
                calls[7].status = 1;
                calls[7].stderr = b"gh: auth unavailable (HTTP 401)".to_vec();
            }
            _ => {
                let mut changed = run_value();
                match case {
                    2 => changed["run_attempt"] = json!(3),
                    3 => changed["head_sha"] = json!("b".repeat(40)),
                    _ => changed["status"] = json!("in_progress"),
                };
                calls[8] = step(api_args("repos/example/repo/actions/runs/123"), changed);
            }
        }
        calls.truncate(expected);
        fixture.scripted_gh(&calls);
        let failure = call(&provider(&fixture)).unwrap_err();
        match case {
            0 => assert!(matches!(failure.kind, GithubErrorKind::IdentityChanged)),
            1 => assert!(matches!(failure.kind, GithubErrorKind::Authentication)),
            _ => assert!(matches!(failure.kind, GithubErrorKind::StaleHead)),
        };
        effects(&fixture, expected, 0);
    }
}
#[test]
fn actions_run_control_membership_original_group_deadline_and_cancel_stop_children_without_auth_or_post(
) {
    for case in 0..2 {
        let fixture = Fixture::new();
        let mut calls = omission(true);
        if case == 0 {
            calls[4].delay_ms = 600;
            calls[5].delay_ms = 600;
        } else {
            calls[4].delay_ms = 600;
            calls[6].delay_ms = 150;
            calls[7].delay_ms = 600;
        }
        fixture.scripted_gh(&calls);
        let started = Instant::now();
        assert!(matches!(
            call(&provider(&fixture).with_timeout(Duration::from_secs(1)))
                .unwrap_err()
                .kind,
            GithubErrorKind::Transport
        ));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "Added read guard cannot restart its original deadline for each child"
        );
        effects(&fixture, if case == 0 { 6 } else { 8 }, 0);
    }
    let fixture = Fixture::new();
    let mut calls = omission(true);
    calls[5].delay_ms = 5000;
    fixture.scripted_gh(&calls);
    let cancel = CancellationToken::default();
    let client = provider(&fixture);
    thread::scope(|scope| {
        let canceller = scope.spawn(|| {
            let deadline = Instant::now() + Duration::from_secs(3);
            while journal(&fixture).len() < 6 {
                assert!(
                    Instant::now() < deadline,
                    "The actual membership child must launch before cancellation"
                );
                thread::yield_now();
            }
            cancel.cancel();
        });
        let failure = client
            .mutate_run(
                &verified_identity(),
                "example/repo",
                &run_target(),
                RunMutation::RerunJob {
                    job_id: 501,
                    debug: true,
                },
                &cancel,
            )
            .unwrap_err();
        canceller.join().unwrap();
        assert!(matches!(failure.kind, GithubErrorKind::Transport));
    });
    effects(&fixture, 6, 0);
}
#[test]
fn actions_run_control_membership_durable_accepted_or_uncertain_original_job_ref_receipt_and_restore_never_resends(
) {
    for failed in [false, true] {
        let fixture = Fixture::new();
        let before = kept_git(&fixture);
        let p = provider(&fixture);
        let mut workspace = ActionsWorkspace::new(verified_identity(), vec!["example/repo".into()]);
        workspace.run = Some(RunPage {
            repository: "example/repo".into(),
            attempt: RunAttempt {
                run: run_target(),
                jobs: vec![],
                definition_note: "Original display is not membership proof".into(),
            },
            selected_job: Some(501),
            logs: None,
            annotations: vec![],
        });
        let plan = ActionsPlan::prepare(
            &workspace,
            ActionsCommand::RerunJob,
            &BTreeMap::from([("debug".into(), "true".into())]),
            &p,
            &Default::default(),
        )
        .unwrap();
        let mut calls = account_steps(42);
        let mut publishing = omission(true);
        if failed {
            publishing[9].status = 1;
            publishing[9].stderr = b"interrupted after original POST (HTTP 500)".to_vec();
            publishing.truncate(10);
        }
        calls.extend(publishing);
        calls.extend(account_steps(42));
        let published_count = calls.len();
        calls.extend(account_steps(42));
        calls.extend(account_steps(42));
        fixture.scripted_gh(&calls);
        let database = fixture.root.join("cache.sqlite3");
        let mut sql = ActionsWriteJournal::open(&database).unwrap();
        let cache = Cache::open(&database).unwrap();
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        let resource = workspace.resource_key(ActionsCommand::RerunJob).unwrap();
        let result = actions_controller::execute(
            &p,
            &mut sql,
            &plan,
            true,
            None,
            |marker| {
                assert_eq!(
                    journal(&fixture).len(),
                    2,
                    "Original scope before private metadata, no provider/post before ordered draft"
                );
                assert_eq!(marker.context.job_id, Some(501));
                let body=json!({"version":3,"purpose":"actions","actions_publication":marker.publication_reference()}).to_string();
                cache
                    .put_draft(&scope, &resource, &body)
                    .map_err(|error| error.to_string())
            },
            &Default::default(),
        );
        let reference = match result {
            ActionsExecution::AcceptedPending {
                publication,
                receipt,
                ..
            } if !failed => {
                assert_eq!(receipt.resource_id, Some(123));
                assert_eq!(receipt.observed_attempt, Some(2));
                publication
            }
            ActionsExecution::Unknown { publication, .. } if failed => publication,
            other => panic!("Unexpected typed original result: {other:?}"),
        };
        effects(&fixture, published_count, 1);
        let saved = cache.draft(&scope, &resource).unwrap().unwrap();
        let saved_json: Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(
            saved_json["actions_publication"]["attempt_id"],
            reference.attempt_id
        );
        let marker = sql
            .original_publication(&p, &verified_identity(), &reference)
            .unwrap();
        assert_eq!(marker.kind, ActionsWriteKind::RerunJob);
        assert_eq!(marker.context.job_id, Some(501));
        assert_eq!(marker.context.attempt, Some(2));
        assert_eq!(
            marker.context.head_sha.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(
            marker.state,
            if failed {
                ActionsWriteState::Uncertain
            } else {
                ActionsWriteState::AcceptedPending
            }
        );
        assert_eq!(marker.receipt.is_some(), !failed);
        workspace.run.as_mut().unwrap().selected_job = Some(999);
        workspace.run.as_mut().unwrap().attempt.run.head_sha = "b".repeat(40);
        let edited = ActionsPlan::prepare(
            &workspace,
            ActionsCommand::RerunJob,
            &BTreeMap::from([("debug".into(), "false".into())]),
            &p,
            &Default::default(),
        )
        .unwrap();
        let restored = actions_controller::execute(
            &p,
            &mut sql,
            &edited,
            true,
            Some(&reference),
            |_| panic!("Restored original must not persist/reclaim edited intent"),
            &Default::default(),
        );
        let ActionsExecution::RestoredOriginal { marker: restored } = restored else {
            panic!("Exact original must restore before edited validation")
        };
        assert_eq!(restored.context, marker.context);
        assert_eq!(restored.state, marker.state);
        assert_eq!(restored.receipt, marker.receipt);
        assert_eq!(cache.draft(&scope, &resource).unwrap().unwrap(), saved);
        assert_eq!(sql.list_unresolved(&scope, 256).unwrap().total, 1);
        effects(&fixture, published_count + 4, 1);
        retained_git(&fixture, &before);
    }
}
