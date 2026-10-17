//! SOURCE ONLY / UNRUN. Genuine Catalogue→workflow-list→Definition factories,
//! installed Git, strict GH and actual SQLite. Root owns all test execution.
use super::*;
use crate::{
    actions_write_journal::*,
    forms::FormAction,
    providers::gh::*,
    scoped_actions_publication_test_support_v3 as w, scoped_actions_resources_test_support_v2 as r,
    scoped_actions_test_support_v1 as f,
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};
#[path = "novice_usability_workflow_v25_tests.rs"]
mod novice_usability_v25;
#[path = "../tests/support/owned_form_prose_v120.rs"]
mod prose_v120;
#[path = "scoped_actions_workflow_edit_v9_tests.rs"]
mod workflow_edit_v9;

#[test]
fn scoped_actions_workflow_v6_owned_ready_refuses_busy_or_old_state_before_one_tab() {
    let words = [
        "Actual current workflow 7",
        "state active",
        "[F4 Acknowledge locally]",
    ];
    // A real publication retains its old private body/footer while the same
    // source is being read. Neither command count nor repeated state text is
    // evidence that this busy presentation admits the next keyboard action.
    assert!(!prose_v120::ready_workflow_body_v124(
        "Observed state active → requested disabled_manually\n[F4 Acknowledge locally] [Esc Cancel]",
        &words
    ));
    assert!(!prose_v120::ready_workflow_body_v124(
        "Actual current workflow 7 • state active\n[F4 Acknowledge locally] [Esc Cancel]",
        &words
    ));
    assert!(!prose_v120::ready_workflow_body_v124(
        "Observed state active → requested disabled_manually\n[F4 Acknowledge locally] [Keep draft / Close]", &words));
    assert!(prose_v120::ready_workflow_body_v124(
        "Actual current workflow 7 • state active\n[View original] [F4 Acknowledge locally] [Keep draft / Close]", &words));
}

