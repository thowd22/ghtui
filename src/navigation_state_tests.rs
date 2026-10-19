use super::*;
use crate::{git::RawPath, navigation_trail::NavigationTrail};
use std::{fs, path::Path, process::Command};

fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
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
fn fixture() -> (tempfile::TempDir, String, String) {
    let directory = tempfile::tempdir().unwrap();
    git(directory.path(), &["init", "--initial-branch=main"]);
    git(
        directory.path(),
        &["config", "user.name", "Navigation Author"],
    );
    git(
        directory.path(),
        &["config", "user.email", "nav@example.invalid"],
    );
    fs::write(directory.path().join("日本語.txt"), "original saved α\n").unwrap();
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-m", "Original ancestry"]);
    let first = git(directory.path(), &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    fs::write(directory.path().join("second.txt"), "later saved β\n").unwrap();
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-m", "Later ancestry"]);
    let second = git(directory.path(), &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    fs::write(directory.path().join("日本語.txt"), "current dirty 🦀\n").unwrap();
    fs::write(directory.path().join("scratch.txt"), "untracked\n").unwrap();
    (directory, first, second)
}
fn workspace(directory: &Path, mode: RepositoryMode, revision: &str) -> Workspace {
    Workspace::load(directory, mode, revision, None)
        .unwrap()
        .unwrap()
}
fn app(workspace: Workspace) -> App {
    let mut app = App {
        view: View::Repository,
        repository_mode: workspace.mode,
        workspace: Some(workspace),
        repository_content_focused: true,
        ..App::default()
    };
    app.observe_navigation();
    app
}
fn state(directory: &Path) -> (String, String, Vec<u8>, Vec<u8>, Vec<u8>) {
    (
        git(directory, &["rev-parse", "HEAD"]),
        git(directory, &["status", "--porcelain=v2", "-z"]),
        fs::read(directory.join(".git/index")).unwrap(),
        fs::read(directory.join("日本語.txt")).unwrap(),
        fs::read(directory.join("scratch.txt")).unwrap(),
    )
}

#[test]
fn navigation_native_back_restores_original_commit_by_key_after_row_reordering() {
    let (directory, first, second) = fixture();
    let before = state(directory.path());
    let mut app = app(workspace(directory.path(), RepositoryMode::Graph, &first));
    let original = app
        .workspace
        .as_ref()
        .unwrap()
        .display_rows_v1()
        .iter()
        .position(|row| row.action == Some(RowAction::Commit(first.clone())))
        .unwrap();
    app.repository_list.selected = original;
    app.observe_navigation();
    app.workspace = Some(workspace(directory.path(), RepositoryMode::Graph, &second));
    app.repository_list.selected = 0;
    app.observe_navigation();
    app.request_navigation(NavigationAction::Back);
    let restore = app.navigation_restore.take().unwrap();
    app.navigation_restore_inflight = true;
    let mut observed = read_repository(&restore, Default::default()).unwrap();
    // Display row indices are not identities. A newly prepared order must be
    // looked up from its actual canonical action before restoring selection.
    let fresh_display = observed.graph_display_v1.as_ref().unwrap();
    observed.graph_display_v1 = Some(fresh_display.with_prepared_heading_for_test().unwrap());
    assert!(validate_repository_completion(
        &app,
        &restore,
        Some(&observed)
    ));
    app.workspace = Some(observed);
    app.finish_navigation(restore);
    assert_eq!(
        app.workspace.as_ref().unwrap().display_rows_v1()[app.repository_list.selected].action,
        Some(RowAction::Commit(first))
    );
    assert_ne!(app.repository_list.selected, original);
    assert!(app.repository_content_focused);
    assert_eq!(app.navigation_trail.history_len(), 0);
    assert_eq!(state(directory.path()), before);
}

#[test]
fn navigation_native_working_breadcrumb_retains_later_basis_not_selected_old_graph() {
    let (directory, first, second) = fixture();
    let before = state(directory.path());
    let mut app = app(workspace(directory.path(), RepositoryMode::Working, "HEAD"));
    app.workspace = Some(workspace(directory.path(), RepositoryMode::Graph, &first));
    app.observe_navigation();
    let link = app.navigation_trail.crumbs()[0].link.clone();
    app.request_navigation(NavigationAction::Breadcrumb(link));
    let restore = app.navigation_restore.take().unwrap();
    app.navigation_restore_inflight = true;
    let observed = read_repository(&restore, Default::default()).unwrap();
    assert_eq!(observed.revision, second);
    assert_eq!(observed.mode, RepositoryMode::Working);
    assert!(validate_repository_completion(
        &app,
        &restore,
        Some(&observed)
    ));
    app.workspace = Some(observed);
    app.finish_navigation(restore);
    assert!(app
        .workspace
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .any(|row| row.label.contains("current dirty 🦀")));
    assert_eq!(state(directory.path()), before);
}

#[test]
fn navigation_native_superseded_token_cannot_install_exact_but_late_source() {
    let (directory, first, second) = fixture();
    let before = state(directory.path());
    let mut app = app(workspace(directory.path(), RepositoryMode::Graph, &first));
    app.workspace = Some(workspace(directory.path(), RepositoryMode::Graph, &second));
    app.observe_navigation();
    app.request_navigation(NavigationAction::Back);
    let restore = app.navigation_restore.take().unwrap();
    app.navigation_restore_inflight = true;
    let observed = read_repository(&restore, Default::default()).unwrap();
    app.dispatch(Action::View(View::Search));
    assert!(!validate_repository_completion(
        &app,
        &restore,
        Some(&observed)
    ));
    assert_eq!(app.navigation_trail.history_len(), 1);
    assert_eq!(app.workspace.as_ref().unwrap().revision, second);
    assert_eq!(app.view, View::Search);
    assert_eq!(state(directory.path()), before);
}

#[test]
fn navigation_native_changed_working_bytes_refuse_old_source_without_history_pop() {
    let (directory, first, _) = fixture();
    let mut app = app(workspace(directory.path(), RepositoryMode::Working, "HEAD"));
    app.workspace = Some(workspace(directory.path(), RepositoryMode::Graph, &first));
    app.observe_navigation();
    app.request_navigation(NavigationAction::Back);
    let restore = app.navigation_restore.take().unwrap();
    app.navigation_restore_inflight = true;
    fs::write(
        directory.path().join("日本語.txt"),
        "changed since bookmark\n",
    )
    .unwrap();
    let before = state(directory.path());
    let error = read_repository(&restore, Default::default()).unwrap_err();
    assert!(error.contains("mutable files/index state changed"));
    app.abandon_navigation_restore();
    assert_eq!(app.navigation_trail.history_len(), 1);
    assert_eq!(app.workspace.as_ref().unwrap().revision, first);
    assert_eq!(state(directory.path()), before);
}

#[test]
fn navigation_native_nested_saved_file_never_falls_back_to_broad_tree() {
    let (directory, first, _) = fixture();
    let before = state(directory.path());
    let original = Workspace::open_search_file(
        directory.path(),
        &RawPath::new("日本語.txt".as_bytes().to_vec()),
        Some(&first),
        1,
        Default::default(),
    )
    .unwrap();
    let origin = original.actor_origin.as_ref().unwrap().clone();
    let mut app = app(original);
    app.workspace = Some(workspace(directory.path(), RepositoryMode::Graph, &first));
    app.observe_navigation();
    app.request_navigation(NavigationAction::Back);
    let restore = app
        .navigation_restore
        .take()
        .expect("genuine original exact file adapter");
    app.navigation_restore_inflight = true;
    let observed = read_repository(&restore, Default::default()).unwrap();
    let preview = observed
        .preview
        .as_ref()
        .expect("exact file, never broad tree fallback");
    assert_eq!(preview.path, RawPath("日本語.txt".as_bytes().to_vec()));
    assert_eq!(preview.revision, first);
    assert_eq!(preview.content, "original saved α\n".as_bytes());
    assert!(!observed
        .rows
        .iter()
        .any(|row| row.label.starts_with("Saved file tree")));
    assert_eq!(
        observed.file_provenance.as_ref().unwrap().path,
        preview.path
    );
    assert!(std::sync::Arc::ptr_eq(
        observed.actor_origin.as_ref().unwrap(),
        &origin
    ));
    assert!(validate_repository_completion(
        &app,
        &restore,
        Some(&observed)
    ));
    app.workspace = Some(observed);
    app.finish_navigation(restore);
    assert!(!app.navigation_trail.is_pending());
    assert_eq!(app.navigation_trail.history_len(), 0);
    assert_eq!(state(directory.path()), before);
}

#[test]
fn navigation_native_raw_paths_and_bounded_hashes_do_not_follow_labels_or_clone_bodies() {
    let left = row_key(&RowAction::File(RawPath::new(b"Name\xff\\x1b".to_vec()))).unwrap();
    let right = row_key(&RowAction::File(RawPath::new(b"name\xff\\x1b".to_vec()))).unwrap();
    assert_ne!(left, right);
    assert_eq!(left.len(), 32);
    assert_ne!(
        left,
        row_key(&RowAction::Directory(RawPath::new(
            b"Name\xff\\x1b".to_vec()
        )))
        .unwrap()
    );
    let mut bounded = BoundedHash::new();
    assert!(bounded.field(&vec![b'x'; 64 * 1024]).is_err());
    let (directory, _, _) = fixture();
    let app = app(workspace(directory.path(), RepositoryMode::Working, "HEAD"));
    let bookmark = capture(&app).unwrap().unwrap();
    assert!(bookmark.source_key().len() < 512);
    assert!(!String::from_utf8_lossy(bookmark.source_key()).contains("current dirty"));
}

#[test]
fn navigation_native_back_binding_mouse_and_resize_use_same_closed_action() {
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::{backend::TestBackend, Terminal};
    assert_eq!(crate::config::default_bindings()["back"], "alt+left");
    let mut keyboard = App::default();
    keyboard.handle_event(Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT)));
    let mut mouse = App::default();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut mouse))
        .unwrap();
    let hit = mouse
        .hits
        .iter()
        .find(|hit| hit.action == Action::Navigation(NavigationAction::Back))
        .unwrap()
        .clone();
    assert!(
        !terminal
            .backend()
            .buffer()
            .cell((hit.rect.x, hit.rect.y))
            .unwrap()
            .modifier
            .contains(ratatui::style::Modifier::UNDERLINED),
        "Only actual focus may underline Back, not the title's inherited style"
    );
    mouse.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: hit.rect.x,
        row: hit.rect.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(keyboard.notice, mouse.notice);
    assert!(mouse.notice.contains("No earlier"));
    mouse.overlay = None;
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut mouse))
        .unwrap();
    mouse.handle_event(Event::Resize(120, 40));
    assert!(mouse.hits.is_empty());
    mouse.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: hit.rect.x,
        row: hit.rect.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(mouse.overlay.is_none());
    let mut trail = NavigationTrail::default();
    let local = NavigationBookmark::new(
        View::Repository,
        NavigationScope::local(std::path::PathBuf::from("/tmp/exact-nav")).unwrap(),
        b"original".to_vec(),
        "graph".into(),
        vec![],
        "Repository".into(),
        Default::default(),
    )
    .unwrap();
    trail.observe_current(local.clone()).unwrap();
    let token = trail.begin(local.clone()).unwrap();
    trail.abandon().unwrap();
    assert!(trail.complete(token, local).is_err());
    assert_eq!(trail.history_len(), 0);
}

