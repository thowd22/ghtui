//! TASK-64 release evidence for `docs/coverage-v1.md`.
//!
//! The document maps every PR and Actions inventory row (the closed requirement model in
//! `providers::gh::CapabilityContractRowV1`) and every local Git operation to native behavior,
//! tests and limitations, and enumerates every key-bar control of the probed view states with
//! its key and click path. These tests keep that document honest:
//!
//! * every inventory row and every Local operation has exactly one row in the document;
//! * every test name the document cites exists as a `#[test]` that is not ignored, in a file
//!   some module declares, and PTY journeys live in the real-binary `tui_e2e` target;
//! * every key-bar control of every probed state has a key, that key resolves to the control
//!   itself (no earlier control shadows it), it has a click target (key bar or tab strip when
//!   enabled; Commands always), and the document lists it;
//! * every Commands (palette) entry of every probed state can be chosen by typing and Enter
//!   and by clicking its row, and both yield the entry's own action.
#[path = "git_support/mod.rs"]
mod git_support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use ghtui::{
    actions_workspace::{ActionsView, ActionsWorkspace},
    app::{Action, App, View},
    command_palette::{CommandPalette, PaletteAction, PaletteEntry, PaletteOutcome},
    footer::{self, ControlGroup, KeyControl, Resolved},
    github_workspace::{GithubWorkspace, PrPage, PrTab},
    local_actions::LocalCommand,
    navigation_catalogue::PreparedCatalogue,
    providers::gh::{
        CapabilityContractRowV1, Coverage, HostIdentity, PrDetails, PrSummary, PullRequest, Queue,
    },
    workspace::{RepositoryMode, Workspace},
};
use ratatui::{backend::TestBackend, Terminal};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

const DOC: &str = "docs/coverage-v1.md";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn document() -> String {
    std::fs::read_to_string(root().join(DOC)).unwrap_or_else(|error| panic!("{DOC}: {error}"))
}

/// The lines of the `## <heading>` section (up to the next `## `).
fn section<'a>(doc: &'a str, heading: &str) -> Vec<&'a str> {
    let mut lines = doc.lines().skip_while(|line| line.trim() != heading);
    assert!(lines.next().is_some(), "{DOC} has no section `{heading}`");
    lines.take_while(|line| !line.starts_with("## ")).collect()
}

/// Table body rows of a section as trimmed cells (header and separator rows dropped).
fn table_rows(lines: &[&str]) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut in_table = false;
    for line in lines {
        let line = line.trim();
        if !line.starts_with('|') {
            in_table = false;
            continue;
        }
        let cells = line
            .trim_matches('|')
            .split(" | ")
            .map(|cell| cell.trim().to_string())
            .collect::<Vec<_>>();
        if !in_table {
            // The header row; the separator follows it.
            in_table = true;
            continue;
        }
        if cells
            .iter()
            .all(|cell| cell.chars().all(|c| c == '-' || c == ':'))
        {
            continue;
        }
        rows.push(cells);
    }
    rows
}

/// Backticked identifiers in a cell that look like Rust test names.
fn test_names(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .filter(|token| {
            !token.is_empty()
                && token
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == ':')
                && token.contains('_')
        })
        .map(|token| token.rsplit("::").next().unwrap().to_string())
        .collect()
}

fn unbacktick(cell: &str) -> &str {
    cell.trim().trim_matches('`')
}

#[derive(Debug)]
struct TestFn {
    file: PathBuf,
    ignored: bool,
}

