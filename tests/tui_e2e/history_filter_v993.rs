//! TASK-99.3: History search, path filter and branch scope in the real binary over the History
//! fixture (branches, a merge, an octopus merge, tags, a local bare remote, a detached HEAD).
//! `/` opens the filter row; text highlights matches at once and dims the rest, which stay
//! visible; the border counts matches and `n`/`N` step through them; hide mode keeps only
//! matches with joined lanes; `author:`/`path:` chips match exactly what `git log` selects;
//! the branch scope walks one branch; chips leave by their `✕`, by Backspace on an empty
//! query and by Esc, which restores the selection. Keyboard and mouse, 140x42 and 80x24, and
//! browsing never changes HEAD, refs, status, the index or config.
use super::*;
use ghtui::config::Config;
use std::{collections::HashSet, path::Path, time::Duration};
use support::history_fixture::*;

/// Short SHAs Git selects over every branch, remote branch, tag and HEAD with History's
/// options (`--full-history -i -F`), plus `extra`.
fn git_selects(repo: &HistoryRepo, extra: &[&str], path: Option<&str>) -> HashSet<String> {
    let mut args = vec![
        "log",
        "--branches",
        "--remotes",
        "--tags",
        "HEAD",
        "--full-history",
        "--regexp-ignore-case",
        "--fixed-strings",
        "--format=%h",
        "--abbrev=7",
    ];
    args.extend_from_slice(extra);
    args.push("--");
    args.extend(path);
    repo.git(&args)
        .lines()
        .map(|line| line.trim()[..7].to_owned())
        .collect()
}

/// Plain text: Git's message grep plus the author name or email.
fn text_matches(repo: &HistoryRepo, text: &str) -> HashSet<String> {
    let mut matches = git_selects(repo, &[&format!("--grep={text}")], None);
    matches.extend(git_selects(repo, &[&format!("--author={text}")], None));
    matches
}

fn screen(s: &Session) -> &vt100::Screen {
    s.completed_cursorless_screen().unwrap()
}

/// Waits for a filter row whose border count is `count` and returns it.
fn wait_count(s: &mut Session, count: usize) -> HistoryFilterRow {
    s.wait_for_history_filter(&format!("{count} History matches"), |row| {
        row.matches == Some(count) && row.next_hint
    })
}

/// Matching rows keep full ink and a bold SHA; the others stay visible, dimmed. Returns the
/// SHAs of the painted matching rows.
fn check_highlight(s: &Session, expected: &HashSet<String>, color: bool) -> HashSet<String> {
    let screen = screen(s);
    let rows = history_rows(screen);
    let context = || format!("\n{}", s.screen());
    let (hits, others): (Vec<_>, Vec<_>) = rows.iter().partition(|row| expected.contains(&row.sha));
    assert!(
        !hits.is_empty() && !others.is_empty(),
        "both kinds visible{}",
        context()
    );
    for row in &hits {
        let sha = screen.cell(row.y, row.sha_x).unwrap();
        assert!(
            sha.bold(),
            "{}: a match has a bold SHA{}",
            row.sha,
            context()
        );
    }
    for row in &others {
        let sha = screen.cell(row.y, row.sha_x).unwrap();
        assert!(
            !sha.bold() || row.selected,
            "{}: a non-match is dimmed{}",
            row.sha,
            context()
        );
    }
    if color {
        let ink = hits[0].message_fg;
        let dim = others[0].message_fg;
        assert!(
            hits.iter().all(|row| row.message_fg == ink),
            "one ink for matches{}",
            context()
        );
        assert!(
            others.iter().all(|row| row.message_fg == dim),
            "one dim ink for the rest{}",
            context()
        );
        assert_ne!(ink, dim, "matches and non-matches differ{}", context());
    }
    // Every row keeps its lane dot, matching or not: nothing is cut out of the graph.
    assert!(
        rows.iter().all(|row| row.graph.contains('●')),
        "{}",
        context()
    );
    hits.iter().map(|row| row.sha.clone()).collect()
}

/// The column where `text` starts on screen row `y`.
fn locate_in_row(screen: &vt100::Screen, y: u16, text: &str) -> Option<u16> {
    let (_, columns) = screen.size();
    let cells: Vec<(u16, String)> = (0..columns)
        .filter_map(|x| screen.cell(y, x).map(|cell| (x, cell)))
        .filter(|(_, cell)| !cell.is_wide_continuation())
        .map(|(x, cell)| {
            let contents = cell.contents();
            (
                x,
                if contents.is_empty() {
                    " ".into()
                } else {
                    contents
                },
            )
        })
        .collect();
    let line: String = cells.iter().map(|(_, text)| text.as_str()).collect();
    let byte = line.find(text)?;
    cells.get(line[..byte].chars().count()).map(|(x, _)| *x)
}

