//! TASK-99.1: the History graph in the real binary over a real repository with
//! branches, a merge, a tag, a local bare remote and a detached HEAD. Lanes join
//! on the merge row itself, stay continuous between rows, keep the messages in
//! one column at 140x42 and 80x24, and selecting rows by keyboard or mouse never
//! changes HEAD, refs, the index, the working tree or config.
use super::*;
use ghtui::config::Config;
use std::{fs, path::Path, time::Duration};

struct Lanes {
    fixture: Fixture,
    /// `(subject, sha)` for every commit the walk must show.
    commits: Vec<(&'static str, String)>,
    baseline: Vec<Vec<u8>>,
}

fn sha(f: &Fixture) -> String {
    f.git(&["rev-parse", "HEAD"]).trim().to_owned()
}
fn commit(f: &Fixture, subject: &'static str, commits: &mut Vec<(&'static str, String)>) {
    f.git(&["commit", "-q", "--allow-empty", "-m", subject]);
    commits.push((subject, sha(f)));
}
fn state(root: &Path) -> Vec<Vec<u8>> {
    let read = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .unwrap();
        assert!(out.status.success());
        out.stdout
    };
    vec![
        read(&["rev-parse", "HEAD"]),
        read(&["show-ref", "--head"]),
        read(&["status", "--porcelain=v2", "--branch", "-z"]),
        read(&["diff", "--binary"]),
        read(&["diff", "--cached", "--binary"]),
        fs::read(root.join(".git/index")).unwrap(),
        fs::read(root.join(".git/config")).unwrap(),
        fs::read(root.join("notes.txt")).unwrap(),
        fs::read(root.join("scratch")).unwrap(),
    ]
}
fn fixture() -> Lanes {
    let f = Fixture::new();
    f.init_git();
    f.git(&["config", "commit.gpgsign", "false"]);
    let mut commits = Vec::new();
    f.commit_file("notes.txt", b"lane notes\n", "Lane root");
    commits.push(("Lane root", sha(&f)));
    commit(&f, "Lane main one", &mut commits);
    f.git(&["switch", "-q", "-c", "feature"]);
    commit(&f, "Lane feature one", &mut commits);
    commit(&f, "Lane feature two", &mut commits);
    f.git(&["switch", "-q", "main"]);
    commit(&f, "Lane main two", &mut commits);
    f.git(&["tag", "v0.1.0"]);
    f.git(&[
        "merge",
        "-q",
        "--no-ff",
        "-m",
        "Lane merge feature",
        "feature",
    ]);
    commits.push(("Lane merge feature", sha(&f)));
    // A local bare remote: origin/main plus a branch only the remote has.
    let bare = f.root.join("remote.git");
    f.git(&["init", "-q", "--bare", bare.to_str().unwrap()]);
    f.git(&["remote", "add", "origin", bare.to_str().unwrap()]);
    f.git(&["switch", "-q", "-c", "remote-only", "v0.1.0"]);
    commit(&f, "Lane remote only", &mut commits);
    f.git(&["push", "-q", "origin", "main", "remote-only"]);
    f.git(&["switch", "-q", "main"]);
    f.git(&["branch", "-q", "-D", "remote-only"]);
    // Detached HEAD on a commit that no branch or tag names.
    f.git(&["switch", "-q", "--detach", "main"]);
    commit(&f, "Lane detached head", &mut commits);
    // The user's uncommitted work: staged, unstaged and untracked bytes.
    fs::write(f.repo().join("notes.txt"), b"staged lane notes\n").unwrap();
    f.git(&["add", "--", "notes.txt"]);
    fs::write(f.repo().join("notes.txt"), b"dirty lane notes\n").unwrap();
    fs::write(f.repo().join("scratch"), b"untracked lane bytes\n").unwrap();
    Config {
        refresh_seconds: 3600,
        avatars: false,
        ..Default::default()
    }
    .save(&f.root.join("config/ghtui/config.toml"))
    .unwrap();
    let baseline = state(&f.repo());
    Lanes {
        fixture: f,
        commits,
        baseline,
    }
}

