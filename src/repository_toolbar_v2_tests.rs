use crate::{
    app::{Action, App, View},
    local_commit_actor_origin_v1_test_support::Real,
    workspace::{RepositoryMode, RowAction, Workspace},
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, layout::Rect, Terminal};

#[test]
fn staging_repository_toolbar_v2_full_hunk_hit_and_key_mouse_canonical_effect_at80_120() {
    let real = Real::new("sha256");
    let workspace = Workspace::load(&real.root, RepositoryMode::Working, "HEAD", None)
        .unwrap()
        .unwrap();
    let selected = workspace
        .display_rows_v1()
        .iter()
        .position(|row| matches!(row.action, Some(RowAction::DiffLine(_))))
        .unwrap();
    let before = real.snapshot();
    real.clear_trace();
    for width in [80, 120] {
        let mut app = App {
            view: View::Repository,
            workspace: Some(workspace.clone()),
            selected,
            ..Default::default()
        };
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        terminal
            .draw(|frame| super::draw(frame, &mut app, Rect::new(1, 3, width - 2, 18)))
            .unwrap();
        let hit = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::RepositoryHunk)
            .unwrap()
            .clone();
        let painted = (hit.rect.x..hit.rect.right())
            .map(|x| terminal.backend().buffer()[(x, hit.rect.y)].symbol())
            .collect::<String>();
        assert_eq!(painted, "[h Hunk]");
        assert!(hit.rect.right() < width);
        let mut keyboard = App {
            view: View::Repository,
            workspace: Some(workspace.clone()),
            selected,
            ..Default::default()
        };
        keyboard.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('h'),
            KeyModifiers::NONE,
        )));
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: hit.rect.x,
            row: hit.rect.y,
            modifiers: KeyModifiers::NONE,
        }));
        let mouse_selection = app
            .repository_selection
            .as_ref()
            .expect("actual mouse hunk selection");
        let key_selection = keyboard
            .repository_selection
            .as_ref()
            .expect("actual keyboard hunk selection");
        assert_eq!(
            (&mouse_selection.start, &mouse_selection.end),
            (&key_selection.start, &key_selection.end)
        );
        assert_eq!(app.status, keyboard.status);
    }
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}
