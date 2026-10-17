//! UNDECLARED child of the frozen scoped checks contracts. All choices come
//! from real Source7/PR13/Checks10 and one retained actual provider/runner.
use super::*;
#[path = "scoped_pr_check_continuations_v2_tests.rs"]
mod continuations_v2;
use crate::{
    pr_checks_native::scoped::choices::{self, PrCheckChoiceUiV1},
    providers::gh::{PrCheckChoiceUnavailableV1, PrCheckSelectedReadPhaseV1},
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

fn run_id(id: u64) -> Value {
    let mut row = actual_run(REPO);
    row["id"] = json!(id);
    row["node_id"] = json!(format!("WR_{id}"));
    row["url"] = json!(format!(
        "https://{HOST}/api/v3/repos/{REPO}/actions/runs/{id}"
    ));
    row["html_url"] = json!(format!("https://{HOST}/{REPO}/actions/runs/{id}"));
    row["display_title"] = json!(format!("POSITIVE_CHOSEN_RUN_{id}"));
    row
}
fn job_id(run: u64, id: u64) -> Value {
    let mut row = actual_job(REPO);
    row["id"] = json!(id);
    row["run_id"] = json!(run);
    row["node_id"] = json!(format!("J_{id}"));
    row["name"] = json!(format!("Genuine member {id} 界🦀"));
    row["url"] = json!(format!(
        "https://{HOST}/api/v3/repos/{REPO}/actions/jobs/{id}"
    ));
    if id != 501 {
        row["check_run_url"] = json!(format!("https://{HOST}/api/v3/repos/{REPO}/check-runs/601"));
    }
    row
}
fn initial_choice(partial: bool, ambiguous: bool) -> Vec<GhStep> {
    let mut steps = bridge_steps(REPO);
    steps[7] = get(
        &format!(
            "repos/{REPO}/actions/runs?check_suite_id=901&head_sha={}&per_page=100&page=1",
            "a".repeat(40)
        ),
        json!({"total_count":if partial || ambiguous {2}else{1},"workflow_runs":if ambiguous {vec![run_id(123),run_id(124)]}else{vec![run_id(123)]}}),
    );
    if ambiguous {
        steps.drain(8..11);
    } else {
        steps[8] = get(
            &format!("repos/{REPO}/actions/runs/123/attempts/2"),
            run_id(123),
        );
        steps[9] = get(
            &format!("repos/{REPO}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
            json!({"total_count":2,"jobs":[job_id(123,501),job_id(123,502)]}),
        );
        steps[10] = get(&format!("repos/{REPO}/actions/runs/123"), run_id(123));
    }
    steps
}
fn fresh_choice(run: u64, jobs: Vec<Value>, total: u64) -> Vec<GhStep> {
    let mut steps = bridge_steps(REPO);
    steps[7] = get(&format!("repos/{REPO}/actions/runs/{run}"), run_id(run));
    steps[8] = get(
        &format!("repos/{REPO}/actions/runs/{run}/attempts/2"),
        run_id(run),
    );
    steps[9] = get(
        &format!("repos/{REPO}/actions/runs/{run}/attempts/2/jobs?per_page=100&page=1"),
        json!({"total_count":total,"jobs":jobs}),
    );
    steps[10] = get(&format!("repos/{REPO}/actions/runs/{run}"), run_id(run));
    assert_eq!(steps.len(), 18);
    steps
}
fn fresh_other_logs(run: u64, member: u64, jobs: Vec<Value>, total: u64) -> Vec<GhStep> {
    let mut steps = principal(42, "other-current");
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.extend([
        bridge_user(42),
        get(&format!("repos/{REPO}"), bridge_repo(REPO)),
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
        get(&format!("repos/{REPO}/actions/runs/{run}"), run_id(run)),
        get(
            &format!("repos/{REPO}/actions/runs/{run}/attempts/2"),
            run_id(run),
        ),
        get(
            &format!("repos/{REPO}/actions/runs/{run}/attempts/2/jobs?per_page=100&page=1"),
            json!({"total_count":total,"jobs":jobs}),
        ),
        GhStep::read(
            &[
                "run",
                "view",
                &run.to_string(),
                "--repo",
                &format!("{HOST}/{REPO}"),
                "--attempt",
                "2",
                "--log",
                "--job",
                &member.to_string(),
            ],
            "OTHER_JOB_PRIVATE_LOG_CANARY\nline2\n",
        ),
        get(
            &format!("repos/{REPO}/actions/jobs/{member}"),
            job_id(run, member),
        ),
        get(&format!("repos/{REPO}/actions/runs/{run}"), run_id(run)),
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
        get(&format!("repos/{REPO}"), bridge_repo(REPO)),
        bridge_user(42),
        get(&format!("repos/{REPO}"), repo(410, REPO)),
    ]);
    steps.extend(principal(42, "other-final-rename"));
    assert_eq!(steps.len(), 18);
    steps
}
fn loaded(fixture: &Fixture, provider: &GithubProvider, limits: &ScopedPrCheckReadLimitsV1) -> App {
    let mut app = app_checked(fixture);
    let captured = input(&mut app);
    let prepared = load_v1(
        captured,
        provider,
        &database(fixture),
        limits,
        &CancellationToken::default(),
    )
    .unwrap();
    install(&mut app, prepared);
    app.dispatch(Action::Close);
    app
}
fn choose_work(app: &mut App, index: usize) -> choices::PrCheckChoiceWorkV1 {
    assert!(choices::has_candidates(app));
    choices::dispatch(app, PrCheckChoiceUiV1::Open);
    assert_eq!(app.overlay, Some(crate::app::Overlay::PrCheckChoices));
    choices::dispatch(app, PrCheckChoiceUiV1::Select(index));
    choices::dispatch(app, PrCheckChoiceUiV1::Confirm);
    assert!(app.form.is_none());
    choices::take_work(app).unwrap()
}
fn finish_choice(
    app: &mut App,
    fixture: &Fixture,
    provider: &GithubProvider,
    index: usize,
    limits: &ScopedPrCheckReadLimitsV1,
) -> PreparedView {
    let work = choose_work(app, index);
    choices::execute(
        work,
        provider,
        &database(fixture),
        limits,
        &CancellationToken::default(),
    )
    .unwrap()
}
fn queue_logs(app: &mut App, member: u64) -> choices::PrCheckChoiceWorkV1 {
    app.actions_action = Some(ActionsAction::Logs(Some(member)));
    assert!(choices::queue_other_logs(app));
    assert!(app.actions_action.is_none());
    assert!(!app.actions_load_requested);
    choices::take_work(app).unwrap()
}
fn jobs(run: u64) -> Vec<Value> {
    vec![job_id(run, 501), job_id(run, 502)]
}
fn scripted(op: Vec<GhStep>) -> (Fixture, Vec<GhStep>) {
    let steps = whole(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    (fixture, steps)
}

#[test]
fn check_choice_partial_positive_run18_then_other_member_logs18_no_unique_authority() {
    let mut op = initial_choice(true, false);
    op.extend(fresh_choice(123, jobs(123), 2));
    op.extend(fresh_other_logs(123, 502, jobs(123), 2));
    let (fixture, steps) = scripted(op);
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let source = app.remote_scope_source.clone().unwrap();
    let scope = app.remote_scope.clone().unwrap();
    let inbox = app.github.as_ref().unwrap().inbox.pull_requests.as_ptr();
    assert!(app.actions.is_none());
    assert!(
        !app.pr_checks_route
            .as_ref()
            .unwrap()
            .observation()
            .run_coverage()
            .enumeration_complete
    );
    let prepared = finish_choice(&mut app, &fixture, &provider, 0, &Default::default());
    install(&mut app, prepared);
    assert_eq!(app.view, View::Actions);
    assert_eq!(
        app.actions
            .as_ref()
            .unwrap()
            .run
            .as_ref()
            .unwrap()
            .attempt
            .run
            .id,
        123
    );
    assert!(!app.actions.as_ref().unwrap().coverage.complete);
    let work = queue_logs(&mut app, 502);
    let prepared = choices::execute(
        work,
        &provider,
        &database(&fixture),
        &Default::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    install(&mut app, prepared);
    let page = app.actions.as_ref().unwrap().run.as_ref().unwrap();
    assert_eq!(page.selected_job, Some(502));
    assert!(page
        .logs
        .as_ref()
        .unwrap()
        .contains("OTHER_JOB_PRIVATE_LOG_CANARY"));
    assert_eq!(
        app.actions.as_ref().unwrap().identity.login,
        "other-final-rename"
    );
    assert_eq!(
        app.github.as_ref().unwrap().inbox.pull_requests.as_ptr(),
        inbox
    );
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &scope));
    assert_eq!(steps.len(), 84);
    read_only(&fixture, &before, &git, &steps);
}
#[test]
fn check_choice_ambiguous_second_genuine_run_is_explicit_not_first_default() {
    let mut op = initial_choice(false, true);
    op.extend(fresh_choice(124, jobs(124), 2));
    let (fixture, steps) = scripted(op);
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    assert!(app.actions.is_none());
    assert_eq!(
        app.pr_checks_route
            .as_ref()
            .unwrap()
            .observation()
            .run_candidates_v1()
            .len(),
        2
    );
    let prepared = finish_choice(&mut app, &fixture, &provider, 1, &Default::default());
    install(&mut app, prepared);
    let page = app.actions.as_ref().unwrap().run.as_ref().unwrap();
    assert_eq!(page.attempt.run.id, 124);
    assert!(page.attempt.run.title.contains("124"));
    assert_eq!(steps.len(), 63);
    read_only(&fixture, &before, &git, &steps);
}
#[test]
fn check_choice_seen_partial_job_can_open_without_unseen_absence_or_mapping_claim() {
    let mut op = initial_choice(false, false);
    op.extend(fresh_choice(123, jobs(123), 3));
    let (fixture, steps) = scripted(op);
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let mut limits = ScopedPrCheckReadLimitsV1::default();
    limits.bridge.max_job_pages = 1;
    let prepared = finish_choice(&mut app, &fixture, &provider, 2, &limits);
    install(&mut app, prepared);
    let observed = app.pr_checks_route.as_ref().unwrap().observation();
    assert!(!observed.job_coverage().enumeration_complete);
    assert_eq!(observed.job_coverage().total_matches, Some(3));
    assert!(observed.link().is_none());
    assert_eq!(
        observed.selected_run_v1().unwrap().selected_job(),
        Some(502)
    );
    assert_eq!(
        app.actions
            .as_ref()
            .unwrap()
            .run
            .as_ref()
            .unwrap()
            .selected_job,
        Some(502)
    );
    read_only(&fixture, &before, &git, &steps);
}
#[test]
fn check_choice_unseen_selected_job_partial_returns_unknown_not_empty_or_retry() {
    let mut op = initial_choice(false, false);
    op.extend(fresh_choice(123, vec![job_id(123, 501)], 2));
    let (fixture, steps) = scripted(op);
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let mut limits = ScopedPrCheckReadLimitsV1::default();
    limits.bridge.max_job_pages = 1;
    let prepared = finish_choice(&mut app, &fixture, &provider, 2, &limits);
    install(&mut app, prepared);
    assert_eq!(app.view, View::Inbox);
    assert!(app.actions.is_none());
    assert!(app.notice.contains("unseen membership is unknown"));
    assert_eq!(
        app.pr_checks_route
            .as_ref()
            .unwrap()
            .observation()
            .choice_unavailable_v1(),
        Some(PrCheckChoiceUnavailableV1::PartialMemberCatalogue)
    );
    read_only(&fixture, &before, &git, &steps);
}
#[test]
fn check_other_logs_missing_partial_member_closes_scope_without_capture_or_empty_logs() {
    let mut logs = fresh_other_logs(123, 502, vec![job_id(123, 501)], 2);
    logs.drain(9..11);
    assert_eq!(logs.len(), 16);
    let mut op = initial_choice(false, false);
    op.extend(logs);
    let (fixture, steps) = scripted(op);
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let mut limits = ScopedPrCheckReadLimitsV1::default();
    limits.bridge.max_job_pages = 1;
    let work = queue_logs(&mut app, 502);
    let prepared = choices::execute(
        work,
        &provider,
        &database(&fixture),
        &limits,
        &CancellationToken::default(),
    )
    .unwrap();
    install(&mut app, prepared);
    let actions = app.actions.as_ref().unwrap();
    assert_eq!(actions.view, crate::actions_workspace::ActionsView::Jobs);
    assert!(actions.run.as_ref().unwrap().logs.is_none());
    assert!(actions
        .warnings
        .iter()
        .any(|s| s.contains("unseen membership is unknown")));
    assert!(!steps
        .iter()
        .any(|s| s.args.first().is_some_and(|s| s == "run")));
    read_only(&fixture, &before, &git, &steps);
}
#[test]
fn check_choice_recreated_genuine_job_node_refuses_before_local_member_projection() {
    let mut chosen = fresh_choice(123, jobs(123), 2);
    let mut row = job_id(123, 502);
    row["node_id"] = json!("REUSED_JOB_DIFFERENT_NODE");
    chosen[9] = get(
        &format!("repos/{REPO}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
        json!({"total_count":2,"jobs":[job_id(123,501),row]}),
    );
    chosen.truncate(10);
    let mut op = initial_choice(false, false);
    op.extend(chosen);
    let (fixture, steps) = scripted(op);
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let prepared = finish_choice(&mut app, &fixture, &provider, 2, &Default::default());
    assert!(prepared.clear_private);
    assert!(!prepared.status.contains("REUSED_JOB"));
    read_only(&fixture, &before, &git, &steps);
}
#[test]
fn check_choice_null_malformed_mismatched_supplied_attempt_never_uses_omission_fallback() {
    for supplied in [Value::Null, json!(1), json!("2")] {
        let mut chosen = fresh_choice(123, jobs(123), 2);
        let mut row = job_id(123, 502);
        row["run_attempt"] = supplied;
        chosen[9] = get(
            &format!("repos/{REPO}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
            json!({"total_count":2,"jobs":[job_id(123,501),row]}),
        );
        chosen.truncate(10);
        let mut op = initial_choice(false, false);
        op.extend(chosen);
        let (fixture, steps) = scripted(op);
        let provider = provider(&fixture);
        let mut app = loaded(&fixture, &provider, &Default::default());
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        assert!(finish_choice(&mut app, &fixture, &provider, 2, &Default::default()).clear_private);
        read_only(&fixture, &before, &git, &steps);
    }
}
#[test]
fn check_choice_advanced_run_attempt_does_not_automatically_adopt_latest_attempt() {
    let mut chosen = fresh_choice(123, jobs(123), 2);
    let mut row = run_id(123);
    row["run_attempt"] = json!(3);
    chosen[7] = get(&format!("repos/{REPO}/actions/runs/123"), row);
    chosen.truncate(8);
    let mut op = initial_choice(true, false);
    op.extend(chosen);
    let (fixture, steps) = scripted(op);
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    assert!(finish_choice(&mut app, &fixture, &provider, 0, &Default::default()).clear_private);
    read_only(&fixture, &before, &git, &steps);
}
#[test]
fn check_choice_closing_repository_unknown_pull_or_recreated_id_hides_prepared_choice() {
    for recreated in [false, true] {
        let mut chosen = fresh_choice(123, jobs(123), 2);
        let mut row = repo(410, REPO);
        if recreated {
            row["id"] = json!(411);
        } else {
            row["permissions"]["pull"] = Value::Null;
        }
        chosen[15] = get(&format!("repos/{REPO}"), row);
        chosen.truncate(16);
        let mut op = initial_choice(true, false);
        op.extend(chosen);
        let (fixture, steps) = scripted(op);
        let provider = provider(&fixture);
        let mut app = loaded(&fixture, &provider, &Default::default());
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        assert!(finish_choice(&mut app, &fixture, &provider, 0, &Default::default()).clear_private);
        read_only(&fixture, &before, &git, &steps);
    }
}
#[test]
fn check_other_logs_capture_error_closes_inner_scope_no_auth_probe_or_private_diagnostics() {
    let mut logs = fresh_other_logs(123, 502, jobs(123), 2);
    logs[9].status = 1;
    logs[9].stderr = "PRIVATE_DIAGNOSTIC界🦀".repeat(10).into_bytes();
    logs[9].stdout.clear();
    logs.truncate(15);
    let mut op = initial_choice(false, false);
    op.extend(logs);
    let (fixture, steps) = scripted(op);
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let work = queue_logs(&mut app, 502);
    let prepared = choices::execute(
        work,
        &provider,
        &database(&fixture),
        &Default::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(prepared.clear_private);
    assert!(!prepared.status.contains("PRIVATE_DIAGNOSTIC"));
    read_only(&fixture, &before, &git, &steps);
}
#[test]
fn check_other_logs_closing_uid_reuse_or_auth_failure_hides_captured_private_text() {
    for auth in [false, true] {
        let mut logs = fresh_other_logs(123, 502, jobs(123), 2);
        logs[14] = bridge_user(43);
        if auth {
            logs[14].status = 1;
            logs[14].stdout.clear();
            logs[14].stderr = b"gh: Requires authentication (HTTP 401) PRIVATE_DIAGNOSTIC".to_vec();
        }
        logs.truncate(15);
        let mut op = initial_choice(false, false);
        op.extend(logs);
        let (fixture, steps) = scripted(op);
        let provider = provider(&fixture);
        let mut app = loaded(&fixture, &provider, &Default::default());
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let work = queue_logs(&mut app, 502);
        let prepared = choices::execute(
            work,
            &provider,
            &database(&fixture),
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
        assert!(prepared.clear_private);
        assert!(!prepared.status.contains("OTHER_JOB_PRIVATE_LOG_CANARY"));
        read_only(&fixture, &before, &git, &steps);
    }
}
#[test]
fn check_choice_shared_deadline_cancel_raw_capture_cap_launches_no_next_child() {
    for mode in 0..3 {
        let mut op = initial_choice(true, false);
        let mut chosen = fresh_choice(123, jobs(123), 2);
        if mode == 2 {
            chosen[4].stderr = "\u{1b}[31m界🦀".repeat(200).into_bytes();
            chosen.truncate(5);
            op.extend(chosen);
        }
        let (fixture, steps) = scripted(op);
        let provider = provider(&fixture);
        let mut app = loaded(&fixture, &provider, &Default::default());
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let work = choose_work(&mut app, 0);
        let cancel = CancellationToken::default();
        let mut limits = ScopedPrCheckReadLimitsV1::default();
        let bounded = if mode == 0 {
            cancel.cancel();
            provider.clone()
        } else if mode == 1 {
            let p = provider
                .clone()
                .with_scoped_read_window(Instant::now() + Duration::from_millis(1), 128 * 1024)
                .unwrap();
            std::thread::sleep(Duration::from_millis(5));
            p
        } else {
            limits.bridge.max_document_bytes = 128;
            limits.bridge.max_row_bytes = 64;
            provider.clone()
        };
        let result =
            choices::execute(work, &bounded, &database(&fixture), &limits, &cancel).unwrap();
        assert!(result.clear_private);
        read_only(&fixture, &before, &git, &steps);
    }
}
#[test]
fn check_choice_original_source_sql_cas_closing_guard_never_rebases_or_writes() {
    let mut chosen = fresh_choice(123, jobs(123), 2);
    chosen[15].delay_ms = 250;
    chosen.truncate(16);
    let mut op = initial_choice(true, false);
    op.extend(chosen);
    let (fixture, steps) = scripted(op);
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let key = app
        .remote_scope_source
        .as_ref()
        .unwrap()
        .saved_view_key()
        .to_owned();
    let path = database(&fixture);
    let journal = fixture.root.join("gh-fixture/commands.jsonl");
    let git = git_state_resource(&fixture);
    let concurrent = std::thread::spawn(move || {
        wait_count(&journal, 64);
        let db = rusqlite::Connection::open(path).unwrap();
        let changed = db.execute(
            "INSERT INTO saved_views(host,login,name,payload) VALUES(?2,'user-id:42',?3,?1) ON CONFLICT(host,login,name) DO UPDATE SET payload=excluded.payload",
            rusqlite::params!["PRIVATE_ORIGINAL_SOURCE_CAS_CANARY", HOST, key],
        ).unwrap();
        assert_eq!(changed, 1, "The race changes an actual original source row");
    });
    let prepared = finish_choice(&mut app, &fixture, &provider, 0, &Default::default());
    concurrent.join().unwrap();
    assert!(prepared.clear_private);
    assert!(!prepared
        .status
        .contains("PRIVATE_ORIGINAL_SOURCE_CAS_CANARY"));
    exact(&fixture, &steps);
    assert_eq!(git_state_resource(&fixture), git);
    let changed: String = sql(&fixture)
        .query_row(
            "SELECT payload FROM saved_views WHERE name=?1",
            [app.remote_scope_source.as_ref().unwrap().saved_view_key()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(changed, "PRIVATE_ORIGINAL_SOURCE_CAS_CANARY");
    let writes:u64=sql(&fixture).query_row("SELECT (SELECT count(*) FROM actions_write_attempts)+(SELECT count(*) FROM pr_write_attempts)",[],|r|r.get(0)).unwrap();
    assert_eq!(writes, 0);
}
#[test]
fn check_choice_selected_guard_occurs_only_after_actual_member_and_final_account_priority() {
    for uid in [42, 43] {
        let mut chosen = fresh_choice(123, jobs(123), 2);
        chosen[14] = bridge_user(uid);
        chosen.truncate(15);
        let mut op = initial_choice(true, false);
        op.extend(chosen);
        let (fixture, steps) = scripted(op);
        let provider = provider(&fixture);
        let app = loaded(&fixture, &provider, &Default::default());
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let original = app.pr_checks_route.as_ref().unwrap().observation().clone();
        let choice = original.choose_run_v1(0).unwrap();
        let source = app.remote_scope_source.as_ref().unwrap();
        let nav = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap();
        let resource = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_resource
            .as_ref()
            .unwrap();
        let mut session = provider
            .begin_repository_catalogue_access(
                source.source(),
                &Default::default(),
                &CancellationToken::default(),
            )
            .unwrap();
        let identity = session.identity().clone();
        let current = session
            .verify(nav.original(), &CancellationToken::default())
            .unwrap();
        let mut phases = vec![];
        let error = provider
            .pr_check_selected_from_resource_with_guard_v1(
                &identity,
                current,
                resource.observed(),
                0,
                &original,
                &choice,
                &Default::default(),
                &CancellationToken::default(),
                |phase| {
                    phases.push((
                        phase,
                        fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
                            .unwrap()
                            .lines()
                            .count(),
                    ));
                    let private: String = sql(&fixture)
                        .query_row("SELECT body FROM drafts LIMIT 1", [], |r| r.get(0))
                        .unwrap();
                    assert!(private.contains(DRAFT));
                    Err(crate::providers::gh::GithubError {
                        kind: crate::providers::gh::GithubErrorKind::Transport,
                        message: format!("PRIVATE_GUARD_DIAGNOSTIC_{private}"),
                    })
                },
            )
            .unwrap_err();
        assert_eq!(
            phases,
            vec![(PrCheckSelectedReadPhaseV1::BeforeSelectedProjection, 58)]
        );
        assert!(!error.message.contains(DRAFT));
        assert!(!error.message.contains("PRIVATE_GUARD_DIAGNOSTIC"));
        if uid == 43 {
            assert!(matches!(
                error.kind,
                crate::providers::gh::GithubErrorKind::IdentityChanged
            ));
        }
        read_only(&fixture, &before, &git, &steps);
    }
}
#[test]
fn check_choice_pure_picker_unicode_capture_resize_and_all_buttons_no_private_io() {
    let (fixture, steps) = scripted(initial_choice(false, false));
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    choices::dispatch(&mut app, PrCheckChoiceUiV1::Open);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|f| {
            let area = f.area();
            choices::ui::draw(f, &mut app, area)
        })
        .unwrap();
    assert!(!app
        .hits
        .iter()
        .any(|hit| matches!(hit.action, Action::Form(_))));
    assert!(choices::ui::event(
        &mut app,
        Event::Paste("PRIVATE_UNKNOWN_KEY_CANARY".into())
    ));
    assert!(choices::ui::event(&mut app, Event::Resize(120, 40)));
    assert!(app.pr_check_choice_picker.is_some());
    assert!(choices::ui::event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE))
    ));
    assert!(!app.refresh_requested);
    choices::dispatch(&mut app, PrCheckChoiceUiV1::Scroll(100));
    choices::dispatch(&mut app, PrCheckChoiceUiV1::Close);
    assert!(app.overlay.is_none());
    assert!(choices::take_work(&mut app).is_none());
    choices::dispatch(&mut app, PrCheckChoiceUiV1::Open);
    choices::dispatch(&mut app, PrCheckChoiceUiV1::Quit);
    assert!(app.quit);
    read_only(&fixture, &before, &git, &steps);
}
#[test]
fn check_choice_queued_foreign_owner_stops_before_worker_child_or_form_restore() {
    let (fixture, steps) = scripted(initial_choice(true, false));
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    choices::dispatch(&mut app, PrCheckChoiceUiV1::Open);
    choices::dispatch(&mut app, PrCheckChoiceUiV1::Confirm);
    app.remote_scope = None;
    assert!(choices::take_work(&mut app).is_none());
    assert!(app.form.is_none());
    assert!(app.actions.is_none());
    assert!(app.pr_check_choice_picker.is_none());
    read_only(&fixture, &before, &git, &steps);
}
#[test]
fn check_choice_opaque_actual_observation_not_same_numeric_run_or_index_authority() {
    let (first, first_steps) = scripted(initial_choice(true, false));
    let first_provider = provider(&first);
    let a = loaded(&first, &first_provider, &Default::default());
    let (second, second_steps) = scripted(initial_choice(true, false));
    let second_provider = provider(&second);
    let b = loaded(&second, &second_provider, &Default::default());
    let original = a.pr_checks_route.as_ref().unwrap().observation();
    let other = b.pr_checks_route.as_ref().unwrap().observation();
    let choice = original.choose_run_v1(0).unwrap();
    assert!(choice.belongs_to(original));
    assert!(!choice.belongs_to(other));
    assert!(original.choose_run_v1(1).is_err());
    assert!(original.choose_job_v1(2).is_err());
    assert_eq!(
        original.run_candidates_v1()[0].run().id,
        other.run_candidates_v1()[0].run().id
    );
    assert_eq!(
        original.run_candidates_v1()[0].node(),
        other.run_candidates_v1()[0].node()
    );
    exact(&first, &first_steps);
    exact(&second, &second_steps);
}
#[test]
fn scoped_actions_rerun_v4_check_choice_selected_row_preserves_supported_and_disabled_controls_without_io(
) {
    let mut op = initial_choice(true, false);
    op.extend(fresh_choice(123, jobs(123), 2));
    let (fixture, steps) = scripted(op);
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let prepared = finish_choice(&mut app, &fixture, &provider, 0, &Default::default());
    install(&mut app, prepared);
    app.dispatch(Action::Actions(ActionsAction::Select(1)));
    let mut supported = 0;
    assert!(
        app.scoped_actions_v1.is_none(),
        "Choice presentation remains the genuine initial PR route, not a fabricated Actions source"
    );
    assert!(
        app.actions
            .as_ref()
            .unwrap()
            .run
            .as_ref()
            .unwrap()
            .selected_job
            .is_none(),
        "Moving the row cursor does not open a job"
    );
    let actual_commands = app
        .actions
        .as_ref()
        .unwrap()
        .controls
        .iter()
        .filter_map(|control| match control.action {
            ActionsAction::Compose(command) => Some(command),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual_commands,
        vec![
            ActionsCommand::ChooseScope,
            ActionsCommand::RerunAll,
            ActionsCommand::RerunFailed,
            ActionsCommand::RerunJob,
            ActionsCommand::Cancel,
            ActionsCommand::ForceCancel,
            ActionsCommand::DeleteRun,
            ActionsCommand::DeleteLogs,
            ActionsCommand::ApproveFork,
        ],
        "Independent complete initial-run Compose inventory after actual row selection"
    );
    let mut refused = 0;
    for control in &app.actions.as_ref().unwrap().controls {
        if let ActionsAction::Compose(command) = &control.action {
            if matches!(
                command,
                ActionsCommand::RerunAll
                    | ActionsCommand::RerunFailed
                    | ActionsCommand::DeleteRun
                    | ActionsCommand::DeleteLogs
            ) {
                assert!(
                    control.disabled_reason.is_none(),
                    "Original completed run keeps its actual supported publisher"
                );
                supported += 1;
                continue;
            }
            assert!(matches!(
                command,
                ActionsCommand::ChooseScope
                    | ActionsCommand::RerunJob
                    | ActionsCommand::Cancel
                    | ActionsCommand::ForceCancel
                    | ActionsCommand::ApproveFork
            ));
            assert_eq!(control.disabled_reason.as_deref(),
                Some("Additional scoped PR-check action needs its own original access/draft/publication admission; nothing sent."),
                "Selecting a row cannot erase the exact scoped disabled-control policy");
            refused += 1;
        }
    }
    assert_eq!(supported, 4);
    assert_eq!(refused, 5);
    for action in [
        ActionsAction::Compose(ActionsCommand::ApproveFork),
        ActionsAction::Compose(ActionsCommand::SearchLogs),
        ActionsAction::PublicationList,
        ActionsAction::Reconcile,
    ] {
        app.dispatch(Action::Actions(action));
        assert!(app.actions_action.is_none());
        assert!(app.form.is_none());
        assert!(!app.actions.as_ref().unwrap().watching);
        assert_eq!(app.overlay, Some(crate::app::Overlay::Notice));
        app.dispatch(Action::Close);
        assert!(app.overlay.is_none());
    }
    // loaded() used the actual Close action. The real main loop consumes its
    // empty continuation-cancel signal before a later command. Prove it has
    // no worker/request/watch to cancel; do not reset a live owner or token.
    assert!(app.pr_check_continuation_cancel_v2);
    assert!(app.pr_check_continuation_request_v2.is_none());
    assert!(app.pr_check_continuation_owner_v2.is_none());
    assert!(app.pr_check_watch_v2.is_none());
    assert!(app.pr_check_choice_request.is_none());
    assert!(!app.pr_checks_request);
    assert!(!app.pr_checks_cancel_actions);
    assert!(!app.actions_load_requested);
    app.pr_check_continuation_cancel_v2 = false;
    let original_observation = app.pr_checks_route.as_ref().unwrap().observation().clone();
    app.dispatch(Action::Actions(ActionsAction::Compose(
        ActionsCommand::RerunAll,
    )));
    let queued = app
        .scoped_actions_work_v1
        .as_ref()
        .expect("Supported rerun captures original bounded work, not legacy foreground action");
    assert!(Arc::ptr_eq(
        queued.source.pr_origin_v2().unwrap().observation(),
        &original_observation,
    ));
    assert_eq!(queued.page.run().unwrap().run().id, 123);
    assert_eq!(queued.page.run().unwrap().run().attempt, 2);
    assert_eq!(
        queued.kind,
        crate::scoped_actions_v1::ScopedActionsRequestKindV1::Publication
    );
    assert!(app.actions_action.is_none());
    assert!(app.form.is_none());
    assert!(app.overlay.is_none());
    read_only(&fixture, &before, &git, &steps);
}
#[test]
fn check_choice_queued_back_or_owner_replaced_drops_late_success_error_and_panic() {
    for mode in 0..4 {
        let (fixture, steps) = scripted(initial_choice(false, false));
        let provider = provider(&fixture);
        let mut app = loaded(&fixture, &provider, &Default::default());
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let work = choose_work(&mut app, 0);
        let origin = work.origin().clone();
        drop(work);
        let mut background = Background::new();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        assert!(
            background.submit_scoped_pr_check_v1(&mut app, View::Inbox, origin, move |_| {
                ready_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                match mode {
                    1 => Err("PRIVATE_CHOOSER_ERROR_CANARY".into()),
                    2 => panic!("PRIVATE_CHOOSER_PANIC_CANARY"),
                    _ => Ok(PreparedView {
                        status: "PRIVATE_CHOOSER_SUCCESS_CANARY".into(),
                        context: Some("PRIVATE_CHOOSER_CONTEXT_CANARY".into()),
                        ..Default::default()
                    }),
                }
            })
        );
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        if mode == 3 {
            app.dispatch(Action::PrChecks(pr_checks_native::PrChecksUiAction::Back));
            background.cancel(View::Inbox);
        } else {
            app.remote_scope = None;
        }
        let old_status = app.status.clone();
        let old_view = app.view;
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            background.poll(&mut app);
            if !app.loading[View::Inbox.index()] {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(app.status, old_status);
        assert_eq!(app.view, old_view);
        assert!(!app.context.contains("PRIVATE_CHOOSER"));
        assert!(app.form.is_none());
        read_only(&fixture, &before, &git, &steps);
    }
}
#[test]
fn check_choice_actual_ten_job_pages27_fixed_offsets_and_positive_all_member_counts() {
    let mut chosen = fresh_choice(123, vec![], 1000);
    let pages = (0..10)
        .map(|page| {
            let rows = (0..100)
                .map(|n| job_id(123, 501 + page * 100 + n))
                .collect::<Vec<_>>();
            get(
                &format!(
                    "repos/{REPO}/actions/runs/123/attempts/2/jobs?per_page=100&page={}",
                    page + 1
                ),
                json!({"total_count":1000,"jobs":rows}),
            )
        })
        .collect::<Vec<_>>();
    chosen.splice(9..10, pages);
    assert_eq!(chosen.len(), 27);
    let mut op = initial_choice(true, false);
    op.extend(chosen);
    let (fixture, steps) = scripted(op);
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let prepared = finish_choice(&mut app, &fixture, &provider, 0, &Default::default());
    install(&mut app, prepared);
    let observed = app.pr_checks_route.as_ref().unwrap().observation();
    assert_eq!(observed.requests(), 21);
    assert_eq!(observed.job_coverage().fetched_pages, 10);
    assert_eq!(observed.job_coverage().admitted_rows, 1000);
    assert!(!observed.job_coverage().enumeration_complete); // Full last page at the cap is still bounded, not an extra exhausted-page proof.
    assert_eq!(observed.selected_run_v1().unwrap().members().len(), 1000);
    assert_eq!(
        app.actions
            .as_ref()
            .unwrap()
            .run
            .as_ref()
            .unwrap()
            .attempt
            .jobs
            .len(),
        1000
    );
    assert_eq!(steps.len(), 75);
    read_only(&fixture, &before, &git, &steps);
}
#[test]
fn check_choice_genuine_same_id_leaf_rename_rebinds_only_selected_closed_navigation() {
    let mut chosen = fresh_choice(123, jobs(123), 2);
    let canonical = "example/renamed";
    for (index, step) in chosen.iter_mut().enumerate() {
        if step.args.first().is_some_and(|s| s == "api") && step.args[1].starts_with("repos/") {
            if index != 2 && index != 15 {
                step.args[1] = step.args[1].replace(REPO, canonical);
            }
            let raw = String::from_utf8(step.stdout.clone())
                .unwrap()
                .replace(REPO, canonical);
            step.stdout = raw.into_bytes();
        }
    }
    let mut op = initial_choice(true, false);
    op.extend(chosen);
    let (fixture, steps) = scripted(op);
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let source = app.remote_scope_source.clone().unwrap();
    let active = app.remote_scope.clone().unwrap();
    let inbox = app.github.as_ref().unwrap().inbox.pull_requests.as_ptr();
    let page_rows = app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .rows
        .as_ptr();
    let prepared = finish_choice(&mut app, &fixture, &provider, 0, &Default::default());
    install(&mut app, prepared);
    assert_eq!(app.actions.as_ref().unwrap().repositories, vec![canonical]);
    assert_eq!(
        app.github
            .as_ref()
            .unwrap()
            .selected
            .as_ref()
            .unwrap()
            .repository,
        REPO
    );
    assert_eq!(
        app.github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .repository,
        canonical
    );
    assert_eq!(
        app.github.as_ref().unwrap().inbox.pull_requests.as_ptr(),
        inbox
    );
    assert_eq!(
        app.github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .rows
            .as_ptr(),
        page_rows
    );
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &active));
    read_only(&fixture, &before, &git, &steps);
}
#[test]
fn check_other_logs_member_before_private_capture_guard_then_real_closing_uid_priority() {
    for uid in [42, 43] {
        let mut logs = fresh_other_logs(123, 502, jobs(123), 2);
        logs[14] = bridge_user(uid);
        logs.remove(9);
        logs.truncate(14);
        let mut op = initial_choice(false, false);
        op.extend(logs);
        let (fixture, steps) = scripted(op);
        let provider = provider(&fixture);
        let app = loaded(&fixture, &provider, &Default::default());
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let origin = app.pr_checks_route.as_ref().unwrap().observation().clone();
        let choice = origin.choose_job_v1(1).unwrap();
        let source = app.remote_scope_source.as_ref().unwrap();
        let nav = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap();
        let resource = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_resource
            .as_ref()
            .unwrap();
        let mut session = provider
            .begin_repository_catalogue_access(
                source.source(),
                &Default::default(),
                &CancellationToken::default(),
            )
            .unwrap();
        let identity = session.identity().clone();
        let current = session
            .verify(nav.original(), &CancellationToken::default())
            .unwrap();
        let mut phases = vec![];
        let error = provider
            .pr_check_job_logs_from_resource_with_guard_v1(
                &identity,
                current,
                resource.observed(),
                0,
                origin,
                &choice,
                &Default::default(),
                &Default::default(),
                &CancellationToken::default(),
                |phase| {
                    phases.push((
                        phase,
                        fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
                            .unwrap()
                            .lines()
                            .count(),
                    ));
                    let private: String = sql(&fixture)
                        .query_row("SELECT body FROM drafts LIMIT 1", [], |r| r.get(0))
                        .unwrap();
                    assert!(private.contains(DRAFT));
                    Err(crate::providers::gh::GithubError {
                        kind: crate::providers::gh::GithubErrorKind::Transport,
                        message: format!("PRIVATE_LOG_GUARD_DIAGNOSTIC_{private}"),
                    })
                },
            )
            .unwrap_err();
        assert_eq!(
            phases,
            vec![
                (crate::providers::gh::PrCheckLogReadPhase::BeforeCapture, 57),
                (
                    crate::providers::gh::PrCheckLogReadPhase::BeforeFinalProof,
                    60
                )
            ]
        );
        assert!(!error.message.contains(DRAFT));
        assert!(!error.message.contains("PRIVATE_LOG_GUARD_DIAGNOSTIC"));
        if uid == 43 {
            assert!(matches!(
                error.kind,
                crate::providers::gh::GithubErrorKind::IdentityChanged
            ));
        }
        read_only(&fixture, &before, &git, &steps);
    }
}
#[test]
fn check_choice_visible_mouse_buttons_row_capture_resize_stale_hits_and_pending_no_io() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let (fixture, steps) = scripted(initial_choice(false, false));
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    choices::dispatch(&mut app, PrCheckChoiceUiV1::Open);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|f| {
            let area = f.area();
            choices::ui::draw(f, &mut app, area)
        })
        .unwrap();
    let hit = |app: &App, wanted: PrCheckChoiceUiV1| {
        app.pr_check_choice_picker
            .as_ref()
            .unwrap()
            .hits
            .iter()
            .find(|(_, action)| *action == wanted)
            .unwrap()
            .0
    };
    let row = hit(&app, PrCheckChoiceUiV1::Select(2));
    let mouse = |rect: ratatui::layout::Rect| {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        })
    };
    assert!(choices::ui::event(&mut app, mouse(row)));
    assert!(app.pr_check_choice_request.is_none());
    terminal
        .draw(|f| {
            let area = f.area();
            choices::ui::draw(f, &mut app, area)
        })
        .unwrap();
    let close = hit(&app, PrCheckChoiceUiV1::Close);
    choices::ui::event(&mut app, Event::Resize(120, 40));
    assert!(choices::ui::event(&mut app, mouse(close)));
    assert_eq!(app.overlay, Some(crate::app::Overlay::PrCheckChoices));
    terminal
        .draw(|f| {
            let area = f.area();
            choices::ui::draw(f, &mut app, area)
        })
        .unwrap();
    let close = hit(&app, PrCheckChoiceUiV1::Close);
    assert!(choices::ui::event(&mut app, mouse(close)));
    assert!(app.overlay.is_none());
    app.loading[View::Inbox.index()] = true;
    choices::dispatch(&mut app, PrCheckChoiceUiV1::Open);
    assert!(app.pr_check_choice_picker.is_none());
    app.dispatch(Action::Close);
    app.dispatch(Action::Actions(ActionsAction::Logs(Some(502))));
    assert!(app.actions_action.is_none());
    app.loading[View::Inbox.index()] = false;
    choices::dispatch(&mut app, PrCheckChoiceUiV1::Open);
    terminal
        .draw(|f| {
            let area = f.area();
            choices::ui::draw(f, &mut app, area)
        })
        .unwrap();
    let quit = hit(&app, PrCheckChoiceUiV1::Quit);
    assert!(choices::ui::event(&mut app, mouse(quit)));
    assert!(app.quit);
    read_only(&fixture, &before, &git, &steps);
}
#[test]
fn check_choice_fresh_actual_parent_head_base_node_source_lifecycle_drift_before_sql() {
    for field in ["head", "base", "node", "database", "source", "lifecycle"] {
        let mut chosen = fresh_choice(123, jobs(123), 2);
        let mut raw = resource_parent(REPO);
        match field {
            "head" => raw["head"]["sha"] = json!("c".repeat(40)),
            "base" => raw["base"]["sha"] = json!("c".repeat(40)),
            "node" => raw["node_id"] = json!("DIFFERENT_PR_NODE"),
            "database" => raw["id"] = json!(2007),
            "source" => raw["head"]["repo"]["node_id"] = json!("DIFFERENT_SOURCE_REPO_NODE"),
            _ => raw["state"] = json!("closed"),
        };
        chosen[5] = get(&format!("repos/{REPO}/pulls/7"), raw);
        chosen.truncate(6);
        let mut op = initial_choice(true, false);
        op.extend(chosen);
        let (fixture, steps) = scripted(op);
        let provider = provider(&fixture);
        let mut app = loaded(&fixture, &provider, &Default::default());
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let result = finish_choice(&mut app, &fixture, &provider, 0, &Default::default());
        assert!(result.clear_private);
        assert!(result.scoped_pr_checks.is_none());
        assert!(!result.status.contains(DRAFT));
        read_only(&fixture, &before, &git, &steps);
    }
}