fn rust_files(directory: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Test names to their definitions, and every source file with its text.
type TestIndex = (BTreeMap<String, Vec<TestFn>>, Vec<(PathBuf, String)>);

/// Every `#[test]` function in `src/` and `tests/`, with whether it is ignored.
fn test_index() -> TestIndex {
    let mut files = Vec::new();
    rust_files(&root().join("src"), &mut files);
    rust_files(&root().join("tests"), &mut files);
    let mut tests: BTreeMap<String, Vec<TestFn>> = BTreeMap::new();
    let mut sources = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let mut pending: Option<bool> = None;
        for line in text.lines() {
            let line = line.trim();
            if line == "#[test]" {
                pending = Some(false);
                continue;
            }
            if let Some(ignored) = pending {
                if line.starts_with("#[") {
                    pending = Some(ignored || line.starts_with("#[ignore"));
                    continue;
                }
                if let Some(rest) = line
                    .strip_prefix("fn ")
                    .or_else(|| line.strip_prefix("async fn "))
                {
                    let name = rest.split(['(', '<']).next().unwrap().trim().to_string();
                    tests.entry(name).or_default().push(TestFn {
                        file: file.clone(),
                        ignored,
                    });
                }
                pending = None;
            }
        }
        // Paired keyboard/mouse journeys are often generated by a local `macro_rules!` whose
        // body holds the `#[test]`; their names are the macro's one-per-line arguments.
        if text.contains("macro_rules!") && text.contains("#[test]") {
            for line in text.lines() {
                let name = line.trim().trim_end_matches(',');
                if name.len() > 8
                    && name.contains('_')
                    && name.starts_with(|c: char| c.is_ascii_lowercase())
                    && name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                {
                    tests.entry(name.to_string()).or_default().push(TestFn {
                        file: file.clone(),
                        ignored: false,
                    });
                }
            }
        }
        sources.push((file, text));
    }
    (tests, sources)
}

/// Module stems (`mod stem`) and `#[path]` file names that some source declares.
fn declarations(sources: &[(PathBuf, String)]) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut stems = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for (_, text) in sources {
        for line in text.lines() {
            if let Some(position) = line.find("mod ") {
                let stem = line[position + 4..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect::<String>();
                if !stem.is_empty() {
                    stems.insert(stem);
                }
            }
            for quoted in line.split('"').skip(1).step_by(2) {
                if let Some(name) = quoted.strip_suffix(".rs") {
                    let name = name.rsplit('/').next().unwrap_or(name);
                    paths.insert(format!("{name}.rs"));
                }
            }
        }
    }
    (stems, paths)
}

/// Whether some source declares `file` as a module (`mod stem;` or a `#[path]`).
fn declared(file: &Path, declarations: &(BTreeSet<String>, BTreeSet<String>)) -> bool {
    let stem = file.file_stem().unwrap().to_string_lossy();
    let name = file.file_name().unwrap().to_string_lossy();
    // A crate root, or a top-level integration test target, is its own root.
    name == "lib.rs"
        || name == "main.rs"
        || file.parent() == Some(root().join("tests").as_path())
        || declarations.0.contains(stem.as_ref())
        || declarations.1.contains(name.as_ref())
}

