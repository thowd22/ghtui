//! TASK-99.4: the History commit detail on a real repository (the shared `Real` fixture: a
//! root commit, an `original` commit with distinct author and committer, a later HEAD, a tag,
//! staged and untracked changes). The pane sits beside the table at 140 columns and becomes
//! the `D` drill-down at 80; keys and clicks reach the same actions; the changed files are read
//! once per commit after the cursor rests, never per keypress, and nothing is written.
use super::*;
use crate::{
    app::{App, Overlay},
    local_commit_actor_origin_v1_test_support::Real,
    workspace::RepositoryMode,
};
use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{backend::TestBackend, Terminal};

fn app(real: &Real) -> App {
    let workspace = Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None)
        .unwrap()
        .unwrap();
    let selected = workspace.graph_focus_row_v1().unwrap();
    App {
        view: View::Repository,
        repository_list: crate::ui_table::ListState {
            selected,
            ..Default::default()
        },
        workspace: Some(workspace),
        repository_visible_rows: 30,
        repository_content_focused: true,
        ..Default::default()
    }
}

fn draw(app: &mut App, terminal: &mut Terminal<TestBackend>) -> String {
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn press(app: &mut App, code: KeyCode) {
    let modifiers = match code {
        KeyCode::Char(c) if c.is_ascii_uppercase() => KeyModifiers::SHIFT,
        _ => KeyModifiers::NONE,
    };
    app.handle_event(Event::Key(KeyEvent::new(code, modifiers)));
}

fn click(app: &mut App, rect: Rect) {
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }));
}

fn root(real: &Real) -> String {
    crate::local_commit_actor_origin_v1_test_support::text(
        &real.root,
        &["rev-list", "--max-parents=0", "HEAD"],
    )
}

/// Draws until the shown commit's files are read (the debounce, then the worker).
fn settle(app: &mut App, terminal: &mut Terminal<TestBackend>) -> String {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let screen = draw(app, terminal);
        let commit = commit_id(app).unwrap();
        if !matches!(app.history_detail_v994.cached(&commit), FilesV994::Waiting) {
            return draw(app, terminal);
        }
        assert!(Instant::now() < deadline, "files never read\n{screen}");
        std::thread::sleep(Duration::from_millis(30));
    }
}

#[test]
fn history_detail_v994_pane_shows_the_commit_beside_the_table_and_reads_files_once_rested() {
    let real = Real::new("sha1");
    let mut app = app(&real);
    let baseline = real.snapshot();
    real.clear_trace();
    let mut terminal = Terminal::new(TestBackend::new(140, 42)).unwrap();
    let screen = draw(&mut app, &mut terminal);
    let pane = app
        .preview_area_v92
        .expect("the detail splits at 140 columns");
    assert!(pane.x >= 80 && pane.right() == 140, "{pane:?}");
    // Short and full SHA, the subject, author and committer, relative and UTC dates, the
    // parent link and the files heading, all from the loaded walk.
    for text in [
        "─ Commit ",
        &real.original[..7],
        &real.original,
        "Original saved actors",
        "Author    ",
        "Original Writer",
        "Committer ",
        "Original Recorder",
        "14 Nov 2023 22:13 UTC",
        "14 Nov 2023 22:15 UTC",
        " ago",
        "Parent    ",
        &root(&real)[..7],
        "MESSAGE",
        "No message body",
        "CHANGED FILES",
        "Reading changed files…",
        "enter diff · f file history · Y blame",
        "y copy SHA",
    ] {
        assert!(screen.contains(text), "{text:?} missing\n{screen}");
    }
    assert_eq!(
        app.history_detail_v994.reads(),
        0,
        "no read before the cursor rests"
    );
    // Scrolling fast through the table runs no Git at all.
    for code in [KeyCode::Down, KeyCode::Up, KeyCode::Down, KeyCode::Up] {
        press(&mut app, code);
        draw(&mut app, &mut terminal);
    }
    assert_eq!(app.history_detail_v994.reads(), 0);
    assert!(real.children().is_empty(), "{:?}", real.children());
    // Resting on the root commit reads its files once; returning to it reads nothing more.
    press(&mut app, KeyCode::Down);
    let screen = settle(&mut app, &mut terminal);
    assert_eq!(commit_id(&app).unwrap(), root(&real));
    assert_eq!(app.history_detail_v994.reads(), 1);
    assert!(screen.contains("CHANGED FILES  1 file · +1"), "{screen}");
    assert!(!screen.contains("−0"), "a zero side is left out\n{screen}");
    assert!(screen.contains("A tracked"), "{screen}");
    assert!(screen.contains("none · root commit"), "{screen}");
    assert!(app
        .hits
        .iter()
        .any(|hit| hit.action == Action::HistoryDetailV994(DetailUiActionV994::Select(0))));
    press(&mut app, KeyCode::Up);
    let screen = settle(&mut app, &mut terminal);
    assert!(screen.contains("No file changes"), "{screen}");
    assert_eq!(app.history_detail_v994.reads(), 2);
    press(&mut app, KeyCode::Down);
    settle(&mut app, &mut terminal);
    assert_eq!(app.history_detail_v994.reads(), 2, "cached per commit");
    assert_eq!(real.snapshot(), baseline, "the detail wrote nothing");
}

