//! Genuine PR/Checks/Run→resource→current List work. Allocation restrictions
//! are not a source factory, a write permit, or executed verification evidence.
use super::*;
use crate::providers::gh::WorkflowCoreJsonV9;

type LoadedResourceV9 = (
    Fixture,
    GithubProvider,
    App,
    Vec<GhStep>,
    Arc<actions::ScopedActionsPageV1>,
    usize,
);
fn loaded_resource() -> LoadedResourceV9 {
    let (f, p, mut app, steps) = setup(resources(42));
    let value = read(&f, &p, &mut app).unwrap();
    // Exact single Back snapshot created by the normal admitted publisher.
    // Its actual charge is the original workspace plus original entry page.
    let entry =
        crate::actions_workspace::scoped_read_v1::retained_bytes(app.actions.as_ref().unwrap())
            .unwrap()
            .checked_add(value.work.page.retained_bytes())
            .unwrap();
    let page = value.page.clone();
    publish_read_v2(&mut app, value);
    (f, p, app, steps, page, entry)
}
fn list_work(app: &mut App) -> Arc<actions::ScopedActionsWorkV1> {
    assert!(native::intercept(app, &ActionsAction::PublicationList));
    let work = native::take_work(app).expect("actual current native List work");
    assert!(work.journal_v5.is_some());
    work
}
#[test]
fn task54_workflow_edit_v9_genuine_pr_graph_precore_policy_borrows_without_children_or_source_effects(
) {
    let (f, p, mut app, steps, page, _) = loaded_resource();
    let original = app.remote_scope_source.as_ref().unwrap().clone();
    let route = app.pr_checks_route.as_ref().unwrap().clone();
    let work = list_work(&mut app);
    assert!(Arc::ptr_eq(&work.page, &page));
    assert!(work.source.pr_origin_v2().is_some());
    let graph = work
        .source
        .pr_origin_v2()
        .unwrap()
        .workflow_owned_bytes_v9()
        .unwrap();
    assert!(graph > original.workflow_owned_bytes_v9().unwrap());
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let commands = fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap();
    let counts = (Arc::strong_count(&original), Arc::strong_count(&route));
    let policy = WorkflowCoreJsonV9::for_history_v9(&p, &work).unwrap();
    assert_eq!(
        counts,
        (Arc::strong_count(&original), Arc::strong_count(&route))
    );
    assert!(!work.cancellation().is_cancelled());
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &original
    ));
    assert!(Arc::ptr_eq(app.pr_checks_route.as_ref().unwrap(), &route));
    assert_eq!(
        fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap(),
        commands
    );
    drop(policy);
    read_only(&f, &before, &git, &steps);
}
#[test]
fn task54_workflow_edit_v9_actual_pr_graph_closes_remaining_heap_before_any_core_child_or_clone() {
    let (f, p, mut app, steps, page, entry) = loaded_resource();
    let source = page.source();
    let pr_graph = source
        .pr_origin_v2()
        .unwrap()
        .workflow_owned_bytes_v9()
        .unwrap();
    let without_pr = source.original().workflow_owned_bytes_v9().unwrap()
        + source.scope().workflow_owned_bytes_v9().unwrap()
        + page.retained_bytes()
        + std::mem::size_of::<WorkflowCoreJsonV9>()
        + 2 * std::mem::size_of::<usize>()
        + crate::providers::runner::WorkflowProcessEnvironmentV9::current_v9()
            .unwrap()
            .retained_bytes_v9();
    let limit = 8 * 1024 * 1024;
    let desired_held = limit - without_pr - pr_graph / 2;
    let workspace = app.actions.as_mut().unwrap();
    let current = crate::actions_workspace::scoped_read_v1::retained_bytes(workspace).unwrap()
        + page.retained_bytes();
    let row = workspace.rows.first_mut().unwrap();
    let old = std::mem::take(&mut row.label);
    let capacity = old.capacity() + desired_held - entry - current;
    row.label = String::with_capacity(capacity);
    row.label.push_str(&old);
    assert_eq!(
        row.label, old,
        "Only genuine visible allocation capacity changes"
    );
    let original = app.remote_scope_source.as_ref().unwrap().clone();
    let route = app.pr_checks_route.as_ref().unwrap().clone();
    let work = list_work(&mut app);
    let held = work.journal_v5.as_ref().unwrap().held_workflow_bytes_v9();
    assert!(
        held + without_pr <= limit,
        "The old incomplete graph would fit"
    );
    assert!(
        held + without_pr + pr_graph > limit,
        "The actual retained PR graph refuses"
    );
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let commands = fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap();
    let counts = (Arc::strong_count(&original), Arc::strong_count(&route));
    let state = (
        app.status.clone(),
        app.loading,
        app.scoped_actions_cancel_v1,
    );
    assert!(WorkflowCoreJsonV9::for_history_v9(&p, &work).is_err());
    assert_eq!(
        counts,
        (Arc::strong_count(&original), Arc::strong_count(&route))
    );
    assert_eq!(
        state,
        (
            app.status.clone(),
            app.loading,
            app.scoped_actions_cancel_v1
        )
    );
    assert!(!work.cancellation().is_cancelled());
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &original
    ));
    assert!(Arc::ptr_eq(app.pr_checks_route.as_ref().unwrap(), &route));
    assert_eq!(
        fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap(),
        commands
    );
    read_only(&f, &before, &git, &steps);
}
