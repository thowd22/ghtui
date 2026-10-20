//! TASK-99.3: `f` on a changed file in PR Files opens History in the local checkout with that
//! file's `path:` chip, following its rename ("was src/old_view.rs"), with no GitHub request
//! and no Git write. A PR of another repository explains why instead. Keyboard and mouse
//! (Commands), 140x42 then 80x24.
use super::*;
use crate::support::history_fixture::{selected_history_row, HistoryRow};

/// HEAD, refs, the index, config and status: what File history must leave alone.
fn git_state(fixture: &Fixture) -> Vec<String> {
    vec![
        fs::read_to_string(fixture.repo().join(".git/HEAD")).unwrap(),
        fixture.git(&["for-each-ref"]),
        format!("{:?}", fs::read(fixture.repo().join(".git/index")).unwrap()),
        fs::read_to_string(fixture.repo().join(".git/config")).unwrap(),
        fixture.git(&["status", "--porcelain=v2", "--branch"]),
    ]
}

/// A checkout whose `src/view.rs` began as `src/old_view.rs`, with its GitHub remote.
fn checkout(fixture: &Fixture, remote: &str) {
    fixture.git(&["init", "-q", "--initial-branch=main"]);
    fixture.git(&["config", "user.name", "History PTY"]);
    fixture.git(&["config", "user.email", "history@example.invalid"]);
    fs::create_dir_all(fixture.repo().join("src")).unwrap();
    fs::write(
        fixture.repo().join("src/old_view.rs"),
        b"pub fn view() {}\n",
    )
    .unwrap();
    fixture.git(&["add", "-A"]);
    fixture.git(&["commit", "-qm", "Add the old view"]);
    fixture.git(&["mv", "src/old_view.rs", "src/view.rs"]);
    fixture.git(&["commit", "-qm", "Rename the view"]);
    fs::write(
        fixture.repo().join("src/view.rs"),
        b"pub fn view() {}\npub fn tuned() {}\n",
    )
    .unwrap();
    fixture.git(&["commit", "-qam", "Tune the view"]);
    fixture.git(&[
        "remote",
        "add",
        "origin",
        &format!("https://github.invalid/{remote}.git"),
    ]);
}

fn open_files(session: &mut Session, fixture: &Fixture, mouse: bool, steps: usize) {
    session.wait_for_screen("ghtui");
    refresh(session, mouse);
    session.wait_for_screen("1 unique PRs across 3 queue matches");
    let before = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("Actual personal source");
    } else {
        session.send_key(b"\r");
    }
    frame(session, before, |s| {
        crate::support::controls::text_has_control(s, "[f Files]") && s.contains(BODY)
    });
    let before = session.completed_cursorless_end();
    choose(session, mouse, "[f Files]", b"f");
    frame(session, before, |s| {
        s.contains("Personal PR Files") && s.contains("File src/view.rs")
    });
    journal(session, fixture, steps);
    session.select_completed_progress_file(mouse, "File src/view.rs");
}

/// `f` by key, or the File history command by mouse.
fn file_history(session: &mut Session, mouse: bool) {
    if mouse {
        session.invoke_via_palette(true, "File history");
    } else {
        session.send_key(b"f");
    }
}

fn row<'a>(rows: &'a [HistoryRow], subject: &str) -> &'a HistoryRow {
    rows.iter()
        .find(|row| row.message.contains(subject))
        .unwrap_or_else(|| panic!("no History row for {subject}: {rows:?}"))
}

fn journey(mouse: bool) {
    let fixture = configured();
    checkout(&fixture, REPO);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let (steps, _) = complete_steps();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 140, 42);
    open_files(&mut session, &fixture, mouse, steps.len());
    file_history(&mut session, mouse);
    session.wait_for_cursorless_condition(
        |screen| {
            crate::support::history_fixture::history_ready(screen)
                && crate::support::history_fixture::history_filter_row(screen).is_some_and(|row| {
                    row.matches == Some(3)
                        && row
                            .removable
                            .iter()
                            .any(|(chip, _)| chip == "path: src/view.rs")
                })
        },
        "History of src/view.rs with its path chip",
        Duration::from_secs(10),
    );
    // Every commit of the file, across the rename: the old name leads its row.
    let rows = session.history_rows();
    assert_eq!(rows.len(), 3, "{rows:?}");
    let short = |subject: &str| {
        fixture
            .git(&[
                "log",
                "-1",
                "--format=%h",
                "--abbrev=7",
                &format!("--grep={subject}"),
            ])
            .trim()
            .to_owned()
    };
    assert!(row(&rows, "Add the old view")
        .message
        .contains("was src/old_view.rs"));
    assert!(row(&rows, "Rename the view")
        .message
        .contains("renamed from src/old_view.rs"));
    assert!(!row(&rows, "Tune the view").message.contains("was "));
    // The bar rests on the file's latest change at HEAD.
    let selected = selected_history_row(session.completed_cursorless_screen().unwrap()).unwrap();
    assert_eq!(selected.sha, short("Tune the view"));
    // The detail names the old path of a commit before the rename.
    let old = row(&rows, "Add the old view").clone();
    if mouse {
        session.click(old.sha_x, old.y);
    } else {
        session.send_key(b"n");
        session.send_key(b"n");
    }
    session.wait_for_history_selected(&short("Add the old view"));
    session.wait_for_cursorless_condition(
        |screen| {
            screen
                .contents()
                .lines()
                .any(|line| line.contains("Path") && line.contains("was src/old_view.rs"))
        },
        "the detail's Path row",
        Duration::from_secs(8),
    );
    // 80x24: the same rows; the old name still leads the selected row.
    session.resize(80, 24);
    session.wait_for_history_ready();
    let narrow = session.history_rows();
    assert!(
        row(&narrow, "was src/old_view.rs").selected,
        "{narrow:?}\n{}",
        session.screen()
    );
    // Reads only: no GitHub request, no Git or config write.
    journal(&mut session, &fixture, steps.len());
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(git_state(&fixture), before);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
}

/// A PR of another repository: `f` (and its command) explains, and nothing opens.
fn other_repository(mouse: bool) {
    let fixture = configured();
    checkout(&fixture, "example/elsewhere");
    let before = git_state(&fixture);
    let (steps, _) = complete_steps();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 80, 24);
    open_files(&mut session, &fixture, mouse, steps.len());
    file_history(&mut session, mouse);
    session.wait_for_screen("File history needs a local checkout");
    assert!(
        session.screen().contains("example/elsewhere"),
        "{}",
        session.screen()
    );
    session.send_key(b"\x1b");
    session.wait_for_cursorless_condition(
        |screen| {
            let text = screen.contents();
            text.contains("Personal PR Files")
                && !text.contains("File history needs a local checkout")
        },
        "the explanation closed over PR Files",
        Duration::from_secs(8),
    );
    // Quit is clicked on the PR Files key bar, not the closed explanation's.
    session.wait_for_settled_footer();
    journal(&mut session, &fixture, steps.len());
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(git_state(&fixture), before);
}

#[test]
fn history_filter_v993_follow_keyboard_pr_file_opens_local_history_across_its_rename() {
    journey(false)
}
#[test]
fn history_filter_v993_follow_mouse_pr_file_opens_local_history_across_its_rename() {
    journey(true)
}
#[test]
fn history_filter_v993_follow_keyboard_pr_file_of_another_repository_explains() {
    other_repository(false)
}
#[test]
fn history_filter_v993_follow_mouse_pr_file_of_another_repository_explains() {
    other_repository(true)
}
