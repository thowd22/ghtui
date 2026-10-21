//! SOURCE-ONLY child of genuine continuation V2 contracts. Every capture and
//! source is produced by real strict-gh factories, not a public/raw proof DTO.
//! Direct publish + draw deliberately does NOT dispatch a synthetic Close.
use super::*;
use crate::{
    local_commit_actor_origin_v1_test_support::Real,
    navigation_local_owner_v1::OriginalLocalNavigationOwnerV1,
    pr_checks_native::scoped::choices::continuations_v2::corrective_v3 as corrective,
    repository_graph_reread_native_v3,
    repository_graph_reread_v3::GraphRereadPreviewV3,
    runtime::pr_check_continuation_corrective_v3_test_support as support,
    workspace::{RepositoryMode, Workspace},
};
use std::sync::mpsc;
#[path = "scoped_pr_check_parallel_v4_tests.rs"]
mod parallel_v4;

fn frame(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
fn drain(background: &mut Background, app: &mut App, view: View) {
    let end = Instant::now() + Duration::from_secs(5);
    while support::pending(background, view) {
        background.poll(app);
        assert!(Instant::now() < end, "Actual completion must be consumed");
        std::thread::yield_now();
    }
}
fn pure(
    app: &mut App,
    f: &Fixture,
    p: &GithubProvider,
) -> (Arc<next::PrCheckContinuationOwnerV2>, PreparedView) {
    let prepared = search(app, f, p, "line2", LoadedLogSearchModeV2::ExactText);
    let owner = app.pr_check_continuation_owner_v2.clone().unwrap();
    (owner, prepared)
}
fn hidden_now(app: &mut App) {
    assert!(app.github.is_none());
    assert!(app.actions.is_none());
    assert!(app.pr_checks_route.is_none());
    assert!(app.pr_checks_origin.is_none());
    assert!(app.pr_check_continuation_owner_v2.is_none());
    assert!(app.pr_check_watch_v2.is_none());
    assert!(app.pr_check_log_matches_v2.is_none());
    assert!(app.remote_scope_source.is_none());
    assert!(app.form.is_none());
    assert!(!app.loading[View::Actions.index()]);
    assert!(!app.refresh_requested);
    let text = frame(app);
    for private in [
        "OTHER_JOB_PRIVATE_LOG_CANARY",
        "PRIVATE_CURRENT_DIAGNOSTIC",
        DRAFT,
    ] {
        assert!(
            !text.contains(private),
            "Private canary absent on SAME completed frame"
        );
        assert!(!app.status.contains(private));
        assert!(!app.notice.contains(private));
    }
    assert!(text.contains("private results hidden"));
}

#[test]
fn corrective_current_hidden_final_uid43_direct_install_draw_has_no_close_or_next_loop_dependency()
{
    for compatibility_install in [false, true] {
        let mut op = fresh_choice(123, jobs(123), 2);
        op[14] = bridge_user(43);
        op.truncate(15);
        let (f, steps) = script(op);
        let p = provider(&f);
        let mut app = loaded(&f, &p, &Default::default());
        let before = sql_state(&f);
        let git = git_state_resource(&f);
        assert!(next::intercept(&mut app, &ActionsAction::Refresh));
        let work = corrective::take_work(&mut app).unwrap();
        let owner = work.owner().clone();
        let result = next::execute(
            work,
            &p,
            &database(&f),
            &Default::default(),
            &Default::default(),
        );
        if compatibility_install {
            // The old typed-only compatibility installer also owns its SAME ticket
            // before cleanup. It must not need the older finish() helper's Close.
            let mut prepared = result.unwrap();
            next::install(&mut app, *prepared.pr_check_continuation_v2.take().unwrap()).unwrap();
        } else {
            assert!(corrective::publish(&mut app, &owner, result));
        }
        hidden_now(&mut app);
        assert_eq!(steps.len(), 65);
        read_only(&f, &before, &git, &steps);
    }
}

#[test]
fn corrective_current_error_and_caught_panic_hide_actual_loaded_capture_before_any_frame_effect() {
    for panic in [false, true] {
        let (f, steps) = capture_script(502);
        let p = provider(&f);
        let mut app = captured(&f, &p, 502);
        assert!(frame(&mut app).contains("OTHER_JOB_PRIVATE_LOG_CANARY"));
        let before = sql_state(&f);
        let git = git_state_resource(&f);
        let (owner, prepared) = pure(&mut app, &f, &p);
        drop(prepared); // A genuine request ticket, no fake completion authority.
        let (release_tx, release_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let mut background = Background::new();
        assert!(
            background.submit_pr_check_continuation_v2(&mut app, owner, move |_| {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                if panic {
                    panic!("PRIVATE_CURRENT_DIAGNOSTIC");
                }
                Err("PRIVATE_CURRENT_DIAGNOSTIC".into())
            })
        );
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        app.notice = "PRIVATE_CURRENT_DIAGNOSTIC".into();
        app.overlay = Some(crate::app::Overlay::Notice);
        release_tx.send(()).unwrap();
        drain(&mut background, &mut app, View::Actions);
        hidden_now(&mut app);
        read_only(&f, &before, &git, &steps);
    }
}

#[test]
fn corrective_stopped_quota_current_error_still_hides_without_retry_or_default_scope() {
    let (f, steps) = script(vec![]);
    let p = provider(&f);
    let mut app = loaded(&f, &p, &Default::default());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    assert!(next::intercept(&mut app, &ActionsAction::Watch));
    let watch = app.pr_check_watch_v2.clone().unwrap();
    std::thread::sleep(Duration::from_millis(5100));
    next::tick(&mut app);
    let work = corrective::take_work(&mut app).unwrap();
    let owner = work.owner().clone();
    corrective::stop_current_quota_for_test(&app);
    assert!(!watch.active());
    assert!(
        owner.owns(&app),
        "Quota/time is not a reason to skip current privacy cleanup"
    );
    let result = next::execute(
        work,
        &p,
        &database(&f),
        &Default::default(),
        &Default::default(),
    );
    assert!(
        result.is_err(),
        "Actual finite quota rejects claim before any child"
    );
    assert!(corrective::publish(&mut app, &owner, result));
    hidden_now(&mut app);
    read_only(&f, &before, &git, &steps);
}

#[test]
fn corrective_foreign_result_ticket_and_old_owner_reject_before_loading_status_private_effects() {
    let (f, steps) = capture_script(502);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let (old, value) = pure(&mut app, &f, &p);
    let (current, newer) = pure(&mut app, &f, &p);
    app.status = "NEW_EXACT_TICKET".into();
    app.loading[View::Actions.index()] = true;
    let rows = app.actions.as_ref().unwrap().rows.as_ptr();
    assert!(!corrective::publish(&mut app, &current, Ok(value)));
    assert!(!corrective::publish(
        &mut app,
        &old,
        Err("PRIVATE_CURRENT_DIAGNOSTIC".into())
    ));
    assert_eq!(app.status, "NEW_EXACT_TICKET");
    assert!(app.loading[View::Actions.index()]);
    assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
    assert!(corrective::publish(&mut app, &current, Ok(newer)));
    assert!(!app.loading[View::Actions.index()]);
    assert_eq!(app.pr_check_log_matches_v2.as_ref().unwrap().indices(), [1]);
    read_only(&f, &before, &git, &steps);
}

#[test]
fn corrective_ownerless_genuine_continuation_and_no_result_refuse_generic_publisher_before_effects()
{
    for ownerless in [false, true] {
        let (f, steps) = capture_script(502);
        let p = provider(&f);
        let mut app = captured(&f, &p, 502);
        let before = sql_state(&f);
        let git = git_state_resource(&f);
        let (owner, prepared) = pure(&mut app, &f, &p);
        let rows = app.actions.as_ref().unwrap().rows.as_ptr();
        let mut background = Background::new();
        if ownerless {
            assert!(background.submit(&mut app, View::Actions, move |_| Ok(prepared)));
        } else {
            drop(prepared);
            assert!(background
                .submit_pr_check_continuation_v2(&mut app, owner, |_| Ok(PreparedView::default())));
        }
        app.status = "NO_KIND_NO_EFFECT".into();
        drain(&mut background, &mut app, View::Actions);
        assert_eq!(app.status, "NO_KIND_NO_EFFECT");
        assert!(
            app.loading[View::Actions.index()],
            "Rejected kind cannot perform generic loading reset"
        );
        assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
        assert!(app.pr_check_log_matches_v2.is_none());
        read_only(&f, &before, &git, &steps);
    }
}

#[test]
fn corrective_genuine_continuation_mixed_generic_effect_flags_are_refused_without_private_reset() {
    let (f, steps) = capture_script(502);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let rows = app.actions.as_ref().unwrap().rows.as_ptr();
    for facet in 0..8 {
        let (owner, mut value) = pure(&mut app, &f, &p);
        match facet {
            0 => value.clear_private = true,
            1 => value.clear_actions_inspection = true,
            2 => value.invalidate_pr_metadata = true,
            3 => value.navigate_to = Some(View::Repository),
            4 => value.status = "PRIVATE_CURRENT_DIAGNOSTIC".into(),
            5 => value.lines.push("PRIVATE_CURRENT_DIAGNOSTIC".into()),
            6 => value.remote_scope_source = app.remote_scope_source.clone(),
            _ => value.restore_search_input = true,
        }
        app.status = "CLOSED_KIND_REQUIRED".into();
        app.loading[View::Actions.index()] = true;
        assert!(!corrective::prepared_only(&value));
        assert!(!corrective::publish(&mut app, &owner, Ok(value)));
        assert_eq!(app.status, "CLOSED_KIND_REQUIRED");
        assert!(app.loading[View::Actions.index()]);
        assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
        assert!(app.remote_scope_source.is_some());
        app.loading[View::Actions.index()] = false;
    }
    read_only(&f, &before, &git, &steps);
}

#[test]
fn corrective_mixed_graph_actual_tentative_visit_drops_lease_and_preserves_local_source() {
    let (f, steps) = capture_script(502);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let real = Real::new("sha1");
    app.workspace =
        Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None).unwrap();
    let original = app
        .workspace
        .as_ref()
        .unwrap()
        .graph_state_v1
        .as_ref()
        .unwrap()
        .clone();
    let local = real.snapshot();
    let preview = GraphRereadPreviewV3::capture(
        app.workspace.as_ref().unwrap(),
        &mut app.original_saved_visits,
    )
    .unwrap();
    let baseline = app
        .original_saved_visits
        .graph_retention_accounting_v2()
        .unwrap()
        .unwrap()
        .sources;
    let local_owner =
        OriginalLocalNavigationOwnerV1::begin(app.workspace.as_ref().unwrap(), Default::default())
            .unwrap();
    let mut mixed = repository_graph_reread_native_v3::work(&preview, &local_owner).unwrap();
    local_owner.operation().seal().unwrap();
    assert_eq!(
        app.original_saved_visits
            .graph_retention_accounting_v2()
            .unwrap()
            .unwrap()
            .sources,
        baseline + 1
    );
    assert!(support::original_local_only(&mixed));
    let (owner, mut private) = pure(&mut app, &f, &p);
    mixed.pr_check_continuation_v2 = private.pr_check_continuation_v2.take();
    assert!(!support::original_local_only(&mixed));
    assert!(!support::original_tree_only(&mixed));
    assert!(!corrective::prepared_only(&mixed));
    app.status = "MIXED_NOT_ADMITTED".into();
    app.loading[View::Actions.index()] = true;
    assert!(!corrective::publish(&mut app, &owner, Ok(mixed)));
    assert_eq!(
        app.original_saved_visits
            .graph_retention_accounting_v2()
            .unwrap()
            .unwrap()
            .sources,
        baseline
    );
    assert!(Arc::ptr_eq(
        app.workspace
            .as_ref()
            .unwrap()
            .graph_state_v1
            .as_ref()
            .unwrap(),
        &original
    ));
    assert_eq!(app.status, "MIXED_NOT_ADMITTED");
    assert!(app.loading[View::Actions.index()]);
    assert_eq!(real.snapshot(), local);
    read_only(&f, &before, &git, &steps);
}

#[test]
fn corrective_mixed_tree_genuine_sealed_result_reciprocal_veto_no_query_or_model_retarget() {
    use crate::{
        git::OriginalTreeIntentV1,
        repository_tree_jump_v1::OriginalTreeJumpV1,
        repository_tree_worker_v1::{OriginalTreeWorkV1, OriginalTreeWorkerOwnerV1},
    };
    let (f, steps) = capture_script(502);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let real = Real::new("sha1");
    let workspace = Workspace::load(&real.root, RepositoryMode::Tree, &real.original, None)
        .unwrap()
        .unwrap();
    let mut jump = OriginalTreeJumpV1::new(OriginalTreeIntentV1::capture(&workspace).unwrap());
    let request = jump.request().unwrap();
    let tree_owner = OriginalTreeWorkerOwnerV1::begin(
        &workspace,
        Some(&jump),
        Arc::new(OriginalTreeWorkV1::Query(request)),
        Default::default(),
    )
    .unwrap();
    let local = real.snapshot();
    let mut mixed = PreparedView {
        original_tree_result_v1: Some(tree_owner.prepare().unwrap()),
        ..Default::default()
    };
    assert!(support::original_tree_only(&mixed));
    let (owner, mut private) = pure(&mut app, &f, &p);
    mixed.pr_check_continuation_v2 = private.pr_check_continuation_v2.take();
    assert!(!support::original_tree_only(&mixed));
    assert!(!corrective::prepared_only(&mixed));
    let pending = jump.pending().unwrap().clone();
    app.status = "TREE_NOT_REPLACED".into();
    assert!(!corrective::publish(&mut app, &owner, Ok(mixed)));
    assert!(jump.accepts_request(&pending));
    assert!(jump.frame().is_none());
    assert_eq!(real.snapshot(), local);
    assert_eq!(app.status, "TREE_NOT_REPLACED");
    read_only(&f, &before, &git, &steps);
}

#[test]
fn corrective_current_error_retires_remote_only_without_mutating_genuine_graph_tree_or_sql() {
    let (f, steps) = capture_script(502);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let real = Real::new("sha256");
    app.workspace =
        Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None).unwrap();
    let local = real.snapshot();
    let graph = app
        .workspace
        .as_ref()
        .unwrap()
        .graph_state_v1
        .as_ref()
        .unwrap()
        .clone();
    let (owner, value) = pure(&mut app, &f, &p);
    drop(value);
    assert!(corrective::publish(
        &mut app,
        &owner,
        Err("PRIVATE_CURRENT_DIAGNOSTIC".into())
    ));
    hidden_now(&mut app);
    assert!(Arc::ptr_eq(
        app.workspace
            .as_ref()
            .unwrap()
            .graph_state_v1
            .as_ref()
            .unwrap(),
        &graph
    ));
    assert_eq!(real.snapshot(), local);
    read_only(&f, &before, &git, &steps);
}

