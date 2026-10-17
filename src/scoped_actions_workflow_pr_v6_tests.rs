//! Genuine original PR Source/Checks/Run route→Workflow, never a Catalogue
//! fabricated from names or a Workflow disguised as Run. SOURCE ONLY/UNRUN.
use super::*;
use crate::{
    forms::{FormAction, FormPurpose},
    scoped_actions_native_v1 as native,
    scoped_actions_native_v1::publication_v3 as publication,
    scoped_actions_publication_test_support_v3 as form_helpers,
};
#[path = "scoped_actions_workflow_corrective_v6_tests.rs"]
mod corrective_v6;
#[path = "scoped_actions_dispatch_extrema_v7_tests.rs"]
mod task54_w7_extrema;
fn workflow(state: &str) -> Value {
    json!({"id":7,"node_id":"WF_7","name":"PRIVATE_PR_WORKFLOW","path":".github/workflows/ci.yml","state":state})
}
fn wf(state: &str) -> GhStep {
    get(
        &format!("repos/{REPO}/actions/workflows/7"),
        workflow(state),
    )
}
fn bracket(body: Vec<GhStep>) -> Vec<GhStep> {
    let mut out = principal(42, "workflow-start");
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    out.push(get(
        &format!("repos/{REPO}/check-runs/600"),
        actual_check(REPO),
    ));
    out.extend(body);
    out.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    out.push(get(
        &format!("repos/{REPO}/check-runs/600"),
        actual_check(REPO),
    ));
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.extend(principal(42, "workflow-end"));
    out
}
fn list() -> Vec<GhStep> {
    let mut step = get(
        &format!("repos/{REPO}/actions/workflows?per_page=100"),
        json!([{"total_count":1,"workflows":[workflow("active")]}]),
    );
    step.args.extend(["--paginate".into(), "--slurp".into()]);
    bracket(vec![step])
}
fn definition() -> Vec<GhStep> {
    let sha = "c".repeat(40);
    bracket(vec![wf("active"),get(&format!("repos/{REPO}/commits/main"),json!({"sha":sha})),GhStep::read(&["api",&format!("repos/{REPO}/contents/.github/workflows/ci.yml?ref={sha}"),"--hostname",HOST,"--method","GET","--header","Accept: application/vnd.github.raw+json","--allow-escape-sequences"],"name: PRIVATE_PR_WORKFLOW\non: [push]\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo NEVER_EXECUTED\n"),wf("active")])
}
fn confirm(uid: u64) -> Vec<GhStep> {
    let mut out = principal(42, "workflow-start");
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    out.push(get(
        &format!("repos/{REPO}/check-runs/600"),
        actual_check(REPO),
    ));
    out.push(wf("active"));
    // The existing mutation guard receives Plan.identity from the actual
    // Preview opening principal, not this new Confirm session's phase label.
    out.extend(principal(42, "workflow-start"));
    out.push(wf("active"));
    out.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    out.push(get(
        &format!("repos/{REPO}/check-runs/600"),
        actual_check(REPO),
    ));
    out.push(wf("active"));
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.extend(principal(42, "workflow-start"));
    out.push(GhStep::read(
        &[
            "api",
            &format!("repos/{REPO}/actions/workflows/7/disable"),
            "--hostname",
            HOST,
            "--method",
            "PUT",
            "--include",
        ],
        "HTTP/2.0 204 No Content\nContent-Length: 0\r\n\r\n",
    ));
    out.push(wf("disabled_manually"));
    // The final prepaid principal is checked against this Confirm's genuine
    // opening identity too; a phase label is not an account-change permission.
    out.push(get("user", json!({"id":uid,"login":"workflow-start"})));
    assert_eq!(out.len(), 18);
    out
}
fn setup(tail: Vec<GhStep>) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    setup_yaml(tail, None)
}
fn setup_yaml(
    tail: Vec<GhStep>,
    yaml: Option<&str>,
) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let (fix, p, mut app, all, owner) = definition_request(tail, yaml);
    let result = publication::execute(&owner, &p, &database(&fix), None).unwrap();
    assert!(publication::publish(&mut app, &owner, result));
    app.scoped_actions_owner_v1 = None;
    (fix, p, app, all)
}
fn definition_request(
    tail: Vec<GhStep>,
    yaml: Option<&str>,
) -> (
    Fixture,
    GithubProvider,
    App,
    Vec<GhStep>,
    Arc<native::ScopedActionsOwnerV1>,
) {
    let mut steps = list();
    let mut definition = definition();
    if let Some(yaml) = yaml {
        let actual = definition
            .iter_mut()
            .find(|step| {
                step.args
                    .iter()
                    .any(|arg| arg == "Accept: application/vnd.github.raw+json")
            })
            .unwrap();
        actual.stdout = yaml.as_bytes().to_vec();
    }
    steps.extend(definition);
    steps.extend(tail);
    let (fix, p, mut app, all) = actions_rerun_v4::setup(steps);
    assert!(native::intercept(
        &mut app,
        &ActionsAction::View(crate::actions_workspace::ActionsView::Workflows)
    ));
    let work = native::take_work(&mut app).expect("Actual initial PR workflow read");
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let result = publication::execute(&owner, &p, &database(&fix), None).unwrap();
    assert!(publication::publish(&mut app, &owner, result));
    app.scoped_actions_owner_v1 = None;
    assert!(native::intercept(
        &mut app,
        &ActionsAction::OpenWorkflow {
            id: 7,
            revision: "main".into()
        }
    ));
    let work = native::take_work(&mut app).unwrap();
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    (fix, p, app, all, owner)
}
fn open(app: &mut App, p: &GithubProvider, fix: &Fixture) {
    assert!(native::intercept(
        app,
        &ActionsAction::Compose(ActionsCommand::DisableWorkflow)
    ));
    let owner = native::ScopedActionsOwnerV1::new(native::take_work(app).unwrap()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let result = publication::execute(&owner, p, &database(fix), None).unwrap();
    assert!(publication::publish(app, &owner, result));
    app.scoped_actions_owner_v1 = None;
    form_helpers::paint(app, 120, 40);
}
fn prepare(tail: Vec<GhStep>) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut out = bracket(vec![wf("active"), wf("active")]);
    out.extend(bracket(vec![wf("active"), wf("active")]));
    out.extend(tail);
    let (fix, p, mut app, all) = setup(out);
    open(&mut app, &p, &fix);
    form_helpers::preview(&mut app, &p, &fix);
    (fix, p, app, all)
}
fn attempt(
    app: &mut App,
    p: &GithubProvider,
    fix: &Fixture,
) -> (
    publication::PreparedResultV3,
    Arc<native::ScopedActionsOwnerV1>,
    crate::drafts::DraftWriter,
) {
    let captured = crate::scoped_actions_workflow_ui_v6::capture(
        app.form.as_ref().unwrap(),
        crate::scoped_actions_workflow_ui_v6::WorkflowPhaseV6::Confirm,
    )
    .unwrap();
    assert_eq!(
        captured.preview().unwrap().plan().identity.login,
        "workflow-start",
        "Actual Preview Plan identity is the shared mutation guard input"
    );
    let work = form_helpers::request(app, FormAction::Submit);
    let drafts = crate::drafts::DraftWriter::new(database(fix));
    let owner =
        native::ScopedActionsOwnerV1::new_publication_v3(work, &database(fix), &drafts.client())
            .unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let value = publication::execute(&owner, p, &database(fix), Some(&drafts.client())).unwrap();
    (value, owner, drafts)
}
#[test]
fn scoped_actions_workflow_v6_pr_workflow_keeps_original_source_nav_and_distinct_definition_commit()
{
    let (fix, _p, app, all) = prepare(vec![]);
    let FormPurpose::ScopedActionsWorkflowV6(state) = &app.form.as_ref().unwrap().purpose else {
        panic!("Actual Workflow Form")
    };
    let source = state.opened().intent().source();
    assert!(source.pr_origin_v2().is_some());
    assert!(Arc::ptr_eq(
        source.original(),
        app.remote_scope_source.as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(
        source.scope(),
        app.remote_scope.as_ref().unwrap()
    ));
    let pr = source.pr_origin_v2().unwrap().durable_pr_context_v3();
    assert_eq!(pr.head_sha, "a".repeat(40));
    assert_ne!(pr.head_sha, state.opened().intent().definition().commit_sha);
    assert!(app.actions.as_ref().unwrap().run.is_none());
    exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_pr_confirm_uses18_actual_children_and_exact_claimed_pr_workflow_aliases(
) {
    let (fix, p, mut app, all) = prepare(confirm(42));
    let (value, owner, _drafts) = attempt(&mut app, &p, &fix);
    let db = rusqlite::Connection::open(database(&fix)).unwrap();
    let (state, context): (String, String) = db
        .query_row(
            "SELECT state,context_json FROM actions_write_attempts",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "confirmed");
    let c: Value = serde_json::from_str(&context).unwrap();
    assert_eq!(c["original_workflow_v6"]["workflow_id"], 7);
    assert_eq!(c["original_repository_v3"]["repository_id"], 410);
    assert_eq!(c["original_pr_v3"]["number"], 7);
    assert_eq!(c["original_pr_v3"]["head_sha"], "a".repeat(40));
    assert!(c.get("original_run_v3").is_none());
    assert!(publication::publish(&mut app, &owner, value));
    form_helpers::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_pr_current_parent_push_before_put_keeps_pending_and_no_source_reissue(
) {
    let mut tail = confirm(42);
    let mut changed = resource_parent(REPO);
    changed["head"]["sha"] = json!("d".repeat(40));
    assert_eq!(tail[9].args[1], format!("repos/{REPO}/pulls/7"));
    tail[9] = get(&format!("repos/{REPO}/pulls/7"), changed);
    tail.truncate(10);
    let (fix, p, mut app, all) = prepare(tail);
    let (_value, _owner, _drafts) = attempt(&mut app, &p, &fix);
    assert_eq!(form_helpers::result_sql(&fix)[0].0, "pending");
    assert!(!all
        .iter()
        .any(|step| step.args.iter().any(|arg| arg == "PUT")));
    exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_pr_final_same_login_changed_uid_hides_only_actual_original_private_source(
) {
    let (fix, p, mut app, all) = prepare(confirm(999));
    let (value, owner, _drafts) = attempt(&mut app, &p, &fix);
    assert!(publication::publish(&mut app, &owner, value));
    assert!(app.form.is_none());
    assert!(app.scoped_actions_v1.is_none());
    assert!(app.github.is_none());
    assert_eq!(form_helpers::result_sql(&fix)[0].0, "confirmed");
    form_helpers::exact(&fix, &all);
}

#[test]
fn scoped_actions_workflow_v6_actual_preview_login_not_confirm_phase_label_guards_before_put() {
    let mut tail = confirm(42);
    tail[7] = get(
        "user",
        json!({"id":42,"login":"different-confirm-phase-label"}),
    );
    tail.truncate(8);
    let (fix, p, mut app, all) = prepare(tail);
    let git = git_state_resource(&fix);
    let (result, _owner, _drafts) = attempt(&mut app, &p, &fix);
    let publication::PreparedResultV3::Workflow(prepared) = result else {
        panic!("Actual Workflow attempt")
    };
    assert!(prepared.attempt_for_test_v6().unwrap().1.is_none());
    assert_eq!(
        form_helpers::result_sql(&fix),
        vec![("pending".into(), true, None)]
    );
    assert!(!all
        .iter()
        .any(|step| step.args.iter().any(|arg| arg == "PUT")));
    assert_eq!(git_state_resource(&fix), git);
    form_helpers::exact(&fix, &all);
}