#[test]
fn scoped_actions_workflow_v6_prose_literal_preserves_actual_adjacent_wrap_and_one_source_space() {
    assert!(prose_v120::adjacent_literal(
        "exact no PUT/    \n   resend; NOT confirmed only observed",
        "no PUT/resend; NOT confirmed only observed"
    ));
    assert!(prose_v120::adjacent_literal(
        "state     \n   disabled_manually",
        "state disabled_manually"
    ));
    assert!(!prose_v120::adjacent_literal(
        "state     \n   disabled_manually",
        "state  disabled_manually"
    ));
}
#[test]
fn scoped_actions_workflow_v6_prose_literal_never_erases_not_only_or_interior_changes() {
    for body in [
        "effect confirmed only observed",
        "effect NOT confirmed observed",
        "effect NOT confXrmed only observed",
        "effect NOT  confirmed only observed",
    ] {
        assert!(!prose_v120::adjacent_literal(
            body,
            "effect NOT confirmed only observed"
        ));
    }
}
#[test]
fn scoped_actions_workflow_v6_prose_literal_rejects_crossframe_and_nonadjacent_content() {
    assert!(!prose_v120::adjacent_literal("no PUT/", "no PUT/resend"));
    assert!(!prose_v120::adjacent_literal("resend", "no PUT/resend"));
    assert!(!prose_v120::adjacent_literal(
        "no PUT/\n\nresend",
        "no PUT/resend"
    ));
    assert!(!prose_v120::adjacent_literal(
        "no PUT/\nother visible row\nresend",
        "no PUT/resend"
    ));
    assert!(!prose_v120::adjacent_literal("no PUT/", ""));
}
fn raw(state: &str) -> Value {
    let mut value = r::workflow();
    value["state"] = json!(state);
    value
}
fn workflow_get(value: Value) -> GhStep {
    f::get(&format!("repos/{}/actions/workflows/7", f::REPO), value)
}
fn phase(state: &str, uid: u64) -> Vec<GhStep> {
    r::wrap(
        vec![workflow_get(raw(state)), workflow_get(raw(state))],
        uid,
    )
}
fn ack(enabled: bool) -> GhStep {
    GhStep::read(
        &[
            "api",
            &format!(
                "repos/{}/actions/workflows/7/{}",
                f::REPO,
                if enabled { "enable" } else { "disable" }
            ),
            "--hostname",
            f::HOST,
            "--method",
            "PUT",
            "--include",
        ],
        "HTTP/2.0 204 No Content\nContent-Length: 0\r\n\r\n",
    )
}
fn attempt(before: &str, after: &str, enabled: bool, uid: u64) -> Vec<GhStep> {
    let mut out = f::principal(42);
    out.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    out.push(workflow_get(raw(before)));
    out.extend(f::principal(42));
    out.extend([workflow_get(raw(before)), workflow_get(raw(before))]);
    out.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    out.extend(f::principal(42));
    out.push(ack(enabled));
    out.push(workflow_get(raw(after)));
    out.push(f::get("user", json!({"id":uid,"login":"octocat"})));
    assert_eq!(out.len(), 14);
    out
}
fn setup(state: &str, tail: Vec<GhStep>) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut steps = r::wrap(
        vec![f::list(
            &format!("repos/{}/actions/workflows?per_page=100", f::REPO),
            json!({"total_count":1,"workflows":[raw(state)]}),
        )],
        42,
    );
    let mut definition = r::definition(raw(state));
    definition[3] = workflow_get(raw(state));
    steps.extend(definition);
    steps.extend(tail);
    let (fixture, provider, mut app, all) = f::setup(steps);
    let result = f::read(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::View(crate::actions_workspace::ActionsView::Workflows),
    )
    .unwrap();
    f::install(&mut app, result);
    let result = f::read(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::OpenWorkflow {
            id: 7,
            revision: "main".into(),
        },
    )
    .unwrap();
    f::install(&mut app, result);
    assert!(app.actions.as_ref().unwrap().run.is_none());
    (fixture, provider, app, all)
}
fn open(app: &mut App, provider: &GithubProvider, fixture: &Fixture, enabled: bool) {
    assert!(super::intercept(
        app,
        &ActionsAction::Compose(if enabled {
            ActionsCommand::EnableWorkflow
        } else {
            ActionsCommand::DisableWorkflow
        })
    ));
    let work = crate::scoped_actions_native_v1::take_work(app).expect("Actual workflow Open work");
    install(app, provider, fixture, work);
}
fn install(
    app: &mut App,
    provider: &GithubProvider,
    fixture: &Fixture,
    work: Arc<ScopedActionsWorkV1>,
) {
    let owner = ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let value = super::super::execute(&owner, provider, &f::database(fixture), None).unwrap();
    assert!(super::super::publish(app, &owner, value));
    app.scoped_actions_owner_v1 = None;
    w::paint(app, 120, 40);
}
fn preview(app: &mut App, provider: &GithubProvider, fixture: &Fixture) {
    let work = w::request(app, FormAction::Preview);
    install(app, provider, fixture, work);
    assert!(app.form.as_ref().unwrap().plan_ready());
}
fn prepared(
    state: &str,
    tail: Vec<GhStep>,
    enabled: bool,
) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut steps = phase(state, 42);
    steps.extend(phase(state, 42));
    steps.extend(tail);
    let (fixture, provider, mut app, all) = setup(state, steps);
    open(&mut app, &provider, &fixture, enabled);
    preview(&mut app, &provider, &fixture);
    (fixture, provider, app, all)
}
fn confirm(
    app: &mut App,
    provider: &GithubProvider,
    fixture: &Fixture,
) -> (
    PreparedResultV3,
    Arc<ScopedActionsOwnerV1>,
    crate::drafts::DraftWriter,
) {
    let work = w::request(app, FormAction::Submit);
    let writer = crate::drafts::DraftWriter::new(f::database(fixture));
    let owner =
        ScopedActionsOwnerV1::new_publication_v3(work, &f::database(fixture), &writer.client())
            .unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let value = super::super::execute(
        &owner,
        provider,
        &f::database(fixture),
        Some(&writer.client()),
    )
    .unwrap();
    (value, owner, writer)
}
fn opened(app: &App) -> Arc<workflow::OpenedWorkflowV6> {
    let FormPurpose::ScopedActionsWorkflowV6(state) = &app.form.as_ref().unwrap().purpose else {
        panic!("Actual workflow Form")
    };
    state.opened().clone()
}
type PreservedEffectsV112 = (
    Vec<(String, String, String)>,
    (String, String, Vec<u8>, Vec<u8>, Vec<u8>),
);
fn no_effects(fixture: &Fixture, before: &PreservedEffectsV112, all: &[GhStep]) {
    assert_eq!(f::sql_state(fixture), before.0);
    assert_eq!(f::git_state(fixture), before.1);
    w::exact(fixture, all);
}
fn assert_no_publication_schema(fixture: &Fixture) {
    let db = rusqlite::Connection::open(f::database(fixture)).unwrap();
    let count:i64=db.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='actions_write_attempts'",[],|row|row.get(0)).unwrap();
    assert_eq!(
        count, 0,
        "Refusal occurred before journal construction; no empty-table fiction"
    );
}

fn assert_visible_actions_focus_v120(form: &mut crate::forms::Form, errors: bool) {
    use crossterm::event::{KeyCode, KeyModifiers};
    assert!(form.preview);
    if errors {
        form.error = "Visible original error; no new authority".into();
    }
    let first = form.fields.len();
    let count = if errors { 5 } else { 3 };
    assert_eq!(
        form.focus, first,
        "Actual installation starts on a visible button"
    );
    for step in 1..=count * 2 {
        assert!(form.key(KeyCode::Tab, KeyModifiers::empty()).is_none());
        assert_eq!(
            form.focus,
            first + step % count,
            "No hidden field in Preview Tab ring"
        );
    }
    for step in 1..=count * 2 {
        assert!(form.key(KeyCode::BackTab, KeyModifiers::SHIFT).is_none());
        assert_eq!(
            form.focus,
            first + (count - step % count) % count,
            "BackTab uses the same complete visible controls"
        );
    }
}

#[test]
fn scoped_actions_workflow_v6_actual_preview_initial_focus_and_visible_tab_backtab_errors() {
    for errors in [false, true] {
        let (fix, _p, mut app, all) = prepared("active", vec![], false);
        let before = (f::sql_state(&fix), f::git_state(&fix));
        assert_visible_actions_focus_v120(app.form.as_mut().unwrap(), errors);
        no_effects(&fix, &before, &all);
    }
}

