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

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ghtui::{
    actions_workspace::{ActionsAction, ActionsCommand, ActionsView, ActionsWorkspace, RunPage},
    app::{Action, App, Overlay, View},
    command_palette::{CommandPalette, PaletteAction, PaletteEntry, PaletteOutcome},
    footer::{self, ControlGroup, KeyControl, Resolved},
    github_workspace::{GithubWorkspace, PrPage, PrTab},
    local_actions::LocalCommand,
    navigation_catalogue::PreparedCatalogue,
    providers::gh::{
        ActionsArtifact, ActionsTarget, Actor, CapabilityContractRowV1, Coverage, HostIdentity,
        PrDetails, PrSummary, PullRequest, Queue, RunAttempt, Workflow, WorkflowDefinition,
        WorkflowRun,
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
    actions_with(view, |_| {})
}

/// An Actions page prepared from `prepare`'s exact targets, without any gh read.
fn actions_with(view: ActionsView, prepare: impl FnOnce(&mut ActionsWorkspace)) -> App {
    let mut workspace = ActionsWorkspace::new(
        identity(Some(42)),
        vec!["example/repo".into(), "example/other".into()],
    );
    prepare(&mut workspace);
    workspace.view = view;
    workspace.prepare_rows();
    let mut app = empty(View::Actions);
    app.actions = Some(workspace);
    app
}

/// A finished, failed pull-request run: rerun all/failed and the deletes are enabled; rerun of
/// a job (none open), cancel, force-cancel and fork approval explain why they are not.
fn run_page(logs: Option<&str>) -> RunPage {
    let actor = |login: &str| Actor {
        login: login.into(),
        actor_type: "User".into(),
        ..Actor::default()
    };
    RunPage {
        repository: "example/other".into(),
        attempt: RunAttempt {
            run: WorkflowRun {
                id: 123,
                workflow_id: 7,
                name: "CI".into(),
                title: "Coverage run".into(),
                head_sha: "a".repeat(40),
                head_branch: "feature".into(),
                event: "pull_request".into(),
                status: "completed".into(),
                conclusion: "failure".into(),
                attempt: 2,
                actor: actor("alice"),
                triggering_actor: actor("current-user"),
                created_at: "2026-10-13T12:00:00Z".into(),
                updated_at: "2026-10-13T12:05:00Z".into(),
                url: String::new(),
            },
            jobs: vec![],
            definition_note: "dependencies unavailable".into(),
        },
        selected_job: None,
        logs: logs.map(str::to_owned),
        logs_skipped_bytes: 0,
        annotations: vec![],
    }
}

fn artifact() -> ActionsArtifact {
    ActionsArtifact {
        id: 41,
        name: "build-output".into(),
        run_id: 123,
        attempt: None,
        head_sha: "a".repeat(40),
        size_in_bytes: 1024,
        expired: false,
        created_at: String::new(),
        updated_at: String::new(),
        expires_at: String::new(),
        digest: None,
    }
}

fn definition() -> WorkflowDefinition {
    WorkflowDefinition {
        workflow: Workflow {
            id: 7,
            name: "CI".into(),
            path: ".github/workflows/ci.yml".into(),
            state: "active".into(),
        },
        revision: "main".into(),
        commit_sha: "c".repeat(40),
        yaml: "on: workflow_dispatch\njobs: {}\n".into(),
        triggers: vec!["workflow_dispatch".into()],
        dispatch_inputs: vec![],
        jobs: vec![],
    }
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
    // The detail pages behind the catalogues (TASK-64 AC2).
    states.push((
        "Actions Run".into(),
        actions_with(ActionsView::Run, |workspace| {
            workspace.run = Some(run_page(None))
        }),
    ));
    states.push((
        "Actions Logs".into(),
        actions_with(ActionsView::Logs, |workspace| {
            workspace.run = Some(run_page(Some("CI\tRun tests\tERROR fixture\n")))
        }),
    ));
    states.push((
        "Actions Artifact".into(),
        actions_with(ActionsView::Artifact, |workspace| {
            workspace.run = Some(run_page(None));
            workspace.artifact = Some(artifact());
        }),
    ));
    states.push((
        "Actions Workflow definition".into(),
        actions_with(ActionsView::Workflow, |workspace| {
            workspace.definition = Some(definition())
        }),
    ));
    states.push((
        "Actions Publications".into(),
        actions(ActionsView::Publications),
    ));
    states.push((
        "Actions Runs (uncertain write)".into(),
        actions_with(ActionsView::Runs, |workspace| {
            workspace.reconciliation_target = Some(ActionsTarget::Run {
                repository: "example/repo".into(),
                id: 123,
            })
        }),
    ));
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
fn doc_row(state: &str, key: &str, control: &KeyControl) -> String {
    format!(
        "| {state} | `{key}` | {} | {} |",
        control.label,
        click_path(control)
    )
}

/// The key a disabled Actions control shares with an enabled control of the same page, which
/// keeps it (`X Enable workflow` beside an enabled `X Disable workflow`): the key does what the
/// enabled control does, and the disabled one explains itself from Commands.
fn shared_key(app: &App, controls: &[KeyControl], control: &KeyControl) -> Option<String> {
    if !control.key.is_empty() || control.disabled.is_none() {
        return None;
    }
    let key = app
        .actions
        .as_ref()?
        .controls
        .iter()
        .map(|prepared| prepared.key_and_text())
        .find(|(key, text)| key.is_some() && *text == control.label)?
        .0?;
    controls
        .iter()
        .any(|other| other.key == key && other.disabled.is_none())
        .then_some(key)
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
            let shared = shared_key(&app, &controls, control);
            let key = shared.clone().unwrap_or_else(|| control.key.clone());
            if !documented.contains(&(state.clone(), key.clone(), control.label.clone())) {
                missing.push(doc_row(&state, &key, control));
            }
            if shared.is_some() {
                // Its shared key belongs to the enabled control; Commands explains this one.
                let explained = catalogue.entries.iter().any(|entry| {
                    matches!(&entry.action, Action::Explain(reason) if reason.contains(&control.label))
                });
                if !explained {
                    failures.push(format!(
                        "{name} shares `{key}` but Commands does not explain it"
                    ));
                }
                continue;
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

#[test]
fn coverage_v64_actions_run_artifact_and_inspect_commands_have_their_own_keys() {
    let press = |app: &App, key: char| {
        footer::resolve(
            &footer::main_controls(app),
            &KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
        )
    };
    let compose = |command| Action::Actions(ActionsAction::Compose(command));
    let run = actions_with(ActionsView::Run, |workspace| {
        workspace.run = Some(run_page(None))
    });
    // Enabled on a finished failed run: the key does the command.
    for (key, command) in [
        ('-', ActionsCommand::DeleteRun),
        ('L', ActionsCommand::DeleteLogs),
    ] {
        assert_eq!(press(&run, key), Some(Resolved::Act(compose(command))));
    }
    // Unavailable here: the key explains why, never nothing.
    for (key, command, reason) in [
        ('J', ActionsCommand::RerunJob, "Open the job to rerun"),
        ('!', ActionsCommand::ForceCancel, "Run already completed"),
        (
            'Y',
            ActionsCommand::ApproveFork,
            "not awaiting fork approval",
        ),
    ] {
        match press(&run, key) {
            Some(Resolved::Explain(explanation)) => assert!(
                explanation.starts_with(command.label()) && explanation.contains(reason),
                "{key}: {explanation}"
            ),
            other => panic!("{key} must explain {command:?}, got {other:?}"),
        }
    }
    let artifact_page = actions_with(ActionsView::Artifact, |workspace| {
        workspace.run = Some(run_page(None));
        workspace.artifact = Some(artifact());
    });
    assert_eq!(
        press(&artifact_page, '-'),
        Some(Resolved::Act(compose(ActionsCommand::DeleteArtifact)))
    );
    for (key, command) in [
        ('X', ActionsCommand::ExportArtifact),
        ('U', ActionsCommand::ExtractArtifact),
    ] {
        match press(&artifact_page, key) {
            Some(Resolved::Explain(explanation)) => assert!(
                explanation.starts_with(command.label())
                    && explanation.contains("download its archive first"),
                "{key}: {explanation}"
            ),
            other => panic!("{key} must explain {command:?}, got {other:?}"),
        }
    }
    let uncertain = actions_with(ActionsView::Runs, |workspace| {
        workspace.reconciliation_target = Some(ActionsTarget::Run {
            repository: "example/repo".into(),
            id: 123,
        })
    });
    assert_eq!(
        press(&uncertain, 'i'),
        Some(Resolved::Act(Action::Actions(ActionsAction::Reconcile)))
    );
    // Every new key has exactly one bearer on its page.
    for (app, keys) in [
        (&run, &["J", "!", "-", "L", "Y"][..]),
        (&artifact_page, &["X", "U", "-"][..]),
        (&uncertain, &["i"][..]),
    ] {
        let controls = footer::main_controls(app);
        for key in keys {
            let bearers = controls
                .iter()
                .filter(|control| control.key == *key)
                .count();
            assert_eq!(bearers, 1, "{key} has {bearers} bearers");
        }
    }
}

// ---- Overlays ----------------------------------------------------------------------------

const OVERLAY_SIZES: [(u16, u16); 2] = [(120, 40), (80, 24)];

fn paint(app: &mut App, width: u16, height: u16) -> Vec<Vec<String>> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ghtui::ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect()
        })
        .collect()
}

/// The box (left, top, right, bottom; borders included) whose top border carries `title`.
fn overlay_box(screen: &[Vec<String>], title: &str) -> Option<(u16, u16, u16, u16)> {
    let title = title.chars().map(String::from).collect::<Vec<_>>();
    for (y, row) in screen.iter().enumerate() {
        let Some(x) = (0..row.len().saturating_sub(title.len()))
            .find(|x| row[*x..*x + title.len()] == title[..])
        else {
            continue;
        };
        let left = (0..x)
            .rev()
            .find(|i| matches!(row[*i].as_str(), "╭" | "┌"))?;
        let right = (x..row.len()).find(|i| matches!(row[*i].as_str(), "╮" | "┐"))?;
        let bottom =
            (y + 1..screen.len()).find(|j| matches!(screen[*j][left].as_str(), "╰" | "└"))?;
        return Some((left as u16, y as u16, right as u16, bottom as u16));
    }
    None
}

type Bounds = (u16, u16, u16, u16);

/// Bracketed tokens painted inside the box: (x, y, text between the brackets).
fn bracketed(
    screen: &[Vec<String>],
    (left, top, right, bottom): Bounds,
) -> Vec<(u16, u16, String)> {
    let mut tokens = Vec::new();
    for y in top + 1..bottom {
        let row = &screen[usize::from(y)];
        let mut x = left + 1;
        while x < right {
            if row[usize::from(x)] == "[" {
                if let Some(close) = (x + 1..right).find(|i| row[usize::from(*i)] == "]") {
                    let text = row[usize::from(x + 1)..usize::from(close)].concat();
                    if !text.contains('[') && !text.trim().is_empty() {
                        tokens.push((x, y, text));
                    }
                    x = close;
                }
            }
            x += 1;
        }
    }
    tokens
}

/// What a control can change in an overlay state; focus is left out because a click also
/// moves it to the clicked control.
fn signature(app: &App) -> String {
    format!(
        "{:?} {:?} notice {} help {} local {} pr {} theme {} {:?} save {} status {:?}",
        app.overlay,
        app.view,
        app.notice_scroll,
        app.context_help_scroll,
        app.local_action_index,
        app.pr_action_index,
        app.settings_theme,
        app.density,
        app.settings_save_requested,
        app.status
    )
}

fn click_at(app: &mut App, x: u16, y: u16) {
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }));
}