fn type_text(s: &mut Session, text: &str) {
    for character in text.chars() {
        let mut buffer = [0; 4];
        s.send_key(character.encode_utf8(&mut buffer).as_bytes());
        s.poll_output(Duration::from_millis(15));
    }
}

fn selected_sha(s: &mut Session) -> String {
    s.wait_for_cursorless_condition(
        |screen| selected_history_row(screen).is_some(),
        "one selected History row",
        Duration::from_secs(8),
    );
    selected_history_row(screen(s)).unwrap().sha
}

fn wait_scope(s: &mut Session, title: &str, loaded: usize) {
    let wanted = format!("History › {title}");
    s.wait_for_cursorless_condition(
        |screen| {
            let text = screen.contents();
            text.contains(&wanted)
                && history_loaded_count(&text) == Some(loaded)
                && history_ready(screen)
        },
        &wanted,
        Duration::from_secs(10),
    );
}

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

#[test]
fn history_filter_v993_keyboard_search_hide_scope_chips_at_140x42_and_80x24() {
    let fixture = Fixture::new();
    let repo = HistoryRepo::build(&fixture, HistoryOptions::default());
    let baseline = repo.state();
    let mut s = launch(&fixture, 140, 42, false);
    s.wait_for_screen("Recent commits");
    s.send_key(b"g");
    s.wait_for_history_ready();
    s.wait_for_settled_footer();
    s.wait_for_history_selected(repo.sha("tweak"));
    assert_eq!(history_loaded_count(&s.screen()), Some(17));

    // `/` opens the filter row; typing highlights at once (no Enter, no Git read).
    s.send_key(b"/");
    s.wait_for_history_filter("filter row", |row| row.text.starts_with('/'));
    type_text(&mut s, "parser");
    let parser = text_matches(&repo, "parser");
    assert_eq!(parser.len(), 3, "{parser:?}");
    let row = wait_count(&mut s, parser.len());
    assert_eq!(row.segments[0], "parser▏", "{row:?}");
    assert!(row.segments.contains(&"all branches".to_owned()), "{row:?}");
    assert_eq!(check_highlight(&s, &parser, true), parser);
    assert!(s
        .screen()
        .contains("non-matching commits stay visible for context"));
    // Enter applies: the bar moves to the first match below HEAD; n/N step and wrap.
    s.send_key(b"\r");
    s.wait_for_cursorless_condition(
        |screen| selected_history_row(screen).is_some_and(|row| parser.contains(&row.sha)),
        "selection on a match",
        Duration::from_secs(10),
    );
    let order: Vec<String> = s
        .history_rows()
        .into_iter()
        .map(|row| row.sha)
        .filter(|sha| parser.contains(sha))
        .collect();
    assert_eq!(selected_sha(&mut s), order[0], "the first match below HEAD");
    for expected in [&order[1], &order[2], &order[0]] {
        s.send_key(b"n");
        s.wait_for_history_selected(expected);
    }
    assert!(s.screen().contains("wrapped"), "{}", s.screen());
    s.send_key(b"N");
    s.wait_for_history_selected(&order[2]);
    // h hides the rest: exactly the matches remain, each with its lane dot.
    s.send_key(b"h");
    s.wait_for_cursorless_condition(
        |screen| history_ready(screen) && history_rows(screen).len() == parser.len(),
        "only matches",
        Duration::from_secs(10),
    );
    let shown: HashSet<String> = s.history_rows().into_iter().map(|row| row.sha).collect();
    assert_eq!(shown, parser);
    assert!(s.history_rows().iter().all(|row| row.graph.contains('●')));
    assert!(s.screen().contains("non-matching commits hidden"));
    s.send_key(b"h");
    s.wait_for_cursorless_condition(
        |screen| history_ready(screen) && history_rows(screen).len() == 17,
        "every commit again",
        Duration::from_secs(10),
    );
    assert_eq!(repo.state(), baseline);

    // A new query: the bar lands on parser2 (feature/parser), and B scopes to that branch.
    s.send_key(b"/");
    s.wait_for_history_filter("query focused", |row| row.segments[0].ends_with('▏'));
    for _ in 0.."parser".len() {
        s.send_key(b"\x7f");
    }
    type_text(&mut s, "config");
    let config = text_matches(&repo, "config");
    wait_count(&mut s, config.len());
    s.send_key(b"\r");
    s.wait_for_history_selected(repo.sha("parser2"));
    s.send_key(b"B");
    let feature = repo
        .git(&["rev-list", "--count", "feature/parser"])
        .trim()
        .parse::<usize>()
        .unwrap();
    wait_scope(&mut s, "feature/parser", feature);
    let row = s.wait_for_history_filter("scope chip", |row| {
        row.removable
            .iter()
            .any(|(chip, _)| chip == "branch: feature/parser")
    });
    assert_eq!(row.matches, Some(2), "{row:?}");
    let shown: HashSet<String> = s.history_rows().into_iter().map(|row| row.sha).collect();
    let expected: HashSet<String> = repo
        .git(&[
            "rev-list",
            "--abbrev-commit",
            "--abbrev=7",
            "feature/parser",
        ])
        .lines()
        .map(|line| line[..7].to_owned())
        .collect();
    assert_eq!(shown, expected, "one branch walked");
    s.wait_for_history_selected(repo.sha("parser2"));
    // Backspace on the empty query removes the last chip (the scope), then the text is gone:
    // every branch again and the selection from before filtering.
    s.send_key(b"/");
    s.wait_for_history_filter("query focused", |row| row.segments[0].ends_with('▏'));
    for _ in 0..="config".len() {
        s.send_key(b"\x7f");
    }
    wait_scope(&mut s, "all branches", 17);
    s.wait_for_history_selected(repo.sha("tweak"));

    // path: chip: exactly Git's pathspec selection, removable chip with ✕.
    type_text(&mut s, "path:src/parser.rs");
    s.send_key(b"\r");
    let path = git_selects(&repo, &[], Some("src/parser.rs"));
    let row = wait_count(&mut s, path.len());
    assert!(
        row.removable
            .iter()
            .any(|(chip, _)| chip == "path: src/parser.rs"),
        "{row:?}"
    );
    assert_eq!(check_highlight(&s, &path, true), path);
    assert_eq!(repo.state(), baseline);

    // 80x24: the same filter row, count and columns; n still steps.
    s.resize(80, 24);
    s.wait_for_main_frame();
    s.wait_for_history_ready();
    let row = wait_count(&mut s, path.len());
    assert!(row.text.contains("path: src/parser.rs ✕"), "{row:?}");
    let columns = history_columns(screen(&s)).unwrap();
    assert!(columns.graph < columns.commit && columns.commit < columns.message);
    let before = selected_sha(&mut s);
    s.send_key(b"n");
    s.wait_for_cursorless_condition(
        |screen| {
            selected_history_row(screen)
                .is_some_and(|row| row.sha != before && path.contains(&row.sha))
        },
        "next path match at 80x24",
        Duration::from_secs(8),
    );
    // Esc clears every chip and returns to the selection before filtering.
    s.send_key(b"\x1b");
    s.wait_for_cursorless_condition(
        |screen| history_ready(screen) && history_filter_row(screen).is_none(),
        "filter cleared",
        Duration::from_secs(10),
    );
    s.wait_for_history_selected(repo.sha("tweak"));
    s.resize(140, 42);
    s.wait_for_main_frame();
    s.wait_for_history_ready();
    assert_eq!(s.history_rows().len(), 17);

    assert_eq!(repo.state(), baseline);
    assert!(s.inspect_command_journal(&fixture).is_empty());
    s.quit_and_assert_restored_terminal(false);
    assert_eq!(repo.state(), baseline);
}