#[test]
fn corrective_prequeue_expiry_clears_only_matching_ticket_without_gh_sql_or_perpetual_pending() {
    let (f, steps) = capture_script(502);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let capture = app
        .pr_checks_route
        .as_ref()
        .unwrap()
        .loaded_capture_v2()
        .unwrap()
        .clone();
    let route = app.pr_checks_route.clone().unwrap();
    next::queue_search(
        &mut app,
        route,
        "line2".into(),
        LoadedLogSearchModeV2::ExactText,
    );
    let deadline = app
        .pr_check_continuation_owner_v2
        .as_ref()
        .unwrap()
        .deadline();
    // Pure scheduler clock boundary; not a fabricated producer/access proof or
    // an assertion that thirty wall-clock seconds actually elapsed in a test.
    assert!(corrective::take_work_at_for_test(&mut app, deadline).is_none());
    assert!(app.pr_check_continuation_owner_v2.is_none());
    assert!(app.pr_check_continuation_request_v2.is_none());
    assert!(app.pr_check_continuation_cancel_v2);
    assert!(!app.loading[View::Actions.index()]);
    assert!(Arc::ptr_eq(
        &capture,
        app.pr_checks_route
            .as_ref()
            .unwrap()
            .loaded_capture_v2()
            .unwrap()
    ));
    assert!(app.status.contains("queued scoped read expired"));
    let route = app.pr_checks_route.clone().unwrap();
    next::queue_search(
        &mut app,
        route.clone(),
        "line2".into(),
        LoadedLogSearchModeV2::ExactText,
    );
    let old = app.pr_check_continuation_request_v2.take().unwrap();
    let old_deadline = old.owner().deadline();
    next::queue_search(
        &mut app,
        route,
        "界".into(),
        LoadedLogSearchModeV2::ExactText,
    );
    let current = app.pr_check_continuation_owner_v2.clone().unwrap();
    // An actual old queued value, not an invented source/proof. Verify the
    // scheduler's stale-ticket branch cannot clear the new ownership pointer.
    app.pr_check_continuation_request_v2 = Some(old);
    app.status = "NEW_QUEUE_TICKET".into();
    app.loading[View::Actions.index()] = true;
    assert!(corrective::take_work_at_for_test(&mut app, old_deadline).is_none());
    assert!(Arc::ptr_eq(
        &current,
        app.pr_check_continuation_owner_v2.as_ref().unwrap()
    ));
    assert_eq!(app.status, "NEW_QUEUE_TICKET");
    assert!(app.loading[View::Actions.index()]);
    read_only(&f, &before, &git, &steps);
}