#[test]
fn scoped_actions_workflow_v6_actual_history_focus_remains_visible_with_hidden_fields() {
    let mut tail = attempt("active", "active", false, 42);
    tail.extend(r::wrap(vec![workflow_get(raw("disabled_manually"))], 42));
    let (fix, p, mut app, all) = prepared("active", tail, false);
    let (result, owner, _writer) = confirm(&mut app, &p, &fix);
    assert!(super::super::publish(&mut app, &owner, result));
    app.scoped_actions_owner_v1 = None;
    w::paint(&mut app, 120, 40);
    let work = w::request(&mut app, FormAction::Submit);
    install(&mut app, &p, &fix, work);
    let before = (f::sql_state(&fix), f::git_state(&fix));
    assert_visible_actions_focus_v120(app.form.as_mut().unwrap(), false);
    assert_visible_actions_focus_v120(app.form.as_mut().unwrap(), true);
    no_effects(&fix, &before, &all);
}

#[test]
fn scoped_actions_workflow_v6_current_history_whole_state_and_path_visible_at80_without_extra_io() {
    use crate::{actions_workspace::ActionsView, app::Action, scoped_actions_native_v1 as native};
    fn draw_actual_actions(app: &mut App, height: u16) -> String {
        assert_eq!(app.view, crate::app::View::Actions);
        assert!(app.overlay.is_none());
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, height)).unwrap();
        terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
        assert_eq!(app.view, crate::app::View::Actions);
        assert!(app.overlay.is_none());
        assert!(app.actions_visible_rows > 0);
        terminal
            .backend()
            .buffer()
            .content
            .chunks(80)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
    for state in ["disabled_manually", "disabled_inactivity", "active"] {
        let mut tail = attempt("active", "active", false, 42);
        tail.extend(r::wrap(vec![], 42));
        tail.extend(r::wrap(vec![workflow_get(raw(state))], 42));
        if state == "disabled_manually" {
            tail.extend(r::wrap(vec![workflow_get(raw(state))], 42));
        }
        let (fix, p, mut app, all) = prepared("active", tail, false);
        let (result, owner, _writer) = confirm(&mut app, &p, &fix);
        assert!(super::super::publish(&mut app, &owner, result));
        app.scoped_actions_owner_v1 = None;
        app.dispatch(Action::Form(FormAction::Close));
        assert!(app.overlay.is_none());
        let before = (f::sql_state(&fix), f::git_state(&fix));
        for action in [
            ActionsAction::PublicationList,
            ActionsAction::PublicationInspect(1),
        ] {
            assert!(native::intercept(&mut app, &action));
            let work = native::take_work(&mut app).expect("Actual original history continuation");
            install(&mut app, &p, &fix, work);
        }
        assert_eq!(
            app.actions.as_ref().unwrap().view,
            ActionsView::PublicationInspection
        );
        assert_eq!(app.view, crate::app::View::Actions);
        assert!(app.overlay.is_none());
        let actual = format!("Actual current workflow 7 / state {state}");
        let whole_path = "Workflow path .github/workflows/ci.yml";
        for height in [20, 24] {
            // Selection uses the same actual native action as keys/hits; no
            // model/observation/source is fabricated for a layout assertion.
            let state_row = app
                .actions
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .position(|row| row.label == actual)
                .unwrap();
            app.dispatch(Action::Actions(ActionsAction::Select(state_row)));
            assert_eq!(app.actions.as_ref().unwrap().selected, state_row);
            let visible = draw_actual_actions(&mut app, height);
            assert!(
                visible.contains(&actual),
                "Full actual state, never disabled_ prefix at80x{height}:\n{visible}"
            );
            let path_row = app
                .actions
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .position(|row| row.label.starts_with(whole_path))
                .unwrap();
            app.dispatch(Action::Actions(ActionsAction::Select(path_row)));
            assert_eq!(app.actions.as_ref().unwrap().selected, path_row);
            let visible = draw_actual_actions(&mut app, height);
            assert!(
                visible.contains(&format!("{whole_path} • no causal claim")),
                "Actual complete current workflow path at80x{height}:\n{visible}"
            );
        }
        assert!(app
            .actions
            .as_ref()
            .unwrap()
            .publication_inspection
            .as_ref()
            .unwrap()
            .lines
            .iter()
            .any(|line| line.contains("Workflow path") && line.contains("no causal claim")));
        if state == "disabled_manually" {
            let original = || {
                let db = rusqlite::Connection::open_with_flags(
                    f::database(&fix),
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )
                .unwrap();
                db.query_row(
                    "SELECT state,json_array(attempt_id,host,principal,target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,claimed,receipt_json,created_at) FROM actions_write_attempts WHERE attempt_id=1",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .unwrap()
            };
            let raw_before = original();
            assert_eq!(raw_before.0, "accepted_pending");
            assert!(native::intercept(
                &mut app,
                &ActionsAction::PublicationAcknowledge(1)
            ));
            let work = native::take_work(&mut app).expect("Actual inspected workflow ACK work");
            install(&mut app, &p, &fix, work);
            assert_eq!(original(), ("transition_observed".into(), raw_before.1));
            for height in [20, 24] {
                for fact in [
                    "Explicit original workflow state ACK committed locally",
                    "Exact receipt retained • no PUT/retry",
                ] {
                    let row = app
                        .actions
                        .as_ref()
                        .unwrap()
                        .rows
                        .iter()
                        .position(|row| row.label == fact)
                        .expect("Actual committed original ACK fact");
                    app.dispatch(Action::Actions(ActionsAction::Select(row)));
                    assert_eq!(app.actions.as_ref().unwrap().selected, row);
                    let visible = draw_actual_actions(&mut app, height);
                    assert!(
                        visible.contains(fact),
                        "Full actual ACK fact, no hidden no-PUT qualifier at80x{height}:\n{visible}"
                    );
                }
            }
        }
        no_effects(&fix, &before, &all);
    }
}