#[test]
fn history_detail_v994_keys_and_clicks_reach_the_same_actions() {
    let real = Real::new("sha1");
    let mut terminal = Terminal::new(TestBackend::new(140, 42)).unwrap();
    let mut keyboard = app(&real);
    let original = real.original.clone();
    draw(&mut keyboard, &mut terminal);
    // Enter on the table opens the diff against the first parent.
    let diff = Action::RepositoryRow(RowAction::Graph(GraphActionV1::Detail {
        commit: original.clone(),
        act: Act::OpenDiff,
    }));
    let keys = footer_keys(&keyboard);
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(crate::footer::resolve_key(&keys, enter), Some(diff.clone()));
    // f waits for the files with a reason instead of a dead key.
    let f = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE);
    assert!(matches!(
        crate::footer::resolve_key(&keys, f),
        Some(Action::Explain(reason)) if reason.contains("still loading")
    ));
    // The parent link and Enter on the parent item choose the same parent.
    let mut mouse = app(&real);
    draw(&mut mouse, &mut terminal);
    let parent = Action::RepositoryRow(RowAction::Graph(GraphActionV1::Detail {
        commit: original.clone(),
        act: Act::Parent(0),
    }));
    let link = mouse
        .hits
        .iter()
        .find(|hit| hit.action == parent)
        .expect("the parent is a link")
        .rect;
    click(&mut mouse, link);
    press(&mut keyboard, KeyCode::Tab);
    assert!(crate::list_preview_v92::preview_focused(&keyboard));
    press(&mut keyboard, KeyCode::Home);
    press(&mut keyboard, KeyCode::Enter);
    assert_eq!(
        keyboard.repository_action,
        Some(RowAction::Graph(GraphActionV1::Detail {
            commit: original.clone(),
            act: Act::Parent(0),
        }))
    );
    assert_eq!(keyboard.repository_action, mouse.repository_action);
    // Choosing the parent moves the bar to it and keeps the loaded window.
    let mut workspace = keyboard.workspace.clone().unwrap();
    let start = workspace.graph_projection_start_v2;
    workspace
        .open(keyboard.repository_action.clone().unwrap())
        .unwrap();
    assert_eq!(workspace.revision, root(&real));
    assert_eq!(workspace.graph_view_v1, GraphViewV1::Timeline);
    assert_eq!(workspace.graph_projection_start_v2, start);

    // y shows the full SHA in a notice, by key and by its token.
    let mut copy = app(&real);
    draw(&mut copy, &mut terminal);
    press(&mut copy, KeyCode::Char('y'));
    assert_eq!(copy.overlay, Some(Overlay::Notice));
    assert!(copy.notice.contains(&original), "{}", copy.notice);
    let mut copy_click = app(&real);
    draw(&mut copy_click, &mut terminal);
    let token = copy_click
        .hits
        .iter()
        .find(|hit| hit.action == Action::HistoryDetailV994(DetailUiActionV994::CopySha))
        .unwrap()
        .rect;
    click(&mut copy_click, token);
    assert_eq!(copy_click.notice, copy.notice);
}