#[test]
fn history_filter_v993_mouse_detached_no_color_chips_scope_and_clear_at_140x42_and_80x24() {
    let fixture = Fixture::new();
    let repo = HistoryRepo::build(
        &fixture,
        HistoryOptions {
            detached_at: Some("release"),
            ..HistoryOptions::default()
        },
    );
    let baseline = repo.state();
    let mut s = launch(&fixture, 140, 42, true);
    s.wait_for_screen("Recent commits");
    s.choose(true, "[g Graph]", b"g");
    s.wait_for_history_ready();
    s.wait_for_settled_footer();
    s.wait_for_history_selected(repo.sha("release"));

    // The `/ Search` token focuses the query; the border count steps on click.
    s.click_visible_action("[/ Search]");
    s.wait_for_history_filter("filter row", |row| row.text.starts_with('/'));
    type_text(&mut s, "config");
    let config = text_matches(&repo, "config");
    assert_eq!(config.len(), 2, "{config:?}");
    wait_count(&mut s, config.len());
    // Without color a match is a bold SHA with the query underlined; the rest stay visible.
    assert_eq!(check_highlight(&s, &config, false), config);
    let screen_now = screen(&s);
    let parser2 = history_rows(screen_now)
        .into_iter()
        .find(|row| row.sha == repo.short("parser2"))
        .unwrap();
    let column = locate_in_row(screen_now, parser2.y, "config");
    let underlined = column.is_some_and(|x| screen_now.cell(parser2.y, x).unwrap().underline());
    assert!(
        underlined,
        "the query is underlined in the subject\n{}",
        s.screen()
    );
    let (count_x, count_y) = s.locate("n next").unwrap();
    s.click(count_x, count_y);
    s.wait_for_history_selected(repo.sha("parser2"));

    // `h Hide non-matches`, then `h Show non-matches`, as clicked tokens.
    s.wait_for_settled_footer();
    s.click_visible_action("[h Hide non-matches]");
    s.wait_for_cursorless_condition(
        |screen| history_ready(screen) && history_rows(screen).len() == config.len(),
        "only matches",
        Duration::from_secs(10),
    );
    s.wait_for_settled_footer();
    s.click_visible_action("[h Show non-matches]");
    s.wait_for_cursorless_condition(
        |screen| history_ready(screen) && history_rows(screen).len() == 17,
        "every commit again",
        Duration::from_secs(10),
    );

    // A click on the scope chip scopes to the selected commit's branch; its ✕ returns.
    let row = s.wait_for_history_filter("scope chip", |row| {
        row.segments.contains(&"all branches".to_owned())
    });
    let x = locate_in_row(screen(&s), row.y, "all branches").unwrap();
    s.click(x + 1, row.y);
    let feature = repo
        .git(&["rev-list", "--count", "feature/parser"])
        .trim()
        .parse::<usize>()
        .unwrap();
    wait_scope(&mut s, "feature/parser", feature);
    let row = s.wait_for_history_filter("scope chip ✕", |row| !row.removable.is_empty());
    let (_, (x, y)) = row
        .removable
        .iter()
        .find(|(chip, _)| chip == "branch: feature/parser")
        .cloned()
        .unwrap();
    s.click(x, y);
    wait_scope(&mut s, "all branches", 17);
    assert_eq!(repo.state(), baseline);

    // 80x24: an author chip typed after the text, removed by its ✕; Esc Clear restores.
    s.resize(80, 24);
    s.wait_for_main_frame();
    s.wait_for_history_ready();
    s.wait_for_settled_footer();
    s.click_visible_action("[/ Search]");
    s.wait_for_history_filter("query focused", |row| row.segments[0].ends_with('▏'));
    type_text(&mut s, " author:novák");
    s.send_key(b"\r");
    let both: HashSet<String> = config
        .intersection(&git_selects(&repo, &["--author=novák"], None))
        .cloned()
        .collect();
    assert_eq!(both.len(), 2);
    let row = s.wait_for_history_filter("author chip", |row| {
        row.removable
            .iter()
            .any(|(chip, _)| chip == "author: novák")
            && row.matches == Some(2)
    });
    let (_, (x, y)) = row
        .removable
        .iter()
        .find(|(chip, _)| chip == "author: novák")
        .cloned()
        .unwrap();
    s.click(x, y);
    s.wait_for_history_filter("author chip removed", |row| {
        row.removable.is_empty() && row.matches == Some(config.len())
    });
    s.wait_for_settled_footer();
    s.click_visible_action("[Esc Clear]");
    s.wait_for_cursorless_condition(
        |screen| history_ready(screen) && history_filter_row(screen).is_none(),
        "filter cleared",
        Duration::from_secs(10),
    );
    s.wait_for_history_selected(repo.sha("release"));
    s.resize(140, 42);
    s.wait_for_main_frame();
    s.wait_for_history_ready();
    assert_eq!(s.history_rows().len(), 17);

    assert_eq!(repo.state(), baseline);
    assert!(s.inspect_command_journal(&fixture).is_empty());
    s.quit_and_assert_restored_terminal(true);
    assert_eq!(repo.state(), baseline);
}
