use super::*;
use crate::{
    app::{Action, App, Overlay, View},
    search::corpus::{LocalSource, RepositorySelection},
};
use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{backend::TestBackend, Terminal};

fn search() -> SearchWorkspace {
    SearchWorkspace::new(
        Scope::new("local", "workspace").unwrap(),
        vec![RepositorySelection {
            repository: "org/repo".into(),
            directory: "/tmp/explicit-repo".into(),
            source: LocalSource::Worktree,
            sets: vec![],
        }],
    )
}

#[test]
fn collaboration_index_native_source_requests_reject_zero_overflow_scope_and_caps() {
    let mut controls = SourceControls::default();
    controls.choose_kind(SourceKind::PullRequest, Some(&search()));
    assert_eq!(controls.request().unwrap().repository, "org/repo");
    for invalid in ["0", "-1", "18446744073709551616", "7\n8"] {
        controls.fields[1] = TextBuffer::new(invalid).unwrap();
        assert!(controls.request().is_err(), "{invalid}");
    }
    controls.fields[1] = TextBuffer::new("7").unwrap();
    for (field, invalid) in [
        (0, "org/repo/extra"),
        (2, "10000"),
        (3, "33554433"),
        (4, "101"),
    ] {
        let previous = controls.fields[field].clone();
        controls.fields[field] = TextBuffer::new(invalid).unwrap();
        assert!(controls.request().is_err());
        controls.fields[field] = previous;
    }
}

#[test]
fn collaboration_index_native_commit_opt_in_uses_local_revision_without_api_pages() {
    let mut controls = SourceControls::default();
    controls.choose_kind(SourceKind::Commits, Some(&search()));
    controls.fields[4] = TextBuffer::new("not-an-api-limit").unwrap();
    let request = controls.request().unwrap();
    assert_eq!(request.kind, SourceKind::Commits);
    assert_eq!(request.context, "HEAD");
    assert_eq!(request.max_pages, 1);
    controls.choose_kind(SourceKind::Issue, Some(&search()));
    assert_eq!(controls.fields[1].text, "1");
    assert!(controls.request().is_err());
}

#[test]
fn collaboration_index_native_sources_saved_scope_and_limits_do_not_broaden() {
    let cache = Cache::memory().unwrap();
    let mut selected = search();
    selected
        .metadata_sources
        .commits
        .push(commits::CommitSelection {
            repository: "org/repo".into(),
            directory: "/tmp/explicit-repo".into(),
            revision: "feature/saved".into(),
            max_commits: 9,
        });
    set_limits(&mut selected, 9, 4096);
    persist(&cache, &selected).unwrap();
    let mut restored = search();
    restore(&cache, &mut restored).unwrap();
    assert_eq!(restored.metadata_sources, selected.metadata_sources);
    assert_eq!(restored.commit_limits.max_total_bytes, 4096);
    let mut other = SearchWorkspace::new(Scope::github("github.com", Some(43)).unwrap(), vec![]);
    restore(&cache, &mut other).unwrap();
    assert!(other.metadata_sources.is_empty());
    cache.save_view(&selected.scope,SAVED_SOURCES,&serde_json::json!({"sources":selected.metadata_sources,"max_documents":0,"max_bytes":4096})).unwrap();
    let mut refused = search();
    assert!(restore(&cache, &mut refused).is_err());
    assert!(refused.metadata_sources.is_empty());
}

#[test]
fn collaboration_index_native_source_overlay_mouse_keyboard_hits_resize_and_no_clickthrough() {
    for (width, height) in [(80, 24), (120, 40)] {
        let mut app = App {
            view: View::Search,
            search: Some(search()),
            ..Default::default()
        };
        app.dispatch(Action::SearchSource(SourceAction::Open));
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        assert_eq!(app.overlay, Some(Overlay::SearchSources));
        assert!(app
            .hits
            .iter()
            .all(|hit| matches!(hit.action, Action::SearchSource(_))));
        for action in [
            SourceAction::Kind(SourceKind::PullRequest),
            SourceAction::Kind(SourceKind::Issue),
            SourceAction::Kind(SourceKind::Commits),
            SourceAction::Load,
            SourceAction::Remove,
            SourceAction::Close,
        ] {
            assert!(app
                .hits
                .iter()
                .any(|hit| hit.action == Action::SearchSource(action.clone())));
        }
        let hit = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::SearchSource(SourceAction::Kind(SourceKind::Commits)))
            .unwrap()
            .rect;
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: hit.x,
            row: hit.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        }));
        assert_eq!(app.source_controls.kind, SourceKind::Commits);
        app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('i'))));
        assert_eq!(app.source_controls.kind, SourceKind::Issue);
        app.loading[View::Search.index()] = true;
        app.dispatch(Action::SearchSource(SourceAction::Load));
        assert!(app.source_action.is_none());
        assert!(app.source_controls.message.contains("No duplicate"));
        app.dispatch(Action::SearchSource(SourceAction::Close));
        assert!(app.cancel_requested);
        assert!(app.overlay.is_none());
    }
}