fn press_key(app: &mut App, event: KeyEvent) {
    app.handle_event(Event::Key(event));
}

fn plain(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

type MakeApp<'a> = Box<dyn Fn() -> App + 'a>;

struct OverlayState<'a> {
    name: &'static str,
    title: &'static str,
    /// Menu overlays list rows chosen by `j`/`k` (or arrows) and `Enter`, or a click.
    menu: bool,
    make: MakeApp<'a>,
}

fn overlay_states(fixture: &git_support::Fixture) -> Vec<OverlayState<'_>> {
    vec![
        OverlayState {
            name: "Help",
            title: "Help • keyboard and mouse",
            menu: false,
            make: Box::new(|| {
                let mut app = inbox(3);
                app.dispatch(Action::Help);
                app
            }),
        },
        OverlayState {
            name: "Explanation",
            title: "Action unavailable • explanation only",
            menu: false,
            make: Box::new(|| {
                let mut app = inbox(3);
                app.notice = (1..=40)
                    .map(|line| format!("Explanation line {line}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                app.overlay = Some(Overlay::Notice);
                app
            }),
        },
        OverlayState {
            name: "Settings",
            title: "Settings • local preferences",
            menu: false,
            make: Box::new(|| {
                let mut app = empty(View::Inbox);
                app.dispatch(Action::Settings);
                app
            }),
        },
        OverlayState {
            name: "Local operations",
            title: "Local operations • explicit preview and confirmation",
            menu: true,
            make: Box::new(move || {
                let mut app = empty(View::Repository);
                app.workspace = Some(
                    Workspace::load(&fixture.root, RepositoryMode::Working, "HEAD", None)
                        .unwrap()
                        .unwrap(),
                );
                app.overlay = Some(Overlay::LocalActions);
                app
            }),
        },
        OverlayState {
            name: "PR actions",
            title: "PR actions • native workflows",
            menu: true,
            make: Box::new(|| {
                let mut app = detail(PrTab::Summary);
                app.overlay = Some(Overlay::PrActions);
                app
            }),
        },
    ]
}

/// Every bracketed control painted in each main overlay is a click target, and its key (or,
/// for a keyless control, Tab focus and Enter) has the same effect as the click. Menu rows are
/// click targets in menu order, and `j` moves the selection to the same row Enter chooses.
/// The document lists every probed control. Commands is probed entry by entry above.
#[test]
fn coverage_v64_main_overlays_controls_have_equal_key_and_click_paths_and_document_rows() {
    let fixture = repository_fixture();
    let doc = document();
    let documented = table_rows(&section(&doc, "## Overlay controls"))
        .into_iter()
        .map(|cells| {
            assert_eq!(cells.len(), 4, "overlay row needs 4 cells: {cells:?}");
            (cells[0].clone(), cells[1].clone())
        })
        .collect::<BTreeSet<_>>();
    let mut failures = Vec::new();
    let mut probed = BTreeSet::new();
    let mut checked = 0;
    for state in overlay_states(&fixture) {
        for (width, height) in OVERLAY_SIZES {
            let name = format!("{} at {width}x{height}", state.name);
            let mut app = (state.make)();
            let screen = paint(&mut app, width, height);
            let Some(bounds) = overlay_box(&screen, state.title) else {
                failures.push(format!("{name}: no overlay titled {:?}", state.title));
                continue;
            };
            let baseline = signature(&app);
            let fresh = || {
                let mut app = (state.make)();
                paint(&mut app, width, height);
                app
            };
            for (x, y, text) in bracketed(&screen, bounds) {
                let control = format!("[{text}]");
                let key = text
                    .split_once(' ')
                    .filter(|(_, label)| !label.trim().is_empty())
                    .and_then(|(key, _)| event(key));
                let Some(hit) = app
                    .hits
                    .iter()
                    .rev()
                    .find(|hit| hit.rect.contains((x, y).into()))
                else {
                    // Bracketed prose is not a control; a keyed button without a hit is a gap.
                    if key.is_some() {
                        failures.push(format!("{name}: {control} has no click target"));
                    }
                    continue;
                };
                if hit.rect.x != x {
                    // A note inside a larger control (a menu row's `[disabled: …]`).
                    continue;
                }
                checked += 1;
                // A value control (`[Theme: Dark]`) is documented by its name.
                let documented_as = match text.split_once(": ") {
                    Some((setting, _)) => format!("[{setting}: …]"),
                    None => control.clone(),
                };
                probed.insert((state.name.to_string(), documented_as));
                let mut clicked = fresh();
                click_at(&mut clicked, x, y);
                let after = signature(&clicked);
                if after == baseline && !text.starts_with("PgUp") {
                    failures.push(format!("{name}: clicking {control} changes nothing"));
                }
                match key {
                    Some(event) => {
                        let mut pressed = fresh();
                        press_key(&mut pressed, event);
                        if signature(&pressed) != after {
                            failures.push(format!(
                                "{name}: {control} key gives {} but its click gives {after}",
                                signature(&pressed)
                            ));
                        }
                    }
                    None => {
                        let reached = (0..8).any(|tabs| {
                            let mut focused = fresh();
                            for _ in 0..tabs {
                                press_key(&mut focused, plain(KeyCode::Tab));
                            }
                            press_key(&mut focused, plain(KeyCode::Enter));
                            signature(&focused) == after
                        });
                        if !reached {
                            failures.push(format!(
                                "{name}: no Tab focus and Enter does what clicking {control} does"
                            ));
                        }
                    }
                }
            }
            if !state.menu {
                continue;
            }
            // Menu rows: full inner-width hits, in menu order from the first row.
            let (left, top, right, bottom) = bounds;
            let rows = app
                .hits
                .iter()
                .filter(|hit| {
                    hit.rect.x == left + 2
                        && hit.rect.width + 4 == right + 1 - left
                        && (top + 1..bottom).contains(&hit.rect.y)
                })
                .map(|hit| (hit.rect, hit.action.clone()))
                .collect::<Vec<_>>();
            let expected = if state.name == "PR actions" {
                app.pr_controls()
            } else {
                LocalCommand::ALL
                    .iter()
                    .map(|(command, label)| (label.to_string(), Action::LocalCommand(*command)))
                    .collect()
            };
            if rows.len() < 3 {
                failures.push(format!(
                    "{name}: only {} menu rows are click targets",
                    rows.len()
                ));
            }
            for (offset, (rect, action)) in rows.iter().enumerate() {
                checked += 1;
                let Some((label, wanted)) = expected.get(offset) else {
                    failures.push(format!("{name}: row {offset} has no menu entry"));
                    continue;
                };
                let painted = screen[usize::from(rect.y)].concat();
                if action != wanted || !painted.contains(label.as_str()) {
                    failures.push(format!(
                        "{name}: row {offset} paints {painted:?} and clicks {action:?}, not {label}"
                    ));
                }
                // Keyboard: `j` moves the `>` selection to the same row; Enter chooses it.
                let mut moved = fresh();
                for _ in 0..offset {
                    press_key(&mut moved, plain(KeyCode::Char('j')));
                }
                let screen = paint(&mut moved, width, height);
                if screen[usize::from(rect.y)][usize::from(rect.x)] != ">" {
                    failures.push(format!("{name}: {offset} × j does not select row {offset}"));
                }
            }
            probed.insert((state.name.to_string(), "Menu rows".to_string()));
        }
    }
    let missing = probed
        .iter()
        .filter(|row| !documented.contains(*row))
        .map(|(overlay, control)| format!("| {overlay} | {control} | | |"))
        .collect::<Vec<_>>();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        missing.is_empty(),
        "{DOC} `## Overlay controls` lacks:\n{}",
        missing.join("\n")
    );
    assert!(checked > 40, "{checked}");
}
