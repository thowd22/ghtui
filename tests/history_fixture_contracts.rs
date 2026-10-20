//! Contracts for the History (TASK-99) test support: the real-Git fixture
//! topology and the History row/filter parsers.
mod support;

use std::collections::BTreeMap;
use support::history_fixture::{
    history_filter_row, history_row, history_rows, selected_history_row, utc_label, HistoryClock,
    HistoryOptions, HistoryRepo, ADA, GRACE, KAT,
};
use support::Fixture;

fn by_sha(repo: &HistoryRepo) -> BTreeMap<String, &'static str> {
    repo.commits
        .iter()
        .map(|commit| (commit.sha.clone(), commit.key))
        .collect()
}

#[test]
fn history_fixture_v99_topology_matches_git_log_graph() {
    let fixture = Fixture::new();
    let repo = HistoryRepo::build(&fixture, HistoryOptions::default());
    let graph = repo.graph();
    println!("{graph}");
    assert_eq!(repo.commits.len(), HistoryRepo::KEYS.len());
    for (commit, key) in repo.commits.iter().zip(HistoryRepo::KEYS) {
        assert_eq!(commit.key, key);
    }

    // Parentage, in Git parent order, for every reachable commit.
    let keys = by_sha(&repo);
    let walked: BTreeMap<&str, Vec<&str>> = repo
        .git(&["log", "--all", "--format=%H %P"])
        .lines()
        .map(|line| {
            let mut ids = line.split(' ');
            let id = keys[ids.next().unwrap()];
            (
                id,
                ids.filter(|parent| !parent.is_empty())
                    .map(|parent| keys[parent])
                    .collect(),
            )
        })
        .collect();
    let expected: BTreeMap<&str, Vec<&str>> = repo
        .commits
        .iter()
        .map(|commit| (commit.key, commit.parents.clone()))
        .collect();
    assert_eq!(walked, expected);
    assert_eq!(repo.commit("octopus").parents.len(), 4);
    assert_eq!(repo.commit("merge_parser").parents.len(), 2);
    assert!(repo.commit("root").parents.is_empty());

    // git log --graph shows the octopus fan-in, ordinary merges and the
    // remote-only and local-only side branches.
    let lines: Vec<&str> = graph.lines().collect();
    assert!(lines
        .iter()
        .any(|line| line.contains("*-") && line.ends_with("Merge topics a, b and c")));
    assert!(lines
        .iter()
        .any(|line| line.trim_end().ends_with("Merge feature/parser into main")));
    assert!(lines.iter().any(|line| line.contains("|\\")));
    assert!(lines.iter().any(|line| line.contains("|/")));
    assert_eq!(lines.iter().filter(|line| line.contains('*')).count(), 17);
    assert_eq!(
        lines.first().unwrap().trim_start_matches(['*', ' ']),
        "Tweak the README wording"
    );
    assert!(lines.last().unwrap().ends_with("Add the README and guide"));

    // Refs: local, remote-tracking (incl. remote-only), lightweight and annotated tags.
    let refs = repo.git(&[
        "for-each-ref",
        "--format=%(refname) %(objecttype) %(*objectname)%(objectname)",
    ]);
    let points = |name: &str| {
        refs.lines()
            .find(|line| line.split(' ').next() == Some(name))
            .unwrap_or_else(|| panic!("missing {name} in\n{refs}"))
            .to_owned()
    };
    for (name, key) in [
        ("refs/heads/main", "tweak"),
        ("refs/heads/feature/parser", "parser2"),
        ("refs/heads/feature/wip", "wip"),
        ("refs/heads/release/1.0", "release"),
        ("refs/heads/topic/a", "topic_a"),
        ("refs/heads/topic/b", "topic_b"),
        ("refs/heads/topic/c", "topic_c"),
        ("refs/remotes/origin/main", "tune"),
        ("refs/remotes/origin/feature/parser", "parser2"),
        ("refs/remotes/origin/release/1.0", "release"),
        ("refs/remotes/origin/hotfix/crash", "hotfix"),
        ("refs/tags/v0.1.0", "lib"),
    ] {
        assert_eq!(points(name), format!("{name} commit {}", repo.sha(key)));
    }
    for (name, key) in [
        ("refs/tags/v0.2.0", "octopus"),
        ("refs/tags/v1.0.0", "release"),
    ] {
        let line = points(name);
        assert!(
            line.starts_with(&format!("{name} tag {}", repo.sha(key))),
            "{line}"
        );
    }
    assert!(!refs.contains("refs/heads/hotfix/crash"));
    assert!(!refs.contains("refs/remotes/origin/feature/wip"));
    assert_eq!(
        repo.git(&["symbolic-ref", "HEAD"]).trim(),
        "refs/heads/main"
    );
    let remote = std::process::Command::new("git")
        .arg("--git-dir")
        .arg(&repo.remote)
        .args(["for-each-ref", "--format=%(refname)"])
        .output()
        .unwrap();
    let remote = String::from_utf8(remote.stdout).unwrap();
    assert!(remote.contains("refs/heads/hotfix/crash") && remote.contains("refs/tags/v1.0.0"));

    // Authors, distinct committer and deterministic dates.
    let people = repo.git(&["log", "--all", "--format=%H|%an|%ae|%cn|%ce|%at|%ct"]);
    for line in people.lines() {
        let fields: Vec<&str> = line.split('|').collect();
        let commit = repo.commit(keys[fields[0]]);
        assert_eq!(
            fields[1..5],
            [
                commit.author.name,
                commit.author.email,
                commit.committer.name,
                commit.committer.email
            ]
        );
        assert_eq!(fields[5], commit.author_time.to_string());
        assert_eq!(fields[6], commit.commit_time.to_string());
    }
    let tune = repo.commit("tune");
    assert_eq!((tune.author, tune.committer), (KAT, GRACE));
    assert!(tune.commit_time > tune.author_time);
    let authors: std::collections::BTreeSet<_> =
        repo.commits.iter().map(|c| c.author.email).collect();
    assert_eq!(authors.len(), 3);
    assert_eq!(repo.commit("root").author, ADA);
    assert_eq!(repo.age("tweak"), "6h");
    assert_eq!(repo.age("tune"), "9h");
    assert_eq!(repo.age("lib"), "2d");
    assert_eq!(repo.age("root"), "3d");

    // The guide is renamed across history and followable to the root commit.
    let follow = repo.git(&[
        "log",
        "--follow",
        "--format=%s",
        "--",
        HistoryRepo::RENAMED_TO,
    ]);
    assert_eq!(
        follow.lines().collect::<Vec<_>>(),
        [
            "Revise the manual guide",
            "Move the guide under docs/manual",
            "Add the README and guide"
        ]
    );
    let names = repo.git(&[
        "show",
        "--format=",
        "-M",
        "--name-status",
        repo.sha("rename"),
    ]);
    assert!(
        names.starts_with("R100\tdocs/guide.md\tdocs/manual/guide.md"),
        "{names}"
    );

    // Read-only Git reads leave the captured state byte-identical.
    let before = repo.state();
    repo.git(&["log", "--all", "--graph", "--oneline"]);
    repo.git(&["status", "--porcelain"]);
    assert_eq!(repo.state(), before);
    assert!(before.status.contains("# branch.head main"));
    assert!(!before.status.contains("# branch.upstream"));
}

