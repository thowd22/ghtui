//! Application-level capture and prepared-diagnostic contracts. Real terminal
//! bytes, subprocess traces and Git/SQL effects are separately covered by PTYs.
use crate::{
    app::{Action, App, Overlay, View},
    capability_inspector::InspectorAction,
    github_workspace::{Freshness, GithubWorkspace},
    providers::gh::HostIdentity,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, Terminal};

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn click(x: u16, y: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    })
}
fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
fn requests(app: &App) -> (bool, bool, bool, bool, bool, bool) {
    (
        app.refresh_requested,
        app.repository_load_requested,
        app.search_load_requested,
        app.actions_load_requested,
        app.metrics_load_requested,
        app.quit,
    )
}

#[test]
fn gh_provider_inspector_app_palette_keyboard_and_mouse_choose_same_readonly_entry_and_reset() {
    for mouse in [false, true] {
        let mut app = App {
            palette_index: 99,
            view: View::Repository,
            context: "Preserved local context".into(),
            status: "GitHub CLI is missing".into(),
            ..App::default()
        };
        let before = requests(&app);
        app.dispatch(Action::Palette);
        assert_eq!(app.palette_index, 0);
        assert_eq!(
            app.command_palette.as_ref().unwrap().selected_index(),
            Some(0)
        );
        assert!(draw(&mut app).contains("Inspect capabilities"));
        if mouse {
            let hit = app
                .hits
                .iter()
                .find(|hit| {
                    hit.action
                        == Action::PaletteInput(crate::command_palette::PaletteAction::Activate(5))
                })
                .unwrap();
            app.handle_event(click(hit.rect.x, hit.rect.y));
        } else {
            for _ in 0..5 {
                app.handle_event(key(KeyCode::Down));
            }
            app.handle_event(key(KeyCode::Enter));
        }
        assert_eq!(app.overlay, Some(Overlay::Capabilities));
        assert_eq!(app.view, View::Repository);
        assert_eq!(app.context, "Preserved local context");
        assert_eq!(
            requests(&app),
            before,
            "Entry is inspection, not a worker/load/quit request"
        );
        assert!(app
            .capability_inspector
            .as_ref()
            .unwrap()
            .details()
            .concat()
            .contains("GitHub CLI is missing"));
        app.dispatch(Action::Inspector(InspectorAction::Close));
        assert!(app.capability_inspector.is_none());
        app.dispatch(Action::Palette);
        assert_eq!(app.palette_index, 0);
    }
}

#[test]
fn gh_provider_inspector_app_offline_source_diagnostic_coexists_with_last_numeric_identity() {
    let identity = HostIdentity {
        host: "enterprise.invalid".into(),
        login: "fixture".into(),
        account_id: Some(42),
        gh_version: "gh version 2.0.0".into(),
    };
    let mut github = GithubWorkspace::empty(identity, vec![], Default::default());
    github.freshness = Freshness::Offline {
        fetched_at: 123,
        error: "Unknown JSON field: host contract unavailable".into(),
    };
    let mut app = App {
        github: Some(github),
        status: "Unrelated focus state".into(),
        ..App::default()
    };
    app.dispatch(Action::Capabilities);
    let inspector = app.capability_inspector.as_ref().unwrap();
    let details = inspector.details().concat();
    assert!(details.contains("Offline/stale"));
    assert!(details.contains("fetched 1970-01-01 00:02:03 UTC"));
    assert!(details.contains("Unknown JSON field"));
    assert!(!details.contains("Unrelated focus state"));
    assert!(details.contains("not observed/granted"));
    assert!(draw(&mut app).contains("account #42"));
}

#[test]
fn gh_provider_inspector_app_capture_blocks_underlying_views_refresh_and_quit_but_accepts_filter_text(
) {
    let mut app = App::default();
    app.dispatch(Action::Capabilities);
    draw(&mut app);
    let before = requests(&app);
    app.handle_event(click(2, 1));
    app.handle_event(click(34, 39));
    assert_eq!(requests(&app), before);
    assert_eq!(app.view, View::Inbox);
    for c in ['4', 'r', 'q'] {
        app.handle_event(key(KeyCode::Char(c)));
    }
    assert_eq!(app.capability_inspector.as_ref().unwrap().query(), "4rq");
    assert_eq!(requests(&app), before);
    assert_eq!(app.overlay, Some(Overlay::Capabilities));
    app.handle_event(key(KeyCode::Esc));
    assert!(
        app.capability_inspector.is_none() && app.capability_hits.is_empty() && app.hits.is_empty()
    );
}

#[test]
fn gh_provider_inspector_app_resize_invalidates_mouse_geometry_before_next_draw() {
    let mut app = App::default();
    app.dispatch(Action::Capabilities);
    draw(&mut app);
    let query_hit = app
        .capability_hits
        .iter()
        .find(|hit| matches!(hit.action, InspectorAction::FilterClick(_)))
        .unwrap()
        .rect;
    app.handle_event(Event::Paste("pr-inline".into()));
    app.handle_event(Event::Resize(80, 24));
    assert!(app.hits.is_empty() && app.capability_hits.is_empty());
    app.handle_event(click(query_hit.x, query_hit.y));
    assert_eq!(
        app.capability_inspector.as_ref().unwrap().query(),
        "pr-inline"
    );
    assert_eq!(
        app.capability_inspector.as_ref().unwrap().selected_id(),
        Some("pr-inline")
    );
    assert_eq!(app.overlay, Some(Overlay::Capabilities));
    assert!(!app.refresh_requested && !app.quit);
}

#[test]
fn gh_provider_inspector_app_bounds_failure_drops_previous_snapshot_and_explains_without_requests()
{
    let mut app = App::default();
    app.dispatch(Action::Capabilities);
    assert!(app.capability_inspector.is_some());
    app.status = "x".repeat(8193);
    app.dispatch(Action::Capabilities);
    assert!(app.capability_inspector.is_none());
    assert_eq!(app.overlay, Some(Overlay::Notice));
    assert!(app.notice.contains("text budget exceeded"));
    assert_eq!(requests(&app), (false, false, false, false, false, false));
}
