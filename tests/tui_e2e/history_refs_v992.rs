//! TASK-99.2: the History table in the real binary over the History fixture (branches, a
//! merge, an octopus merge, lightweight and annotated tags, a local bare remote with a
//! remote-only branch, a local-only branch, a detached HEAD). Every ref is a typed chip on its
//! commit, a branch and its same-named origin branch on one commit share a `⇅` chip, columns
//! line up with right-aligned ages at 140x42 and 80x24, the `▌` bar follows the keyboard and
//! the mouse, and browsing never changes HEAD, refs, status, the index or config.
use super::*;
use ghtui::config::Config;
use std::{collections::HashMap, path::Path, time::Duration};
use support::history_fixture::*;

fn expected_chips(detached: bool) -> HashMap<&'static str, Vec<&'static str>> {
    let mut chips: HashMap<&'static str, Vec<&'static str>> = HashMap::from([
        ("tweak", vec!["HEAD", "main"]),
        ("wip", vec!["feature/wip"]),
        ("hotfix", vec!["origin/hotfix/crash"]),
        ("tune", vec!["origin/main"]),
        ("release", vec!["release/1.0 ⇅", "v1.0.0"]),
        ("octopus", vec!["v0.2.0"]),
        ("topic_a", vec!["topic/a"]),
        ("topic_b", vec!["topic/b"]),
        ("topic_c", vec!["topic/c"]),
        ("parser2", vec!["feature/parser ⇅"]),
        ("lib", vec!["v0.1.0"]),
    ]);
    if detached {
        chips.insert("tweak", vec!["main"]);
        chips.insert("release", vec!["HEAD", "release/1.0 ⇅", "v1.0.0"]);
    }
    chips
}

fn key_of<'a>(repo: &'a HistoryRepo, row: &HistoryRow) -> &'a str {
    repo.commits
        .iter()
        .find(|commit| commit.short() == row.sha)
        .unwrap_or_else(|| panic!("row {} is not a fixture commit", row.sha))
        .key
}

/// The right edge of the painted AGE text on screen row `y` (inside the pane).
fn age_end(screen: &vt100::Screen, y: u16) -> u16 {
    let (_, right, _, _) = history_pane(screen).unwrap();
    (0..right)
        .rev()
        .find(|x| {
            screen
                .cell(y, *x)
                .is_some_and(|cell| !cell.contents().trim().is_empty())
        })
        .unwrap()
}

/// Checks every visible row against the fixture: SHA, chips, subject (whole, or cut with
/// `…`), author, age, aligned message starts and right-aligned ages. Returns the keys shown.
fn check_rows(
    s: &Session,
    repo: &HistoryRepo,
    chips: &HashMap<&str, Vec<&str>>,
    all: bool,
) -> Vec<&'static str> {
    let screen = s.completed_cursorless_screen().unwrap();
    let rows = history_rows(screen);
    let columns = history_columns(screen).unwrap();
    let context = || format!("\n{}", s.screen());
    assert!(rows.len() >= 5, "too few History rows{}", context());
    if all {
        assert_eq!(
            rows.len(),
            repo.commits.len(),
            "every commit is a row{}",
            context()
        );
    }
    let mut keys = Vec::new();
    let end = age_end(screen, rows[0].y);
    for row in &rows {
        let commit = repo.commit(key_of(repo, row));
        let key = commit.key;
        keys.push(key);
        assert_eq!(row.sha.len(), 7, "{key}: 7-character SHA{}", context());
        let expected = chips.get(key).cloned().unwrap_or_default();
        // A narrow MESSAGE (80 columns, or beside the commit detail from 120) shows the
        // leading chips and counts the rest as `+N`.
        let counted = row.chips.split_last().and_then(|(last, shown)| {
            let hidden = last.strip_prefix('+')?.parse::<usize>().ok()?;
            (shown.len() + hidden == expected.len()
                && shown.iter().zip(&expected).all(|(chip, want)| chip == want))
            .then_some(())
        });
        assert!(
            row.chips == expected || counted.is_some(),
            "{key}: chips {:?} not {expected:?}{}",
            row.chips,
            context()
        );
        let subject = commit.subject;
        assert!(
            row.message == subject
                || row
                    .message
                    .strip_suffix('…')
                    .is_some_and(|cut| subject.starts_with(cut.trim_end())),
            "{key}: message {:?}{}",
            row.message,
            context()
        );
        let name = commit.author.name;
        assert!(
            row.author.contains(commit.author.initials),
            "{key}: author chip {:?}{}",
            row.author,
            context()
        );
        let shown = row.author.rsplit_once(' ').map_or("", |(_, last)| last);
        assert!(
            row.author.ends_with(name)
                || shown.ends_with('…') && name.contains(shown.trim_end_matches('…')),
            "{key}: author {:?}{}",
            row.author,
            context()
        );
        assert_eq!(row.age, repo.age(key), "{key}: age{}", context());
        assert_eq!(
            age_end(screen, row.y),
            end,
            "{key}: ages right-aligned{}",
            context()
        );
        // Every message starts in the MESSAGE column: a chip's padding or the subject.
        let start = screen.cell(row.y, columns.message + 1).unwrap().contents();
        assert!(
            !start.trim().is_empty(),
            "{key}: message start{}",
            context()
        );
        // The graph keeps a dot on every row.
        assert!(
            row.graph.contains('●'),
            "{key}: graph {:?}{}",
            row.graph,
            context()
        );
    }
    // One message color for every row (dimming is for search, TASK-99.3).
    assert!(rows
        .windows(2)
        .all(|pair| pair[0].message_fg == pair[1].message_fg));
    keys
}