#[test]
fn history_fixture_v99_detached_head_fixed_clock_and_edge_repos() {
    let fixture = Fixture::new();
    // 2026-10-18 20:01:00 UTC
    let epoch = 1_792_353_660;
    let repo = HistoryRepo::build(
        &fixture,
        HistoryOptions {
            clock: HistoryClock::Fixed(epoch),
            detached_at: Some("release"),
        },
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), repo.sha("release"));
    let symbolic = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo.repo)
        .args(["symbolic-ref", "-q", "HEAD"])
        .output()
        .unwrap();
    assert!(!symbolic.status.success());
    assert!(repo.state().head.ends_with(repo.sha("release")));
    assert_eq!(repo.commit("tweak").author_time, epoch - 6 * 3600);
    assert_eq!(utc_label(epoch), "18 Oct 20:01 UTC");
    assert_eq!(repo.utc("tweak"), "18 Oct 14:01 UTC");
    assert_eq!(utc_label(0), "1 Jan 00:00 UTC");

    let unborn = Fixture::new();
    let path = HistoryRepo::unborn(&unborn);
    let head = unborn.git(&["symbolic-ref", "HEAD"]);
    assert_eq!(head.trim(), "refs/heads/main");
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&path)
        .args(["rev-parse", "--verify", "-q", "HEAD"])
        .output()
        .unwrap()
        .stdout
        .is_empty());

    let single = Fixture::new();
    let sha = HistoryRepo::root_only(&single);
    assert_eq!(single.git(&["rev-list", "--all"]).trim(), sha);
    assert_eq!(single.git(&["log", "--format=%P"]).trim(), "");

    let linear = Fixture::new();
    let ids = HistoryRepo::linear(&linear, 1200);
    assert_eq!(ids.len(), 1200);
    assert_eq!(linear.git(&["rev-parse", "HEAD"]).trim(), ids[1199]);
    assert_eq!(
        linear.git(&["log", "-1", "--format=%s"]).trim(),
        "Linear commit 01200"
    );
    assert_eq!(linear.git(&["status", "--porcelain"]), "");
}

