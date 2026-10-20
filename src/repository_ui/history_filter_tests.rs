//! TASK-99.3: the History filter row's keys and footer tokens on a real repository: `/`
//! focuses the query, typing highlights at once, Enter applies through the same graph action a
//! click sends, n/N step and wrap, Backspace on an empty query removes the last chip, Esc
//! clears, the scope key picks the selected commit's branch, and every token has its key.
use super::*;
use crate::{
    footer::{layout, resolve_key},
    local_commit_actor_origin_v1_test_support::{git, text},
    workspace::RepositoryMode,
};
use std::path::Path;

fn repository(directory: &Path) -> std::path::PathBuf {
    let root = directory.join("filter-ui");
    std::fs::create_dir(&root).unwrap();
    git(&root, &["init", "-q", "--initial-branch=main"], None);
    for (key, value) in [
        ("user.name", "Ada Lovelace"),
        ("user.email", "ada@example.invalid"),
        ("commit.gpgsign", "false"),
    ] {
        git(&root, &["config", key, value], None);
    }
    for subject in [
        "Add the README",
        "Parse the config header",
        "Document install",
    ] {
        git(
            &root,
            &["commit", "-q", "--allow-empty", "-m", subject],
            None,
        );
    }
    git(&root, &["switch", "-q", "-c", "feature/x", "HEAD~1"], None);
    git(
        &root,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "Tune the config parser",
        ],
        None,
    );
    git(&root, &["switch", "-q", "main"], None);
    root
}

fn app(root: &Path) -> App {
    let mut app = App {
        view: View::Repository,
        workspace: Some(
            Workspace::load(root, RepositoryMode::Graph, "HEAD", None)
                .unwrap()
                .unwrap(),
        ),
        ..Default::default()
    };
    app.repository_list.selected = app
        .workspace
        .as_ref()
        .unwrap()
        .graph_focus_row_v1()
        .unwrap();
    app.repository_visible_rows = 20;
    app
}

fn press(app: &mut App, code: KeyCode) {
    let event = KeyEvent::new(code, KeyModifiers::NONE);
    if !key(app, event) {
        if let Some(action) = resolve_key(&controls(app), event) {
            app.dispatch(action);
        }
    }
}

/// Runs the pending worker action as the background worker would.
fn settle(app: &mut App) {
    if let Some(action) = app.repository_action.take() {
        let workspace = app.workspace.as_mut().unwrap();
        workspace.open(action).unwrap();
        app.repository_list.selected = workspace.graph_focus_row_v1().unwrap();
    }
}

/// Keys the whole main footer paints at `width`.
fn keys(app: &App, width: u16) -> Vec<String> {
    let controls = crate::footer::main_controls(app);
    layout(&controls, width)
        .into_iter()
        .map(|placed| controls[placed.index].key.clone())
        .collect()
}