fn chars(screen: &vt100::Screen, row: u16) -> Vec<char> {
    let (_, cols) = screen.size();
    (0..cols)
        .map(|col| {
            screen
                .cell(row, col)
                .and_then(|cell| cell.contents().chars().next())
                .unwrap_or(' ')
        })
        .collect()
}

/// One painted commit row: its screen row, the dot column and the short-SHA column.
struct Row {
    subject: &'static str,
    y: u16,
    line: Vec<char>,
    dot: usize,
    sha: usize,
    selected: bool,
}

/// Checks the painted graph and returns the commit rows it found, top to bottom.
fn check_graph(
    lanes: &Lanes,
    screen: &vt100::Screen,
    all_visible: bool,
) -> Result<Vec<Row>, String> {
    let parsed = support::history_fixture::history_rows(screen);
    let mut rows = Vec::new();
    for (subject, id) in &lanes.commits {
        let Some(row) = parsed.iter().find(|row| row.sha == id[..7]) else {
            if all_visible {
                return Err(format!("{subject} is not painted"));
            }
            continue;
        };
        if !row
            .message
            .starts_with(subject.split_at(subject.len().min(8)).0)
        {
            return Err(format!("{subject} shows {:?}", row.message));
        }
        let line = chars(screen, row.y);
        let sha = usize::from(row.sha_x);
        let dot = line[..sha]
            .iter()
            .position(|c| *c == '●')
            .ok_or(format!("{subject} has no commit dot"))?;
        rows.push(Row {
            subject,
            y: row.y,
            line,
            dot,
            sha,
            selected: row.selected,
        });
    }
    if rows.len() < 4 {
        return Err(format!("only {} commit rows painted", rows.len()));
    }
    rows.sort_by_key(|row| row.y);
    // One row per commit: no connector rows between them.
    if screen.contents().contains('↳') {
        return Err("a separate connector row is painted".into());
    }
    // SHAs (and so messages) start in one column under the COMMIT title.
    let sha = rows[0].sha;
    if let Some(row) = rows.iter().find(|row| row.sha != sha) {
        return Err(format!("{} starts at {} not {sha}", row.subject, row.sha));
    }
    let left = rows.iter().map(|row| row.dot).min().unwrap();
    let graph = |row: &Row| row.line[left..sha].iter().collect::<String>();
    for pair in rows.windows(2) {
        let (above, below) = (&pair[0], &pair[1]);
        if below.y != above.y + 1 {
            return Err(format!(
                "{} and {} are not adjacent",
                above.subject, below.subject
            ));
        }
        for x in left..sha {
            if below.line[x] == '│' && matches!(above.line[x], ' ' | '─') {
                return Err(format!(
                    "lane at column {x} breaks between\n{}\n{}",
                    graph(above),
                    graph(below)
                ));
            }
        }
    }
    if let Some(merge) = rows.iter().find(|row| row.subject == "Lane merge feature") {
        let tail = merge.line[merge.dot + 1..sha].iter().collect::<String>();
        let link = tail.trim_start_matches(['─', '│']).chars().next();
        if !tail.starts_with('─') || !matches!(link, Some('╮' | '┬' | '┤' | '┼')) {
            return Err(format!(
                "merge row does not join on its row: {:?}",
                graph(merge)
            ));
        }
    } else if all_visible {
        return Err("merge row missing".into());
    }
    if let Some(fork) = rows.iter().find(|row| row.subject == "Lane main one") {
        if !graph(fork).contains(['╯', '╰', '┴']) {
            return Err(format!(
                "fork row does not close the feature lane: {:?}",
                graph(fork)
            ));
        }
    }
    Ok(rows)
}

