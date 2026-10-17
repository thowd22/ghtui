//! SOURCE ONLY / UNRUN. Actual sources/Forms/plans, no DTO capability.

fn target_and_debug_have_distinct_real_rows(width: u16, height: u16) {
    for command in [
        ActionsCommand::RerunAll,
        ActionsCommand::RerunFailed,
        ActionsCommand::RerunJob,
    ] {
        let (fixture, _provider, mut app, all) = r::setup(command, false, vec![]);
        let sql = f::sql_state(&fixture);
        let git = f::git_state(&fixture);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| crate::forms::draw(frame, &mut app, frame.area()))
            .unwrap();
        let rows: Vec<String> = terminal
            .backend()
            .buffer()
            .content
            .chunks(width as usize)
            .map(|cells| cells.iter().map(|c| c.symbol()).collect())
            .collect();
        let target = rows
            .iter()
            .position(|row| row.contains("Numeric repository"))
            .expect("Actual original numeric repository/run/attempt stays visible");
        let repository = rows
            .iter()
            .position(|row| row.contains("Current repository:"))
            .expect("Actual current name not overwritten by choices");
        let label = rows
            .iter()
            .position(|row| row.contains("Enable debug logging?"))
            .expect("Same generic actual choice label");
        let input = app.form.as_ref().unwrap().field_rects[0];
        assert_eq!(input.y as usize, label + 1);
        assert!(
            target > input.y as usize + 1 && repository > target,
            "Prepared target BELOW actual label/value/choices; no hidden overlap"
        );
        assert!(rows[label + 2].contains("[false]") && rows[label + 2].contains("[true]"));
        assert!(app
            .hits
            .iter()
            .any(|hit| hit.action == Action::Form(FormAction::Choice(0, 1))
                && hit.rect.y as usize == label + 2));
        assert!(app
            .hits
            .iter()
            .any(|hit| hit.action == Action::Form(FormAction::Field(0)) && hit.rect == input));
        assert!(
            !app.form.as_ref().unwrap().plan_ready(),
            "Intent layout never grants Confirm"
        );
        assert_eq!(f::sql_state(&fixture), sql);
        assert_eq!(f::git_state(&fixture), git);
        w::exact(&fixture, &all);
    }
}
#[test]
fn scoped_actions_rerun_v4_intent_debug_field_cannot_overwrite_real_target_at80columns() {
    target_and_debug_have_distinct_real_rows(80, 24);
}
#[test]
fn scoped_actions_rerun_v4_intent_debug_field_cannot_overwrite_real_target_at120columns() {
    target_and_debug_have_distinct_real_rows(120, 40);
}