#[test]
fn scoped_actions_workflow_v6_same_shared_run_preview_skips_hidden_debug_field() {
    let mut phases = w::read_phase(w::running(), 42);
    phases.extend(w::read_phase(w::running(), 42));
    let (fix, p, mut app, all) = w::setup(phases);
    w::open(&mut app, &p, &fix);
    w::preview(&mut app, &p, &fix);
    let before = (f::sql_state(&fix), f::git_state(&fix));
    assert_visible_actions_focus_v120(app.form.as_mut().unwrap(), false);
    assert_visible_actions_focus_v120(app.form.as_mut().unwrap(), true);
    no_effects(&fix, &before, &all);
}

#[test]
fn scoped_actions_workflow_v6_actual_definition_source_has_no_run_or_guessed_target() {
    let (fix, p, mut app, all) = setup("active", phase("active", 42));
    let before = (f::sql_state(&fix), f::git_state(&fix));
    open(&mut app, &p, &fix, false);
    let actual = opened(&app);
    assert!(Arc::ptr_eq(
        actual.intent().page(),
        &app.scoped_actions_v1.as_ref().unwrap().page
    ));
    assert_eq!(actual.target().model().id, 7);
    assert_eq!(actual.target().node_v6(), Some("WF_7"));
    assert!(actual.intent().page().run().is_none());
    no_effects(&fix, &before, &all);
}
#[test]
fn scoped_actions_workflow_v6_open_and_preview_are_readonly_capacity_bounded_and_explicit_whole_workflow(
) {
    let mut steps = phase("active", 42);
    steps.extend(phase("active", 42));
    let (fix, p, mut app, all) = setup("active", steps);
    let before = (f::sql_state(&fix), f::git_state(&fix));
    open(&mut app, &p, &fix, false);
    preview(&mut app, &p, &fix);
    let form = app.form.as_ref().unwrap();
    assert!(form.plan_ready());
    assert!(form
        .plan_lines()
        .iter()
        .any(|s| s == "Disable workflow • entire repository • all refs"));
    assert!(form
        .plan_lines()
        .iter()
        .any(|s| s.contains(&"c".repeat(40))));
    assert!(opened(&app).retained() <= 8 * 1024 * 1024);
    assert!(form.fields.is_empty());
    no_effects(&fix, &before, &all);
}
#[test]
fn scoped_actions_workflow_v6_definition_clone_preflight_charges_capacity_before_yaml_plan_clone() {
    let (fix, _p, app, all) = setup("active", vec![]);
    let before = (f::sql_state(&fix), f::git_state(&fix));
    let state = app.scoped_actions_v1.as_ref().unwrap();
    let intent =
        workflow::ScopedWorkflowIntentV6::capture(state.page.clone(), state.epoch, false).unwrap();
    workflow::definition_clone_preflight_v6(intent.definition()).unwrap();
    let mut actual = (**intent.definition()).clone();
    let original_yaml = actual.yaml.clone();
    actual.yaml.reserve_exact(2 * 1024 * 1024);
    assert_eq!(
        actual.yaml, original_yaml,
        "Same actual YAML; only retained capacity grew"
    );
    assert!(workflow::definition_clone_preflight_v6(&actual).is_err());
    assert!(Arc::ptr_eq(
        intent.definition(),
        state
            .page
            .resource_v2()
            .unwrap()
            .workflow_definition_v6()
            .unwrap()
            .1
    ));
    no_effects(&fix, &before, &all);
}
#[test]
fn scoped_actions_workflow_v6_prepared_label_bound_refuses_before_shared_document_allocation() {
    let (fix, p, mut app, all) = setup("active", phase("active", 42));
    open(&mut app, &p, &fix, false);
    let before = (f::sql_state(&fix), f::git_state(&fix));
    let actual = opened(&app);
    let token = crate::providers::runner::CancellationToken::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let doc = workflow::display_v6(actual.lines(), &token, deadline).unwrap();
    assert!(doc.preparation_peak_bound() < 2 * 1024 * 1024);
    let mut oversized = actual.lines().to_vec();
    oversized.push("x".repeat(32 * 1024 + 1));
    assert!(workflow::display_v6(&oversized, &token, deadline).is_err());
    assert!(workflow::display_v6(&vec!["".into(); 33], &token, deadline).is_err());
    assert!(Arc::ptr_eq(&actual, &opened(&app)));
    no_effects(&fix, &before, &all);
}
#[test]
fn scoped_actions_workflow_v6_enable_and_disable_form_purposes_keep_actual_workflow_not_run() {
    for (state, enabled) in [("active", false), ("disabled_manually", true)] {
        let (fix, p, mut app, all) = setup(state, phase(state, 42));
        open(&mut app, &p, &fix, enabled);
        assert_eq!(opened(&app).intent().enabled(), enabled);
        assert!(matches!(
            app.form.as_ref().unwrap().purpose,
            FormPurpose::ScopedActionsWorkflowV6(_)
        ));
        assert!(app.actions.as_ref().unwrap().run.is_none());
        w::exact(&fix, &all);
    }
}
#[test]
fn scoped_actions_workflow_v6_already_enabled_refuses_capture_without_gh_sql_or_epoch_change() {
    let (fix, _p, mut app, all) = setup("active", vec![]);
    let before = (f::sql_state(&fix), f::git_state(&fix));
    let epoch = app.scoped_actions_v1.as_ref().unwrap().epoch;
    assert!(super::intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::EnableWorkflow)
    ));
    assert!(app.scoped_actions_work_v1.is_none());
    assert_eq!(app.scoped_actions_v1.as_ref().unwrap().epoch, epoch);
    no_effects(&fix, &before, &all);
}
#[test]
fn scoped_actions_workflow_v6_already_disabled_refuses_without_a_new_attempt() {
    let (fix, _p, mut app, all) = setup("disabled_manually", vec![]);
    let before = (f::sql_state(&fix), f::git_state(&fix));
    assert!(super::intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::DisableWorkflow)
    ));
    assert!(app.scoped_actions_work_v1.is_none());
    no_effects(&fix, &before, &all);
}
#[test]
fn scoped_actions_workflow_v6_unknown_state_never_enables_a_publisher() {
    let (fix, _p, mut app, all) = setup("future_state", vec![]);
    for command in [
        ActionsCommand::EnableWorkflow,
        ActionsCommand::DisableWorkflow,
    ] {
        super::intercept(&mut app, &ActionsAction::Compose(command));
        assert!(app.scoped_actions_work_v1.is_none());
    }
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_catalogue_run_page_cannot_issue_workflow_intent() {
    let (fix, _p, app, all) = f::setup(vec![]);
    assert!(workflow::ScopedWorkflowIntentV6::capture(
        app.scoped_actions_v1.as_ref().unwrap().page.clone(),
        1,
        false
    )
    .is_err());
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_changed_scope_rejects_before_token_or_epoch_effects() {
    let (fix, _p, mut app, all) = setup("active", vec![]);
    let state = app.scoped_actions_v1.as_ref().unwrap();
    let epoch = state.epoch;
    let before = (f::sql_state(&fix), f::git_state(&fix));
    app.remote_scope = None;
    super::intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::DisableWorkflow),
    );
    assert!(app.scoped_actions_work_v1.is_none());
    assert_eq!(app.scoped_actions_v1.as_ref().unwrap().epoch, epoch);
    no_effects(&fix, &before, &all);
}
#[test]
fn scoped_actions_workflow_v6_real_confirm_disable_uses_one_put_and_typed_workflow_receipt() {
    let (fix, p, mut app, all) = prepared(
        "active",
        attempt("active", "disabled_manually", false, 42),
        false,
    );
    let git = f::git_state(&fix);
    let (value, owner, _writer) = confirm(&mut app, &p, &fix);
    let rows = w::result_sql(&fix);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "confirmed");
    let receipt: Value = serde_json::from_str(rows[0].2.as_ref().unwrap()).unwrap();
    assert_eq!(receipt["kind"], "workflow_state");
    assert_eq!(receipt["resource_id"], 7);
    assert!(super::super::publish(&mut app, &owner, value));
    assert!(app.form.as_ref().unwrap().actions_publication.is_some());
    assert_eq!(f::git_state(&fix), git);
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_real_confirm_enable_retains_distinct_workflow_metadata() {
    let (fix, p, mut app, all) = prepared(
        "disabled_inactivity",
        attempt("disabled_inactivity", "active", true, 42),
        true,
    );
    let (_value, _owner, _writer) = confirm(&mut app, &p, &fix);
    let db = rusqlite::Connection::open(f::database(&fix)).unwrap();
    let context: String = db
        .query_row("SELECT context_json FROM actions_write_attempts", [], |r| {
            r.get(0)
        })
        .unwrap();
    let context: Value = serde_json::from_str(&context).unwrap();
    assert_eq!(context["original_workflow_v6"]["workflow_id"], 7);
    assert_eq!(context["original_workflow_v6"]["node"], "WF_7");
    assert!(context.get("original_run_v3").is_none());
    assert_eq!(context["run_id"], Value::Null);
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_accepted_unchanged_state_stays_protected_not_success() {
    let (fix, p, mut app, all) = prepared("active", attempt("active", "active", false, 42), false);
    let (value, owner, _writer) = confirm(&mut app, &p, &fix);
    assert_eq!(w::result_sql(&fix)[0].0, "accepted_pending");
    assert!(super::super::publish(&mut app, &owner, value));
    assert!(app.form.as_ref().unwrap().error.contains("NOT observed"));
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_final_numeric_uid_change_never_publishes_private_underlay() {
    let (fix, p, mut app, all) = prepared(
        "active",
        attempt("active", "disabled_manually", false, 999),
        false,
    );
    let (value, owner, _writer) = confirm(&mut app, &p, &fix);
    assert!(super::super::publish(&mut app, &owner, value));
    assert!(app.form.is_none());
    assert!(app.scoped_actions_v1.is_none());
    assert_eq!(w::result_sql(&fix)[0].0, "confirmed");
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_recreated_workflow_node_before_put_refuses_and_keeps_pending() {
    let mut tail = attempt("active", "disabled_manually", false, 42);
    let mut changed = raw("active");
    changed["node_id"] = json!("WF_RECREATED");
    tail[3] = workflow_get(changed);
    tail.truncate(4);
    let (fix, p, mut app, all) = prepared("active", tail, false);
    let (value, owner, _writer) = confirm(&mut app, &p, &fix);
    assert_no_publication_schema(&fix);
    assert!(super::super::publish(&mut app, &owner, value));
    assert!(app.form.is_none());
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_post_put_recreated_node_is_uncertain_never_accepted_as_same_workflow()
{
    let mut tail = attempt("active", "disabled_manually", false, 42);
    let mut changed = raw("disabled_manually");
    changed["node_id"] = json!("WF_RECREATED");
    tail[12] = workflow_get(changed);
    tail.truncate(13);
    let (fix, p, mut app, all) = prepared("active", tail, false);
    let (_value, _owner, _writer) = confirm(&mut app, &p, &fix);
    assert_eq!(w::result_sql(&fix)[0].0, "uncertain");
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_post_put_missing_workflow_is_uncertain_not_deletion_receipt() {
    let mut tail = attempt("active", "disabled_manually", false, 42);
    tail[12].stdout.clear();
    tail[12].status = 1;
    tail[12].stderr = b"HTTP404 PRIVATE_WORKFLOW_CANARY".to_vec();
    tail.truncate(13);
    let (fix, p, mut app, all) = prepared("active", tail, false);
    let (_value, _owner, _writer) = confirm(&mut app, &p, &fix);
    assert_eq!(w::result_sql(&fix)[0].0, "uncertain");
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_nonempty_http204_body_retains_uncertainty_and_never_resends() {
    let mut tail = attempt("active", "disabled_manually", false, 42);
    tail[11].stdout = b"HTTP/2.0 204 No Content\nContent-Length: 1\r\n\r\nx".to_vec();
    tail.truncate(12);
    let (fix, p, mut app, all) = prepared("active", tail, false);
    let (_value, _owner, _writer) = confirm(&mut app, &p, &fix);
    assert_eq!(w::result_sql(&fix)[0].0, "uncertain");
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_hidden_tiny_preview_has_no_confirm_hit_or_work() {
    let (fix, _p, mut app, all) = prepared("active", vec![], false);
    app.form
        .as_mut()
        .unwrap()
        .clear_preview_display_geometry_v1();
    let screen = w::paint(&mut app, 40, 10);
    assert!(screen.to_ascii_lowercase().contains("resize"));
    assert!(!app
        .hits
        .iter()
        .any(|hit| hit.action == Action::Form(FormAction::Submit)));
    app.form.as_mut().unwrap().dispatch(&FormAction::Submit);
    assert!(!super::capture_form_work(&mut app));
    assert!(app.scoped_actions_work_v1.is_none());
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_resize_keeps_same_preview_without_renewal_but_rejects_old_request() {
    let (fix, _p, mut app, all) = prepared("active", vec![], false);
    let old = workflow_ui::capture(app.form.as_ref().unwrap(), WorkflowPhaseV6::Confirm).unwrap();
    app.form
        .as_mut()
        .unwrap()
        .clear_preview_display_geometry_v1();
    assert!(!old.matches_form(app.form.as_ref().unwrap()));
    w::paint(&mut app, 120, 40);
    assert!(app.form.as_ref().unwrap().plan_ready());
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_confirm_captures_same_original_clock_token_and_is_single_use() {
    let (fix, _p, mut app, all) = prepared("active", vec![], false);
    let work = w::request(&mut app, FormAction::Submit);
    let Some(PublicationWorkV3::Workflow(publication)) = work.publication_v3.as_deref() else {
        panic!("Actual Workflow Work")
    };
    let confirm = publication.confirm().unwrap();
    assert!(confirm.cancellation().same_instance(work.cancellation()));
    assert_eq!(confirm.deadline(), work.deadline());
    confirm.start().unwrap();
    assert!(confirm.start().is_err());
    work.cancellation().cancel();
    assert!(confirm.start().is_err());
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_genuinely_newer_form_close_does_not_cancel_old_confirm_or_newer_presentation(
) {
    let mut tail = phase("active", 42);
    let (fix, p, mut app, all) = prepared("active", std::mem::take(&mut tail), false);
    let work = w::request(&mut app, FormAction::Submit);
    let owner = ScopedActionsOwnerV1::new(work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner);
    let old = opened(&app);
    let fresh = workflow::open(
        old.intent().clone(),
        &p,
        &f::database(&fix),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    app.form = Some(workflow_ui::form(fresh).unwrap());
    w::paint(&mut app, 120, 40);
    let status = app.status.clone();
    let hits = app.hits.len();
    super::super::cancel(&mut app);
    assert!(!work.cancellation().is_cancelled());
    assert!(app.form.is_some());
    assert_eq!(app.status, status);
    assert_eq!(app.hits.len(), hits);
    assert!(!app.scoped_actions_cancel_v1);
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_same_current_confirm_resize_logically_cancels_only_original_token() {
    let (fix, _p, mut app, all) = prepared("active", vec![], false);
    let work = w::request(&mut app, FormAction::Submit);
    let owner = ScopedActionsOwnerV1::new(work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner);
    assert!(super::super::resize(&mut app));
    assert!(work.cancellation().is_cancelled());
    assert!(app.form.is_none());
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_all_visible_footer_buttons_have_same_key_mouse_actions() {
    let (fix, _p, mut app, all) = prepared("active", vec![], false);
    let screen = w::paint(&mut app, 80, 24);
    for (label, action) in [
        ("[Preview]", FormAction::Preview),
        ("[Confirm action]", FormAction::Submit),
        ("[Keep draft / Close]", FormAction::Close),
    ] {
        assert!(screen.contains(label));
        assert!(app
            .hits
            .iter()
            .any(|hit| hit.action == Action::Form(action.clone())));
    }
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_legacy_none_context_bytes_remain_exact_and_appended_field_absent() {
    assert_eq!(
        serde_json::to_string(&ActionsImmutableContext::default()).unwrap(),
        r#"{"run_id":null,"attempt":null,"head_sha":null,"workflow_commit":null,"job_id":null}"#
    );
    let old: ActionsImmutableContext = serde_json::from_str(
        r#"{"run_id":null,"attempt":null,"head_sha":null,"workflow_commit":null,"job_id":null}"#,
    )
    .unwrap();
    assert!(old.original_workflow_v6.is_none());
}
#[test]
fn scoped_actions_workflow_v6_some_context_keeps_exact_original_current_numeric_aliases() {
    let (fix, p, mut app, all) = prepared("active", vec![], false);
    let FormPurpose::ScopedActionsWorkflowV6(state) = &app.form.as_ref().unwrap().purpose else {
        panic!("Actual Workflow")
    };
    let request =
        workflow_ui::capture(app.form.as_ref().unwrap(), WorkflowPhaseV6::Confirm).unwrap();
    let preview = request.preview().unwrap();
    let descriptor = preview
        .plan()
        .scoped_workflow_state_descriptor_v6(
            &p,
            state.opened().intent().source(),
            preview.target(),
            false,
        )
        .unwrap();
    assert!(descriptor
        .target
        .resource_keys()
        .iter()
        .any(|key| key == "github.invalid/repository-id/10000/actions/workflow/7"));
    assert!(descriptor
        .target
        .resource_keys()
        .iter()
        .any(|key| key.ends_with("/example/repo0000/workflow/7")));
    assert!(descriptor.intent.context.original_run_v3.is_none());
    assert!(descriptor.intent.context.original_workflow_v6.is_some());
    app.form.as_mut().unwrap().dispatch(&FormAction::Close);
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_workflow_metadata_cannot_authorize_run_or_dispatch_kind() {
    let context = ActionsImmutableContext {
        original_repository_v3: Some(ActionsOriginalRepositoryContextV3 {
            repository_id: 10000,
            repository_node: "R_10000".into(),
            original_name: f::REPO.into(),
        }),
        original_workflow_v6: Some(ActionsOriginalWorkflowContextV6 {
            workflow_id: 7,
            node: "WF_7".into(),
            path: ".github/workflows/ci.yml".into(),
        }),
        ..Default::default()
    };
    let scope = crate::storage::Scope::github(f::HOST, Some(42)).unwrap();
    let target = ActionsWriteTarget::from_original_context_v3(
        &scope,
        &ActionsTarget::Workflow {
            repository: f::REPO.into(),
            id: 7,
        },
        &context,
    )
    .unwrap();
    for kind in [
        ActionsWriteKind::Cancel,
        ActionsWriteKind::DispatchWorkflow,
        ActionsWriteKind::DeleteLogs,
    ] {
        let intent = ActionsWriteIntent::new(
            kind,
            context.clone(),
            ActionsSafeOptions {
                enabled: Some(true),
                ..Default::default()
            },
            b"",
        )
        .unwrap();
        assert!(intent.fingerprint(&target).is_err());
    }
}
#[test]
fn scoped_actions_workflow_v6_mixed_run_workflow_context_is_rejected_without_provider() {
    let context = ActionsImmutableContext {
        original_repository_v3: Some(ActionsOriginalRepositoryContextV3 {
            repository_id: 1,
            repository_node: "R_1".into(),
            original_name: f::REPO.into(),
        }),
        original_run_v3: Some(ActionsOriginalRunContextV3 {
            run_node: "WR_123".into(),
            workflow_id: 7,
            check_suite_id: None,
        }),
        original_workflow_v6: Some(ActionsOriginalWorkflowContextV6 {
            workflow_id: 7,
            node: "WF_7".into(),
            path: ".github/workflows/ci.yml".into(),
        }),
        run_id: Some(123),
        attempt: Some(2),
        head_sha: Some("a".repeat(40)),
        ..Default::default()
    };
    assert!(ActionsWriteIntent::new(
        ActionsWriteKind::EnableWorkflow,
        context,
        Default::default(),
        b""
    )
    .is_err());
}
#[test]
fn scoped_actions_workflow_v6_public_saved_workflow_scalar_cannot_construct_actual_form_intent() {
    let (fix, _p, app, all) = f::setup(vec![]);
    let saved: Workflow = serde_json::from_value(raw("active")).unwrap();
    assert_eq!(saved.id, 7);
    assert!(workflow::ScopedWorkflowIntentV6::capture(
        app.scoped_actions_v1.as_ref().unwrap().page.clone(),
        1,
        false
    )
    .is_err());
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_pure_back_from_preview_preserves_original_source_and_has_zero_io() {
    let (fix, _p, mut app, all) = prepared("active", vec![], false);
    let source = opened(&app).intent().source().clone();
    let before = (f::sql_state(&fix), f::git_state(&fix));
    app.form.as_mut().unwrap().dispatch(&FormAction::Preview);
    assert!(Arc::ptr_eq(opened(&app).intent().source(), &source));
    assert!(app.scoped_actions_work_v1.is_none());
    no_effects(&fix, &before, &all);
}

#[test]
fn scoped_actions_workflow_v6_late_workflow_preview_cannot_replace_new_genuine_run_form_or_token() {
    let mut steps = phase("active", 42);
    steps.extend(phase("active", 42));
    let (fix, p, mut app, all) = setup("active", steps);
    open(&mut app, &p, &fix, false);
    let work = w::request(&mut app, FormAction::Preview);
    let owner = ScopedActionsOwnerV1::new(work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared = super::super::execute(&owner, &p, &f::database(&fix), None).unwrap();
    let (other, op, mut newer, other_steps) = w::setup(w::read_phase(w::running(), 42));
    w::open(&mut newer, &op, &other);
    let newer_work = w::request(&mut newer, FormAction::Preview);
    let newer_owner = ScopedActionsOwnerV1::new(newer_work.clone()).unwrap();
    newer.scoped_actions_owner_v1 = Some(newer_owner);
    let before = (f::sql_state(&other), f::git_state(&other));
    let status = newer.status.clone();
    let source = newer.remote_scope_source.clone().unwrap();
    let hits = format!("{:?}", newer.hits);
    assert!(!super::super::publish(&mut newer, &owner, prepared));
    assert!(!super::super::fail(&mut newer, &owner));
    assert!(Arc::ptr_eq(
        newer.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert_eq!(newer.status, status);
    assert_eq!(format!("{:?}", newer.hits), hits);
    assert!(!newer_work.cancellation().is_cancelled());
    assert!(!work.cancellation().is_cancelled());
    assert!(matches!(
        newer.form.as_ref().unwrap().purpose,
        FormPurpose::ScopedActionsV3(_)
    ));
    no_effects(&other, &before, &other_steps);
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_late_run_preview_cannot_replace_new_genuine_workflow_form_or_token() {
    let mut run_steps = w::read_phase(w::running(), 42);
    run_steps.extend(w::read_phase(w::running(), 42));
    let (fix, p, mut old, all) = w::setup(run_steps);
    w::open(&mut old, &p, &fix);
    let old_work = w::request(&mut old, FormAction::Preview);
    let old_owner = ScopedActionsOwnerV1::new(old_work.clone()).unwrap();
    old.scoped_actions_owner_v1 = Some(old_owner.clone());
    let prepared = super::super::execute(&old_owner, &p, &f::database(&fix), None).unwrap();
    let (other, op, mut newer, other_steps) = setup("active", phase("active", 42));
    open(&mut newer, &op, &other, false);
    let work = w::request(&mut newer, FormAction::Preview);
    let owner = ScopedActionsOwnerV1::new(work.clone()).unwrap();
    newer.scoped_actions_owner_v1 = Some(owner);
    let status = newer.status.clone();
    let hits = format!("{:?}", newer.hits);
    let before = (f::sql_state(&other), f::git_state(&other));
    let source = newer.remote_scope_source.clone().unwrap();
    assert!(!super::super::publish(&mut newer, &old_owner, prepared));
    assert!(!super::super::fail(&mut newer, &old_owner));
    assert!(Arc::ptr_eq(
        newer.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert_eq!(newer.status, status);
    assert_eq!(format!("{:?}", newer.hits), hits);
    assert!(!work.cancellation().is_cancelled());
    assert!(!old_work.cancellation().is_cancelled());
    assert!(matches!(
        newer.form.as_ref().unwrap().purpose,
        FormPurpose::ScopedActionsWorkflowV6(_)
    ));
    no_effects(&other, &before, &other_steps);
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_legacy_none_exact_transport_has_no_include_new_core_or_sql() {
    let fixture = Fixture::new();
    let mut steps = f::principal(42);
    steps.push(workflow_get(raw("active")));
    steps.push(GhStep::read(
        &[
            "api",
            &format!("repos/{}/actions/workflows/7/disable", f::REPO),
            "--hostname",
            f::HOST,
            "--method",
            "PUT",
        ],
        "",
    ));
    steps.push(workflow_get(raw("disabled_manually")));
    fixture.scripted_gh(&steps);
    let provider = f::provider(&fixture);
    let identity = HostIdentity {
        host: f::HOST.into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "actual fixture".into(),
    };
    let model: Workflow = serde_json::from_value(raw("active")).unwrap();
    let updated = provider
        .set_workflow_enabled(
            &identity,
            f::REPO,
            &model,
            false,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(updated.state, "disabled_manually");
    w::exact(&fixture, &steps);
    assert!(
        !f::database(&fixture).exists(),
        "Legacy None gained no source/Core/SQLite path"
    );
}
#[test]
fn scoped_actions_workflow_v6_native_metadata_read_cap_is64k_and_never_widens_original_issuer() {
    let mut phase = phase("active", 42);
    let mut oversized = raw("active");
    oversized["name"] = json!("X".repeat(20 * 1024));
    phase[3] = workflow_get(oversized);
    phase.truncate(4);
    let (fix, provider, mut app, all) = setup("active", phase);
    let provider = provider
        .with_scoped_read_window(
            std::time::Instant::now() + std::time::Duration::from_secs(30),
            16 * 1024,
        )
        .unwrap();
    assert!(super::intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::DisableWorkflow)
    ));
    let work = crate::scoped_actions_native_v1::take_work(&mut app).unwrap();
    let owner = ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let before = (f::sql_state(&fix), f::git_state(&fix));
    assert!(super::super::execute(&owner, &provider, &f::database(&fix), None).is_err());
    assert!(app.form.is_none());
    no_effects(&fix, &before, &all);
}