#[test]
fn corrective_late_success_error_panic_reject_before_new_local_owner_status_or_loading_effects() {
    for failure in [0, 1, 2] {
        let (f, steps) = capture_script(502);
        let p = provider(&f);
        let mut app = captured(&f, &p, 502);
        let before = sql_state(&f);
        let git = git_state_resource(&f);
        let (owner, prepared) = pure(&mut app, &f, &p);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut background = Background::new();
        assert!(
            background.submit_pr_check_continuation_v2(&mut app, owner, move |_| {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                match failure {
                    1 => Err("PRIVATE_CURRENT_DIAGNOSTIC".into()),
                    2 => panic!("PRIVATE_CURRENT_DIAGNOSTIC"),
                    _ => Ok(prepared),
                }
            })
        );
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        app.dispatch(Action::PrChecks(pr_checks_native::PrChecksUiAction::Back));
        app.view = View::Repository;
        app.status = "NEW_LOCAL_FRAME".into();
        app.context = "LOCAL_CONTEXT".into();
        app.loading[View::Repository.index()] = true;
        release_tx.send(()).unwrap();
        drain(&mut background, &mut app, View::Actions);
        assert_eq!(app.status, "NEW_LOCAL_FRAME");
        assert_eq!(app.context, "LOCAL_CONTEXT");
        assert!(app.loading[View::Repository.index()]);
        assert!(app.pr_check_log_matches_v2.is_none());
        assert!(
            app.remote_scope_source.is_some(),
            "Late failure must not privacy-fence newer owner"
        );
        read_only(&f, &before, &git, &steps);
    }
}