fn wait_selected_key(s: &mut Session, repo: &HistoryRepo, key: &str) -> HistoryRow {
    let row = s.wait_for_history_selected(repo.sha(key));
    let screen = s.completed_cursorless_screen().unwrap();
    assert_eq!(
        history_rows(screen)
            .iter()
            .filter(|row| row.selected)
            .count(),
        1,
        "exactly one ▌ row\n{}",
        s.screen()
    );
    row
}

/// Moves the bar to `key` with Down presses (keyboard) or a click on its SHA cell (mouse),
/// scrolling with the wheel first when the row is below the viewport.
fn select(s: &mut Session, repo: &HistoryRepo, mouse: bool, key: &str) {
    let target = repo.short(key).to_owned();
    if mouse {
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        while !s.history_rows().iter().any(|row| row.sha == target) {
            let screen = s.completed_cursorless_screen().unwrap();
            let rows = history_rows(screen);
            let last = rows.last().unwrap();
            s.wheel(last.sha_x, last.y, true);
            s.poll_output(Duration::from_millis(80));
            assert!(
                std::time::Instant::now() < deadline,
                "{key} never scrolled into view\n{}",
                s.screen()
            );
        }
        s.click_history_row(&target);
    } else {
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            s.poll_output(Duration::from_millis(30));
            let rows = s.history_rows();
            let selected = rows.iter().position(|row| row.selected);
            let wanted = rows.iter().position(|row| row.sha == target);
            match (selected, wanted) {
                (Some(at), Some(wanted)) if at == wanted => break,
                (Some(at), Some(wanted)) if wanted < at => s.send_key(b"\x1b[A"),
                _ => s.send_key(b"\x1b[B"),
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{key} never reached by the keyboard\n{}",
                s.screen()
            );
            s.poll_output(Duration::from_millis(60));
        }
        // The bar is the selection; Enter would open the commit's diff (TASK-99.4).
    }
    wait_selected_key(s, repo, key);
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    let detached = mouse;
    let repo = HistoryRepo::build(
        &fixture,
        HistoryOptions {
            detached_at: detached.then_some("release"),
            ..HistoryOptions::default()
        },
    );
    Config {
        refresh_seconds: 3600,
        avatars: false,
        ..Default::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    let baseline = repo.state();
    let chips = expected_chips(detached);
    let env: Vec<(&str, &str)> = if mouse {
        vec![("NO_COLOR", "1")]
    } else {
        vec![]
    };
    let mut s = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        140,
        42,
        &env,
    );
    s.wait_for_screen("Recent commits");
    s.choose(mouse, "[g History]", b"g");
    s.wait_for_history_ready();
    s.wait_for_settled_footer();
    let opened = if detached { "release" } else { "tweak" };
    wait_selected_key(&mut s, &repo, opened);
    let text = s.screen();
    assert_eq!(history_loaded_count(&text), Some(17), "{text}");
    assert!(text.contains("History › repo › all branches"), "{text}");
    // Every ref `git for-each-ref` lists is a chip of its commit (synced pairs share one);
    // a symbolic `origin/HEAD` only points at another remote branch.
    let keys = check_rows(&s, &repo, &chips, true);
    assert_eq!(keys[0], "tweak", "the newest commit leads");
    let refs = repo.git(&[
        "for-each-ref",
        "--format=%(if)%(symref)%(then)%(else)%(refname:short)%(end)",
        "refs/heads",
        "refs/remotes",
        "refs/tags",
    ]);
    // `check_rows` matched every painted chip (and every `+N` count) to these expectations.
    let painted = chips
        .values()
        .flatten()
        .map(|chip| chip.trim_end_matches(" ⇅").to_owned())
        .collect::<Vec<_>>();
    for name in refs.lines().filter(|name| !name.is_empty()) {
        let local = name.strip_prefix("origin/").unwrap_or(name);
        assert!(
            painted.iter().any(|chip| chip == name || chip == local),
            "{name} has no chip: {painted:?}"
        );
    }
    // The octopus merge joins its lanes on its own row.
    let octopus = s
        .history_rows()
        .into_iter()
        .find(|row| row.sha == repo.short("octopus"))
        .unwrap();
    assert!(octopus.graph.contains('─'), "{:?}", octopus.graph);

    select(&mut s, &repo, mouse, "octopus");
    check_rows(&s, &repo, &chips, true);
    assert_eq!(repo.state(), baseline);

    // 80x24: the same columns, cut with an ellipsis, the bar still on the octopus merge.
    s.resize(80, 24);
    s.wait_for_main_frame();
    s.wait_for_history_ready();
    wait_selected_key(&mut s, &repo, "octopus");
    let screen = s.completed_cursorless_screen().unwrap();
    let columns = history_columns(screen).unwrap();
    assert!(columns.graph < columns.commit && columns.commit < columns.message);
    assert!(columns.message < columns.author && columns.author < columns.age);
    check_rows(&s, &repo, &chips, false);
    // Scrolling below the viewport keeps the bar and the columns.
    select(&mut s, &repo, mouse, "lib");
    check_rows(&s, &repo, &chips, false);
    assert_eq!(repo.state(), baseline);

    s.resize(140, 42);
    s.wait_for_main_frame();
    s.wait_for_history_ready();
    wait_selected_key(&mut s, &repo, "lib");
    check_rows(&s, &repo, &chips, true);

    assert_eq!(repo.state(), baseline);
    assert!(s.inspect_command_journal(&fixture).is_empty());
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(repo.state(), baseline);
}

#[test]
fn history_refs_v992_keyboard_chips_columns_and_bar_at_140x42_and_80x24() {
    journey(false)
}
#[test]
fn history_refs_v992_mouse_detached_no_color_chips_columns_and_bar_at_140x42_and_80x24() {
    journey(true)
}