#[test]
fn history_detail_v994_files_select_and_open_the_same_way_by_key_and_click() {
    let real = Real::new("sha1");
    let mut terminal = Terminal::new(TestBackend::new(140, 42)).unwrap();
    let mut app = app(&real);
    press(&mut app, KeyCode::Down);
    settle(&mut app, &mut terminal);
    let commit = root(&real);
    // The file row is a click target that selects it; f and Y then follow that file.
    let select = Action::HistoryDetailV994(DetailUiActionV994::Select(0));
    let row = app
        .hits
        .iter()
        .find(|hit| hit.action == select)
        .unwrap()
        .rect;
    click(&mut app, row);
    assert!(crate::list_preview_v92::preview_focused(&app));
    let keys = footer_keys(&app);
    let path = crate::git::RawPath(b"tracked".to_vec());
    for (key, act) in [
        ('f', Act::FileHistory(path.clone())),
        ('Y', Act::Blame(path.clone())),
    ] {
        let modifiers = if key == 'Y' {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        assert_eq!(
            crate::footer::resolve_key(&keys, KeyEvent::new(KeyCode::Char(key), modifiers)),
            Some(detail(&commit, act))
        );
    }
    // Enter on the focused file opens the diff; the root has no parent, so it explains.
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.overlay, Some(Overlay::Notice));
    assert!(app.notice.contains("Root commit"), "{}", app.notice);
    // Opening file history from the detail starts at this commit and path.
    let mut workspace = app.workspace.clone().unwrap();
    workspace
        .open(RowAction::Graph(GraphActionV1::Detail {
            commit: commit.clone(),
            act: Act::FileHistory(path.clone()),
        }))
        .unwrap();
    // File history is History itself with a `path:` chip for the file (TASK-99).
    assert!(workspace.file_provenance.is_none());
    assert_eq!(workspace.mode, crate::workspace::RepositoryMode::Graph);
    assert_eq!(
        workspace.history_filter_v993.spec.path.as_ref(),
        Some(&path)
    );
}

#[test]
fn history_detail_v994_drilldown_below_the_split_width() {
    let real = Real::new("sha1");
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let mut app = app(&real);
    let screen = draw(&mut app, &mut terminal);
    assert!(app.preview_area_v92.is_none(), "no split at 80 columns");
    assert!(!screen.contains("─ Commit "), "{screen}");
    // D (and its alias P) open the drill-down of the row under the bar.
    let details = Some(RowAction::Graph(GraphActionV1::Detail {
        commit: real.original.clone(),
        act: Act::Details,
    }));
    press(&mut app, KeyCode::Char('P'));
    assert_eq!(app.repository_action, details);
    let mut app = self::app(&real);
    draw(&mut app, &mut terminal);
    press(&mut app, KeyCode::Char('D'));
    assert_eq!(app.repository_action, details);
    let mut workspace = app.workspace.take().unwrap();
    workspace
        .open(app.repository_action.take().unwrap())
        .unwrap();
    assert!(drilldown(&workspace));
    app.workspace = Some(workspace);
    let screen = draw(&mut app, &mut terminal);
    for text in [
        "› Commit ",
        &real.original[..7],
        "Original saved actors",
        "Original Writer",
        "Original Recorder",
        "UTC",
        "Enter Open",
    ] {
        assert!(screen.contains(text), "{text:?} missing\n{screen}");
    }
    // The full SHA stays visible beside its label in the 76-cell body.
    assert!(screen.contains(&real.original), "{screen}");
    // Back returns to the table with the bar on the same commit.
    press(&mut app, KeyCode::Char('u'));
    assert_eq!(
        app.repository_action,
        Some(RowAction::Graph(GraphActionV1::Back))
    );
}

#[test]
fn history_detail_v994_text_helpers_wrap_words_fit_paths_and_date_both_ways() {
    assert_eq!(
        wrap(
            "A checkout whose core.worktree points at an unrelated directory",
            20
        ),
        [
            "A checkout whose",
            "core.worktree points",
            "at an unrelated",
            "directory"
        ]
    );
    assert_eq!(fit_path("src/git.rs", 20), "src/git.rs");
    assert_eq!(
        fit_path("tests/tui_e2e/local/stale_worktree_v1.rs", 30),
        "tests/…/stale_worktree_v1.rs"
    );
    assert_eq!(
        fit_path("tests/tui_e2e/local/stale_worktree_v1.rs", 22),
        "…/stale_worktree_v1.rs"
    );
    assert_eq!(
        crate::editor::display_width(&fit_path("a/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", 12)),
        12
    );
    // 1700000000 is 14 Nov 2023 22:13:20 UTC.
    assert_eq!(
        when(1_700_000_000, 1_700_007_200),
        "2h ago · 14 Nov 22:13 UTC"
    );
    assert_eq!(
        when(1_700_000_000, 1_800_000_000),
        format!(
            "{} ago · 14 Nov 2023 22:13 UTC",
            crate::ui_table::age_from_seconds(100_000_000)
        )
    );
}