const WIDTH: usize = 100;

fn framed(inner: &str) -> String {
    let visible = strip(inner).chars().count();
    format!("│{inner}{}│", " ".repeat(WIDTH - 2 - visible))
}

fn strip(text: &str) -> String {
    let mut out = String::new();
    let mut escape = false;
    for c in text.chars() {
        match (escape, c) {
            (false, '\x1b') => escape = true,
            (true, 'm') => escape = false,
            (true, _) => {}
            (false, c) => out.push(c),
        }
    }
    out
}

fn chip(text: &str, color: u8) -> String {
    format!("\x1b[{color}m {text} \x1b[0m")
}

fn pad(text: &str, width: usize) -> String {
    format!("{text}{}", " ".repeat(width - strip(text).chars().count()))
}

/// Interior columns: bar 1, GRAPH 2, COMMIT 10, MESSAGE 19, AUTHOR 75, AGE 93..97.
fn table_row(bar: &str, graph: &str, sha: &str, message: &str, author: &str, age: &str) -> String {
    framed(&format!(
        "{bar}{}{}{}{}{age:>4}",
        pad(graph, 8),
        pad(sha, 9),
        pad(message, 56),
        pad(author, 18)
    ))
}

fn mock_frame() -> vt100::Screen {
    let title = "╭─ History › repo › all branches ";
    let right = " 2 matches · n next ─╮";
    let fill = WIDTH - title.chars().count() - right.chars().count();
    let lines = [
        format!("{title}{}{right}", "─".repeat(fill)),
        framed(" /  parser   path: src/ ✕   all branches    matches message · author · path · SHA"),
        framed(&"─".repeat(WIDTH - 2)),
        framed(&format!(
            " {}{}{}{}{:>4}",
            pad("GRAPH", 8),
            pad("COMMIT", 9),
            pad("MESSAGE", 56),
            pad("AUTHOR", 18),
            "AGE"
        )),
        table_row(
            " ",
            "●",
            "a4a6e45",
            &format!(
                "{}{} {} Tweak the README wording",
                chip("HEAD", 46),
                chip("main", 46),
                chip("origin/main", 45)
            ),
            "\x1b[41mAL\x1b[0m Ada Lovelace",
            "6h",
        ),
        table_row(
            " ",
            "│ ●",
            "9f31c2a",
            "\x1b[2mSketch the wip idea\x1b[0m",
            "AL Ada Lovelace",
            "7h",
        ),
        table_row(
            "▌",
            "●─╮",
            "7c0d1e2",
            &format!("{} Merge topics a, b and c", chip("v0.2.0", 43)),
            "GH Grace Hopper",
            "12h",
        ),
        table_row(
            " ",
            "●",
            "b96a9eb",
            "Add the README and guide",
            "AL Ada Lovelace",
            "3d",
        ),
        framed(""),
        format!("╰{}╯", "─".repeat(WIDTH - 2)),
        "  ✓  Read-only · 4 commits loaded · scroll for more".to_owned(),
    ];
    let mut parser = vt100::Parser::new(12, WIDTH as u16, 0);
    parser.process(lines.join("\r\n").as_bytes());
    parser.screen().clone()
}

