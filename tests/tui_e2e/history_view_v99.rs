//! TASK-99: History is one view for the graph, file history and commit search, in the real
//! binary over the History fixture (branches, a merge, an octopus merge, tags, a local bare
//! remote). `g` (or its `g History` tab), `H`/`f` on a saved file and the `Show History`
//! command all open it. At 140x42 the pane reads `History › repo › all branches` with the
//! commit detail beside the table, the key bar reads like the view (`/ Search`, `n Next
//! match`, `f File history`, `Enter Diff`, `Y Blame`, `C Compare`, `y Copy SHA`), the status
//! line counts the loaded commits and the last row marks where loaded history ends. A search
//! and the detail work together; Back returns from the diff to the same commit and from a
//! file's History to the file. At 80x24 the same view keeps its search and drill-down. Keyboard
//! and mouse; HEAD, refs, status, the index and config never change.
use super::*;
use ghtui::config::Config;
use std::{collections::HashSet, path::Path, time::Duration};
use support::history_fixture::*;

const BACK_KEY: &[u8] = b"\x1b[1;3D";

fn launch(fixture: &Fixture, width: u16, height: u16, no_color: bool) -> Session {
    Config {
        refresh_seconds: 3600,
        avatars: false,
        ..Default::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    let env: Vec<(&str, &str)> = if no_color {
        vec![("NO_COLOR", "1")]
    } else {
        vec![]
    };
    Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        fixture,
        width,
        height,
        &env,
    )
}

fn type_text(s: &mut Session, text: &str) {
    for character in text.chars() {
        let mut buffer = [0; 4];
        s.send_key(character.encode_utf8(&mut buffer).as_bytes());
        s.poll_output(Duration::from_millis(15));
    }
}

/// Short SHAs whose message or author matches `text` the way History's plain search does.
fn text_matches(repo: &HistoryRepo, text: &str) -> HashSet<String> {
    let mut matches = HashSet::new();
    for option in [format!("--grep={text}"), format!("--author={text}")] {
        matches.extend(
            repo.git(&[
                "log",
                "--branches",
                "--remotes",
                "--tags",
                "HEAD",
                "--regexp-ignore-case",
                "--fixed-strings",
                "--format=%h",
                "--abbrev=7",
                &option,
            ])
            .lines()
            .map(|line| line.trim()[..7].to_owned()),
        );
    }
    matches
}

/// The full SHA of a fixture commit given its short SHA.
fn full_sha(repo: &HistoryRepo, short: &str) -> String {
    repo.commits
        .iter()
        .find(|commit| commit.sha.starts_with(short))
        .unwrap_or_else(|| panic!("{short} is not a fixture commit"))
        .sha
        .clone()
}

