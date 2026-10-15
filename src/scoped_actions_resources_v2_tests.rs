//! Meaningful declarations over installed Git, SQLite and strict fake-gh.
//! SOURCE ONLY / UNRUN: Root owns all execution and nonzero count verification.
use super::*;
use crate::{
    scoped_actions_resources_test_support_v2 as r, scoped_actions_test_support_v1 as f,
    scoped_actions_v1 as core,
};
use serde_json::json;
fn a() -> ActionsAction {
    ActionsAction::Artifacts
}
#[test]
fn scoped_actions_resources_v2_settings_use_original_repository_existing_parsers_no_writes() {
    for (view, label) in [
        (ActionsView::Secrets, "SCOPED_SECRET_NAME_ONLY"),
        (ActionsView::Variables, "PRIVATE_ACTUAL_VARIABLE"),
        (ActionsView::Policies, "selected"),
        (ActionsView::Environments, "ACTUAL_PRODUCTION_ENV"),
        (ActionsView::Runners, "ACTUAL_LINUX_RUNNER"),
        (ActionsView::Caches, "ACTUAL_CACHE_KEY"),
    ] {
        let (fix, p, mut app, all) = f::setup(r::setting(view));
        let sql = f::sql_state(&fix);
        let git = f::git_state(&fix);
        let value = f::read(&mut app, &p, &fix, ActionsAction::View(view)).unwrap();
        assert_eq!(value.workspace.view, view);
        assert!(
            value
                .workspace
                .rows
                .iter()
                .any(|row| row.label.contains(label)),
            "Actual existing parser/rows for {view:?}"
        );
        assert!(value
            .workspace
            .controls
            .iter()
            .filter(|c| matches!(c.action, ActionsAction::Compose(_)))
            .all(|c| c.disabled_reason.is_some()));
        assert_unchanged(&fix, &sql, &git);
        f::exact(&fix, &all);
    }
}
#[test]
fn scoped_actions_resources_v2_settings_403_is_unknown_not_empty_admin_grant() {
    let mut op = r::setting(ActionsView::Secrets);
    op[3].stdout.clear();
    op[3].stderr = b"HTTP 403: Resource not accessible by integration".to_vec();
    op[3].status = 1;
    let (fix, p, mut app, all) = f::setup(op);
    let value = f::read(
        &mut app,
        &p,
        &fix,
        ActionsAction::View(ActionsView::Secrets),
    )
    .unwrap();
    assert!(value.workspace.secrets.is_empty());
    assert!(value
        .workspace
        .warnings
        .iter()
        .any(|s| s.contains("unavailable") && s.contains("no administrator")));
    assert!(value
        .workspace
        .controls
        .iter()
        .filter(|c| matches!(c.action, ActionsAction::Compose(_)))
        .all(|c| c.disabled_reason.is_some()));
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_missing_actual_default_branch_disables_open_no_head_guess() {
    let mut op = r::workflows();
    for at in [2, 4] {
        let mut raw = f::repo();
        raw["default_branch"] = json!(null);
        op[at].stdout = raw.to_string().into_bytes();
    }
    let (fix, p, mut app, all) = f::setup(op);
    let value = f::read(&mut app, &p, &fix, w()).unwrap();
    assert!(value
        .page
        .resource_v2()
        .unwrap()
        .read
        .repository()
        .default_branch()
        .is_none());
    assert!(
        matches!(&value.workspace.rows[0].action,Some(ActionsAction::Unavailable(reason))if reason.contains("no HEAD"))
    );
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_scope_change_retires_old_resource_before_any_new_request() {
    let (fix, p, mut app, all) = f::setup(r::workflows());
    let value = f::read(&mut app, &p, &fix, w()).unwrap();
    f::install(&mut app, value);
    app.remote_scope = None;
    crate::scoped_actions_native_v1::synchronize(&mut app);
    assert!(app.scoped_actions_v1.is_none());
    assert!(app.scoped_actions_work_v1.is_none());
    f::exact(&fix, &all);
}
fn w() -> ActionsAction {
    ActionsAction::View(ActionsView::Workflows)
}
fn selected_artifact() -> ActionsAction {
    ActionsAction::OpenArtifact(701)
}
fn selected_workflow() -> ActionsAction {
    ActionsAction::OpenWorkflow {
        id: 7,
        revision: "main".into(),
    }
}
type GitState = (String, String, Vec<u8>, Vec<u8>, Vec<u8>);
#[test]
fn scoped_actions_resources_v2_archive_original_narrow_512k_provider_is_not_widened_by_4m_limits() {
    let mut op = r::artifacts(vec![r::artifact()]);
    op.extend(r::archive(
        r::zip("large.txt", &vec![b'X'; 600 * 1024]),
        false,
    ));
    let (fix, p, mut app, all) = f::opened(op);
    let value = f::read(&mut app, &p, &fix, a()).unwrap();
    f::install(&mut app, value);
    let p = p
        .with_scoped_read_window(
            std::time::Instant::now() + std::time::Duration::from_secs(30),
            512 * 1024,
        )
        .unwrap();
    let sql = f::sql_state(&fix);
    let git = f::git_state(&fix);
    assert!(f::read(&mut app, &p, &fix, selected_artifact()).is_err());
    assert_unchanged(&fix, &sql, &git);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_archive_immutable_pool_keeps_original512k_after_public_clone_widen()
{
    let oversized = r::zip("large.txt", &vec![b'Q'; 600 * 1024]);
    let mut raw = crate::test_support::GhStep::read(
        &[
            "api",
            &format!("repos/{}/actions/artifacts/701/zip", f::REPO),
            "--hostname",
            f::HOST,
            "--method",
            "GET",
            "--allow-escape-sequences",
        ],
        "",
    );
    raw.stdout = oversized;
    let mut op = r::artifacts(vec![r::artifact()]);
    op.extend(r::wrap(
        vec![
            f::get(
                &format!("repos/{}/actions/artifacts/701", f::REPO),
                r::artifact(),
            ),
            raw,
        ],
        42,
    ));
    let (fix, p, mut app, all) = f::opened(op);
    let value = f::read(&mut app, &p, &fix, a()).unwrap();
    let resource = value.page.resource_v2().unwrap();
    let ScopedActionsResourceBodyV2::Artifacts { targets, .. } = resource.read.body() else {
        panic!("Genuine actual artifact read")
    };
    let target = targets[0].clone();
    let token = CancellationToken::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let original = p
        .clone()
        .with_scoped_read_window(deadline, 512 * 1024)
        .unwrap();
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_response_bytes = 9 * 1024 * 1024;
    limits.transport.max_document_bytes = 128 * 1024;
    limits.max_repository_reads = 2;
    limits.max_references = 1;
    let mut session = original
        .begin_repository_catalogue_access(value.page.source().original().source(), &limits, &token)
        .unwrap();
    session
        .verify(value.page.source().reference(), &token)
        .unwrap();
    let window = session
        .reserve_actions_archive_v2(&target, 1, &token)
        .unwrap();
    assert_eq!(window.max_output_bytes(), 512 * 1024);
    let cloned = original
        .clone()
        .with_measured_read_window(window)
        .unwrap()
        .with_scoped_read_window(deadline + std::time::Duration::from_secs(60), 1024 * 1024)
        .unwrap();
    assert!(cloned
        .download_artifact(f::REPO, target.model(), r::limits(), &token)
        .is_err());
    session
        .verify(value.page.source().reference(), &token)
        .unwrap();
    let closed = session.close_with_guard(&token, || Ok(())).unwrap();
    let accounting = closed.external_read_accounting().unwrap();
    assert_eq!(accounting.children_started(), 2);
    assert!(accounting.sealed());
    f::exact(&fix, &all);
}
fn assert_unchanged(
    fixture: &crate::test_support::Fixture,
    sql: &Vec<(String, String, String)>,
    git: &GitState,
) {
    assert_eq!(f::sql_state(fixture), *sql);
    assert_eq!(f::git_state(fixture), *git);
}
#[test]
fn scoped_actions_resources_v2_partial_run_page_opens_actual_member_without_all_history() {
    let mut steps = r::page(1, None, 10000, vec![f::run()]);
    steps.extend(f::operation(core::ScopedActionsRequestKindV1::Attempt, 42));
    let (fix, p, mut app, all) = f::setup(steps);
    let page = f::read(&mut app, &p, &fix, ActionsAction::Refresh).unwrap();
    assert!(!page.workspace.coverage.complete);
    assert_eq!(page.workspace.runs.len(), 1);
    f::install(&mut app, page);
    let opened = f::read(
        &mut app,
        &p,
        &fix,
        ActionsAction::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 2,
        },
    )
    .unwrap();
    assert_eq!(opened.page.run().unwrap().run().id, 123);
    assert_eq!(opened.page.run().unwrap().run().attempt, 2);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_next_page_sends_one_real_page_and_retains_partial() {
    let mut second = f::run();
    second["id"] = json!(124);
    second["node_id"] = json!("WR_124");
    let mut steps = r::page(1, None, 10000, vec![f::run()]);
    steps.extend(r::page(2, None, 10000, vec![second]));
    let (fix, p, mut app, all) = f::setup(steps);
    let first = f::read(&mut app, &p, &fix, ActionsAction::Refresh).unwrap();
    f::install(&mut app, first);
    let next = f::read(
        &mut app,
        &p,
        &fix,
        ActionsAction::ScopedReadV2(ScopedActionsReadActionV2::RunPage(1)),
    )
    .unwrap();
    assert_eq!(next.workspace.runs[0].run.id, 124);
    assert_eq!(next.page.run_window_v2().unwrap().query().page(), 2);
    assert!(!next.workspace.coverage.complete);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_status_filter_is_actual_query_and_resets_page_one() {
    let (fix, p, mut app, all) = f::setup(r::page(1, Some("failure"), 300, vec![f::run()]));
    let value = f::read(
        &mut app,
        &p,
        &fix,
        ActionsAction::ScopedReadV2(ScopedActionsReadActionV2::FailedRuns),
    )
    .unwrap();
    assert_eq!(
        value
            .page
            .run_window_v2()
            .unwrap()
            .query()
            .filter()
            .status
            .as_deref(),
        Some("failure")
    );
    assert!(!value.workspace.coverage.complete);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_empty_partial_page_is_not_absence_or_next_authority() {
    let (fix, p, mut app, all) = f::setup(r::page(1, None, 9999, vec![]));
    let value = f::read(&mut app, &p, &fix, ActionsAction::Refresh).unwrap();
    assert!(!value.workspace.coverage.complete);
    assert!(value.page.run_window_v2().unwrap().next_query().is_none());
    assert!(value
        .workspace
        .coverage
        .reasons
        .iter()
        .any(|s| s.contains("partial")));
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_invalid_page_or_control_query_is_not_authority() {
    for page in [0, SCOPED_RUN_MAX_PAGE_V2 + 1] {
        assert!(ScopedRunQueryV2::new(RunFilter::default(), page).is_err());
    }
    assert!(ScopedRunQueryV2::first(RunFilter {
        branch: Some("main\nprivate".into()),
        ..Default::default()
    })
    .is_err());
    assert!(ScopedRunQueryV2::first(RunFilter {
        status: Some("invented".into()),
        ..Default::default()
    })
    .is_err());
}
#[test]
fn scoped_actions_resources_v2_query_bounds_preserve_real_unicode_and_exact_head() {
    let q = ScopedRunQueryV2::new(
        RunFilter {
            branch: Some("feature/界".into()),
            head_sha: Some("a".repeat(40)),
            ..Default::default()
        },
        9,
    )
    .unwrap();
    assert_eq!(q.adjacent(1).unwrap().page(), 10);
    assert_eq!(q.filter().branch.as_deref(), Some("feature/界"));
    assert_eq!(
        q.filter().head_sha.as_deref(),
        Some("a".repeat(40).as_str())
    );
}
#[test]
fn scoped_actions_resources_v2_artifact_inventory_preserves_unknown_attempt_and_actual_run() {
    let (fix, p, mut app, all) = f::opened(r::artifacts(vec![r::artifact()]));
    let sql = f::sql_state(&fix);
    let git = f::git_state(&fix);
    let value = f::read(&mut app, &p, &fix, a()).unwrap();
    assert_eq!(value.workspace.artifacts[0].id, 701);
    assert_eq!(value.workspace.artifacts[0].attempt, None);
    assert_eq!(value.page.run().unwrap().run().id, 123);
    assert!(value.workspace.rows[0].label.contains("not exposed"));
    assert_unchanged(&fix, &sql, &git);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_unobserved_artifact_id_never_downloads() {
    let (fix, p, mut app, all) = f::opened(r::artifacts(vec![r::artifact()]));
    let value = f::read(&mut app, &p, &fix, a()).unwrap();
    let page = value.page.clone();
    f::install(&mut app, value);
    assert!(
        core::ScopedActionsWorkV1::capture(page, &ActionsAction::OpenArtifact(702), 1).is_err()
    );
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_artifact_duplicate_ids_refuse_no_partial_complete_inventory() {
    let mut op = r::artifacts(vec![r::artifact(), r::artifact()]);
    op.remove(5);
    let (fix, p, mut app, all) = f::opened(op);
    assert!(f::read(&mut app, &p, &fix, a()).is_err());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_artifact_other_numeric_repository_refuses() {
    let mut raw = r::artifact();
    raw["workflow_run"]["repository_id"] = json!(999);
    let mut op = r::artifacts(vec![raw]);
    op.remove(5);
    let (fix, p, mut app, all) = f::opened(op);
    assert!(f::read(&mut app, &p, &fix, a()).is_err());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_artifact_other_head_refuses_before_download() {
    let mut raw = r::artifact();
    raw["workflow_run"]["head_sha"] = json!("b".repeat(40));
    let mut op = r::artifacts(vec![raw]);
    op.remove(5);
    let (fix, p, mut app, all) = f::opened(op);
    assert!(f::read(&mut app, &p, &fix, a()).is_err());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_archive_and_retained_preview_use_same_actual_bytes_zero_new_children(
) {
    let mut op = r::artifacts(vec![r::artifact()]);
    op.extend(r::archive(
        r::zip("nested/界.txt", b"ACTUAL_PRIVATE_ARCHIVE\nsecond\n"),
        true,
    ));
    let (fix, p, mut app, all) = f::opened(op);
    let sql = f::sql_state(&fix);
    let git = f::git_state(&fix);
    let value = f::read(&mut app, &p, &fix, a()).unwrap();
    f::install(&mut app, value);
    let value = f::read(&mut app, &p, &fix, selected_artifact()).unwrap();
    assert!(value.workspace.archive.is_some());
    f::install(&mut app, value);
    let value = f::read(&mut app, &p, &fix, ActionsAction::ArtifactEntry(0)).unwrap();
    assert!(value
        .workspace
        .artifact_preview
        .as_deref()
        .unwrap()
        .contains("ACTUAL_PRIVATE_ARCHIVE"));
    assert!(value
        .workspace
        .controls
        .iter()
        .filter(|c| matches!(c.action, ActionsAction::Compose(_)))
        .all(|c| c.disabled_reason.is_some()));
    assert_unchanged(&fix, &sql, &git);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_unsafe_traversal_archive_refuses_without_local_extraction() {
    let mut op = r::artifacts(vec![r::artifact()]);
    op.extend(r::archive(r::zip("../escape", b"never publish"), false));
    let (fix, p, mut app, all) = f::opened(op);
    let value = f::read(&mut app, &p, &fix, a()).unwrap();
    f::install(&mut app, value);
    assert!(f::read(&mut app, &p, &fix, selected_artifact()).is_err());
    assert!(!fix.root.join("escape").exists());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_workflow_inventory_uses_actual_default_branch_and_numeric_member() {
    let (fix, p, mut app, all) = f::setup(r::workflows());
    let value = f::read(&mut app, &p, &fix, w()).unwrap();
    assert_eq!(value.workspace.workflows[0].id, 7);
    assert_eq!(
        value
            .page
            .resource_v2()
            .unwrap()
            .read
            .repository()
            .default_branch(),
        Some("main")
    );
    assert!(matches!(
        &value.workspace.rows[0].action,
        Some(ActionsAction::OpenWorkflow { id: 7, revision }) if revision == "main"
    ));
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_workflow_definition_retains_requested_ref_separately_from_real_sha()
{
    let mut op = r::workflows();
    op.extend(r::definition(r::workflow()));
    let (fix, p, mut app, all) = f::setup(op);
    let value = f::read(&mut app, &p, &fix, w()).unwrap();
    f::install(&mut app, value);
    let value = f::read(&mut app, &p, &fix, selected_workflow()).unwrap();
    let d = value.workspace.definition.as_ref().unwrap();
    assert_eq!(d.revision, "main");
    assert_eq!(d.commit_sha, "c".repeat(40));
    assert_eq!(d.jobs[0].id, "actual_job");
    assert!(value
        .workspace
        .rows
        .iter()
        .any(|row| row.label.contains("never-executed")));
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_workflow_closing_node_replacement_refuses_prepared_private_yaml() {
    let mut changed = r::workflow();
    changed["node_id"] = json!("WF_REPLACED");
    let mut op = r::workflows();
    op.extend(r::definition(changed));
    let (fix, p, mut app, all) = f::setup(op);
    let value = f::read(&mut app, &p, &fix, w()).unwrap();
    f::install(&mut app, value);
    assert!(f::read(&mut app, &p, &fix, selected_workflow()).is_err());
    assert!(app.actions.as_ref().unwrap().definition.is_none());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_final_changed_numeric_uid_refuses_private_inventory() {
    let mut op = r::workflows();
    let last = op.len() - 1;
    op[last] = f::get("user", json!({"id":99,"login":"octocat"}));
    let (fix, p, mut app, all) = f::setup(op);
    assert!(f::read(&mut app, &p, &fix, w()).is_err());
    assert!(app.actions.as_ref().unwrap().workflows.is_empty());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_queued_cancel_prevents_all_resource_children() {
    let (fix, p, mut app, all) = f::setup(vec![]);
    assert!(crate::scoped_actions_native_v1::intercept(&mut app, &w()));
    let work = crate::scoped_actions_native_v1::take_work(&mut app).unwrap();
    work.cancellation().cancel();
    assert!(core::execute(work, &p, &f::database(&fix)).is_err());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_prepared_resource_retained_capacity_not_only_text_length() {
    let (fix, p, mut app, all) = f::setup(r::workflows());
    let mut value = f::read(&mut app, &p, &fix, w()).unwrap();
    value.workspace.workflows[0]
        .path
        .reserve_exact(core::PREPARED_LIMIT_V1);
    assert!(crate::actions_workspace::scoped_read_v1::retained_bytes(&value.workspace).is_err());
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_resources_v2_resource_back_moves_original_workspace_with_zero_requests() {
    let (fix, p, mut app, all) = f::setup(r::workflows());
    let original = app.actions.as_ref().unwrap().rows[0].label.clone();
    let value = f::read(&mut app, &p, &fix, w()).unwrap();
    f::install(&mut app, value);
    assert!(crate::scoped_actions_native_v1::intercept(
        &mut app,
        &ActionsAction::Back
    ));
    assert_eq!(app.actions.as_ref().unwrap().rows[0].label, original);
    f::exact(&fix, &all);
}
