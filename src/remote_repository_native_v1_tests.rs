//! Genuine original-source input contracts complement the real-binary journeys.
use super::*;
use crate::{remote_repository_v1_test_support as fx, test_support::Fixture};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, Terminal};

#[path = "remote_repository_native_v1_body_tests.rs"]
mod body_tests;

#[test]
fn scoped_pr_native_diff_local_saved_context_retires_when_actual_remote_browser_opens() {
    native_local_context_retirement(false);
}
#[test]
fn scoped_pr_native_diff_local_mutable_context_retires_when_actual_remote_browser_opens() {
    native_local_context_retirement(true);
}
fn native_local_context_retirement(mutable: bool) {
    use crate::{
        git::Comparison,
        original_diff_sides_v1_test_support as local,
        providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
        repository_native_diff_v1::NativeDiffStateV1,
        workspace::{RepositoryMode, Workspace},
    };
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let (mut app, steps) = loaded(&fixture, &objects);
    let remote_owner = app.remote_repository.as_ref().unwrap().owner.clone();
    let (real, a, b, _) = local::fixture("sha256");
    let workspace = if mutable {
        Workspace::load(&real.root, RepositoryMode::Working, "HEAD", None)
            .unwrap()
            .unwrap()
    } else {
        let mut workspace = local::workspace(&real, Comparison::BranchChanges { base: a, head: b });
        local::source(&workspace).install(&mut workspace).unwrap();
        workspace
    };
    app.view = View::Repository;
    if mutable {
        let mut state =
            NativeDiffStateV1::<crate::git::MutableDiffSourceV2>::open_mutable(&workspace, 118)
                .unwrap();
        let request = state.request().unwrap();
        let op =
            LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
        let prepared = request.prepare(&op).unwrap();
        op.seal().unwrap();
        assert!(state.accept(prepared));
        assert!(state.current_frame().is_some());
        app.native_mutable_diff_v2 = Some(state);
    } else {
        let mut state = NativeDiffStateV1::open(&workspace, 118).unwrap();
        let request = state.request().unwrap();
        let op =
            LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
        let prepared = request.prepare(&op).unwrap();
        op.seal().unwrap();
        assert!(state.accept(prepared));
        assert!(state.current_frame().is_some());
        app.native_diff_v1 = Some(state);
    }
    let origin = workspace.actor_origin.as_ref().unwrap().clone();
    app.workspace = Some(workspace);
    let before = real.snapshot();
    real.clear_trace();
    crate::repository_native_diff_native_v1::synchronize(&mut app);
    assert!(app.native_diff_v1.is_none());
    assert!(app.native_mutable_diff_v2.is_none());
    assert!(app.original_diff_cancel_v1);
    assert!(app.original_diff_work_v1.is_none());
    assert!(
        Arc::ptr_eq(
            &app.remote_repository.as_ref().unwrap().owner,
            &remote_owner
        ),
        "only local native state retires; genuine remote browser is untouched"
    );
    assert!(
        Arc::ptr_eq(
            app.workspace
                .as_ref()
                .unwrap()
                .actor_origin
                .as_ref()
                .unwrap(),
            &origin
        ),
        "retained actual local Workspace is unchanged"
    );
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}

fn loaded(fixture: &Fixture, objects: &fx::Objects) -> (App, Vec<crate::test_support::GhStep>) {
    let mut steps = fx::source_steps();
    steps.extend(fx::prefix());
    steps.extend(fx::default_payload(objects));
    steps.extend(fx::suffix(42));
    fixture.scripted_gh(&steps);
    let source = fx::source(fixture);
    let state = initial(
        source,
        0,
        &fx::provider(fixture),
        &fixture.root.join("cache.sqlite3"),
        &Default::default(),
    )
    .unwrap();
    let mut app = App::default();
    install_initial(&mut app, state);
    (app, steps)
}

fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    event(app, Event::Key(KeyEvent::new(code, modifiers)));
}

#[test]
fn remote_tree_v1_native_selected_full_commit_replacement_keeps_new_literal_and_clears_error() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let (mut app, steps) = loaded(&fixture, &objects);
    dispatch(&mut app, RemoteBrowseActionV1::CommitFocus);
    event(&mut app, Event::Paste("a".repeat(20)));
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app
        .remote_repository
        .as_ref()
        .unwrap()
        .input_error
        .is_some());
    assert!(take_request(&mut app).is_none());
    key(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    event(&mut app, Event::Paste(objects.commit.clone()));
    key(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    let replacement = String::from_utf8(fx::git(&fixture.repo(), &["rev-parse", "HEAD"]))
        .unwrap()
        .trim()
        .to_owned();
    assert_ne!(replacement, objects.commit);
    event(&mut app, Event::Paste(replacement.clone()));
    let state = app.remote_repository.as_ref().unwrap();
    let Some(RemoteDialogV1::Commit(editor)) = &state.dialog else {
        panic!("owned commit editor")
    };
    assert_eq!(editor.text, replacement);
    assert_eq!(editor.cursor, replacement.len());
    assert!(editor.selection().is_none());
    assert!(state.input_error.is_none());
    assert!(state.pending.is_none());
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.remote_repository.as_ref().unwrap().dialog.is_none());
    assert!(take_request(&mut app).is_none());
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}

#[test]
fn remote_tree_v1_native_pending_cancel_mouse_and_keyboard_retain_actual_cancellation_after_exit() {
    for mouse in [false, true] {
        let fixture = Fixture::new();
        let objects = fx::objects(&fixture);
        let (mut app, steps) = loaded(&fixture, &objects);
        dispatch(&mut app, RemoteBrowseActionV1::Branches);
        let work = take_request(&mut app).unwrap();
        assert!(work.owner.owns(&app));
        let mut terminal =
            Terminal::new(TestBackend::new(if mouse { 120 } else { 80 }, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let state = app.remote_repository.as_ref().unwrap();
        let rect = state
            .hits
            .iter()
            .find(|(_, action)| *action == RemoteBrowseActionV1::Back)
            .unwrap()
            .0;
        assert!(rect.width > 0);
        if mouse {
            event(
                &mut app,
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: rect.x,
                    row: rect.y,
                    modifiers: KeyModifiers::NONE,
                }),
            );
        } else {
            key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        }
        assert!(app.remote_repository.is_none());
        assert!(!work.owner.owns(&app));
        assert!(
            take_cancel(&mut app),
            "actual worker cancellation survives dropping the browser state"
        );
        assert!(!take_cancel(&mut app));
        assert!(take_request(&mut app).is_none());
        fx::exact(&fixture, &steps);
        fx::preserved(&fixture, &objects);
    }
}