/// The key bar's tokens (two-space separated) in display order.
fn footer_tokens(s: &Session) -> Vec<String> {
    let screen = s.completed_cursorless_screen().unwrap();
    let (rows, columns) = screen.size();
    let line = screen.rows(0, columns).nth(usize::from(rows - 1)).unwrap();
    line.split("  ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Every wanted token is painted, in this order.
fn assert_footer_order(s: &Session, wanted: &[&str]) {
    let tokens = footer_tokens(s);
    let mut last = None;
    for want in wanted {
        let at = tokens
            .iter()
            .position(|token| token == want)
            .unwrap_or_else(|| panic!("{want:?} not in the key bar {tokens:?}"));
        assert!(
            last.is_none_or(|last| at > last),
            "{want:?} out of order in {tokens:?}"
        );
        last = Some(at);
    }
}

/// `f` and `Y` join the bar once the shown commit's changed files are read.
fn wait_file_keys(s: &mut Session) {
    s.wait_for_cursorless_condition(
        |screen| {
            let (rows, columns) = screen.size();
            screen
                .rows(0, columns)
                .nth(usize::from(rows - 1))
                .is_some_and(|bar| bar.contains("f File history") && bar.contains("Y Blame"))
        },
        "the commit's file keys in the bar",
        Duration::from_secs(10),
    );
}

/// Waits for a settled History frame with every text in `wanted` on screen.
fn wait_texts(s: &mut Session, wanted: &[&str]) {
    let wanted: Vec<String> = wanted.iter().map(|text| (*text).to_owned()).collect();
    s.wait_for_cursorless_condition(
        |screen| {
            let text = screen.contents();
            wanted.iter().all(|want| text.contains(want.as_str()))
        },
        &format!("screen with {wanted:?}"),
        Duration::from_secs(10),
    );
}

/// The History mode tab is the selected one: filled (bold) or reversed without color.
fn history_tab_selected(screen: &vt100::Screen) -> bool {
    support::locate_screen(screen, " g History ").is_some_and(|(column, row)| {
        screen
            .cell(row, column + 3)
            .is_some_and(|cell| cell.inverse() || cell.bold())
    })
}

/// Opens `name` from the saved tree: a click on its row, or Down until it is reversed and
/// then Enter.
fn tree_file(s: &mut Session, mouse: bool, name: &str) {
    s.wait_for_settled_footer();
    s.choose(mouse, "[t Tree]", b"t");
    s.wait_for_screen("Saved file tree");
    if mouse {
        s.click_visible_action(name);
    } else {
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            s.poll_output(Duration::from_millis(1));
            if let Some((column, row)) = s.locate(name) {
                if s.cell_inverted(row, column) {
                    s.send_key(b"\r");
                    break;
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{name} never keyboard-selected\n{}",
                s.screen()
            );
            s.send_key(b"\x1b[B");
            s.poll_output(Duration::from_millis(50));
        }
    }
    s.wait_for_screen(&format!("{name} at saved version"));
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    let repo = HistoryRepo::build(&fixture, HistoryOptions::default());
    let baseline = repo.state();
    let mut s = launch(&fixture, 140, 42, mouse);
    s.wait_for_screen("Recent commits");

    // g (or the `g History` tab) opens History on every branch.
    s.wait_for_settled_footer();
    s.choose(mouse, "[g History]", b"g");
    s.wait_for_history_ready();
    s.wait_for_settled_footer();
    s.wait_for_history_selected(repo.sha("tweak"));
    wait_file_keys(&mut s);
    wait_texts(
        &mut s,
        &[
            "History › repo › all branches",
            "Read-only · 17 commits loaded",
            // The loaded-history boundary is a visible row under the last commit.
            "All ancestors in this original saved walk are loaded",
            // The detail beside the table, with the full SHA and the UTC date.
            "CHANGED FILES",
            repo.sha("tweak"),
            &repo.utc("tweak"),
        ],
    );
    let screen = s.completed_cursorless_screen().unwrap();
    assert!(history_tab_selected(screen), "{}", s.screen());
    assert_eq!(history_rows(screen).len(), 17, "{}", s.screen());
    let text = s.screen();
    assert!(!text.contains("g Graph"), "the mode is History\n{text}");
    assert!(
        !text.contains("History pan"),
        "lanes are not panned\n{text}"
    );
    // `n`/`N` join the bar once something filters (disabled controls leave it).
    assert_footer_order(
        &s,
        &[
            "/ Search",
            "f File history",
            "Enter Diff",
            "Y Blame",
            "C Compare",
            "y Copy SHA",
        ],
    );
    assert_eq!(repo.state(), baseline);

    // Search and the detail together: matches keep full ink, the rest stay visible, the
    // border counts them and the detail follows the bar from match to match.
    s.choose(mouse, "[/ Search]", b"/");
    s.wait_for_history_filter("filter row", |row| row.text.starts_with('/'));
    type_text(&mut s, "guide");
    let guide = text_matches(&repo, "guide");
    assert!(guide.len() >= 2, "{guide:?}");
    s.wait_for_history_filter("guide matches", |row| {
        row.matches == Some(guide.len()) && row.next_hint
    });
    s.send_key(b"\r");
    s.wait_for_cursorless_condition(
        |screen| selected_history_row(screen).is_some_and(|row| guide.contains(&row.sha)),
        "the bar on a match",
        Duration::from_secs(10),
    );
    let first = selected_history_row(s.completed_cursorless_screen().unwrap())
        .unwrap()
        .sha;
    wait_texts(
        &mut s,
        &[
            &full_sha(&repo, &first),
            "CHANGED FILES",
            "non-matching commits stay visible for context",
        ],
    );
    assert_eq!(s.history_rows().len(), 17, "non-matches stay visible");
    wait_file_keys(&mut s);
    s.wait_for_settled_footer();
    assert_footer_order(
        &s,
        &[
            "/ Search",
            "Esc Clear filter",
            "n Next match",
            "f File history",
            "Enter Diff",
            "Y Blame",
            "y Copy SHA",
        ],
    );
    s.choose(mouse, "[n Next match]", b"n");
    s.wait_for_cursorless_condition(
        |screen| {
            selected_history_row(screen)
                .is_some_and(|row| guide.contains(&row.sha) && row.sha != first)
        },
        "the next match",
        Duration::from_secs(10),
    );
    let second = selected_history_row(s.completed_cursorless_screen().unwrap())
        .unwrap()
        .sha;
    wait_texts(&mut s, &[&full_sha(&repo, &second), "CHANGED FILES"]);
    assert_eq!(repo.state(), baseline);

    // Enter opens the match's diff; Back returns to History with the bar on it.
    s.wait_for_settled_footer();
    s.choose(mouse, "[Enter Diff]", b"\r");
    wait_texts(&mut s, &["Saved commit changes", "Compared parent 1:"]);
    assert!(history_columns(s.completed_cursorless_screen().unwrap()).is_none());
    s.choose(mouse, "Alt-← Back", BACK_KEY);
    s.wait_for_history_ready();
    s.wait_for_history_selected(&second);
    s.wait_for_settled_footer();
    assert_eq!(repo.state(), baseline);

    // f on the shown commit's file: the same History with that file's `path:` chip; Esc Clear
    // returns to every commit.
    wait_file_keys(&mut s);
    s.wait_for_settled_footer();
    s.choose(mouse, "[f File history]", b"f");
    let row = s.wait_for_history_filter("path chip", |row| {
        row.removable
            .iter()
            .any(|(chip, _)| chip.starts_with("path: "))
    });
    assert!(row.text.contains("✕"), "{row:?}");
    wait_texts(&mut s, &["History › repo › all branches"]);
    s.wait_for_settled_footer();
    s.choose(mouse, "[Esc Clear filter]", b"\x1b");
    s.wait_for_cursorless_condition(
        |screen| history_filter_row(screen).is_none() && history_ready(screen),
        "filter cleared",
        Duration::from_secs(10),
    );
    assert_eq!(s.history_rows().len(), 17);
    assert_eq!(repo.state(), baseline);

    // A saved file's History: `H` (an alias of `f`) by key, the `f File history` token by
    // mouse. The bar lands on the file's latest change; Back returns to the file.
    tree_file(&mut s, mouse, "README.md");
    s.wait_for_settled_footer();
    s.choose(mouse, "[f File history]", b"H");
    s.wait_for_history_filter("README path chip", |row| {
        row.removable
            .iter()
            .any(|(chip, _)| chip == "path: README.md")
    });
    s.wait_for_history_ready();
    s.wait_for_history_selected(repo.sha("install"));
    let screen = s.completed_cursorless_screen().unwrap();
    assert!(history_tab_selected(screen), "{}", s.screen());
    s.wait_for_settled_footer();
    s.choose(mouse, "Alt-← Back", BACK_KEY);
    // Back restores the same file at the same saved version (its line view).
    s.wait_for_screen("README.md:1 / saved version");
    assert_eq!(repo.state(), baseline);

    // The `Show History` command opens the same view.
    s.wait_for_settled_footer();
    s.invoke_via_palette(mouse, "Show History");
    s.wait_for_history_ready();
    assert!(history_tab_selected(
        s.completed_cursorless_screen().unwrap()
    ));
    s.wait_for_settled_footer();

    // 80x24: the same view; search and the drill-down detail still work.
    s.resize(80, 24);
    s.wait_for_main_frame();
    s.wait_for_history_ready();
    s.wait_for_settled_footer();
    wait_texts(&mut s, &["History ›", "Read-only · 17 commits loaded"]);
    assert_footer_order(&s, &["/ Search", "? Help"]);
    let selected = selected_history_row(s.completed_cursorless_screen().unwrap())
        .unwrap()
        .sha;
    s.choose(mouse, "[/ Search]", b"/");
    s.wait_for_history_filter("filter row", |row| row.text.starts_with('/'));
    type_text(&mut s, "guide");
    s.wait_for_history_filter("guide matches at 80x24", |row| {
        row.matches == Some(guide.len())
    });
    s.send_key(b"\r");
    s.wait_for_cursorless_condition(
        |screen| selected_history_row(screen).is_some_and(|row| guide.contains(&row.sha)),
        "the bar on a match at 80x24",
        Duration::from_secs(10),
    );
    let matched = selected_history_row(s.completed_cursorless_screen().unwrap())
        .unwrap()
        .sha;
    // The match becomes the saved version before the bar settles; a click taken earlier can
    // land on a transient `Esc Cancel` token.
    wait_texts(&mut s, &[&format!("Saved: {}", full_sha(&repo, &matched))]);
    s.wait_for_settled_footer();
    s.choose(mouse, "[D Details]", b"D");
    wait_texts(
        &mut s,
        &[&format!("Commit {matched}"), &full_sha(&repo, &matched)],
    );
    s.wait_for_settled_footer();
    s.choose(mouse, "[u Back]", b"u");
    s.wait_for_history_ready();
    s.wait_for_history_selected(&matched);
    s.wait_for_settled_footer();
    s.choose(mouse, "[Esc Clear]", b"\x1b");
    s.wait_for_cursorless_condition(
        |screen| history_filter_row(screen).is_none() && history_ready(screen),
        "filter cleared at 80x24",
        Duration::from_secs(10),
    );
    s.wait_for_history_selected(&selected);

    assert_eq!(repo.state(), baseline);
    assert!(s.inspect_command_journal(&fixture).is_empty());
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(repo.state(), baseline);
}

#[test]
fn history_view_v99_keyboard_g_h_palette_search_detail_back_at_140x42_and_80x24() {
    journey(false)
}
#[test]
fn history_view_v99_mouse_no_color_tab_file_history_search_detail_back_at_140x42_and_80x24() {
    journey(true)
}
