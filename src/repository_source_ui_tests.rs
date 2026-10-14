use super::*;
use crate::{
    providers::runner::CancellationToken,
    repository_source_identity::PreparedRepositorySourceIdentity,
    workspace::{RepositoryMode, Workspace},
};
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, Terminal};
use std::{
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn fixture() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--initial-branch=main"]);
    git(dir.path(), &["config", "user.name", "Source Author"]);
    git(
        dir.path(),
        &["config", "user.email", "source@example.invalid"],
    );
    fs::write(dir.path().join("file.txt"), "saved\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-m", "saved source"]);
    fs::write(dir.path().join("file.txt"), "dirty\n").unwrap();
    let workspace = Workspace::load(dir.path(), RepositoryMode::Graph, "HEAD", None)
        .unwrap()
        .unwrap();
    (dir, workspace)
}
fn ready(workspace: Workspace) -> App {
    let source = Arc::new(PreparedRepositorySourceIdentity::prepare(&workspace).unwrap());
    App {
        view: View::Repository,
        repository_source_identity: Some(source),
        workspace: Some(workspace),
        repository_content_focused: true,
        ..Default::default()
    }
}
fn draw(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    terminal.backend().buffer().clone()
}
fn key(app: &mut App, code: KeyCode) {
    app.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}
fn click(app: &mut App, rect: Rect) {
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }));
}

#[test]
fn repository_source_identity_ui_key_mouse_exact_owner_capture_and_close_preserve_git() {
    let (dir, workspace) = fixture();
    let index = fs::read(dir.path().join(".git/index")).unwrap();
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    for mouse in [false, true] {
        let mut app = ready(workspace.clone());
        app.focus = 4;
        draw(&mut app, 80, 24);
        let expected_focus = if mouse {
            app.hits
                .iter()
                .position(|hit| hit.action == Action::RepositorySource)
                .unwrap()
        } else {
            4
        };
        if mouse {
            let rect = app
                .hits
                .iter()
                .find(|hit| hit.action == Action::RepositorySource)
                .unwrap()
                .rect;
            click(&mut app, rect);
        } else {
            key(&mut app, KeyCode::Char('I'));
        }
        assert_eq!(app.overlay, Some(Overlay::RepositorySource));
        assert!(Arc::ptr_eq(
            app.repository_source_overlay.as_ref().unwrap(),
            app.repository_source_identity.as_ref().unwrap()
        ));
        let buffer = draw(&mut app, 80, 24);
        let text = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(
            text.contains("Repository source identity • read-only")
                && !text.contains("Action unavailable")
        );
        key(&mut app, KeyCode::Char('3'));
        assert_eq!(app.view, View::Repository);
        assert!(!app.search_load_requested);
        if mouse {
            let rect = app
                .hits
                .iter()
                .find(|hit| hit.action == Action::Close)
                .unwrap()
                .rect;
            click(&mut app, rect);
        } else {
            key(&mut app, KeyCode::Esc);
        }
        assert!(app.overlay.is_none() && app.repository_source_overlay.is_none());
        assert_eq!(app.focus, expected_focus);
        assert!(app.repository_content_focused);
        assert!(
            !app.refresh_requested
                && !app.repository_load_requested
                && app.repository_action.is_none()
                && app.form.is_none()
        );
    }
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(dir.path().join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(dir.path().join("file.txt")).unwrap(), b"dirty\n");
}

#[test]
fn repository_source_identity_ui_paging_wheel_hover_focus_resize_and_missing_source_are_explicit() {
    let (_dir, workspace) = fixture();
    for (width, height) in [(120, 40), (80, 24), (40, 12)] {
        let mut app = ready(workspace.clone());
        app.dispatch(Action::RepositorySource);
        let buffer = draw(&mut app, width, height);
        assert_eq!(app.hits.len(), 3);
        assert!(app.hits.iter().all(|hit| !hit.rect.is_empty()
            && hit.rect.right() <= width
            && hit.rect.bottom() <= height));
        for (index, hit) in app.hits.iter().enumerate() {
            assert_eq!(
                buffer
                    .cell((hit.rect.x, hit.rect.y))
                    .unwrap()
                    .modifier
                    .contains(Modifier::UNDERLINED),
                index == 0
            );
        }
        let before = app.repository_source_scroll;
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(app.repository_source_scroll, before);
        let body = app.repository_source_area;
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: body.x,
            row: body.y,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(app.repository_source_scroll, before + 3);
        key(&mut app, KeyCode::Home);
        key(&mut app, KeyCode::PageDown);
        assert_eq!(app.repository_source_scroll, app.repository_source_rows);
        draw(&mut app, width, height);
        key(&mut app, KeyCode::Tab);
        key(&mut app, KeyCode::Tab);
        key(&mut app, KeyCode::Enter);
        assert!(app.overlay.is_none());
        app.dispatch(Action::RepositorySource);
        draw(&mut app, width, height);
        let close = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::Close)
            .unwrap()
            .rect;
        app.handle_event(Event::Resize(80, 24));
        assert!(app.hits.is_empty());
        click(&mut app, close);
        assert_eq!(app.overlay, Some(Overlay::RepositorySource));
    }
    let mut missing = App {
        view: View::Repository,
        ..Default::default()
    };
    key(&mut missing, KeyCode::Char('I'));
    assert_eq!(missing.overlay, Some(Overlay::Notice));
    assert!(missing.notice.contains("unavailable"));
}

#[test]
fn repository_source_identity_ui_replaced_equal_text_owner_cannot_retarget_open_details() {
    let (_dir, workspace) = fixture();
    let mut app = ready(workspace.clone());
    app.dispatch(Action::RepositorySource);
    draw(&mut app, 80, 24);
    let old = app.repository_source_overlay.as_ref().unwrap().clone();
    app.repository_source_identity = Some(Arc::new(
        PreparedRepositorySourceIdentity::prepare(&workspace).unwrap(),
    ));
    assert_eq!(
        old.detail_text(),
        app.repository_source_identity
            .as_ref()
            .unwrap()
            .detail_text()
    );
    key(&mut app, KeyCode::PageDown);
    assert!(
        app.repository_source_overlay.is_none() && app.overlay.is_none() && app.hits.is_empty()
    );
}

#[test]
fn repository_source_identity_runtime_prepares_source_in_worker_and_failed_prepare_drops_old_owner()
{
    let (_dir, workspace) = fixture();
    let mut app = ready(workspace.clone());
    app.dispatch(Action::RepositorySource);
    let mut background = crate::runtime::Background::new();
    let mut bad = workspace;
    bad.revision = "oversized".repeat(600);
    assert!(background.submit(
        &mut app,
        View::Repository,
        move |_cancel: CancellationToken| Ok(crate::runtime::PreparedView {
            workspace: Some(bad),
            status: "prepared workspace with unavailable source".into(),
            ..Default::default()
        })
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.loading[View::Repository.index()] {
        background.poll(&mut app);
        assert!(
            Instant::now() < deadline,
            "actual bounded worker completion"
        );
        std::thread::yield_now();
    }
    assert!(app.workspace.is_some());
    assert!(app.repository_source_identity.is_none() && app.repository_source_overlay.is_none());
    assert!(app.overlay.is_none());
    assert_eq!(app.status, "prepared workspace with unavailable source");
}