#[test]
fn corrective_global_refresh_and_direct_actions_refresh_queue_same_typed_fresh_work_legacy_none_disabled(
) {
    for global in [false, true] {
        let (f, steps) = script(vec![]);
        let p = provider(&f);
        let mut app = loaded(&f, &p, &Default::default());
        let before = sql_state(&f);
        let git = git_state_resource(&f);
        let source = app.remote_scope_source.clone().unwrap();
        app.dispatch(if global {
            Action::Refresh
        } else {
            Action::Actions(ActionsAction::Refresh)
        });
        let work = corrective::take_work(&mut app)
            .expect("Same admitted continuation, not legacy disabled Refresh");
        assert!(Arc::ptr_eq(
            work.owner().origin(),
            app.pr_checks_origin.as_ref().unwrap()
        ));
        assert!(work.owner().owns(&app));
        assert!(Arc::ptr_eq(
            &source,
            app.remote_scope_source.as_ref().unwrap()
        ));
        assert!(app.form.is_none());
        assert!(app.actions_action.is_none());
        assert!(!app.actions_load_requested);
        assert!(!app.refresh_requested);
        read_only(&f, &before, &git, &steps);
    }
    let mut legacy = App {
        view: View::Actions,
        ..Default::default()
    };
    assert!(!corrective::forward_global_refresh(&mut legacy));
    assert!(legacy.pr_check_continuation_request_v2.is_none());
}

