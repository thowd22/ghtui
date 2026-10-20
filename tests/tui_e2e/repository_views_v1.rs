//! Daily repository browsing: readable graph rows, tree Back to the opened
//! file, and plain diff file headings, with independent Git effect checks.
use crate::support::{Fixture, Session};
use std::{
    fs,
    time::{Duration, Instant},
};

fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}

fn select_row(session: &mut Session, mouse: bool, label: &str) {
    if mouse {
        session.click_visible_action(label);
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if let Some((column, row)) = session.locate(label) {
            if session.cell_inverted(row, column) {
                session.send_key(b"\r");
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "Row never keyboard-selected: {label}\n{}",
            session.screen()
        );
        session.send_key(b"\x1b[B");
        session.poll_output(Duration::from_millis(50));
    }
}

fn wait_selected(session: &mut Session, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(20));
        if let Some((column, row)) = session.locate(label) {
            if session.cell_inverted(row, column) {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "Row never reselected: {label}\n{}",
            session.screen()
        );
    }
}

#[test]
fn v1_repo_views_keyboard_graph_tree_back_and_plain_diff_headings() {
    journey(false);
}

#[test]
fn v1_repo_views_mouse_graph_tree_back_and_plain_diff_headings() {
    journey(true);
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("notes.txt", b"first note\n", "First note");
    fixture.commit_file("src/main.rs", b"fn main() {}\n", "Add the main program");
    fixture.commit_file("notes.txt", b"second note\n", "Second note");
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let seconds = fixture
        .git(&["log", "-1", "--format=%at"])
        .trim()
        .to_owned();
    let date = String::from_utf8(
        std::process::Command::new("date")
            .args(["-u", "-d", &format!("@{seconds}"), "+%F"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    fs::write(fixture.repo().join("src/main.rs"), b"uncommitted edit\n").unwrap();
    let refs = fixture.git(&["show-ref"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2"]);

    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    // One name for a commit authored and committed by the same person, in the AUTHOR column.
    wait_row(
        &mut session,
        &[
            &format!("{}  Second note", &head[..7]),
            "Fixture Author",
            &date,
        ],
    );
    assert!(!session.screen().contains("• committer Fixture Author"));

    choose(&mut session, mouse, "[g History]", b"g");
    session.wait_for_screen("Visible commits 1–3 of 3 loaded (limit 10000)");
    session.wait_for_screen(&format!(
        "▶ selected {} Second note (HEAD, main) • Fixture Author",
        &head[..8]
    ));
    session.wait_for_screen("Add the main program • Fixture Author");
    assert!(
        !session.screen().contains("↳ parent"),
        "linear history uses one row per commit\n{}",
        session.screen()
    );

    choose(&mut session, mouse, "[t Tree]", b"t");
    session.wait_for_screen("Saved file tree");
    session.wait_for_screen("· notes.txt");
    // Folders come first, as in a file manager.
    assert!(
        session.locate("▸ src").unwrap().1 < session.locate("· notes.txt").unwrap().1,
        "{}",
        session.screen()
    );
    select_row(&mut session, mouse, "▸ src");
    session.wait_for_screen("▾ src");
    select_row(&mut session, mouse, "· main.rs");
    session.wait_for_screen("src/main.rs at saved version");
    session.wait_for_screen("fn main() {}");
    session.wait_for_control("[u Back to tree]");
    // Keyboard uses Esc; the mouse clicks the visible Back control.
    choose(&mut session, mouse, "[u Back to tree]", b"\x1b");
    session.wait_for_screen("Saved file tree");
    session.wait_for_screen("▾ src");
    wait_selected(&mut session, "· main.rs");
    assert!(!session.screen().contains("uncommitted edit"));

    choose(&mut session, mouse, "[d Changes]", b"d");
    session.wait_for_screen("Saved commit changes");
    session.wait_for_screen("+second note");
    let screen = session.screen();
    assert!(screen.contains("│notes.txt "), "{screen}");
    assert!(!screen.contains("notes.txt → notes.txt"), "{screen}");
    assert!(!screen.contains("index "), "{screen}");

    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), head);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2"]), status);
    assert_eq!(
        fs::read(fixture.repo().join("src/main.rs")).unwrap(),
        b"uncommitted edit\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

const REPOSITORY_PANE: &str = "Repository • focused view";
/// The Overview's commits panel (TASK-93): its rows carry the selection bar.
const COMMITS_PANE: &str = "Recent commits — saved versions";

/// Waits for one screen line holding every part (a table row read by its cells).
fn wait_row(session: &mut Session, parts: &[&str]) -> String {
    let wanted = parts
        .iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>();
    session
        .try_wait_for_condition(
            |screen| {
                screen
                    .lines()
                    .any(|line| wanted.iter().all(|part| line.contains(part.as_str())))
            },
            &format!("row with {parts:?}"),
            Duration::from_secs(8),
        )
        .unwrap();
    session
        .screen()
        .lines()
        .find(|line| parts.iter().all(|part| line.contains(part)))
        .unwrap()
        .to_owned()
}

#[test]
fn design_table_v91_keyboard_repository_overview_commit_table() {
    overview_table(false);
}

#[test]
fn design_table_v91_mouse_repository_overview_commit_table() {
    overview_table(true);
}

/// TASK-91 on the actual executable: Overview's recent commits are aligned COMMIT, SUBJECT,
/// AUTHOR, AGE and DATE columns with a `▌` bar on the selected row; keyboard and mouse open the
/// same commit, whose details keep the full id; short rows keep every column at 80.
fn overview_table(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("notes.txt", b"first note\n", "First note");
    fixture.commit_file("src/main.rs", b"fn main() {}\n", "Add the main program");
    fixture.commit_file("notes.txt", b"second note\n", "Second note");
    let middle = fixture.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let seconds = fixture
        .git(&["log", "-1", "--format=%at"])
        .trim()
        .to_owned();
    let date = String::from_utf8(
        std::process::Command::new("date")
            .args(["-u", "-d", &format!("@{seconds}"), "+%F"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    fs::write(fixture.repo().join("notes.txt"), b"uncommitted edit\n").unwrap();
    let refs = fixture.git(&["show-ref"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2"]);

    let mut session = Session::launch(&fixture, 140, 42);
    session.wait_for_screen("Recent commits");
    let row = wait_row(
        &mut session,
        &[
            &format!("{}  Add the main program", &middle[..7]),
            "Fixture Author",
            &date,
        ],
    );
    assert!(!row.contains(&middle[..8]), "{row}");
    assert!(!row.contains(" • "), "{row}");
    // The subjects of all three commits start in one column.
    let screen = session.screen();
    let columns = ["First note", "Add the main program", "Second note"]
        .iter()
        .map(|subject| session.locate(subject).expect(subject).0)
        .collect::<Vec<_>>();
    assert!(
        columns.windows(2).all(|pair| pair[0] == pair[1]),
        "{screen}"
    );

    if mouse {
        let (column, row) = session.locate("Add the main program").unwrap();
        session.click(column, row);
    } else {
        let deadline = Instant::now() + Duration::from_secs(8);
        while !session
            .selected_row(COMMITS_PANE)
            .is_some_and(|row| row.contains("Add the main program"))
        {
            assert!(
                Instant::now() < deadline,
                "row never selected\n{}",
                session.screen()
            );
            session.send_key(b"\x1b[B");
            session.poll_output(Duration::from_millis(50));
        }
        let selected = session.wait_for_selected_row(COMMITS_PANE, "Add the main program");
        assert!(selected.contains(&middle[..7]), "{selected}");
        session.send_key(b"\r");
    }
    // The opened commit's detail row keeps the full id.
    session.wait_for_screen(&format!("Commit {middle} — a saved version"));

    choose(&mut session, mouse, "[o Overview]", b"o");
    session.wait_for_screen("Recent commits");
    session.resize(80, 24);
    let row = wait_row(
        &mut session,
        &[&format!(
            "{}  Second note",
            &fixture.git(&["rev-parse", "HEAD"])[..7]
        )],
    );
    // Short subjects leave room for every column at 80; a long one drops DATE first (unit
    // test `design_table_v91_repository_overview_commits_align_and_keep_full_sha_in_details`).
    assert!(row.contains("Fixture Author"), "{row}");
    assert!(row.contains(&date), "{row}");
    assert!(!row.contains('…'), "{row}");

    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2"]), status);
    assert_eq!(
        fs::read(fixture.repo().join("notes.txt")).unwrap(),
        b"uncommitted edit\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

fn last_row(session: &Session) -> String {
    session
        .screen()
        .lines()
        .last()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn design_footer_v90_keyboard_repository_tabs_key_bar_and_disabled_keys() {
    key_bar(false);
}

#[test]
fn design_footer_v90_mouse_repository_tabs_key_bar_and_disabled_keys() {
    key_bar(true);
}

/// TASK-90 on the actual executable: the Repository modes are one tab row, its actions are
/// `key label` tokens in the bottom key bar (compact at 80x24), a disabled key explains itself
/// and a key dropped for width still works. Browsing never changes the repository.
fn key_bar(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("notes.txt", b"first note\nsecond line\n", "First note");
    fs::write(
        fixture.repo().join("notes.txt"),
        b"first note\nchanged line\n",
    )
    .unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2"]);

    let mut session = Session::launch(&fixture, 140, 42);
    session.wait_for_screen("Recent commits");
    session.wait_for_control("[m Local operations]");
    let screen = session.screen();
    // The bracketed toolbar rows above the content are gone.
    for old in ["[o Overview]", "[l Legend]", "[C Compare]", "[J Jump]"] {
        assert!(!screen.contains(old), "{old} still painted\n{screen}");
    }
    // At 140 the preview pane narrows the Repository pane (TASK-93): the tab strip uses its
    // compact spacing and All edits its compact label.
    for tab in ["o Overview  ", " g History ", " t Tree ", " w Working "] {
        assert!(screen.contains(tab), "{tab} tab missing\n{screen}");
    }
    assert!(
        screen.contains(" e All edits") || screen.contains(" e Edits"),
        "All edits tab missing\n{screen}"
    );
    let footer = last_row(&session);
    for token in [
        "+ Stage",
        "K Commit",
        "C Compare",
        "m Local operations",
        "l Legend",
        "a Author",
        "c Committer",
    ] {
        assert!(footer.contains(token), "{token} not in {footer:?}");
    }
    assert!(footer.trim_end().ends_with("? Help  : Commands  q Quit"));

    // Jump needs a saved tree: its token is not painted and its key explains why.
    session.assert_control_absent("[J Jump]");
    session.press_disabled_key(b"J", "Choose an original immutable Tree");
    if mouse {
        session.close_overlay_by_mouse("Action unavailable");
    } else {
        session.close_overlay_by_key("Action unavailable");
    }

    // A mode tab and a key-bar token act like their keys.
    choose(&mut session, mouse, "[w Working]", b"w");
    session.wait_for_screen("+changed line");
    session.wait_for_control("[h Hunk]");
    session.choose_control(mouse, "[m Local operations]");
    session.wait_for_screen("Local operations • explicit preview");
    session.close_overlay_by_key("Local operations • explicit preview");

    // 80x24: compact labels; Compare left the bar but its key still opens the form.
    session.resize_owned_footer(
        80,
        24,
        REPOSITORY_PANE,
        &[
            "[h Hunk]",
            "[m Git ops]",
            "[? Help]",
            "[: Cmds]",
            "[q Quit]",
        ],
    );
    session.assert_control_absent("[C Compare]");
    session.send_key(b"C");
    session.wait_for_screen("Compare two saved versions");
    session.send_key(b"\x1b");
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Compare two saved versions"),
            "comparison form closed",
            Duration::from_secs(8),
        )
        .unwrap();
    choose(&mut session, mouse, "[i Staged]", b"i");
    session.wait_for_screen("Staged for the next commit");

    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2"]), status);
    assert_eq!(
        fs::read(fixture.repo().join("notes.txt")).unwrap(),
        b"first note\nchanged line\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
