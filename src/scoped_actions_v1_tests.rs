use super::*;
use crate::{scoped_actions_native_v1 as native, scoped_actions_test_support_v1 as f};
use serde_json::json;
fn open() -> ActionsAction {
    ActionsAction::OpenRun {
        repository: f::REPO.into(),
        id: 123,
        attempt: 2,
    }
}
#[test]
fn scoped_actions_source_initial_genuine_seed_keeps_exact_scope_and_numeric_repository() {
    let (fixture, _, app, all) = f::setup(vec![]);
    let source = app.scoped_actions_v1.as_ref().expect("actual native state");
    assert!(app.remote_scope_source.as_ref().is_some());
    assert_eq!(app.actions.as_ref().unwrap().identity.account_id, Some(42));
    assert_eq!(app.actions.as_ref().unwrap().runs[0].run.id, 123);
    assert_eq!(all.len(), 14);
    let _ = source;
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_source_unobserved_requested_name_or_id_does_not_create_work() {
    let (fixture, _, mut app, all) = f::setup(vec![]);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    assert!(native::intercept(
        &mut app,
        &ActionsAction::OpenRun {
            repository: "other/name".into(),
            id: 123,
            attempt: 2
        }
    ));
    assert!(app.scoped_actions_work_v1.is_none());
    assert!(app.form.is_none());
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_source_queued_deadline_expires_without_any_child() {
    let (fixture, p, mut app, all) = f::setup(vec![]);
    assert!(native::intercept(&mut app, &open()));
    let mut work = native::take_work(&mut app).unwrap();
    Arc::get_mut(&mut work).unwrap().deadline = Instant::now() - Duration::from_millis(1);
    assert!(execute(work, &p, &f::database(&fixture)).is_err());
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_source_original_token_cancel_prevents_child_and_preserves_git_sql() {
    let (fixture, p, mut app, all) = f::setup(vec![]);
    let git = f::git_state(&fixture);
    let sql = f::sql_state(&fixture);
    assert!(native::intercept(&mut app, &open()));
    let work = native::take_work(&mut app).unwrap();
    work.cancellation().cancel();
    assert!(execute(work, &p, &f::database(&fixture)).is_err());
    assert_eq!(f::git_state(&fixture), git);
    assert_eq!(f::sql_state(&fixture), sql);
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_source_repository_numeric_replacement_refuses_before_run_body() {
    let mut op = f::operation(ScopedActionsRequestKindV1::Attempt, 42);
    let mut row = f::repo();
    row["id"] = json!(99999);
    op[2].stdout = row.to_string().into_bytes();
    op.truncate(3);
    let (fixture, p, mut app, all) = f::setup(op);
    assert!(f::read(&mut app, &p, &fixture, open()).is_err());
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_source_original_sql_race_stops_before_final_principal_and_keeps_new_bytes() {
    let op = f::operation(ScopedActionsRequestKindV1::Attempt, 42);
    let tail = op[..6].iter().chain(op[7..8].iter()).cloned().collect();
    let (fixture, p, mut app, all) = f::setup(tail);
    assert!(native::intercept(&mut app, &open()));
    let work = native::take_work(&mut app).unwrap();
    Cache::open(&f::database(&fixture))
        .unwrap()
        .save_view(
            work.source.original.scope(),
            work.source.original.saved_view_key(),
            &json!({"ORIGINAL_SQL_RACE_CANARY":"must not overwrite"}),
        )
        .unwrap();
    let before = f::sql_state(&fixture);
    assert!(execute(work, &p, &f::database(&fixture)).is_err());
    assert_eq!(f::sql_state(&fixture), before);
    let saved = Cache::open(&f::database(&fixture))
        .unwrap()
        .saved_view(
            app.remote_scope_source.as_ref().unwrap().scope(),
            app.remote_scope_source.as_ref().unwrap().saved_view_key(),
        )
        .unwrap();
    assert_eq!(
        saved,
        Some(json!({"ORIGINAL_SQL_RACE_CANARY":"must not overwrite"}))
    );
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_source_retained_capacity_not_only_length_is_charged() {
    let (fixture, p, mut app, all) =
        f::setup(f::operation(ScopedActionsRequestKindV1::Attempt, 42));
    let mut value = f::read(&mut app, &p, &fixture, open()).unwrap();
    value.workspace.rows[0]
        .label
        .reserve_exact(PREPARED_LIMIT_V1);
    assert!(charge(&value.workspace, &value.page).is_err());
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_source_final_identity_replacement_charges_independent_actual_capacities() {
    let (fixture, p, mut app, all) =
        f::setup(f::operation(ScopedActionsRequestKindV1::Attempt, 42));
    let mut value = f::read(&mut app, &p, &fixture, open()).unwrap();
    let observed_identity = value.workspace.identity.clone();
    value.workspace.identity.host.reserve_exact(900);
    Arc::get_mut(&mut value.page)
        .unwrap()
        .identity
        .login
        .reserve_exact(1500);
    assert_ne!(
        identity_capacity(&value.workspace.identity),
        identity_capacity(&value.page.identity)
    );
    value.retained_bytes = charge(&value.workspace, &value.page).unwrap();
    replace_final_identity(
        &mut value.workspace,
        &mut value.page,
        &mut value.retained_bytes,
        &observed_identity,
    )
    .unwrap();
    assert_eq!(
        value.retained_bytes,
        charge(&value.workspace, &value.page).unwrap()
    );
    assert_eq!(value.workspace.identity.account_id, Some(42));
    assert_eq!(value.page.identity.login, observed_identity.login);
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_source_pure_prepared_jobs_uses_original_model_with_zero_children() {
    let (fixture, p, mut app, all) = f::opened(vec![]);
    let git = f::git_state(&fixture);
    let sql = f::sql_state(&fixture);
    let value = f::read(&mut app, &p, &fixture, ActionsAction::OpenJob(501)).unwrap();
    assert_eq!(value.workspace.view, ActionsView::Jobs);
    assert_eq!(
        value.workspace.run.as_ref().unwrap().selected_job,
        Some(501)
    );
    assert!(value
        .workspace
        .rows
        .iter()
        .any(|row| row.label.contains("Actual failed step")));
    assert_eq!(
        value.workspace.rows[value.workspace.selected].action,
        Some(ActionsAction::OpenJob(501))
    );
    assert_eq!(value.workspace.top, value.workspace.selected);
    assert_eq!(f::git_state(&fixture), git);
    assert_eq!(f::sql_state(&fixture), sql);
    f::exact(&fixture, &all);
}
