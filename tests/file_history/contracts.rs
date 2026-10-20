//! Real Git and prepared-rendering contracts for native immutable-file provenance.
#[path = "../git_support/mod.rs"]
mod git_support;
use ghtui::{
    git::{Comparison, GitRepository, RawPath},
    providers::runner::CancellationToken,
    workspace::{FileAction, ProvenanceView, RepositoryMode, RowAction, Workspace},
};
use git_support::{git, Fixture};
use std::fs;
fn readonly_status(directory: &std::path::Path) -> Vec<u8> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(["status", "--porcelain=v2", "-z"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(output.status.success());
    output.stdout
}
fn open(fixture: &Fixture, revision: &str, path: &RawPath) -> Workspace {
    Workspace::open_search_file(
        &fixture.root,
        path,
        Some(revision),
        1,
        CancellationToken::default(),
    )
    .unwrap()
}
fn action(workspace: &mut Workspace, action: FileAction) {
    workspace.open(RowAction::FileProvenance(action)).unwrap();
}
fn assert_untouched(fixture: &Fixture, head: &str, index: &[u8], status: &[u8], dirty: &[u8]) {
    assert_eq!(fixture.head(), head);
    assert_eq!(fs::read(fixture.root.join(".git/index")).unwrap(), index);
    assert_eq!(readonly_status(&fixture.root), status);
    assert_eq!(fs::read(fixture.root.join("renamed.txt")).unwrap(), dirty);
}

#[test]
fn file_history_chosen_revision_pages_across_rename_keep_raw_historical_paths_and_actors() {
    let fixture = Fixture::linear();
    let root = String::from_utf8(git(&fixture.root, ["rev-parse", "HEAD~1"]).stdout)
        .unwrap()
        .trim()
        .to_owned();
    git(&fixture.root, ["config", "user.name", "Actual Integrator"]);
    git(
        &fixture.root,
        ["config", "user.email", "integrator@example.invalid"],
    );
    git(&fixture.root, ["mv", "readme.txt", "renamed.txt"]);
    let chosen = fixture.commit("Renamed saved file");
    fixture.write("renamed.txt", b"first\nnew future edit\n");
    fixture.commit("Future not in chosen history");
    let head = fixture.head();
    let dirty = b"user dirty checkout bytes\n";
    fixture.write("renamed.txt", dirty);
    let index = fs::read(fixture.root.join(".git/index")).unwrap();
    let status = readonly_status(&fixture.root);
    let path = RawPath::new(b"renamed.txt".to_vec());
    let first = fixture
        .repository
        .file_history_page(&chosen, &path, 0, 1)
        .unwrap();
    assert_eq!(first.entries.len(), 1);
    assert!(first.has_more);
    assert_eq!(first.entries[0].commit.id, chosen);
    assert_eq!(first.entries[0].path, Some(path.clone()));
    assert_eq!(
        first.entries[0].previous_path,
        Some(RawPath::new(b"readme.txt".to_vec()))
    );
    let second = fixture
        .repository
        .file_history_page(&chosen, &path, 1, 1)
        .unwrap();
    assert_eq!(second.entries.len(), 1);
    assert!(second.has_more);
    assert_eq!(second.entries[0].path.as_ref().unwrap().0, b"readme.txt");
    assert_eq!(second.entries[0].commit.author.name, "Fixture Author");
    assert_eq!(second.entries[0].commit.committer.name, "Fixture Author");
    let third = fixture
        .repository
        .file_history_page(&chosen, &path, 2, 1)
        .unwrap();
    assert_eq!(third.entries[0].commit.id, root);
    assert!(!third.has_more);
    assert!(fixture
        .repository
        .file_history_page(&chosen, &path, 0, 0)
        .is_err());
    assert!(fixture
        .repository
        .file_history_page(&chosen, &path, 10_000, 1)
        .is_err());
    assert_untouched(&fixture, &head, &index, &status, dirty);
}

#[test]
fn file_history_blame_range_retains_exact_chosen_lines_original_rename_path_and_boundary() {
    let fixture = Fixture::linear();
    let responsible = fixture.head();
    git(&fixture.root, ["mv", "readme.txt", "renamed.txt"]);
    let chosen = fixture.commit("Rename only");
    fixture.write(
        "renamed.txt",
        b"different current line\nlatest second line\n",
    );
    fixture.commit("Future outside chosen blame");
    let path = RawPath::new(b"renamed.txt".to_vec());
    let first = fixture
        .repository
        .blame_lines(&chosen, &path, 1, 1)
        .unwrap();
    assert_eq!(first[0].line.text, b"first");
    assert!(first[0].boundary);
    assert_eq!(first[0].line.final_line, 1);
    assert_eq!(first[0].line.original_line, 1);
    assert_eq!(first[0].line.path.0, b"readme.txt");
    let second = fixture
        .repository
        .blame_lines(&chosen, &path, 2, 1)
        .unwrap();
    assert_eq!(second[0].line.commit, responsible);
    assert_eq!(second[0].line.text, b"changed");
    assert_eq!(second[0].line.original_line, 2);
    assert_eq!(second[0].line.final_line, 2);
    assert!(!second[0].ignored && !second[0].unblamable);
    assert!(fixture
        .repository
        .blame_lines(&chosen, &path, 0, 1)
        .is_err());
    assert!(fixture
        .repository
        .blame_lines(&chosen, &path, 1, 1001)
        .is_err());
}

#[test]
fn file_history_workspace_blame_graph_introduced_diff_original_file_and_back_preserve_origin() {
    let fixture = Fixture::linear();
    let responsible = fixture.head();
    git(&fixture.root, ["mv", "readme.txt", "renamed.txt"]);
    let chosen = fixture.commit("Rename source");
    let path = RawPath::new(b"renamed.txt".to_vec());
    let head = fixture.head();
    let dirty = b"dirty unrelated to selected snapshot\n";
    fixture.write("renamed.txt", dirty);
    let index = fs::read(fixture.root.join(".git/index")).unwrap();
    let status = readonly_status(&fixture.root);
    let mut workspace = open(&fixture, &chosen, &path);
    action(&mut workspace, FileAction::Blame);
    action(&mut workspace, FileAction::BlameLine(2));
    assert_eq!(workspace.selected_commit.as_ref().unwrap().id, responsible);
    assert!(workspace
        .rows
        .iter()
        .any(|row| row.label == "Original path: readme.txt"));
    assert_eq!(workspace.file_provenance.as_ref().unwrap().revision, chosen);
    action(&mut workspace, FileAction::Graph);
    assert_eq!(workspace.mode, RepositoryMode::Graph);
    assert_eq!(workspace.revision, responsible);
    assert_eq!(
        workspace
            .graph_state_v1
            .as_ref()
            .unwrap()
            .selected_commit(&responsible)
            .unwrap()
            .id,
        responsible
    );
    let focus = workspace.graph_focus_row_v1().unwrap();
    assert_eq!(
        workspace.display_rows_v1()[focus].action,
        Some(RowAction::Commit(responsible.clone()))
    );
    // History is its own view (TASK-99): it carries no file source, so global Back (not
    // file Back) returns to the blame; reopen it here.
    assert!(workspace.file_provenance.is_none());
    workspace = open(&fixture, &chosen, &path);
    action(&mut workspace, FileAction::Blame);
    assert!(workspace.rows[0].label.starts_with("Line blame"));
    action(&mut workspace, FileAction::BlameLine(2));
    action(&mut workspace, FileAction::Diff);
    assert_eq!(workspace.mode, RepositoryMode::CommitChanges);
    assert!(
        matches!(workspace.comparison.as_ref().unwrap().comparison,Comparison::CommitChanges {ref commit,parent:0} if commit==&responsible)
    );
    assert!(workspace
        .rows
        .iter()
        .any(|row| row.label.contains("+changed")));
    action(&mut workspace, FileAction::Back);
    action(&mut workspace, FileAction::BlameLine(2));
    action(&mut workspace, FileAction::OriginalFile);
    assert_eq!(workspace.preview.as_ref().unwrap().path.0, b"readme.txt");
    assert_eq!(
        workspace.preview.as_ref().unwrap().content,
        b"first\nchanged\n"
    );
    action(&mut workspace, FileAction::Back);
    assert_eq!(workspace.revision, chosen);
    assert_eq!(
        workspace.file_provenance.as_ref().unwrap().view,
        ProvenanceView::Blame
    );
    assert!(workspace
        .rows
        .iter()
        .any(|row| row.label.contains("Related PR: unavailable")));
    assert_untouched(&fixture, &head, &index, &status, dirty);
}

#[test]
fn file_history_merge_parent_roles_and_default_copy_move_limits_are_explicit() {
    let fixture = Fixture::linear();
    git(&fixture.root, ["switch", "-c", "feature"]);
    git(&fixture.root, ["config", "user.name", "Distinct Committer"]);
    git(
        &fixture.root,
        ["config", "user.email", "committer@example.invalid"],
    );
    fixture.write("readme.txt", b"first\nfeature line\n");
    git(&fixture.root, ["add", "readme.txt"]);
    git(
        &fixture.root,
        [
            "commit",
            "--author=Feature Author <author@example.invalid>",
            "-m",
            "Feature text provenance",
        ],
    );
    let responsible = fixture.head();
    git(&fixture.root, ["switch", "main"]);
    fixture.write("other.txt", b"main only\n");
    fixture.commit("Main branch edit");
    git(
        &fixture.root,
        [
            "merge",
            "--no-ff",
            "feature",
            "-m",
            "Two parent integration",
        ],
    );
    let merge = fixture.head();
    let mut workspace = open(&fixture, &merge, &RawPath::new(b"readme.txt".to_vec()));
    action(&mut workspace, FileAction::Blame);
    action(&mut workspace, FileAction::BlameLine(2));
    let commit = workspace.selected_commit.as_ref().unwrap();
    assert_eq!(commit.id, responsible);
    assert_eq!(commit.author.name, "Feature Author");
    assert_eq!(commit.committer.name, "Distinct Committer");
    action(&mut workspace, FileAction::Back);
    assert!(workspace.rows.iter().any(|row| row
        .label
        .contains("moved/copied line detection (-M/-C) is off")));
    // File history is History with a path chip across both merge parents (TASK-99).
    action(&mut workspace, FileAction::PathHistory);
    assert_eq!(workspace.mode, RepositoryMode::Graph);
    assert!(workspace.file_provenance.is_none());
    assert_eq!(
        workspace.history_filter_v993.spec.path,
        Some(RawPath::new(b"readme.txt".to_vec()))
    );
    // The selection is the file's latest change: the feature commit (or the merge, when Git
    // lists the merge itself for the path).
    assert!(
        workspace.revision == responsible || workspace.revision == merge,
        "{}",
        workspace.revision
    );
    assert_eq!(fixture.head(), merge);
}

#[test]
fn file_history_binary_empty_lfs_and_missing_file_show_limits_without_invented_lines() {
    let fixture = Fixture::unborn();
    fixture.write("binary", b"binary\0payload");
    fixture.write("empty", b"");
    fixture.write("pointer",b"version https://git-lfs.github.com/spec/v1\noid sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nsize 5000\n");
    let head = fixture.commit("Special saved objects");
    for (name, expected) in [
        ("binary", "binary/LFS"),
        ("empty", "Empty saved text"),
        ("pointer", "binary/LFS"),
    ] {
        let mut workspace = open(&fixture, &head, &RawPath::new(name.as_bytes().to_vec()));
        action(&mut workspace, FileAction::Blame);
        assert!(workspace
            .rows
            .iter()
            .any(|row| row.label.contains(expected)));
        assert!(workspace.file_provenance.as_ref().unwrap().blame.is_empty());
    }
    assert!(Workspace::open_search_file(
        &fixture.root,
        &RawPath::new(b"missing".to_vec()),
        Some(&head),
        1,
        CancellationToken::default()
    )
    .is_err());
    assert_eq!(fixture.head(), head);
}

#[test]
fn file_history_shallow_boundaries_and_missing_parent_diff_are_honest_without_checkout() {
    let fixture = Fixture::linear();
    let directory = tempfile::Builder::new()
        .prefix("ghtui-history-shallow-")
        .tempdir()
        .unwrap();
    let target = directory.path().join("shallow");
    git(
        directory.path(),
        [
            "-c",
            "protocol.file.allow=always",
            "clone",
            "--no-local",
            "--depth=1",
            &format!("file://{}", fixture.root.display()),
            target.to_str().unwrap(),
        ],
    );
    let repository = GitRepository::discover(&target).unwrap().unwrap();
    let head = repository.resolve_original("HEAD").unwrap();
    let index = fs::read(target.join(".git/index")).unwrap();
    let mut workspace = Workspace::open_search_file(
        &target,
        &RawPath::new(b"readme.txt".to_vec()),
        Some(&head),
        1,
        CancellationToken::default(),
    )
    .unwrap();
    action(&mut workspace, FileAction::Blame);
    assert!(workspace
        .rows
        .iter()
        .any(|row| row.label.starts_with("SHALLOW:")));
    assert!(workspace
        .file_provenance
        .as_ref()
        .unwrap()
        .blame
        .iter()
        .all(|line| line.boundary));
    action(&mut workspace, FileAction::BlameLine(1));
    assert!(workspace
        .open(RowAction::FileProvenance(FileAction::Diff))
        .is_err());
    assert_eq!(repository.resolve_original("HEAD").unwrap(), head);
    assert_eq!(fs::read(target.join(".git/index")).unwrap(), index);
}

#[test]
fn file_history_machine_headers_and_quoted_non_utf8_rename_paths_are_not_display_aliases() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let fixture = Fixture::unborn();
    let old = b"GHTUI_FILE_HISTORY";
    let new = b"odd\tline\n\xff";
    fixture.index_blob(old, b"literal machine marker path\n");
    let original = fixture.commit_index("Original raw object path");
    git(
        &fixture.root,
        [
            OsStr::new("update-index"),
            OsStr::new("--force-remove"),
            OsStr::from_bytes(old),
        ],
    );
    fixture.index_blob(new, b"literal machine marker path\n");
    let chosen = fixture.commit_index("Rename exact raw object bytes");
    let path = RawPath::new(new.to_vec());
    let page = fixture
        .repository
        .file_history_page(&chosen, &path, 0, 10)
        .unwrap();
    assert_eq!(page.entries.len(), 2);
    assert_eq!(page.entries[0].path, Some(path.clone()));
    assert_eq!(
        page.entries[0].previous_path,
        Some(RawPath::new(old.to_vec()))
    );
    assert_eq!(page.entries[1].commit.id, original);
    assert_eq!(page.entries[1].path, Some(RawPath::new(old.to_vec())));
    let blame = fixture
        .repository
        .blame_lines(&chosen, &path, 1, 1)
        .unwrap();
    assert_eq!(blame[0].line.commit, original);
    assert_eq!(blame[0].line.path.0, old);
    assert_eq!(blame[0].line.text, b"literal machine marker path");
    assert_eq!(path.display(), "odd\\tline\\n\\xff");
}