#[test]
fn history_filter_v993_keys_tokens_and_query_focus() {
    let directory = tempfile::tempdir().unwrap();
    let root = repository(directory.path());
    let before = git(&root, &["for-each-ref"], None);
    let mut app = app(&root);
    assert!(active(&app) && !shown(&app));
    // Nothing filters: `/` is offered, `n` explains, Esc is not taken from Cancel.
    let tokens = controls(&app);
    assert!(tokens
        .iter()
        .any(|token| token.key == "/" && token.disabled.is_none()));
    assert!(tokens.iter().all(|token| token.key != "Esc"));
    let next = tokens.iter().find(|token| token.key == "n").unwrap();
    assert!(next.disabled.as_deref().unwrap().contains("Search first"));
    for width in [80, 140] {
        let placed = keys(&app, width);
        assert!(placed.contains(&"/".to_owned()), "{width}: {placed:?}");
        assert!(placed.contains(&"q".to_owned()), "{width}: {placed:?}");
    }
    // `/` focuses the query; typing (even `n` and `q`) is text and highlights at once.
    press(&mut app, KeyCode::Char('/'));
    assert!(editing(&app) && shown(&app));
    for character in "config".chars() {
        press(&mut app, KeyCode::Char(character));
    }
    assert_eq!(app.history_filter_input_v993.as_deref(), Some("config"));
    let workspace = app.workspace.as_ref().unwrap();
    assert_eq!(workspace.history_filter_v993.count(), 2, "live highlight");
    assert!(app.repository_action.is_none(), "typing reads no Git");
    // Enter applies through the same graph action the filter row's click sends.
    press(&mut app, KeyCode::Enter);
    assert!(!editing(&app));
    assert!(matches!(
        app.repository_action,
        Some(RowAction::Graph(GraphActionV1::Filter(ref spec))) if spec.text == "config"
    ));
    settle(&mut app);
    let workspace = app.workspace.as_ref().unwrap();
    let state = workspace.graph_state_v1.clone().unwrap();
    assert_eq!(
        workspace
            .history_filter_v993
            .matched(&state, &workspace.revision),
        Some(true),
        "the selection moved to a match"
    );
    // Esc is offered now; n/N step through both matches and wrap. `/`, `n` and Esc are painted
    // at 80 and 140 columns, so the mouse reaches them too.
    assert!(controls(&app).iter().any(|token| token.key == "Esc"));
    for width in [80, 140] {
        let placed = keys(&app, width);
        for key in ["/", "n", "Esc"] {
            assert!(placed.contains(&key.to_owned()), "{width}: {placed:?}");
        }
    }
    let first = app.repository_list.selected;
    press(&mut app, KeyCode::Char('n'));
    let second = app.repository_list.selected;
    assert_ne!(first, second);
    let one = app.status.clone();
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(
        app.repository_list.selected, first,
        "two matches: n wraps back"
    );
    let two = app.status.clone();
    assert!(
        one.starts_with("Match ") && two.starts_with("Match "),
        "{one} / {two}"
    );
    assert!(
        one.contains("wrapped") != two.contains("wrapped"),
        "exactly one step wraps: {one} / {two}"
    );
    press(&mut app, KeyCode::Char('N'));
    assert_eq!(app.repository_list.selected, second);
    // A typed chip, then Backspace on the empty query removes it.
    press(&mut app, KeyCode::Char('/'));
    for _ in 0.."config".len() {
        press(&mut app, KeyCode::Backspace);
    }
    for character in "author:ada".chars() {
        press(&mut app, KeyCode::Char(character));
    }
    press(&mut app, KeyCode::Enter);
    settle(&mut app);
    let spec = &app.workspace.as_ref().unwrap().history_filter_v993.spec;
    assert_eq!(spec.author.as_deref(), Some("ada"));
    assert_eq!(spec.text, "");
    press(&mut app, KeyCode::Char('/'));
    press(&mut app, KeyCode::Backspace);
    assert!(matches!(
        app.repository_action,
        Some(RowAction::Graph(GraphActionV1::Filter(ref spec))) if spec.author.is_none()
    ));
    settle(&mut app);
    press(&mut app, KeyCode::Esc);
    assert!(!editing(&app));
    // `B` scopes to the selected commit's branch, and back to every branch.
    let feature = text(&root, &["rev-parse", "feature/x"]);
    let row = app
        .workspace
        .as_ref()
        .unwrap()
        .graph_action_index_v1(&RowAction::Commit(feature.clone()))
        .unwrap();
    app.repository_list.selected = row;
    press(&mut app, KeyCode::Char('B'));
    assert!(matches!(
        app.repository_action,
        Some(RowAction::Graph(GraphActionV1::Filter(ref spec)))
            if spec.scope == crate::history::filter::HistoryScopeV993::Branch("feature/x".into())
    ));
    settle(&mut app);
    assert_eq!(
        history_filter_title(&app),
        "feature/x",
        "the breadcrumb names the scope"
    );
    press(&mut app, KeyCode::Esc);
    settle(&mut app);
    let workspace = app.workspace.as_ref().unwrap();
    assert!(workspace.history_filter_v993.spec.is_empty());
    assert!(workspace
        .graph_state_v1
        .as_ref()
        .unwrap()
        .last_page()
        .all_refs());
    assert_eq!(git(&root, &["for-each-ref"], None), before);
}

fn history_filter_title(app: &App) -> String {
    title_scope(app.workspace.as_ref().unwrap())
}