fn newer_genuine_form_survives_old_rerun(command: ActionsCommand) {
    let (old_fixture, _p, mut old_app, old_steps) = r::setup(command, true, vec![]);
    let old_work = w::request(&mut old_app, FormAction::Submit);
    let drafts = crate::drafts::DraftWriter::new(f::database(&old_fixture));
    let old = ScopedActionsOwnerV1::new_publication_v3(
        old_work,
        &f::database(&old_fixture),
        &drafts.client(),
    )
    .unwrap();
    let (fixture, _provider, mut app, all) = r::setup(command, false, vec![]);
    let new_work = w::request(&mut app, FormAction::Preview);
    app.scoped_actions_work_v1 = Some(new_work.clone());
    app.scoped_actions_owner_v1 = Some(old.clone());
    let source = app.scoped_actions_v1.as_ref().unwrap().source.clone();
    let page = app.scoped_actions_v1.as_ref().unwrap().page.clone();
    let epoch = app.scoped_actions_v1.as_ref().unwrap().epoch;
    let status = app.status.clone();
    let hits = format!("{:?}", app.hits);
    let loading = app.loading;
    let sql = f::sql_state(&fixture);
    let old_sql = f::sql_state(&old_fixture);
    let git = f::git_state(&fixture);
    let old_git = f::git_state(&old_fixture);
    assert_eq!(applicable_cancel_v3(&app), None);
    super::super::resize(&mut app);
    app.dispatch(Action::Form(FormAction::Close));
    assert!(Arc::ptr_eq(
        app.scoped_actions_owner_v1.as_ref().unwrap(),
        &old
    ));
    assert!(Arc::ptr_eq(
        app.scoped_actions_work_v1.as_ref().unwrap(),
        &new_work
    ));
    assert!(!old.cancellation().is_cancelled());
    assert!(!new_work.cancellation().is_cancelled());
    assert!(!app.scoped_actions_cancel_v1);
    assert_eq!(app.status, status);
    assert_eq!(format!("{:?}", app.hits), hits);
    assert_eq!(app.loading, loading);
    let state = app.scoped_actions_v1.as_ref().unwrap();
    assert!(Arc::ptr_eq(&state.source, &source));
    assert!(Arc::ptr_eq(&state.page, &page));
    assert_eq!(state.epoch, epoch);
    assert!(app.form.as_ref().unwrap().submitting);
    assert!(new_work
        .publication_v3
        .as_ref()
        .unwrap()
        .request()
        .unwrap()
        .matches_form(app.form.as_ref().unwrap()));
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::sql_state(&old_fixture), old_sql);
    assert_eq!(f::git_state(&fixture), git);
    assert_eq!(f::git_state(&old_fixture), old_git);
    w::exact(&fixture, &all);
    w::exact(&old_fixture, &old_steps);
}
#[test]
fn scoped_actions_rerun_v4_all_old_confirm_cannot_resize_close_new_genuine_source_form() {
    newer_genuine_form_survives_old_rerun(ActionsCommand::RerunAll);
}
#[test]
fn scoped_actions_rerun_v4_failed_old_confirm_cannot_resize_close_new_genuine_source_form() {
    newer_genuine_form_survives_old_rerun(ActionsCommand::RerunFailed);
}
#[test]
fn scoped_actions_rerun_v4_job_old_confirm_cannot_resize_close_new_genuine_source_form() {
    newer_genuine_form_survives_old_rerun(ActionsCommand::RerunJob);
}

use super::*;
use crate::{
    forms::FormAction, scoped_actions_publication_test_support_v3 as w,
    scoped_actions_rerun_test_support_v4 as r, scoped_actions_test_support_v1 as f,
};
use serde_json::json;

