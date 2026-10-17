//! Test-only actual closed Catalogue/Run/member and shared Form producers.
use crate::{
    actions_workspace::{ActionsAction, ActionsCommand},
    app::{App, Overlay},
    forms::{FormAction, FormPurpose},
    providers::gh::*,
    scoped_actions_native_v1::{self as native, publication_v3 as publication},
    scoped_actions_publication_test_support_v3 as w, scoped_actions_test_support_v1 as f,
    scoped_actions_v1::ScopedActionsRequestKindV1 as K,
    test_support::{Fixture, GhStep},
};
use std::sync::Arc;
pub(crate) fn base(tail: Vec<GhStep>) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut steps = f::operation(K::Attempt, 42);
    steps.extend(tail);
    let (fixture, provider, mut app, all) = f::setup(steps);
    let value = f::read(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 2,
        },
    )
    .unwrap();
    f::install(&mut app, value);
    (fixture, provider, app, all)
}
pub(crate) fn open(
    app: &mut App,
    provider: &GithubProvider,
    fixture: &Fixture,
    command: ActionsCommand,
) {
    assert!(native::intercept(app, &ActionsAction::Compose(command)));
    let work = native::take_work(app).expect("Actual closed rerun work");
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let value = publication::execute(&owner, provider, &f::database(fixture), None).unwrap();
    assert!(publication::publish(app, &owner, value));
    app.scoped_actions_owner_v1 = None;
    w::paint(app, 120, 40);
    assert_eq!(app.overlay, Some(Overlay::Form));
}
pub(crate) fn setup(
    command: ActionsCommand,
    preview: bool,
    tail: Vec<GhStep>,
) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut steps = w::read_phase(f::run(), 42);
    if preview {
        steps.extend(w::read_phase(f::run(), 42));
    }
    steps.extend(tail);
    let (fixture, provider, mut app, all) = base(steps);
    if command == ActionsCommand::RerunJob {
        // Ordinary actual selected-job view preparation retains the genuine
        // old member Arc, is worker-prepared and adds ZERO gh/SQL.
        let value = f::read(&mut app, &provider, &fixture, ActionsAction::OpenJob(501)).unwrap();
        f::install(&mut app, value);
    }
    open(&mut app, &provider, &fixture, command);
    if preview {
        w::preview(&mut app, &provider, &fixture);
    }
    (fixture, provider, app, all)
}
pub(crate) fn preview(
    app: &App,
) -> Arc<crate::scoped_actions_publication_v3::ScopedActionsPreviewV3> {
    let FormPurpose::ScopedActionsV3(state) = &app.form.as_ref().unwrap().purpose else {
        panic!("Actual original rerun Form")
    };
    state.prepared_preview().unwrap().clone()
}
pub(crate) fn choose_debug(app: &mut App, value: bool) {
    app.form
        .as_mut()
        .unwrap()
        .dispatch(&FormAction::Choice(0, usize::from(value)));
    w::paint(app, 120, 40);
}