#[test]
fn corrective_bounded_invalid_regex_keeps_genuine_capture_success_moves_only_matches() {
    let (f, steps) = capture_script(502);
    let p = provider(&f);
    let mut app = captured(&f, &p, 502);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let original = app
        .pr_checks_route
        .as_ref()
        .unwrap()
        .loaded_capture_v2()
        .unwrap()
        .clone();
    let invalid = search(&mut app, &f, &p, "[", LoadedLogSearchModeV2::Regex);
    let owner = app.pr_check_continuation_owner_v2.clone().unwrap();
    assert!(corrective::publish(&mut app, &owner, Ok(invalid)));
    assert!(Arc::ptr_eq(
        &original,
        app.pr_checks_route
            .as_ref()
            .unwrap()
            .loaded_capture_v2()
            .unwrap()
    ));
    assert!(app.pr_check_log_matches_v2.is_none());
    // Legitimate UI Close only closes the known query diagnostic, not a hidden
    // cleanup substitute; the failure-immediacy tests above never use Close.
    app.dispatch(Action::Close);
    let (owner, exact) = pure(&mut app, &f, &p);
    assert!(corrective::publish(&mut app, &owner, Ok(exact)));
    assert!(app
        .pr_check_log_matches_v2
        .as_ref()
        .unwrap()
        .belongs_to(&original));
    assert_eq!(app.pr_check_log_matches_v2.as_ref().unwrap().indices(), [1]);
    read_only(&f, &before, &git, &steps);
}