/// TASK-52: `A` and the `A actions runs` token open the shared Actions view on the runs of the
/// exact commit, with one meaning; nothing is read here (the worker reads on the next turn).
#[test]
fn history_detail_actions_runs_key_and_click_open_actions_for_the_exact_commit() {
    let real = Real::new("sha1");
    let baseline = real.snapshot();
    let mut terminal = Terminal::new(TestBackend::new(140, 42)).unwrap();
    let wanted = Some(crate::actions_workspace::ActionsAction::CommitRuns(
        real.original.clone(),
    ));
    let mut keyboard = app(&real);
    draw(&mut keyboard, &mut terminal);
    let screen = draw(&mut keyboard, &mut terminal);
    assert!(screen.contains("A actions runs"), "{screen}");
    let keys = footer_keys(&keyboard);
    assert!(keys
        .iter()
        .any(|key| key.key == "A" && key.disabled.is_none()));
    press(&mut keyboard, KeyCode::Char('A'));
    assert_eq!(keyboard.view, View::Actions);
    assert_eq!(keyboard.actions_action, wanted);
    assert!(keyboard.actions_load_requested);

    let mut mouse = app(&real);
    draw(&mut mouse, &mut terminal);
    let token = mouse
        .hits
        .iter()
        .find(|hit| hit.action == Action::HistoryDetailV994(DetailUiActionV994::ActionsRuns))
        .expect("the actions runs token is clickable")
        .rect;
    click(&mut mouse, token);
    assert_eq!(mouse.view, View::Actions);
    assert_eq!(mouse.actions_action, wanted);
    assert_eq!(real.snapshot(), baseline);

    // Only a full object ID reaches the run filter.
    assert!(crate::actions_workspace::commit_runs_sha(&real.original[..7]).is_err());
    assert!(crate::actions_workspace::commit_runs_sha(&"g".repeat(40)).is_err());
    assert_eq!(
        crate::actions_workspace::commit_runs_sha(&"AB".repeat(20)).unwrap(),
        "ab".repeat(20)
    );
}

/// TASK-67: a merge explains its parents in plain words; ordinary and root commits do not.
#[test]
fn novice_usability_v67_merge_detail_explains_parents_in_plain_words() {
    let (a, b, c, d) = (
        "a".repeat(40),
        "b".repeat(40),
        "c".repeat(40),
        "d".repeat(40),
    );
    assert_eq!(merge_explanation::<String>(&[]), None);
    assert_eq!(merge_explanation(std::slice::from_ref(&a)), None);
    let two = merge_explanation(&[a.clone(), b.clone()]).unwrap();
    assert_eq!(
        two,
        "bbbbbbb (parent 2) was merged into aaaaaaa (parent 1, the branch that was checked \
         out); changes below are compared with parent 1"
    );
    let octopus = merge_explanation(&[a, b, c, d]).unwrap();
    assert!(
        octopus.starts_with(
            "bbbbbbb, ccccccc, ddddddd (parents 2–4) were merged into aaaaaaa (parent 1"
        ),
        "{octopus}"
    );
    // Terminal control bytes in a parent never reach the screen.
    let hostile = merge_explanation(&["a\u{1b}[31m".to_owned(), "b".into()]).unwrap();
    assert!(!hostile.contains('\u{1b}'), "{hostile:?}");
    // Wrapped at the narrow detail width, every piece fits and no word is split.
    let width = 80 - LABEL - 1;
    for part in wrap(&two, width) {
        assert!(crate::editor::display_width(&part) <= width, "{part}");
    }
    let joined = wrap(&two, 30).join(" ");
    assert_eq!(
        joined.split_whitespace().collect::<Vec<_>>(),
        two.split_whitespace().collect::<Vec<_>>()
    );
}