fn assert_cited_tests(
    cells: &[(String, String, bool)],
    tests: &BTreeMap<String, Vec<TestFn>>,
    sources: &[(PathBuf, String)],
) -> usize {
    let declarations = declarations(sources);
    let mut failures = Vec::new();
    let mut checked = 0;
    for (row, cell, pty) in cells {
        for name in test_names(cell) {
            checked += 1;
            let Some(found) = tests.get(&name) else {
                failures.push(format!(
                    "{row}: `{name}` is not a #[test] in src/ or tests/"
                ));
                continue;
            };
            let live = found
                .iter()
                .filter(|test| !test.ignored && declared(&test.file, &declarations))
                .filter(|test| {
                    !pty || test
                        .file
                        .strip_prefix(root().join("tests"))
                        .is_ok_and(|path| path.starts_with("tui_e2e"))
                })
                .count();
            if live == 0 {
                failures.push(format!(
                    "{row}: `{name}` is ignored, undeclared, or (as a PTY journey) not in tests/tui_e2e: {found:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    checked
}

#[test]
fn coverage_v64_inventory_maps_every_pr_and_actions_row_once_with_tests_and_limits() {
    let doc = document();
    let (tests, sources) = test_index();
    let mut seen = BTreeMap::new();
    let mut cited = Vec::new();
    for (heading, prefix) in [
        ("## PR inventory", "pr."),
        ("## Actions inventory", "actions."),
    ] {
        for cells in table_rows(&section(&doc, heading)) {
            assert_eq!(cells.len(), 5, "{heading}: row needs 5 cells: {cells:?}");
            let key = unbacktick(&cells[0]).to_string();
            assert!(key.starts_with(prefix), "{heading}: {key} is misplaced");
            assert!(
                *seen.entry(key.clone()).and_modify(|n| *n += 1).or_insert(1) == 1,
                "{key} is listed twice"
            );
            let (native, code, pty, limits) = (&cells[1], &cells[2], &cells[3], &cells[4]);
            assert!(!native.is_empty() && !limits.is_empty(), "{key}: {cells:?}");
            assert!(
                !test_names(code).is_empty(),
                "{key}: cite at least one code/provider test"
            );
            let gap = limits.starts_with("**Gap**");
            let journeys = test_names(pty);
            if !gap {
                // A native row needs both input paths in the real binary.
                assert!(
                    journeys.iter().any(|name| name.contains("keyboard"))
                        && journeys.iter().any(|name| name.contains("mouse")),
                    "{key}: cite a keyboard and a mouse PTY journey (or mark the row **Gap**)"
                );
            }
            // A browser handoff is never native support.
            assert!(
                !native.to_ascii_lowercase().starts_with("browser"),
                "{key}: a browser handoff does not count as native behavior"
            );
            cited.push((key.clone(), code.clone(), false));
            cited.push((key, pty.clone(), true));
        }
    }
    let expected = CapabilityContractRowV1::ALL
        .iter()
        .map(|row| row.key().to_string())
        .collect::<BTreeSet<_>>();
    let listed = seen.keys().cloned().collect::<BTreeSet<_>>();
    assert_eq!(
        listed, expected,
        "{DOC} must list exactly the inventory rows of the requirement model"
    );
    assert!(assert_cited_tests(&cited, &tests, &sources) >= 2 * expected.len());
}

#[test]
fn coverage_v64_local_git_operations_have_menu_key_click_and_existing_tests() {
    let doc = document();
    let (tests, sources) = test_index();
    let rows = table_rows(&section(&doc, "## Local Git operations"));
    let mut seen = BTreeMap::new();
    let mut cited = Vec::new();
    for cells in &rows {
        assert_eq!(cells.len(), 5, "Local Git row needs 5 cells: {cells:?}");
        let label = cells[0].clone();
        assert!(
            *seen
                .entry(label.clone())
                .and_modify(|n| *n += 1)
                .or_insert(1)
                == 1,
            "{label} is listed twice"
        );
        assert!(
            !cells[1].is_empty() && !cells[2].is_empty(),
            "{label}: key and click paths"
        );
        cited.push((label.clone(), cells[3].clone(), false));
        let journeys = test_names(&cells[4]);
        assert!(
            journeys.iter().any(|name| name.contains("keyboard"))
                && journeys.iter().any(|name| name.contains("mouse")),
            "{label}: cite a keyboard and a mouse PTY journey"
        );
        cited.push((label, cells[4].clone(), true));
    }
    let expected = LocalCommand::ALL
        .iter()
        .map(|(_, label)| label.to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        seen.keys().cloned().collect::<BTreeSet<_>>(),
        expected,
        "{DOC} must list every Local operation exactly once"
    );
    assert!(assert_cited_tests(&cited, &tests, &sources) >= 2 * expected.len());
}

// ---- Probed view states ------------------------------------------------------------------

fn identity(account: Option<u64>) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "current-user".into(),
        account_id: account,
        gh_version: "2.99.0".into(),
    }
}

fn pull_request(number: u64) -> PullRequest {
    PullRequest {
        host: "github.invalid".into(),
        id: format!("PR_{number}"),
        repository: "example/actual".into(),
        number,
        title: format!("Coverage row {number}"),
        body: String::new(),
        url: String::new(),
        author: Default::default(),
        state: "open".into(),
        draft: false,
        created_at: String::new(),
        updated_at: String::new(),
        queues: BTreeSet::from([Queue::Authored]),
    }
}

fn inbox(count: u64) -> App {
    let mut github = GithubWorkspace::empty(identity(Some(42)), vec![], BTreeSet::new());
    github.inbox.pull_requests = (1..=count).map(pull_request).collect();
    github.selected = github.inbox.pull_requests.first().map(Into::into);
    let mut app = empty(View::Inbox);
    app.github = Some(github);
    app.github_content_focused = true;
    app
}

fn detail(tab: PrTab) -> App {
    let mut app = inbox(1);
    app.github.as_mut().unwrap().pr = Some(PrPage {
        details: PrDetails {
            summary: PrSummary {
                pr: pull_request(7),
                head_sha: "a".repeat(40),
                base_sha: "b".repeat(40),
                head_ref: "feature".into(),
                base_ref: "main".into(),
                merged: false,
                auto_merge: false,
                merge_commit_sha: None,
                mergeable: None,
                mergeable_state: "unknown".into(),
                additions: 0,
                deletions: 0,
                changed_files: 0,
            },
            comments: vec![],
            commits: vec![],
            files: vec![],
            reviews: vec![],
            timeline: vec![],
            coverage: Coverage {
                complete: true,
                ..Default::default()
            },
        },
        tab,
        diff: None,
        checks: None,
        required_checks: None,
        rows: vec![],
        context: None,
        metadata_display_v51: None,
        review_progress: None,
    });
    app
}

fn actions(view: ActionsView) -> App {
    let mut workspace = ActionsWorkspace::new(
        identity(Some(42)),
        vec!["example/repo".into(), "example/other".into()],
    );
    workspace.view = view;
    workspace.prepare_rows();
    let mut app = empty(View::Actions);
    app.actions = Some(workspace);
    app
}

fn search(corpus: bool) -> App {
    let mut app = empty(View::Search);
    app.search = Some(ghtui::search::workspace::SearchWorkspace::new(
        ghtui::storage::Scope::new("local", "workspace").unwrap(),
        vec![ghtui::search::corpus::RepositorySelection {
            repository: "local/current".into(),
            directory: "/tmp/example".into(),
            source: ghtui::search::corpus::LocalSource::Worktree,
            sets: vec![],
        }],
    ));
    app.search_controls.corpus = corpus;
    app
}

fn empty(view: View) -> App {
    let mut app = App::default();
    app.view = view;
    app
}

/// Every probed state, by the name the document uses. The repository fixture must outlive
/// the states that loaded it.
fn states(fixture: &git_support::Fixture) -> Vec<(String, App)> {
    let mut states = vec![
        ("Inbox (no account)".to_string(), empty(View::Inbox)),
        ("Inbox (empty queue)".into(), inbox(0)),
        ("Inbox (3 PRs)".into(), inbox(3)),
    ];
    for (tab, label, _) in PrTab::ALL {
        states.push((format!("PR {label}"), detail(tab)));
    }
    states.push(("Repository (none)".into(), empty(View::Repository)));
    for mode in [
        RepositoryMode::Overview,
        RepositoryMode::Graph,
        RepositoryMode::Tree,
        RepositoryMode::Working,
        RepositoryMode::Staged,
        RepositoryMode::Uncommitted,
    ] {
        let workspace = Workspace::load(&fixture.root, mode, "HEAD", None)
            .unwrap_or_else(|error| panic!("{mode:?}: {error}"))
            .unwrap_or_else(|| panic!("{mode:?}: the fixture is a repository"));
        let mut app = empty(View::Repository);
        app.workspace = Some(workspace);
        states.push((format!("Repository {mode:?}"), app));
    }
    states.push(("Search".into(), search(false)));
    states.push(("Search (corpus)".into(), search(true)));
    states.push(("Actions (no account)".into(), empty(View::Actions)));
    for (view, label) in ActionsView::CATALOGS {
        states.push((format!("Actions {label}"), actions(view)));
    }
    states.push(("Dashboard (not loaded)".into(), empty(View::Dashboard)));
    states
}

fn repository_fixture() -> git_support::Fixture {
    let fixture = git_support::Fixture::linear();
    // A staged and an unstaged change so Working, Staged and Uncommitted have rows.
    fixture.write("staged.txt", b"staged\n");
    git_support::git(&fixture.root, ["add", "staged.txt"]);
    fixture.write("readme.txt", b"first\nchanged\nworking\n");
    fixture
}

/// The key event a painted key token stands for, or `None` when the token is not a key.
fn event(key: &str) -> Option<KeyEvent> {
    if let Some(letter) = key.strip_prefix("Ctrl-") {
        let mut letters = letter.chars();
        return match (letters.next(), letters.next()) {
            (Some(letter), None) => Some(KeyEvent::new(
                KeyCode::Char(letter.to_ascii_lowercase()),
                KeyModifiers::CONTROL,
            )),
            _ => None,
        };
    }
    let code = match key.to_ascii_lowercase().as_str() {
        "esc" => KeyCode::Esc,
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "space" => KeyCode::Char(' '),
        "backspace" => KeyCode::Backspace,
        "del" | "delete" => KeyCode::Delete,
        "pgup" => KeyCode::PageUp,
        "pgdn" => KeyCode::PageDown,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "up" | "↑" => KeyCode::Up,
        "down" | "↓" => KeyCode::Down,
        "left" | "←" => KeyCode::Left,
        "right" | "→" => KeyCode::Right,
        name => {
            if let Some(number) = name
                .strip_prefix('f')
                .and_then(|number| number.parse::<u8>().ok())
                .filter(|number| (1..=24).contains(number))
            {
                KeyCode::F(number)
            } else {
                let mut characters = key.chars();
                match (characters.next(), characters.next()) {
                    (Some(character), None) => KeyCode::Char(character),
                    _ => return None,
                }
            }
        }
    };
    let modifiers = match code {
        KeyCode::Char(c) if c.is_ascii_uppercase() => KeyModifiers::SHIFT,
        _ => KeyModifiers::NONE,
    };
    Some(KeyEvent::new(code, modifiers))
}

fn click_path(control: &KeyControl) -> &'static str {
    if control.group == ControlGroup::Mode {
        "tab strip, Commands"
    } else if control.disabled.is_some() {
        "Commands (explains)"
    } else {
        "key bar, Commands"
    }
}

/// The document row for one control: `| state | `key` | label | click path |`.
fn doc_row(state: &str, control: &KeyControl) -> String {
    format!(
        "| {state} | `{}` | {} | {} |",
        control.key,
        control.label,
        click_path(control)
    )
}

fn copy(entry: &PaletteEntry) -> PaletteEntry {
    PaletteEntry {
        id: entry.id.clone(),
        label: entry.label.clone(),
        keywords: entry.keywords.clone(),
        action: entry.action.clone(),
        key: entry.key.clone(),
    }
}

#[test]
fn coverage_v64_every_key_bar_control_has_a_key_a_click_target_and_a_document_row() {
    let fixture = repository_fixture();
    let doc = document();
    let documented = table_rows(&section(&doc, "## Key bar controls by view"))
        .into_iter()
        .map(|cells| {
            assert_eq!(cells.len(), 4, "key bar row needs 4 cells: {cells:?}");
            (
                cells[0].clone(),
                unbacktick(&cells[1]).to_string(),
                cells[2].clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    let mut failures = Vec::new();
    let mut missing = Vec::new();
    let mut checked = 0;
    for (state, app) in states(&fixture) {
        let controls = footer::main_controls(&app);
        let catalogue = PreparedCatalogue::prepare(&app, 1)
            .unwrap_or_else(|error| panic!("{state}: Commands refused: {error}"));
        let placed = footer::layout(&controls, 1000)
            .into_iter()
            .map(|placed| placed.index)
            .collect::<BTreeSet<_>>();
        for (index, control) in controls.iter().enumerate() {
            checked += 1;
            let name = format!("{state}: `{} {}`", control.key, control.label);
            if !documented.contains(&(state.clone(), control.key.clone(), control.label.clone())) {
                missing.push(doc_row(&state, control));
            }
            // A key binding: the painted key token is a real key and resolves to this control.
            if control.key.is_empty() {
                failures.push(format!("{name} has no key binding"));
                continue;
            }
            let Some(event) = event(&control.key) else {
                failures.push(format!("{name}: `{}` is not a key", control.key));
                continue;
            };
            let expected = match &control.disabled {
                Some(reason) => Resolved::Explain(reason.clone()),
                None => Resolved::Act(control.action.clone()),
            };
            match footer::resolve(&controls, &event) {
                Some(resolved) if resolved == expected => {}
                // The tab already shown may lend its key to a view control (PR Files: `f` is
                // File history there); clicking the tab still works and pressing it is moot.
                Some(_) if control.group == ControlGroup::Mode && control.selected => {}
                other => failures.push(format!(
                    "{name}: its key resolves to {other:?}, another control shadows it (control {index})"
                )),
            }
            // A click target: an enabled bar control is placed on a wide bar; a Mode control
            // is a tab. Every control (disabled ones as their explanation) is in Commands.
            if control.group != ControlGroup::Mode
                && control.disabled.is_none()
                && !placed.contains(&index)
            {
                failures.push(format!("{name} is enabled but never placed on the key bar"));
            }
            if control.action == Action::Palette {
                continue;
            }
            let in_commands = catalogue.entries.iter().any(|entry| {
                entry.key.as_deref() == Some(control.key.as_str())
                    && if control.disabled.is_some() {
                        matches!(entry.action, Action::Explain(_))
                    } else {
                        entry.action == control.action
                            || matches!(&entry.action, Action::Explain(_))
                    }
            });
            if !in_commands {
                failures.push(format!("{name} has no Commands row with its key"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        missing.is_empty(),
        "{DOC} `## Key bar controls by view` lacks these rows:\n{}",
        missing.join("\n")
    );
    assert!(checked > 150, "{checked}");
}

#[test]
fn coverage_v64_every_commands_entry_is_chosen_by_typing_enter_and_by_clicking_its_row() {
    let fixture = repository_fixture();
    let mut checked = 0;
    for (state, app) in states(&fixture) {
        let catalogue = PreparedCatalogue::prepare(&app, 1)
            .unwrap_or_else(|error| panic!("{state}: Commands refused: {error}"));
        let entries = catalogue.entries;
        for (index, entry) in entries.iter().enumerate() {
            let fresh = || CommandPalette::new(entries.iter().map(copy).collect()).unwrap();
            let name = format!("{state}: Commands `{}` ({})", entry.label, entry.id);
            // Keyboard: type the label, move to the row, press Enter.
            let mut keyboard = fresh();
            for character in entry.label.chars() {
                let action = keyboard
                    .key(KeyCode::Char(character), KeyModifiers::NONE)
                    .unwrap_or_else(|| panic!("{name}: typing `{character}` is not a key"));
                assert_ne!(
                    std::mem::discriminant(&keyboard.dispatch(action)),
                    std::mem::discriminant(&PaletteOutcome::Ignored),
                    "{name}"
                );
            }
            assert!(
                keyboard.visible_indices().contains(&index),
                "{name}: typing its label does not find it"
            );
            for _ in 0..=keyboard.visible_indices().len() {
                if keyboard.selected_index() == Some(index) {
                    break;
                }
                let down = keyboard.key(KeyCode::Down, KeyModifiers::NONE).unwrap();
                keyboard.dispatch(down);
            }
            assert_eq!(
                keyboard.selected_index(),
                Some(index),
                "{name}: Down never reaches it"
            );
            let enter = keyboard.key(KeyCode::Enter, KeyModifiers::NONE).unwrap();
            match keyboard.dispatch(enter) {
                PaletteOutcome::Chosen {
                    original_index,
                    action,
                } => {
                    assert_eq!(original_index, index, "{name}");
                    assert_eq!(action, entry.action, "{name}: Enter chose another action");
                }
                other => panic!("{name}: Enter gave {other:?}"),
            }
            // Mouse: select the row so it is in view, then click where it was painted.
            let mut mouse = fresh();
            mouse.dispatch(PaletteAction::IndexedRow(index));
            let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
            let mut hits = Vec::new();
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    hits = mouse.draw(frame, area, app.theme());
                })
                .unwrap();
            let rect = hits
                .iter()
                .find(|hit| hit.action == PaletteAction::Activate(index))
                .unwrap_or_else(|| panic!("{name}: its row has no click target"))
                .rect;
            let click = mouse
                .mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y)
                .unwrap_or_else(|| panic!("{name}: the click lands nowhere"));
            match mouse.dispatch(click) {
                PaletteOutcome::Chosen {
                    original_index,
                    action,
                } => {
                    assert_eq!(original_index, index, "{name}");
                    assert_eq!(
                        action, entry.action,
                        "{name}: the click chose another action"
                    );
                }
                other => panic!("{name}: the click gave {other:?}"),
            }
            checked += 1;
        }
    }
    assert!(checked > 300, "{checked}");
}

#[test]
fn coverage_v64_pr_actions_menu_rows_are_documented() {
    let doc = document();
    let documented = table_rows(&section(&doc, "## PR actions menu"))
        .into_iter()
        .map(|cells| cells[0].clone())
        .collect::<BTreeSet<_>>();
    let mut missing = Vec::new();
    let mut checked = 0;
    for (tab, _, _) in PrTab::ALL {
        for (label, _) in detail(tab).pr_controls() {
            checked += 1;
            if !documented.contains(&label) {
                missing.push(format!(
                    "| {label} | `p`, then `j`/`k` and `Enter` | click the row |"
                ));
            }
        }
    }
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "{DOC} `## PR actions menu` lacks:\n{}",
        missing.join("\n")
    );
    assert!(checked > 20, "{checked}");
}
