//! TASK-99.3: `f` on a file result in Search opens History with that file's `path:` chip in
//! the real binary over the History fixture. The chip follows the file across its rename
//! (docs/guide.md -> docs/manual/guide.md): the commit that added it under its old name joins
//! the matches and its row and detail say "was docs/guide.md". Keyboard and mouse (Commands,
//! row clicks), 140x42 and 80x24; no GitHub request and no Git write.
use super::*;
use ghtui::config::Config;
use std::{path::Path, time::Duration};
use support::history_fixture::*;

fn launch(fixture: &Fixture, width: u16, height: u16) -> Session {
    Config {
        refresh_seconds: 3600,
        avatars: false,
        ..Default::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        fixture,
        width,
        height,
        &[],
    )
}

fn open_search(s: &mut Session, mouse: bool) {
    s.wait_for_screen("Recent commits");
    s.choose(mouse, "3 Search", b"3");
    s.wait_for_screen("Selected local search corpus ready");
    s.wait_for_settled_footer();
}

fn search(s: &mut Session, mouse: bool, query: &str) {
    s.choose(mouse, "[/ Query]", b"/");
    s.send_key(b"\x01");
    s.paste(query);
    if mouse {
        s.wait_for_settled_footer();
        s.click_visible_action("[Enter Search]");
    } else {
        s.send_key(b"\r");
    }
    s.wait_for_screen("1 results • Exact");
    s.wait_for_row(&["guide.md"]);
}

/// `f` by key, or the File history command by mouse.
fn file_history(s: &mut Session, mouse: bool) {
    if mouse {
        s.invoke_via_palette(true, "File history");
    } else {
        s.send_key(b"f");
    }
}

/// History of docs/manual/guide.md: three matches (two under the new name, the root commit
/// under the old one) and the path chip.
fn wait_file_history(s: &mut Session, repo: &HistoryRepo) {
    s.wait_for_cursorless_condition(
        |screen| {
            history_ready(screen)
                && history_filter_row(screen).is_some_and(|row| {
                    row.matches == Some(3)
                        && row
                            .removable
                            .iter()
                            .any(|(chip, _)| chip == "path: docs/manual/guide.md")
                })
        },
        "History of docs/manual/guide.md",
        Duration::from_secs(10),
    );
    // Git's own answer: the plain pathspec stops at the rename; --follow reaches the root.
    let plain = repo.git(&[
        "log",
        "--all",
        "--full-history",
        "--format=%h",
        "--abbrev=7",
        "--",
        HistoryRepo::RENAMED_TO,
    ]);
    assert!(!plain.contains(repo.short("root")), "{plain}");
    let follow = repo.git(&[
        "log",
        "--follow",
        "--format=%h",
        "--abbrev=7",
        "--",
        HistoryRepo::RENAMED_TO,
    ]);
    assert!(follow.contains(repo.short("root")), "{follow}");
    // The bar rests on the file's latest change at HEAD.
    s.wait_for_history_selected(repo.sha("manual"));
}

fn message(s: &Session, repo: &HistoryRepo, key: &str) -> HistoryRow {
    s.history_rows()
        .into_iter()
        .find(|row| row.sha == repo.short(key))
        .unwrap_or_else(|| panic!("{key} row not visible\n{}", s.screen()))
}

fn wait_detail_path(s: &mut Session) {
    s.wait_for_cursorless_condition(
        |screen| {
            screen
                .contents()
                .lines()
                .any(|line| line.contains("Path") && line.contains("was docs/guide.md"))
        },
        "the detail's Path row names the old path",
        Duration::from_secs(8),
    );
}

#[test]
fn history_filter_v993_follow_keyboard_search_result_opens_file_history_at_140x42_and_80x24() {
    let fixture = Fixture::new();
    let repo = HistoryRepo::build(&fixture, HistoryOptions::default());
    let baseline = repo.state();
    let mut s = launch(&fixture, 140, 42);
    open_search(&mut s, false);
    // No result yet: the key explains what it needs.
    s.send_key(b"f");
    s.wait_for_screen("Select a result from");
    s.send_key(b"\x1b");
    s.wait_for_cursorless_condition(
        |screen| !screen.contents().contains("Select a result from"),
        "the explanation closes",
        Duration::from_secs(8),
    );
    search(&mut s, false, "line two, revised");
    file_history(&mut s, false);
    wait_file_history(&mut s, &repo);
    let root = message(&s, &repo, "root");
    assert!(
        root.message.starts_with("was docs/guide.md"),
        "the old name leads the row: {root:?}"
    );
    let rename = message(&s, &repo, "rename");
    assert!(
        rename.message.contains("renamed from docs/guide.md"),
        "{rename:?}"
    );
    assert!(!message(&s, &repo, "manual").message.contains("was "));
    // n steps through the file's commits down to the one before the rename.
    s.send_key(b"n");
    s.wait_for_history_selected(repo.sha("rename"));
    s.send_key(b"n");
    s.wait_for_history_selected(repo.sha("root"));
    wait_detail_path(&mut s);
    assert_eq!(repo.state(), baseline);

    // 80x24: the row keeps the old name ahead of its subject.
    s.resize(80, 24);
    s.wait_for_main_frame();
    s.wait_for_history_ready();
    let root = message(&s, &repo, "root");
    assert!(
        root.selected && root.message.starts_with("was docs/guide.md"),
        "{root:?}"
    );

    assert_eq!(repo.state(), baseline);
    assert!(s.inspect_command_journal(&fixture).is_empty());
    s.quit_and_assert_restored_terminal(false);
    assert_eq!(repo.state(), baseline);
}

#[test]
fn history_filter_v993_follow_mouse_search_result_opens_file_history_at_80x24_and_140x42() {
    let fixture = Fixture::new();
    let repo = HistoryRepo::build(&fixture, HistoryOptions::default());
    let baseline = repo.state();
    let mut s = launch(&fixture, 80, 24);
    open_search(&mut s, true);
    search(&mut s, true, "line two, revised");
    file_history(&mut s, true);
    wait_file_history(&mut s, &repo);
    assert_eq!(repo.state(), baseline);

    // 140x42: every commit is on screen; a click on the old-name row shows its Path.
    s.resize(140, 42);
    s.wait_for_main_frame();
    s.wait_for_history_ready();
    let root = message(&s, &repo, "root");
    assert!(root.message.starts_with("was docs/guide.md"), "{root:?}");
    s.click_history_row(repo.sha("root"));
    s.wait_for_history_selected(repo.sha("root"));
    wait_detail_path(&mut s);

    assert_eq!(repo.state(), baseline);
    assert!(s.inspect_command_journal(&fixture).is_empty());
    s.quit_and_assert_restored_terminal(true);
    assert_eq!(repo.state(), baseline);
}