#[test]
fn check_choice_candidate_toolbar_clears_entire_prior_row_without_private_io() {
    let (fixture, steps) = scripted(initial_choice(false, false));
    let provider = provider(&fixture);
    let mut app = loaded(&fixture, &provider, &Default::default());
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    assert!(choices::has_candidates(&app));
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            frame.render_widget(
                ratatui::widgets::Paragraph::new("PRIOR_PRIVATE_FRAME_CANARY".repeat(4)),
                area,
            );
            let remaining = choices::ui::draw_launch(frame, &mut app, area);
            assert_eq!(remaining.y, area.y + 1);
        })
        .unwrap();
    let row: String = (0..80)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
        .collect();
    assert_eq!(row.trim_end(), "[C Candidates]");
    assert!(!row.contains("PRIVATE_FRAME"));
    assert!(app
        .hits
        .iter()
        .any(|hit| hit.action == Action::PrCheckChoice(PrCheckChoiceUiV1::Open)));
    read_only(&fixture, &before, &git, &steps);
}

#[test]
fn check_choice_rename_preserves_same_base_head_identity_but_not_a_fork_alias() {
    for same_base in [true, false] {
        let canonical = "example/renamed";
        let mut chosen = fresh_choice(123, jobs(123), 2);
        for (index, step) in chosen.iter_mut().enumerate() {
            if step.args.first().is_some_and(|arg| arg == "api")
                && step.args[1].starts_with("repos/")
            {
                if index != 2 && index != 15 {
                    step.args[1] = step.args[1].replace(REPO, canonical);
                }
                step.stdout = String::from_utf8(std::mem::take(&mut step.stdout))
                    .unwrap()
                    .replace(REPO, canonical)
                    .into_bytes();
            }
        }
        if !same_base {
            let mut parent: Value = serde_json::from_slice(&chosen[5].stdout).unwrap();
            parent["head"]["repo"]["full_name"] = json!("fork/renamed");
            chosen[5].stdout = serde_json::to_vec(&parent).unwrap();
            chosen.truncate(6);
        }
        let mut operations = initial_choice(true, false);
        operations.extend(chosen);
        let mut steps = whole(&operations);
        if same_base {
            for step in &mut steps {
                if let Ok(mut raw) = serde_json::from_slice::<Value>(&step.stdout) {
                    if raw["head"]["repo"].is_object() && raw["base"]["repo"]["id"] == 410 {
                        raw["head"]["repo"] = raw["base"]["repo"].clone();
                        step.stdout = serde_json::to_vec(&raw).unwrap();
                    }
                }
            }
        }
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let provider = provider(&fixture);
        let mut app = loaded(&fixture, &provider, &Default::default());
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let original = app.remote_scope_source.clone().unwrap();
        let prepared = finish_choice(&mut app, &fixture, &provider, 0, &Default::default());
        assert_eq!(prepared.clear_private, !same_base);
        if same_base {
            install(&mut app, prepared);
            assert_eq!(app.actions.as_ref().unwrap().repositories, vec![canonical]);
            assert!(Arc::ptr_eq(
                app.remote_scope_source.as_ref().unwrap(),
                &original
            ));
        }
        read_only(&fixture, &before, &git, &steps);
    }
}