#[test]
fn navigation_native_query_execution_replaces_search_source_without_hiding_origin() {
    use crate::search::{
        corpus::{LocalSource, RepositorySelection},
        workspace::SearchWorkspace,
    };
    let (directory, first, _) = fixture();
    let before = state(directory.path());
    let mut app = app(workspace(directory.path(), RepositoryMode::Graph, &first));
    app.dispatch(Action::View(View::Search));
    app.search_load_requested = false;
    app.search = Some(SearchWorkspace::new(
        crate::storage::Scope::new("local", "navigation").unwrap(),
        vec![RepositorySelection {
            repository: "local/repo".into(),
            directory: directory.path().to_owned(),
            source: LocalSource::Worktree,
            sets: vec![],
        }],
    ));
    app.observe_navigation();
    assert_eq!(app.navigation_trail.history_len(), 1);
    app.search.as_mut().unwrap().input = "query changed β".into();
    app.observe_navigation();
    app.search.as_mut().unwrap().mode = crate::search::SearchMode::Regex;
    app.observe_navigation();
    assert_eq!(
        app.navigation_trail.history_len(),
        1,
        "Query/mode replacement must not become an unrelated extra page visit"
    );
    app.request_navigation(NavigationAction::Back);
    assert_eq!(app.view, View::Repository);
    assert_eq!(app.workspace.as_ref().unwrap().revision, first);
    assert!(app.navigation_restore.is_none());
    assert_eq!(app.navigation_trail.history_len(), 0);
    assert_eq!(state(directory.path()), before);
}