fn wait_graph(s: &mut Session, lanes: &Lanes, all_visible: bool, selected: &str) {
    let mut last = String::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(20));
        if let Some(screen) = s.completed_cursorless_screen() {
            match check_graph(lanes, screen, all_visible) {
                Ok(rows)
                    if rows
                        .iter()
                        .any(|row| row.selected && row.subject == selected) =>
                {
                    return
                }
                Ok(_) => last = format!("{selected} is not the selected commit"),
                Err(error) => last = error,
            }
        }
        if std::time::Instant::now() >= deadline {
            s.save_artifacts();
            panic!(
                "History lanes were not painted correctly: {last}\n{}",
                s.screen()
            );
        }
    }
}

/// In color, the merge row's dot and the lane it starts are different palette colors.
fn assert_lane_colors(s: &Session, lanes: &Lanes) {
    let screen = s.completed_cursorless_screen().unwrap();
    let rows = check_graph(lanes, screen, true).unwrap();
    let merge = rows
        .iter()
        .find(|row| row.subject == "Lane merge feature")
        .unwrap();
    let link = (merge.dot + 1..merge.sha)
        .find(|x| matches!(merge.line[*x], '╮' | '┬' | '┤' | '┼'))
        .unwrap();
    let color = |x: usize| screen.cell(merge.y, x as u16).unwrap().fgcolor();
    assert_ne!(
        color(merge.dot),
        color(link),
        "the joined lane keeps its own color"
    );
    assert_ne!(color(merge.dot), vt100::Color::Default);
}

fn select(s: &mut Session, mouse: bool, subject: &str) {
    if mouse {
        s.click_visible_action(subject);
        return;
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(20));
        if s.history_rows()
            .iter()
            .any(|row| row.selected && subject.starts_with(row.message.trim_end_matches('…')))
        {
            s.send_key(b"\r");
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{subject} never reached by the keyboard\n{}",
            s.screen()
        );
        s.send_key(b"\x1b[B");
    }
}

fn journey(mouse: bool) {
    let lanes = fixture();
    let env: Vec<(&str, &str)> = if mouse {
        vec![("NO_COLOR", "1")]
    } else {
        vec![]
    };
    let mut s = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &lanes.fixture,
        140,
        42,
        &env,
    );
    s.wait_for_screen("Recent commits");
    s.choose(mouse, "[g Graph]", b"g");
    s.wait_for_screen("8 commits loaded");
    s.wait_for_settled_footer();
    // Opened at the detached HEAD, every branch, the remote-only branch and the tag show.
    wait_graph(&mut s, &lanes, true, "Lane detached head");
    if !mouse {
        assert_lane_colors(&s, &lanes);
    }
    // Selecting a commit re-centres the same walk on it without a Git write.
    select(&mut s, mouse, "Lane feature one");
    wait_graph(&mut s, &lanes, true, "Lane feature one");
    s.wait_for_screen("8 commits loaded");
    assert_eq!(state(&lanes.fixture.repo()), lanes.baseline);

    // At 80x24 the graph column, the joins and the alignment hold for every visible row.
    s.resize(80, 24);
    s.wait_for_main_frame();
    wait_graph(&mut s, &lanes, false, "Lane feature one");
    select(&mut s, mouse, "Lane main two");
    wait_graph(&mut s, &lanes, false, "Lane main two");
    s.resize(140, 42);
    s.wait_for_main_frame();
    wait_graph(&mut s, &lanes, true, "Lane main two");

    assert_eq!(state(&lanes.fixture.repo()), lanes.baseline);
    assert!(s.inspect_command_journal(&lanes.fixture).is_empty());
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(state(&lanes.fixture.repo()), lanes.baseline);
}

#[test]
fn history_lanes_v991_keyboard_branches_merge_tag_remote_detached_at_140x42_and_80x24() {
    journey(false)
}
#[test]
fn history_lanes_v991_mouse_branches_merge_tag_remote_detached_at_140x42_and_80x24() {
    journey(true)
}
