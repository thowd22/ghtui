//! Genuine original producer and exact offline Cancel protocol. UNRUN source.
use crate::{
    actions_workspace::{ActionsAction as A, ActionsCommand},
    app::{App, View},
    providers::gh::*,
    scoped_actions_native_v1::{self as native, publication_v3 as publication},
    scoped_actions_test_support_v1 as f,
    scoped_actions_v1::ScopedActionsRequestKindV1 as K,
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};
use std::{fs, sync::Arc};
pub(crate) fn running() -> Value {
    let mut value = f::run();
    value["status"] = json!("in_progress");
    value["conclusion"] = Value::Null;
    value
}
pub(crate) fn cancelled() -> Value {
    let mut value = running();
    value["status"] = json!("completed");
    value["conclusion"] = json!("cancelled");
    value
}
pub(crate) fn read_phase(value: Value, uid: u64) -> Vec<GhStep> {
    let mut steps = f::principal(42);
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    steps.extend((0..2).map(|_| {
        f::get(
            &format!("repos/{}/actions/runs/123", f::REPO),
            value.clone(),
        )
    }));
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    steps.extend(f::principal(uid));
    assert_eq!(steps.len(), 8);
    steps
}
pub(crate) fn post(fails: bool) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/{}/actions/runs/123/cancel", f::REPO),
            "--hostname",
            f::HOST,
            "--method",
            "POST",
        ],
        "{}",
    );
    if fails {
        step.status = 1;
        step.stderr = b"PRIVATE_POST_FAILURE_NOT_FOR_TTY".to_vec();
    }
    step
}
pub(crate) fn confirm_phase(receipt: Value, post_failure: bool, uid: u64) -> Vec<GhStep> {
    let mut steps = f::principal(42);
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    steps.push(f::get(
        &format!("repos/{}/actions/runs/123", f::REPO),
        running(),
    ));
    steps.extend(f::principal(42));
    steps.push(f::get(
        &format!("repos/{}/actions/runs/123", f::REPO),
        running(),
    ));
    steps.push(f::get(
        &format!("repos/{}/actions/runs/123", f::REPO),
        running(),
    ));
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    steps.extend(f::principal(42));
    steps.push(post(post_failure));
    if !post_failure {
        steps.push(f::get(
            &format!("repos/{}/actions/runs/123", f::REPO),
            receipt,
        ));
        steps.push(f::get("user", json!({"login":"octocat","id":uid})));
    }
    assert_eq!(steps.len(), if post_failure { 12 } else { 14 });
    steps
}
pub(crate) fn setup(tail: Vec<GhStep>) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut operation = f::operation(K::Attempt, 42);
    for step in &mut operation {
        if step.args.get(1).is_some_and(|value| {
            value.ends_with("/actions/runs/123") || value.ends_with("/actions/runs/123/attempts/2")
        }) {
            step.stdout = running().to_string().into_bytes();
        }
    }
    operation.extend(tail);
    let (fixture, provider, mut app, all) = f::setup(operation);
    let prepared = f::read(
        &mut app,
        &provider,
        &fixture,
        A::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 2,
        },
    )
    .unwrap();
    f::install(&mut app, prepared);
    assert_eq!(
        app.actions
            .as_ref()
            .unwrap()
            .run
            .as_ref()
            .unwrap()
            .attempt
            .run
            .status,
        "in_progress"
    );
    (fixture, provider, app, all)
}
pub(crate) fn paint(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            crate::forms::draw(frame, app, area);
        })
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
pub(crate) fn open_work(app: &mut App) -> Arc<crate::scoped_actions_v1::ScopedActionsWorkV1> {
    assert!(native::intercept(app, &A::Compose(ActionsCommand::Cancel)));
    native::take_work(app).expect("actual original Cancel work")
}
pub(crate) fn open(app: &mut App, provider: &GithubProvider, fixture: &Fixture) {
    let work = open_work(app);
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared = publication::execute(&owner, provider, &f::database(fixture), None).unwrap();
    assert!(publication::publish(app, &owner, prepared));
    app.scoped_actions_owner_v1 = None;
    paint(app, 120, 40);
}
pub(crate) fn request(
    app: &mut App,
    action: crate::forms::FormAction,
) -> Arc<crate::scoped_actions_v1::ScopedActionsWorkV1> {
    app.form.as_mut().unwrap().dispatch(&action);
    assert!(publication::capture_form_work(app));
    native::take_work(app).expect("actual exact Form original Work")
}
pub(crate) fn preview(app: &mut App, provider: &GithubProvider, fixture: &Fixture) {
    let work = request(app, crate::forms::FormAction::Preview);
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let value = publication::execute(&owner, provider, &f::database(fixture), None).unwrap();
    assert!(publication::publish(app, &owner, value));
    app.scoped_actions_owner_v1 = None;
    paint(app, 120, 40);
    assert!(app.form.as_ref().unwrap().plan_ready());
    assert_eq!(app.view, View::Actions);
}
pub(crate) fn exact(fixture: &Fixture, steps: &[GhStep]) {
    let rows = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        rows.len(),
        steps.len(),
        "Exact actual child count; no fallback or retry"
    );
    for (actual, expected) in rows.iter().zip(steps) {
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["stdin_hex"], "");
    }
}
pub(crate) fn result_sql(fixture: &Fixture) -> Vec<(String, bool, Option<String>)> {
    let connection = rusqlite::Connection::open_with_flags(
        f::database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut query = connection
        .prepare(
            "SELECT state,claimed,receipt_json FROM actions_write_attempts ORDER BY attempt_id",
        )
        .unwrap();
    let rows = query
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap();
    rows.map(Result::unwrap).collect()
}