#[test]
fn history_fixture_v99_row_filter_and_selection_parsers() {
    let screen = mock_frame();
    let rows = history_rows(&screen);
    assert_eq!(rows.len(), 4, "{:?}", screen.contents());
    assert_eq!(rows[0].graph, "●");
    assert_eq!(rows[0].sha, "a4a6e45");
    assert_eq!(rows[0].chips, ["HEAD", "main", "origin/main"]);
    assert_eq!(rows[0].message, "Tweak the README wording");
    assert_eq!(rows[0].author, "AL Ada Lovelace");
    assert_eq!(rows[0].initials.as_deref(), Some("AL"));
    assert_eq!(rows[0].age, "6h");
    assert!(!rows[0].selected);
    assert_eq!(rows[1].graph, "│ ●");
    assert!(rows[1].chips.is_empty());
    assert_eq!(rows[1].message, "Sketch the wip idea");
    assert_eq!(rows[2].graph, "●─╮");
    assert_eq!(rows[2].chips, ["v0.2.0"]);
    assert_eq!(rows[2].age, "12h");
    assert!(rows[2].selected);
    assert_eq!(rows[3].age, "3d");
    let selected = selected_history_row(&screen).unwrap();
    assert_eq!(selected.sha, "7c0d1e2");
    assert_eq!((selected.y, selected.sha_x), (6, 10));

    let filter = history_filter_row(&screen).unwrap();
    assert_eq!(filter.y, 1);
    assert_eq!(
        filter.segments,
        [
            "parser",
            "path: src/ ✕",
            "all branches",
            "matches message · author · path · SHA"
        ]
    );
    assert_eq!(filter.removable.len(), 1);
    assert_eq!(filter.removable[0].0, "path: src/");
    let (x, y) = filter.removable[0].1;
    assert_eq!(screen.cell(y, x).unwrap().contents(), "✕");
    assert_eq!(filter.matches, Some(2));
    assert!(filter.next_hint);
    assert_eq!(
        support::history_fixture::history_loaded_count(&screen.contents()),
        Some(4)
    );

    // Text-only form, as returned by Session::selected_row or a raw line.
    let row = history_row(
        "▌●─╮     7c0d1e2   v0.2.0   origin/main  Merge topics a, b and c   GH Grace Hopper   12h",
    )
    .unwrap();
    assert!(row.selected);
    assert_eq!(row.graph, "●─╮");
    assert_eq!(row.chips, ["v0.2.0", "origin/main"]);
    assert_eq!(row.message, "Merge topics a, b and c");
    assert_eq!(row.author, "GH Grace Hopper");
    assert_eq!(row.initials.as_deref(), Some("GH"));
    assert_eq!(row.age, "12h");
    let plain = history_row("│ │ ●     9f31c2a  Sketch the wip idea      AL Ada Lovelace    7h  │")
        .unwrap();
    assert_eq!(
        (plain.graph.as_str(), plain.sha.as_str(), plain.selected),
        ("│ ●", "9f31c2a", false)
    );
    assert!(plain.chips.is_empty());
    assert!(history_row(" GRAPH   COMMIT   MESSAGE   AUTHOR   AGE").is_none());
    assert!(history_row(" 111 commits on 6 branches").is_none());
}
