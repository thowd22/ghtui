//! UNDECLARED child of genuine scoped choice contracts. Strict Source7/PR13/
//! Checks10 plus actual shared factory/runner; code/provider/SQL, NOT PTY.
use super::*;
use crate::pr_checks_native::scoped::choices::continuations_v2::{
    self as next, ui, LoadedLogSearchModeV2,
};
use ratatui::{backend::TestBackend, Terminal};
#[path = "scoped_pr_check_corrective_v3_tests.rs"]
mod corrective_v3_tests;
#[path = "scoped_pr_failed_step_navigation_v8_tests.rs"]
mod failed_steps_v8;

fn annotation(id: u64, total: Option<u64>) -> Value {
    let mut raw = actual_check(REPO);
    raw["id"] = json!(id);
    raw["node_id"] = json!(format!("CR_{id}"));
    // Original source has its genuine original CR node; never infer from ID.
    if id == 600 {
        raw["node_id"] = actual_check(REPO)["node_id"].clone();
    }
    raw["url"] = json!(format!(
        "https://{HOST}/api/v3/repos/{REPO}/check-runs/{id}"
    ));
    raw["output"] = json!({"annotations_count":total});
    raw
}
fn annotation_row() -> Value {
    json!({"path":"src/界.rs","start_line":7,"end_line":9,"annotation_level":"failure","title":"Actual reported test","message":"ANNOTATION_PRIVATE_CANARY\nUTF8 界🦀","raw_details":"details current API only"})
}
fn annotations(member: u64, rows: Vec<Value>, total: Option<u64>) -> Vec<GhStep> {
    let check = if member == 501 { 600 } else { 601 };
    let mut op = principal(42, "annotations-current");
    op.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    op.extend([
        bridge_user(42),
        get(&format!("repos/{REPO}"), bridge_repo(REPO)),
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
        get(&format!("repos/{REPO}/actions/runs/123"), run_id(123)),
        get(
            &format!("repos/{REPO}/actions/runs/123/attempts/2"),
            run_id(123),
        ),
        get(
            &format!("repos/{REPO}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
            json!({"total_count":2,"jobs":jobs(123)}),
        ),
        get(
            &format!("repos/{REPO}/check-runs/{check}"),
            annotation(check, total),
        ),
        get(
            &format!("repos/{REPO}/check-runs/{check}/annotations?per_page=100&page=1"),
            json!(rows),
        ),
        get(
            &format!("repos/{REPO}/check-runs/{check}"),
            annotation(check, total),
        ),
        get(
            &format!("repos/{REPO}/actions/jobs/{member}"),
            job_id(123, member),
        ),
        get(&format!("repos/{REPO}/actions/runs/123"), run_id(123)),
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
        get(&format!("repos/{REPO}"), bridge_repo(REPO)),
        bridge_user(42),
        get(&format!("repos/{REPO}"), repo(410, REPO)),
    ]);
    op.extend(principal(42, "annotations-final"));
    assert_eq!(op.len(), 20);
    op
}
fn script(op: Vec<GhStep>) -> (Fixture, Vec<GhStep>) {
    let mut all = initial_choice(false, false);
    all.extend(op);
    scripted(all)
}
fn complete(
    app: &mut App,
    fixture: &Fixture,
    provider: &GithubProvider,
    action: ActionsAction,
    limits: &ScopedPrCheckReadLimitsV1,
) -> PreparedView {
    assert!(next::intercept(app, &action));
    assert!(app.form.is_none());
    let work = next::take_work(app).expect("Actual volatile request");
    next::execute(
        work,
        provider,
        &database(fixture),
        limits,
        &CancellationToken::default(),
    )
    .unwrap()
}
fn finish(app: &mut App, mut prepared: PreparedView) {
    let value = prepared
        .pr_check_continuation_v2
        .take()
        .expect("Actual sealed continuation");
    next::install(app, *value).unwrap();
    app.dispatch(Action::Close);
}
fn capture_script(member: u64) -> (Fixture, Vec<GhStep>) {
    let mut op = fresh_other_logs(123, member, jobs(123), 2);
    op[9].stdout = "OTHER_JOB_PRIVATE_LOG_CANARY • 界🦀\nline2\n"
        .as_bytes()
        .to_vec();
    script(op)
}
fn captured(fixture: &Fixture, provider: &GithubProvider, member: u64) -> App {
    let mut app = loaded(fixture, provider, &Default::default());
    if member == 502 {
        let work = queue_logs(&mut app, 502);
        let p = choices::execute(
            work,
            provider,
            &database(fixture),
            &Default::default(),
            &Default::default(),
        )
        .unwrap();
        install(&mut app, p);
    } else {
        let route = app.pr_checks_route.clone().unwrap();
        let workspace = app.actions.take().unwrap();
        let p = apply_v1(
            route,
            workspace,
            ActionsAction::Logs(Some(501)),
            provider,
            &database(fixture),
            &Default::default(),
            &Default::default(),
        )
        .unwrap();
        install(&mut app, p);
    }
    assert!(app
        .pr_checks_route
        .as_ref()
        .unwrap()
        .loaded_capture_v2()
        .is_some());
    app
}
fn search(
    app: &mut App,
    fixture: &Fixture,
    provider: &GithubProvider,
    query: &str,
    mode: LoadedLogSearchModeV2,
) -> PreparedView {
    let route = app.pr_checks_route.clone().unwrap();
    next::queue_search(app, route, query.into(), mode);
    let work = next::take_work(app).unwrap();
    next::execute(
        work,
        provider,
        &database(fixture),
        &Default::default(),
        &Default::default(),
    )
    .unwrap()
}
fn read(app: &App) -> &crate::providers::gh::PrCheckAnnotationsReadV2 {
    app.pr_checks_route
        .as_ref()
        .unwrap()
        .annotations_v2()
        .unwrap()
}
#[test]
fn continued_annotations_actual_matched_job20_current_location_not_canonical_range() {
    let (f, s) = script(annotations(501, vec![annotation_row()], Some(1)));
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let prepared = complete(
        &mut app,
        &f,
        &p,
        ActionsAction::Annotations(501),
        &Default::default(),
    );
    finish(&mut app, prepared);
    let r = read(&app);
    assert_eq!(r.target().unwrap().job_id(), 501);
    assert_eq!(r.check_id(), Some(600));
    assert_eq!(r.rows()[0].lines(), Some((7, 9)));
    assert_eq!(r.rows()[0].path(), Some("src/界.rs"));
    assert!(r.coverage().complete);
    assert_eq!(r.requests(), 14);
    assert_eq!(
        app.actions.as_ref().unwrap().view,
        crate::actions_workspace::ActionsView::Annotations
    );
    assert!(app.actions.as_ref().unwrap().rows[0]
        .label
        .contains("not a canonical file range"));
    assert_eq!(s.len(), 68);
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_annotations_other502_has_actual601_not_origin600() {
    let (f, s) = script(annotations(502, vec![annotation_row()], Some(1)));
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let prepared = complete(
        &mut app,
        &f,
        &p,
        ActionsAction::Annotations(502),
        &Default::default(),
    );
    finish(&mut app, prepared);
    assert_eq!(read(&app).target().unwrap().job_id(), 502);
    assert_eq!(read(&app).check_id(), Some(601));
    assert_eq!(
        app.pr_checks_route
            .as_ref()
            .unwrap()
            .observation()
            .check_id(),
        600
    );
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_annotations_other502_foreign_suite_refuses_before_private_guard_or_page() {
    let mut op = annotations(502, vec![annotation_row()], Some(1));
    let mut wrong = annotation(601, Some(1));
    wrong["check_suite"]["id"] = json!(999);
    op[9] = get(&format!("repos/{REPO}/check-runs/601"), wrong);
    op.truncate(10);
    let (f, s) = script(op);
    let p = provider(&f);
    let app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let route = app.pr_checks_route.as_ref().unwrap();
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
    let mut session = p
        .begin_repository_catalogue_access(
            source.source(),
            &Default::default(),
            &Default::default(),
        )
        .unwrap();
    let id = session.identity().clone();
    let current = session.verify(nav.original(), &Default::default()).unwrap();
    let choice = route.observation().choose_job_v1(1).unwrap();
    let mut guards = 0;
    let error = p
        .pr_check_annotations_from_resource_with_guard_v2(
            &id,
            current,
            resource.observed(),
            0,
            route.observation().clone(),
            &choice,
            &Default::default(),
            &Default::default(),
            |_| {
                guards += 1;
                Ok(())
            },
        )
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::IdentityChanged));
    assert_eq!(
        guards, 0,
        "Foreign suite must not reach either private SQL guard"
    );
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_annotations_exhausted_actual_empty_not_permission_unknown_zero() {
    let (f, s) = script(annotations(501, vec![], Some(0)));
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let prepared = complete(
        &mut app,
        &f,
        &p,
        ActionsAction::Annotations(501),
        &Default::default(),
    );
    finish(&mut app, prepared);
    assert!(read(&app).coverage().complete);
    assert!(read(&app).rows().is_empty());
    assert_eq!(read(&app).coverage().advertised_count, Some(0));
    assert!(app.pr_check_annotations_detail_v2.is_none());
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_annotations_nullable_fields_are_partial_not_line0_or_original_anchor() {
    let mut row = annotation_row();
    row["start_line"] = Value::Null;
    row["path"] = Value::Null;
    row["annotation_level"] = Value::Null;
    let (f, s) = script(annotations(501, vec![row], None));
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let prepared = complete(
        &mut app,
        &f,
        &p,
        ActionsAction::Annotations(501),
        &Default::default(),
    );
    finish(&mut app, prepared);
    let row = &read(&app).rows()[0];
    assert_eq!(row.lines(), None);
    assert_eq!(row.path(), None);
    assert!(!read(&app).coverage().complete);
    assert!(app.actions.as_ref().unwrap().rows[2]
        .label
        .contains("reported lines unavailable"));
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_annotations_full_fixed_page_cap_partial_no_unrequested_page2() {
    let (f, s) = script(annotations(501, vec![annotation_row(); 100], Some(200)));
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    // Factory-specific one-page cap: use direct genuine current access below.
    let route = app.pr_checks_route.clone().unwrap();
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
    let mut session = p
        .begin_repository_catalogue_access(
            source.source(),
            &Default::default(),
            &Default::default(),
        )
        .unwrap();
    let id = session.identity().clone();
    let current = session.verify(nav.original(), &Default::default()).unwrap();
    let choice = route.observation().choose_job_v1(0).unwrap();
    let limits = crate::providers::gh::PrCheckAnnotationLimitsV2 {
        max_pages: 1,
        max_rows: 100,
        ..Default::default()
    };
    let r = p
        .pr_check_annotations_from_resource_with_guard_v2(
            &id,
            current,
            resource.observed(),
            0,
            route.observation().clone(),
            &choice,
            &limits,
            &Default::default(),
            |_| Ok(()),
        )
        .unwrap();
    assert_eq!(r.coverage().fetched_pages, 1);
    assert_eq!(r.rows().len(), 100);
    assert!(!r.coverage().complete);
    assert!(!r.coverage().api_exhausted);
    session.verify(nav.original(), &Default::default()).unwrap();
    session.close(&Default::default()).unwrap();
    app.actions_action = None;
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_annotations_endpoint403_closes_actual_member_source_partial_not_false_empty() {
    let mut op = annotations(501, vec![], Some(1));
    op[10].stdout.clear();
    op[10].stderr = b"gh: unavailable (HTTP 403)".to_vec();
    op[10].status = 1;
    let (f, s) = script(op);
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let prepared = complete(
        &mut app,
        &f,
        &p,
        ActionsAction::Annotations(501),
        &Default::default(),
    );
    finish(&mut app, prepared);
    assert!(read(&app).coverage().failure.is_some());
    assert!(!read(&app).coverage().complete);
    assert_eq!(read(&app).coverage().fetched_pages, 0);
    assert!(read(&app).rows().is_empty());
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_annotations_foreign_link_or_check_node_head_refuses_before_private_guard() {
    for field in ["head_sha", "node_id"] {
        let mut op = annotations(501, vec![annotation_row()], Some(1));
        let mut raw = annotation(600, Some(1));
        raw[field] = json!(if field == "head_sha" {
            "b".repeat(40)
        } else {
            "different-node".into()
        });
        op[9] = get(&format!("repos/{REPO}/check-runs/600"), raw);
        op.truncate(10);
        let (f, s) = script(op);
        let p = provider(&f);
        let mut app = loaded(&f, &p, &Default::default());
        let before = sql_state(&f);
        let git = git_state_resource(&f);
        let prepared = complete(
            &mut app,
            &f,
            &p,
            ActionsAction::Annotations(501),
            &Default::default(),
        );
        finish(&mut app, prepared);
        assert!(app.actions.is_none());
        assert!(app.pr_checks_route.is_none());
        read_only(&f, &before, &git, &s);
    }
}
#[test]
fn continued_annotations_missing_member_does_not_read_private_sql_or_guess_check() {
    let mut op = annotations(502, vec![annotation_row()], Some(1));
    op[8] = get(
        &format!("repos/{REPO}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
        json!({"total_count":2,"jobs":[job_id(123,501)]}),
    );
    op.drain(9..13);
    op.truncate(13);
    let (f, s) = script(op);
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let prepared = complete(
        &mut app,
        &f,
        &p,
        ActionsAction::Annotations(502),
        &Default::default(),
    );
    finish(&mut app, prepared);
    assert!(app.actions.is_none());
    assert!(!app.status.contains("complete 0"));
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_annotations_local_guard_failure_real_closing_uid_priority_no_private_diagnostics() {
    for uid in [42, 43] {
        let mut op = annotations(501, vec![annotation_row()], Some(1));
        op.remove(10);
        op[15] = bridge_user(uid);
        op.truncate(16);
        let (f, s) = script(op);
        let p = provider(&f);
        let app = loaded(&f, &p, &Default::default());
        let before = sql_state(&f);
        let git = git_state_resource(&f);
        let route = app.pr_checks_route.as_ref().unwrap();
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
        let mut session = p
            .begin_repository_catalogue_access(
                source.source(),
                &Default::default(),
                &Default::default(),
            )
            .unwrap();
        let id = session.identity().clone();
        let current = session.verify(nav.original(), &Default::default()).unwrap();
        let selected = route.observation().choose_job_v1(0).unwrap();
        let mut phases = vec![];
        let error = p
            .pr_check_annotations_from_resource_with_guard_v2(
                &id,
                current,
                resource.observed(),
                0,
                route.observation().clone(),
                &selected,
                &Default::default(),
                &Default::default(),
                |phase| {
                    phases.push((
                        phase,
                        fs::read_to_string(f.root.join("gh-fixture/commands.jsonl"))
                            .unwrap()
                            .lines()
                            .count(),
                    ));
                    let body: String = sql(&f)
                        .query_row("SELECT body FROM drafts LIMIT 1", [], |r| r.get(0))
                        .unwrap();
                    assert!(body.contains(DRAFT));
                    Err(crate::providers::gh::GithubError {
                        kind: crate::providers::gh::GithubErrorKind::Transport,
                        message: format!("PRIVATE_DIAGNOSTIC_{body}"),
                    })
                },
            )
            .unwrap_err();
        assert_eq!(
            phases,
            vec![(
                crate::providers::gh::PrCheckAnnotationPhaseV2::BeforeAnnotations,
                58
            )]
        );
        assert!(!error.message.contains(DRAFT));
        assert!(!error.message.contains("PRIVATE_DIAGNOSTIC"));
        if uid == 43 {
            assert!(matches!(
                error.kind,
                crate::providers::gh::GithubErrorKind::IdentityChanged
            ));
        }
        read_only(&f, &before, &git, &s);
    }
}
#[test]
fn continued_annotations_closing_source_sql_cas_hides_without_rebasing_or_final_reads() {
    let mut op = annotations(501, vec![annotation_row()], Some(1));
    op[17].delay_ms = 250;
    op.truncate(18);
    let (f, s) = script(op);
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let git = git_state_resource(&f);
    let path = database(&f);
    let key = app
        .remote_scope_source
        .as_ref()
        .unwrap()
        .saved_view_key()
        .to_owned();
    let journal = f.root.join("gh-fixture/commands.jsonl");
    let concurrent = std::thread::spawn(move || {
        wait_count(&journal, 66);
        let db = rusqlite::Connection::open(path).unwrap();
        assert_eq!(db.execute("INSERT INTO saved_views(host,login,name,payload) VALUES(?1,'user-id:42',?2,'NEW_ORIGINAL_SQL_CANARY') ON CONFLICT(host,login,name) DO UPDATE SET payload=excluded.payload",rusqlite::params![HOST,key]).unwrap(),1);
    });
    let prepared = complete(
        &mut app,
        &f,
        &p,
        ActionsAction::Annotations(501),
        &Default::default(),
    );
    concurrent.join().unwrap();
    finish(&mut app, prepared);
    assert!(app.actions.is_none());
    assert!(!app.status.contains("NEW_ORIGINAL_SQL_CANARY"));
    assert_eq!(git_state_resource(&f), git);
    exact(&f, &s);
    assert_eq!(
        sql(&f)
            .query_row("SELECT count(*) FROM actions_write_attempts", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn continued_annotations_deadline_and_cancel_no_new_children_or_body_fallback() {
    for cancelled in [false, true] {
        let (f, s) = script(vec![]);
        let p = provider(&f);
        let mut app = loaded(&f, &p, &Default::default());
        let before = sql_state(&f);
        let git = git_state_resource(&f);
        assert!(next::intercept(&mut app, &ActionsAction::Annotations(501)));
        let work = next::take_work(&mut app).unwrap();
        let cancel = CancellationToken::default();
        if cancelled {
            cancel.cancel();
        }
        let bounded = p
            .clone()
            .with_scoped_read_window(Instant::now(), 128 * 1024)
            .unwrap();
        let prepared =
            next::execute(work, &bounded, &database(&f), &Default::default(), &cancel).unwrap();
        finish(&mut app, prepared);
        assert!(app.actions.is_none());
        read_only(&f, &before, &git, &s);
    }
}
#[test]
fn continued_exact_refresh18_observed_attempt_preserves_no_mutation_or_required_authority() {
    let (f, s) = script(fresh_choice(123, jobs(123), 2));
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let prepared = complete(
        &mut app,
        &f,
        &p,
        ActionsAction::Refresh,
        &Default::default(),
    );
    finish(&mut app, prepared);
    assert_eq!(
        app.actions
            .as_ref()
            .unwrap()
            .run
            .as_ref()
            .unwrap()
            .attempt
            .run
            .attempt,
        2
    );
    assert!(!app.actions.as_ref().unwrap().coverage.complete);
    assert!(app.form.is_none());
    assert_eq!(s.len(), 66);
    read_only(&f, &before, &git, &s);
}
fn advance() -> Vec<GhStep> {
    let mut run = run_id(123);
    run["run_attempt"] = json!(3);
    run["check_suite_id"] = json!(902);
    let mut op = principal(42, "advance-current");
    op.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    op.extend([
        bridge_user(42),
        get(&format!("repos/{REPO}"), bridge_repo(REPO)),
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
        get(&format!("repos/{REPO}/check-runs/600"), actual_check(REPO)),
        get(&format!("repos/{REPO}/actions/runs/123"), run.clone()),
        get(&format!("repos/{REPO}/actions/runs/123"), run),
        get(&format!("repos/{REPO}/check-runs/600"), actual_check(REPO)),
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
        get(&format!("repos/{REPO}"), bridge_repo(REPO)),
        bridge_user(42),
        get(&format!("repos/{REPO}"), repo(410, REPO)),
    ]);
    op.extend(principal(42, "advance-final"));
    assert_eq!(op.len(), 16);
    op
}
#[test]
fn continued_attempt_advance16_retains_original2_and_latest3_no_auto_adoption() {
    let (f, s) = script(advance());
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let prepared = complete(
        &mut app,
        &f,
        &p,
        ActionsAction::Refresh,
        &Default::default(),
    );
    finish(&mut app, prepared);
    assert_eq!(
        app.actions
            .as_ref()
            .unwrap()
            .run
            .as_ref()
            .unwrap()
            .attempt
            .run
            .attempt,
        2
    );
    let advanced = app.pr_check_attempt_advance_v2.as_ref().unwrap();
    assert_eq!(advanced.latest().attempt, 3);
    assert_eq!(advanced.latest_suite(), 902);
    assert!(Arc::ptr_eq(
        advanced.original(),
        app.pr_checks_route.as_ref().unwrap().observation()
    ));
    assert!(app.pr_check_watch_v2.is_none());
    assert_eq!(s.len(), 64);
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_loaded_other502_capture_exact_job_provenance_not_original501() {
    let (f, s) = capture_script(502);
    let p = provider(&f);
    let app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let route = app.pr_checks_route.as_ref().unwrap();
    let capture = route.loaded_capture_v2().unwrap();
    assert_eq!(capture.captured_target_v2().job_id(), 502);
    assert_eq!(capture.captured_target_v2().attempt(), 2);
    assert_eq!(capture.captured_target_v2().job_node(), Some("J_502"));
    assert_eq!(route.observation().link().unwrap().selected_job(), 501);
    assert!(next::loaded_capture_allowed(
        route,
        app.actions.as_ref().unwrap()
    ));
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_loaded_log_exact_unicode_search_zero_gh_sql_git_form() {
    let (f, s) = capture_script(502);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let capture = app
        .pr_checks_route
        .as_ref()
        .unwrap()
        .loaded_capture_v2()
        .cloned()
        .unwrap();
    let prepared = search(&mut app, &f, &p, "界🦀", LoadedLogSearchModeV2::ExactText);
    finish(&mut app, prepared);
    let found = app.pr_check_log_matches_v2.as_ref().unwrap();
    assert_eq!(found.indices(), [0]);
    assert!(found.belongs_to(&capture));
    assert!(app.form.is_none());
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_loaded_log_regex_per_line_zero_gh_no_draft_persistence() {
    let (f, s) = capture_script(502);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let prepared = search(
        &mut app,
        &f,
        &p,
        r"^line[2-9]$",
        LoadedLogSearchModeV2::Regex,
    );
    finish(&mut app, prepared);
    assert_eq!(app.pr_check_log_matches_v2.as_ref().unwrap().indices(), [1]);
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_loaded_log_invalid_regex_retains_actual_capture_no_transport() {
    let (f, s) = capture_script(502);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let old = app
        .pr_checks_route
        .as_ref()
        .unwrap()
        .loaded_capture_v2()
        .cloned()
        .unwrap();
    let prepared = search(&mut app, &f, &p, "[", LoadedLogSearchModeV2::Regex);
    finish(&mut app, prepared);
    assert!(app.pr_check_log_matches_v2.is_none());
    assert!(Arc::ptr_eq(
        &old,
        app.pr_checks_route
            .as_ref()
            .unwrap()
            .loaded_capture_v2()
            .unwrap()
    ));
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_loaded_search_missing_capture_or_unseen_job_has_no_form_or_sql_fallback() {
    let (f, s) = script(vec![]);
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    assert!(next::intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::SearchLogs)
    ));
    assert!(app.pr_check_log_editor_v2.is_none());
    assert!(app.form.is_none());
    assert!(next::take_work(&mut app).is_none());
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_search_editor_unicode_focus_capture_resize_and_key_mouse_parity() {
    let (f, s) = capture_script(502);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    assert!(next::intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::SearchLogs)
    ));
    assert!(ui::event(&mut app, Event::Paste("界e\u{301}🦀".into())));
    assert_eq!(
        app.pr_check_log_editor_v2.as_ref().unwrap().query(),
        "界e\u{301}🦀"
    );
    assert!(ui::event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
    ));
    assert_eq!(
        app.pr_check_log_editor_v2.as_ref().unwrap().query(),
        "界e\u{301}"
    );
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            ui::draw(frame, &mut app, area)
        })
        .unwrap();
    assert!(ui::event(&mut app, Event::Resize(60, 18)));
    assert!(ui::event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
    ));
    ui::dispatch(
        &mut app,
        ui::PrCheckContinueUiV2::Mode(LoadedLogSearchModeV2::Regex),
    );
    ui::dispatch(&mut app, ui::PrCheckContinueUiV2::Close);
    assert!(app.pr_check_log_editor_v2.is_none());
    assert!(app.form.is_none());
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_annotations_detail_borrows_actual_row_not_file_range_or_write_target() {
    let (f, s) = script(annotations(501, vec![annotation_row()], Some(1)));
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let prepared = complete(
        &mut app,
        &f,
        &p,
        ActionsAction::Annotations(501),
        &Default::default(),
    );
    finish(&mut app, prepared);
    app.dispatch(Action::Actions(ActionsAction::Select(2)));
    ui::dispatch(&mut app, ui::PrCheckContinueUiV2::Details);
    assert_eq!(
        app.overlay,
        Some(crate::app::Overlay::PrCheckContinuationV2)
    );
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            ui::draw(frame, &mut app, area)
        })
        .unwrap();
    assert!(ui::event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
    ));
    assert!(app.form.is_none());
    ui::dispatch(&mut app, ui::PrCheckContinueUiV2::Close);
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_watch_arm_stop_zero_children_never_legacy_watching_or_publication() {
    let (f, s) = script(vec![]);
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    assert!(next::intercept(&mut app, &ActionsAction::Watch));
    let watch = app.pr_check_watch_v2.clone().unwrap();
    assert!(watch.active());
    assert!(!app.actions.as_ref().unwrap().watching);
    assert!(next::intercept(&mut app, &ActionsAction::Watch));
    assert!(!watch.active());
    assert!(app.pr_check_watch_v2.is_none());
    assert!(app.pr_check_continuation_cancel_v2);
    assert!(app.form.is_none());
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_late_search_source_supersession_success_error_panic_owned_before_effects() {
    for failure in [0, 1, 2] {
        let (f, s) = capture_script(502);
        let p = provider(&f);
        let mut app = captured(&f, &p, 502);
        let before = sql_state(&f);
        let git = git_state_resource(&f);
        let work = {
            let route = app.pr_checks_route.clone().unwrap();
            next::queue_search(
                &mut app,
                route,
                "line2".into(),
                LoadedLogSearchModeV2::ExactText,
            );
            next::take_work(&mut app).unwrap()
        };
        let owner = work.owner().clone();
        let mut background = Background::new();
        let (tx, rx) = std::sync::mpsc::channel();
        let (start_tx, start_rx) = std::sync::mpsc::channel();
        let db = database(&f);
        let cloned = p.clone();
        assert!(
            background.submit_pr_check_continuation_v2(&mut app, owner, move |cancel| {
                start_tx.send(()).unwrap();
                rx.recv().unwrap();
                match failure {
                    1 => Err("PRIVATE_LATE_ERROR_CANARY".into()),
                    2 => panic!("PRIVATE_LATE_PANIC_CANARY"),
                    _ => next::execute(work, &cloned, &db, &Default::default(), &cancel),
                }
            })
        );
        start_rx.recv().unwrap();
        next::retire(&mut app);
        app.status = "NEW_OWNER_STABLE".into();
        tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            background.poll(&mut app);
            if !app.loading[View::Actions.index()] {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(app.status, "NEW_OWNER_STABLE");
        assert!(app.pr_check_log_matches_v2.is_none());
        assert!(!app.status.contains("PRIVATE_LATE"));
        read_only(&f, &before, &git, &s);
    }
}
#[test]
fn continued_selection_rebuild_admitted_search_controls_preserved_other_compose_disabled() {
    let (f, s) = capture_script(502);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    for index in [0, 1] {
        app.dispatch(Action::Actions(ActionsAction::Select(index)));
        let ws = app.actions.as_ref().unwrap();
        assert!(ws
            .controls
            .iter()
            .find(|c| c.action == ActionsAction::Compose(ActionsCommand::SearchLogs))
            .unwrap()
            .disabled_reason
            .is_none());
        assert!(ws
            .controls
            .iter()
            .find(|c| c.action == ActionsAction::Compose(ActionsCommand::ChooseScope))
            .unwrap()
            .disabled_reason
            .is_some());
    }
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_same_owner_cached_back_stops_watch_search_without_any_gh_sql() {
    let (f, s) = capture_script(502);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    assert!(next::intercept(&mut app, &ActionsAction::Watch));
    let watch = app.pr_check_watch_v2.clone().unwrap();
    app.dispatch(Action::PrChecks(pr_checks_native::PrChecksUiAction::Back));
    assert!(!watch.active());
    assert_eq!(app.view, View::Inbox);
    assert!(app.pr_check_log_matches_v2.is_none());
    assert!(app.pr_checks_route.is_none());
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_watch_current_failed_proof_hides_private_even_when_quota_stops() {
    let mut op = fresh_choice(123, jobs(123), 2);
    op[14] = bridge_user(43);
    op.truncate(15);
    let (f, s) = script(op);
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    assert!(next::intercept(&mut app, &ActionsAction::Watch));
    std::thread::sleep(Duration::from_millis(5100));
    next::tick(&mut app);
    let work = next::take_work(&mut app).unwrap();
    let prepared = next::execute(
        work,
        &p,
        &database(&f),
        &Default::default(),
        &Default::default(),
    )
    .unwrap();
    finish(&mut app, prepared);
    assert!(app.actions.is_none());
    assert!(app.pr_checks_route.is_none());
    assert!(app.pr_check_watch_v2.is_none());
    assert!(!app.status.contains(DRAFT));
    assert_eq!(s.len(), 63);
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_annotation_huge_multibyte_failed_capture_no_further_child_or_false_proof() {
    let mut op = annotations(501, vec![], Some(1));
    op[10].stdout = "界".repeat(100000).into_bytes();
    op[10].stderr = "\u{1b}\u{7}🦀".repeat(50000).into_bytes();
    op[10].status = 1;
    op.truncate(11);
    let (f, s) = script(op);
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let prepared = complete(
        &mut app,
        &f,
        &p,
        ActionsAction::Annotations(501),
        &Default::default(),
    );
    finish(&mut app, prepared);
    assert!(app.actions.is_none());
    assert_eq!(s.len(), 59);
    assert!(!app.status.contains("🦀"));
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_annotation_private_callback_time_uses_original_clock_no_next_child() {
    let mut op = annotations(501, vec![annotation_row()], Some(1));
    op.truncate(10);
    let (f, s) = script(op);
    let p = provider(&f);
    let app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let route = app.pr_checks_route.as_ref().unwrap();
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
    let mut session = p
        .begin_repository_catalogue_access(
            source.source(),
            &Default::default(),
            &Default::default(),
        )
        .unwrap();
    let id = session.identity().clone();
    let current = session.verify(nav.original(), &Default::default()).unwrap();
    let selected = route.observation().choose_job_v1(0).unwrap();
    let mut phases = 0;
    let limits = crate::providers::gh::PrCheckAnnotationLimitsV2 {
        transport: crate::providers::gh::PrCheckRunReadLimits {
            max_duration: Duration::from_secs(2),
            ..Default::default()
        },
        ..Default::default()
    };
    let error = p
        .pr_check_annotations_from_resource_with_guard_v2(
            &id,
            current,
            resource.observed(),
            0,
            route.observation().clone(),
            &selected,
            &limits,
            &Default::default(),
            |_| {
                phases += 1;
                let body: String = sql(&f)
                    .query_row("SELECT body FROM drafts LIMIT 1", [], |r| r.get(0))
                    .unwrap();
                assert!(body.contains(DRAFT));
                std::thread::sleep(Duration::from_millis(2100));
                Ok(())
            },
        )
        .unwrap_err();
    assert_eq!(phases, 1);
    assert!(!error.message.contains(DRAFT));
    assert_eq!(s.len(), 58);
    read_only(&f, &before, &git, &s);
}
#[test]
fn continued_other502_manual_log_refresh18_replaces_capture_without_retained_history() {
    let mut op = fresh_other_logs(123, 502, jobs(123), 2);
    let mut fresh = fresh_other_logs(123, 502, jobs(123), 2);
    fresh[9].stdout = b"NEW_OTHER502_REFRESH_CAPTURE\nline2\n".to_vec();
    op.extend(fresh);
    let (f, s) = script(op);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let old = Arc::downgrade(
        app.pr_checks_route
            .as_ref()
            .unwrap()
            .loaded_capture_v2()
            .unwrap(),
    );
    let prepared = complete(
        &mut app,
        &f,
        &p,
        ActionsAction::Refresh,
        &Default::default(),
    );
    finish(&mut app, prepared);
    let current = app
        .pr_checks_route
        .as_ref()
        .unwrap()
        .loaded_capture_v2()
        .unwrap();
    assert_eq!(current.captured_target_v2().job_id(), 502);
    assert!(current.logs().contains("NEW_OTHER502_REFRESH_CAPTURE"));
    assert!(
        old.upgrade().is_none(),
        "No capture history/route chain retained after a genuine replacement"
    );
    assert!(app.pr_check_log_matches_v2.is_none());
    assert_eq!(s.len(), 84);
    read_only(&f, &before, &git, &s);
}