fn descriptor(
    app: &App,
    provider: &crate::providers::gh::GithubProvider,
) -> crate::actions_workspace::ActionsPublicationDescriptor {
    let preview = r::preview(app);
    let intent = preview.opened().intent();
    preview
        .plan()
        .scoped_run_descriptor_v4(
            provider,
            intent.source(),
            preview.target(),
            intent.selected_job(),
            intent.action(preview.options()),
        )
        .unwrap()
}
fn unchanged(
    fixture: &crate::test_support::Fixture,
    sql: &[(String, String, String)],
    git: &(String, String, Vec<u8>, Vec<u8>, Vec<u8>),
    all: &[crate::test_support::GhStep],
) {
    assert_eq!(f::sql_state(fixture), sql);
    assert_eq!(&f::git_state(fixture), git);
    assert!(w::result_sql(fixture).is_empty());
    w::exact(fixture, all);
}
#[test]
fn scoped_actions_rerun_v4_all_private_plan_is_actual_original_run_with_false_debug() {
    let (fixture, provider, app, all) = r::setup(ActionsCommand::RerunAll, true, vec![]);
    let value = descriptor(&app, &provider);
    assert_eq!(
        value.intent.kind,
        crate::actions_write_journal::ActionsWriteKind::RerunAll
    );
    assert!(!value.intent.options.debug_logging);
    assert_eq!(value.intent.context.run_id, Some(123));
    assert_eq!(value.intent.context.attempt, Some(2));
    assert_eq!(value.intent.context.job_id, None);
    assert_eq!(
        value
            .intent
            .context
            .original_run_v3
            .as_ref()
            .unwrap()
            .run_node,
        "WR_123"
    );
    assert!(app.form.as_ref().unwrap().plan_ready());
    assert!(w::result_sql(&fixture).is_empty());
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_rerun_v4_failed_private_plan_is_not_all_or_cancel() {
    let (fixture, provider, app, all) = r::setup(ActionsCommand::RerunFailed, true, vec![]);
    let value = descriptor(&app, &provider);
    assert_eq!(
        value.intent.kind,
        crate::actions_write_journal::ActionsWriteKind::RerunFailed
    );
    assert_eq!(
        value.intent.context.head_sha.as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert!(!value.intent.options.debug_logging);
    assert!(w::result_sql(&fixture).is_empty());
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_rerun_v4_selected_job_comes_from_actual_member_not_check_url_number() {
    let (fixture, provider, app, all) = r::setup(ActionsCommand::RerunJob, true, vec![]);
    let preview = r::preview(&app);
    let intent = preview.opened().intent();
    assert_eq!(intent.selected_job().unwrap().id(), 501);
    assert!(Arc::ptr_eq(
        intent.page().run().unwrap(),
        intent.selected_job().unwrap().run()
    ));
    let value = descriptor(&app, &provider);
    assert_eq!(value.intent.context.job_id, Some(501));
    assert_ne!(value.intent.context.job_id, Some(600));
    assert!(w::result_sql(&fixture).is_empty());
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_rerun_v4_actual_choice_true_is_bound_to_new_exact_preview() {
    let (fixture, provider, mut app, all) = r::setup(
        ActionsCommand::RerunFailed,
        false,
        w::read_phase(f::run(), 42),
    );
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    r::choose_debug(&mut app, true);
    w::preview(&mut app, &provider, &fixture);
    assert!(descriptor(&app, &provider).intent.options.debug_logging);
    assert_eq!(app.form.as_ref().unwrap().values()["debug"], "true");
    unchanged(&fixture, &sql, &git, &all);
}
#[test]
fn scoped_actions_rerun_v4_actual_debug_edit_retires_old_preview_and_request() {
    let (fixture, _provider, mut app, all) = r::setup(ActionsCommand::RerunAll, true, vec![]);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    assert_eq!(
        app.form.as_ref().unwrap().focus,
        app.form.as_ref().unwrap().fields.len(),
        "Prepared Preview focuses a visible control, never its hidden debug field"
    );
    let old = r::preview(&app);
    let request = ui::capture(app.form.as_ref().unwrap(), ComposerPhaseV3::Confirm).unwrap();
    r::choose_debug(&mut app, true);
    assert!(!request.matches_form(app.form.as_ref().unwrap()));
    assert!(!app.form.as_ref().unwrap().plan_ready());
    assert!(old.available());
    app.form.as_mut().unwrap().dispatch(&FormAction::Submit);
    assert!(!capture_form_work(&mut app));
    assert!(app.scoped_actions_work_v1.is_none());
    unchanged(&fixture, &sql, &git, &all);
}
#[test]
fn scoped_actions_rerun_v4_debug_invalid_text_cannot_capture_or_invoke() {
    let (fixture, _provider, mut app, all) = r::setup(ActionsCommand::RerunAll, false, vec![]);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let form = app.form.as_mut().unwrap();
    form.fields[0].editor = crate::editor::TextBuffer::new("not-a-bool").unwrap();
    assert!(ui::capture(form, ComposerPhaseV3::Preview).is_err());
    assert!(app.scoped_actions_work_v1.is_none());
    unchanged(&fixture, &sql, &git, &all);
}
#[test]
fn scoped_actions_rerun_v4_foreign_choice_schema_or_extra_field_is_not_approval() {
    let (fixture, _provider, mut app, all) = r::setup(ActionsCommand::RerunFailed, false, vec![]);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let form = app.form.as_mut().unwrap();
    form.fields[0].choices.push("perhaps".into());
    assert!(ui::capture(form, ComposerPhaseV3::Preview).is_err());
    unchanged(&fixture, &sql, &git, &all);
}
#[test]
fn scoped_actions_rerun_v4_no_selected_member_does_not_create_job_intent_or_api_work() {
    let (fixture, _provider, mut app, all) = r::base(vec![]);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let journal_tables = || {
        rusqlite::Connection::open_with_flags(
            f::database(&fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap()
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='actions_write_attempts'",
            [],
            |row| row.get::<_, u64>(0),
        )
        .unwrap()
    };
    assert_eq!(journal_tables(), 0, "No Composer/journal has been opened");
    assert!(ScopedActionsIntentV3::capture_command_v4(
        app.scoped_actions_v1.as_ref().unwrap().page.clone(),
        1,
        ActionsCommand::RerunJob
    )
    .is_err());
    assert!(intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::RerunJob)
    ));
    assert!(app.scoped_actions_work_v1.is_none());
    assert!(app.form.is_none());
    assert_eq!(journal_tables(), 0, "Refusal must not initialize a journal");
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_rerun_v4_public_plan_command_cannot_relabel_private_rerun_or_job() {
    for command in [
        ActionsCommand::RerunAll,
        ActionsCommand::RerunFailed,
        ActionsCommand::RerunJob,
    ] {
        let (fixture, provider, app, all) = r::setup(command, true, vec![]);
        let preview = r::preview(&app);
        let intent = preview.opened().intent();
        let mut plan = preview.plan().as_ref().clone();
        plan.command = ActionsCommand::Cancel;
        assert!(plan
            .scoped_run_descriptor_v4(
                &provider,
                intent.source(),
                preview.target(),
                intent.selected_job(),
                intent.action(preview.options())
            )
            .is_err());
        assert!(w::result_sql(&fixture).is_empty());
        w::exact(&fixture, &all);
    }
}
#[test]
fn scoped_actions_rerun_v4_equal_provider_strings_never_adopt_another_actual_runner() {
    let (fixture, provider, app, all) = r::setup(ActionsCommand::RerunAll, true, vec![]);
    let preview = r::preview(&app);
    let intent = preview.opened().intent();
    let other = f::provider(&fixture);
    assert_eq!(other.host(), provider.host());
    assert!(preview
        .plan()
        .scoped_run_descriptor_v4(
            &other,
            intent.source(),
            preview.target(),
            None,
            intent.action(preview.options())
        )
        .is_err());
    assert!(w::result_sql(&fixture).is_empty());
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_rerun_v4_tiny_focus_or_mouse_hit_cannot_confirm_any_rerun_variant() {
    for command in [
        ActionsCommand::RerunAll,
        ActionsCommand::RerunFailed,
        ActionsCommand::RerunJob,
    ] {
        let (fixture, _provider, mut app, all) = r::setup(command, true, vec![]);
        let sql = f::sql_state(&fixture);
        let git = f::git_state(&fixture);
        let screen = w::paint(&mut app, 40, 10);
        assert!(screen.contains("resize"));
        assert!(!app
            .hits
            .iter()
            .any(|hit| hit.action == Action::Form(FormAction::Submit)));
        app.form.as_mut().unwrap().dispatch(&FormAction::Submit);
        assert!(!capture_form_work(&mut app));
        unchanged(&fixture, &sql, &git, &all);
    }
}
#[test]
fn scoped_actions_rerun_v4_saved_debug_text_is_descriptive_and_never_restores_approval() {
    let (fixture, _provider, mut app, all) = r::setup(ActionsCommand::RerunAll, false, vec![]);
    r::choose_debug(&mut app, true);
    let encoded = app.form.as_ref().unwrap().draft();
    let raw: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(raw["values"]["debug"], json!("true"));
    assert!(raw["actions_publication"].is_null());
    assert!(app.form.as_mut().unwrap().restore_draft(&encoded).is_err());
    assert!(!app.form.as_ref().unwrap().plan_ready());
    assert!(w::result_sql(&fixture).is_empty());
    w::exact(&fixture, &all);
}