#[test]
fn file_history_prepared_controls_narrow_hits_disabled_busy_input_parity_and_roles() {
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ghtui::{
        app::{Action, App, Overlay, View},
        theme::ColorMode,
    };
    use ratatui::{backend::TestBackend, Terminal};
    let fixture = Fixture::linear();
    let mut workspace = open(
        &fixture,
        &fixture.head(),
        &RawPath::new(b"readme.txt".to_vec()),
    );
    action(&mut workspace, FileAction::Blame);
    let make_app = |busy| {
        let mut app = App::new();
        app.view = View::Repository;
        app.workspace = Some(workspace.clone());
        app.repository_mode = RepositoryMode::Tree;
        app.color_mode = ColorMode::TrueColor;
        app.no_color = false;
        app.repository_content_focused = true;
        app.loading[View::Repository.index()] = busy;
        app
    };
    let provenance =
        |file_action: FileAction| Action::RepositoryRow(RowAction::FileProvenance(file_action));
    for busy in [false, true] {
        let mut keyboard = make_app(busy);
        let mut mouse = make_app(busy);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| ghtui::ui::draw(frame, &mut mouse))
            .unwrap();
        // The key bar is the last row; its tokens are the file-history click targets.
        let bar = (0..80)
            .map(|column| terminal.backend().buffer()[(column, 23)].symbol())
            .collect::<String>();
        let hits = mouse.hits.clone();
        let bar_hit = |file_action: FileAction| {
            hits.iter()
                .find(|hit| hit.rect.y == 23 && hit.action == provenance(file_action.clone()))
                .map(|hit| hit.rect)
        };
        assert!(
            bar.trim_end().ends_with("q Quit"),
            "The Global tail fits 80 columns: {bar}"
        );
        assert!(mouse.hits.iter().all(|hit| hit.rect.right() <= 80));
        // Disabled controls leave the bar; the key still answers with the reason.
        let disabled = if busy {
            vec![
                FileAction::PathHistory,
                FileAction::Blame,
                FileAction::Previous,
                FileAction::Next,
            ]
        } else {
            // Two lines fit one blame page: neither an earlier nor a later page exists.
            vec![FileAction::Previous, FileAction::Next]
        };
        for file_action in &disabled {
            assert_eq!(
                bar_hit(file_action.clone()),
                None,
                "A disabled control is not a dead click target: {file_action:?}: {bar}"
            );
        }
        if busy {
            assert!(bar.contains("Esc Cancel"), "{bar}");
        } else {
            for (file_action, token) in [
                (FileAction::PathHistory, "f History"),
                (FileAction::Blame, "Y Blame"),
            ] {
                let rect = bar_hit(file_action)
                    .unwrap_or_else(|| panic!("{token} is a key-bar click target: {bar}"));
                let painted = (rect.x..rect.right())
                    .map(|column| terminal.backend().buffer()[(column, 23)].symbol())
                    .collect::<String>();
                assert_eq!(painted, token, "Full control must fit 80 columns: {bar}");
            }
            assert!(!bar.contains("P Prev"), "{bar}");
            assert!(!bar.contains("N Next"), "{bar}");
            let next = ghtui::footer::resolve_key(
                &ghtui::footer::main_controls(&mouse),
                KeyEvent::new(KeyCode::Char('N'), KeyModifiers::NONE),
            );
            assert!(
                matches!(&next, Some(Action::Explain(reason)) if reason.contains("No next page")),
                "{next:?}"
            );
        }
        let key = if busy { 'f' } else { 'P' };
        let event = KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE);
        let explanation = ghtui::footer::resolve_key(&ghtui::footer::main_controls(&mouse), event)
            .expect("A disabled key still resolves");
        assert!(
            matches!(explanation, Action::Explain(_)),
            "Disabled keys use the shared explanation"
        );
        // Pointer users reach the same explanation through Commands, which dispatches it.
        mouse.dispatch(explanation);
        keyboard.handle_event(Event::Key(event));
        assert_eq!(keyboard.overlay, Some(Overlay::Notice));
        assert_eq!(mouse.overlay, keyboard.overlay);
        assert_eq!(mouse.notice, keyboard.notice);
        assert!(keyboard.repository_action.is_none());
        assert!(mouse.repository_action.is_none());
        if !busy {
            assert!(
                keyboard.notice.contains("First source page"),
                "{}",
                keyboard.notice
            );
            // The current Blame mode is the selected token: bold primary text, not muted.
            let rect = bar_hit(FileAction::Blame).unwrap();
            let label = &terminal.backend().buffer()[(rect.x + 2, 23)];
            assert!(label.modifier.contains(ratatui::style::Modifier::BOLD));
            let history = bar_hit(FileAction::PathHistory).unwrap();
            assert!(!terminal.backend().buffer()[(history.x + 2, 23)]
                .modifier
                .contains(ratatui::style::Modifier::BOLD));
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(text.contains("author Fixture Author"));
            assert!(text.contains("committer Fixture Author"));
            assert!(text.contains("Related PR: unavailable"));
        }
    }
    let mut keyboard = make_app(false);
    let mut mouse = make_app(false);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| ghtui::ui::draw(frame, &mut mouse))
        .unwrap();
    let expected = Action::RepositoryRow(RowAction::FileProvenance(FileAction::Blame));
    let rect = mouse
        .hits
        .iter()
        .find(|hit| hit.rect.y == 23 && hit.action == expected)
        .unwrap()
        .rect;
    mouse.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }));
    keyboard.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('Y'),
        KeyModifiers::NONE,
    )));
    assert_eq!(
        mouse.repository_action,
        Some(RowAction::FileProvenance(FileAction::Blame))
    );
    assert_eq!(keyboard.repository_action, mouse.repository_action);
    assert_eq!(mouse.overlay, None);
    assert_eq!(keyboard.overlay, None);
}
