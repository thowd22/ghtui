use super::*;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;

fn commit(id: &str, parents: &[&str]) -> LaneCommit {
    LaneCommit::new(id, parents.iter().copied())
}

fn texts(rows: &[GraphRow]) -> Vec<String> {
    rows.iter()
        .map(|row| row.text().trim_end().to_owned())
        .collect()
}

fn lay(commits: &[LaneCommit]) -> Vec<GraphRow> {
    layout(commits, LaneOptions::default()).unwrap()
}

#[test]
fn history_lanes_v991_branch_fork_and_merge_match_the_mockup_shapes() {
    let commits = [
        commit("m1", &["m2"]).with_labels(["HEAD", "main"]),
        commit("m2", &["m3"]),
        commit("f1", &["f2"]).with_labels(["agent/stale-worktree"]),
        commit("f2", &["m4"]),
        commit("m3", &["m4"]).with_labels(["v0.1.0"]),
        commit("m4", &["m5"]),
        commit("m5", &["a1", "l1"]),
        commit("l1", &["l2"]).with_labels(["origin/agent/actions-logs"]),
        commit("l2", &["m7"]),
        commit("a1", &["m7"]),
        commit("m7", &["m8"]),
        commit("m8", &[]),
    ];
    let rows = lay(&commits);
    assert_eq!(
        texts(&rows),
        [
            "●",
            "●",
            "│ ●",
            "│ ●",
            "● │",
            "●─╯",
            "●─╮",
            "│ ●",
            "│ ●",
            "● │",
            "●─╯",
            "●"
        ]
    );
    assert!(rows[0].tip && rows[2].tip && !rows[1].tip);
    assert!(rows[11].root && !rows[10].root);
    let labels: Vec<_> = rows.iter().map(|row| row.label.as_deref()).collect();
    assert_eq!(labels[4], Some("HEAD"), "v0.1.0 sits on the HEAD lane");
    assert_eq!(labels[3], Some("agent/stale-worktree"));
    assert_eq!(labels[8], Some("origin/agent/actions-logs"));
    // The fork lane keeps its color down to its convergence connector.
    let fork = rows[2].slot;
    assert_ne!(fork, rows[0].slot);
    assert_eq!(rows[3].cells[2].slot, Some(fork));
    assert_eq!(rows[5].cells[1].slot, Some(fork));
    assert_eq!(rows[5].cells[2].slot, Some(fork));
    // The merged lane gets a fresh slot used by its connector and its commits.
    assert_eq!(rows[6].cells[2].slot, Some(rows[7].slot));
    assert_eq!(rows[7].slot, rows[8].slot);
    assert_eq!(
        rows[6].spans(),
        [
            ("●".to_owned(), Some(rows[6].slot)),
            ("─╮".to_owned(), Some(rows[7].slot))
        ]
    );
}

#[test]
fn history_lanes_v991_crossings_holes_joins_and_octopus_connectors() {
    let crossing = lay(&[
        commit("a", &["b"]),
        commit("c", &["d"]),
        commit("b", &["e", "f"]),
        commit("d", &["e"]),
        commit("e", &["g"]),
        commit("f", &["g"]),
        commit("g", &[]),
    ]);
    assert_eq!(
        texts(&crossing),
        ["●", "│ ●", "●─┿─╮", "│ ● │", "●─╯ │", "│   ●", "●───╯"]
    );
    let join = lay(&[
        commit("a", &["m"]),
        commit("t", &["x"]),
        commit("x", &["y", "m"]),
        commit("y", &["m"]),
        commit("m", &[]),
    ]);
    assert_eq!(texts(&join), ["●", "│ ●", "├─●", "│ ●", "●─╯"]);
    let octopus = lay(&[
        commit("o", &["p1", "p2", "p3", "p4"]),
        commit("p4", &["r"]),
        commit("p3", &["r"]),
        commit("p2", &["r"]),
        commit("p1", &["r"]),
        commit("r", &[]),
    ]);
    assert_eq!(
        texts(&octopus),
        [
            "●─┬─┬─╮",
            "│ │ │ ●",
            "│ │ ● │",
            "│ ● │ │",
            "● │ │ │",
            "●─┴─┴─╯"
        ]
    );
    let slots: BTreeSet<_> = octopus[0].cells.iter().filter_map(|c| c.slot).collect();
    assert_eq!(slots.len(), 4, "each octopus parent lane has its own slot");
    // Octopus parents already awaited by other lanes join them on both sides.
    let joined = lay(&[
        commit("l", &["p1"]),
        commit("o", &["x", "p1", "p2"]),
        commit("r", &["p2"]),
        commit("x", &["p1"]),
        commit("p1", &["p2"]),
        commit("p2", &[]),
    ]);
    assert_eq!(joined[1].text().trim_end(), "├─●─╮");
    assert_eq!(joined[2].text().trim_end(), "│ │ │ ●");
    assert_eq!(joined[3].text().trim_end(), "│ ● │ │");
}

#[test]
fn history_lanes_v991_roots_cool_their_column_and_boundaries_end_lanes() {
    let rows = lay(&[
        commit("a", &[]),
        commit("b", &[]),
        commit("c", &["d"]),
        commit("d", &[]),
    ]);
    assert_eq!(texts(&rows), ["●", "  ●", "●", "●"]);
    let tips: Vec<_> = rows.iter().map(|row| row.tip).collect();
    assert_eq!(tips, [true, true, true, false]);
    let mut layout = LaneLayout::default();
    let cut = layout
        .push(&commit("s", &["hidden"]).with_boundary(true))
        .unwrap();
    assert!(cut.boundary && cut.root);
    assert_eq!(
        layout.open_lanes(),
        0,
        "a shallow boundary invents no parent edge"
    );
    let next = layout.push(&commit("t", &[])).unwrap();
    assert_eq!(
        next.column, 1,
        "the boundary column stays blank for one row"
    );
}

#[test]
fn history_lanes_v991_rejects_duplicates_upward_edges_and_lane_overflow() {
    let mut layout = LaneLayout::default();
    layout.push(&commit("a", &["b"])).unwrap();
    assert!(layout
        .push(&commit("a", &["b"]))
        .unwrap_err()
        .contains("repeats"));
    let mut layout = LaneLayout::default();
    layout.push(&commit("a", &[])).unwrap();
    assert!(layout
        .push(&commit("b", &["a"]))
        .unwrap_err()
        .contains("above"));
    let mut layout = LaneLayout::new(LaneOptions {
        max_lanes: 3,
        ..LaneOptions::default()
    });
    for id in ["t1", "t2", "t3"] {
        layout.push(&commit(id, &["base"])).unwrap();
    }
    assert!(layout
        .push(&commit("t4", &["base"]))
        .unwrap_err()
        .contains("3 lanes"));
}

#[test]
fn history_lanes_v991_width_cap_ends_rows_in_an_overflow_marker() {
    let mut commits: Vec<_> = (0..6)
        .map(|n| commit(&format!("t{n}"), &["base"]))
        .collect();
    commits.push(commit("base", &[]));
    let options = LaneOptions {
        max_columns: 3,
        ..LaneOptions::default()
    };
    let rows = layout(&commits, options).unwrap();
    assert_eq!(
        texts(&rows),
        ["●", "│ ●", "│ │ ●", "│ │ ●…", "│ │ ●…", "│ │ ●…", "●─┴─…"]
    );
    for row in &rows {
        assert!(row.cells.len() <= 2 * options.max_columns);
        assert_eq!(row.overflow, row.width > 3, "{}", row.text());
    }
    assert_eq!(rows[5].width, 6);
    assert_eq!(rows[5].column, 5);
    let marker = rows[6].cells.last().unwrap();
    assert_eq!((marker.glyph, marker.slot), ('…', None));
    let full = layout(&commits, LaneOptions::default()).unwrap();
    assert_eq!(full[6].text(), "●─┴─┴─┴─┴─╯");
    assert!(full.iter().all(|row| !row.overflow));
}

#[test]
fn history_lanes_v991_slots_rotate_through_the_palette_and_skip_open_lanes() {
    let options = LaneOptions {
        palette: 3,
        ..LaneOptions::default()
    };
    let rows = layout(
        &[
            commit("a", &["z"]),
            commit("b", &["z"]),
            commit("c", &["z"]),
            commit("z", &["y"]),
            commit("d", &["y"]),
            commit("y", &[]),
        ],
        options,
    )
    .unwrap();
    let slots: Vec<_> = rows.iter().map(|row| row.slot).collect();
    // a,b,c take 0,1,2; z keeps a's slot; d rotates to 0 but 0 is open, so 1.
    assert_eq!(slots, [0, 1, 2, 0, 1, 0]);
    assert!(rows
        .iter()
        .flat_map(|row| &row.cells)
        .all(|cell| cell.slot.is_none_or(|s| s < 3)));
}

// ---- Real Git DAG fixtures -------------------------------------------------

struct Repo {
    dir: tempfile::TempDir,
    root: std::path::PathBuf,
    clock: std::cell::Cell<u64>,
}

fn run_git(root: &Path, args: &[&str], date: Option<u64>) -> String {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Lane Fixture")
        .env("GIT_AUTHOR_EMAIL", "lanes@example.invalid")
        .env("GIT_COMMITTER_NAME", "Lane Fixture")
        .env("GIT_COMMITTER_EMAIL", "lanes@example.invalid");
    if let Some(date) = date {
        let date = format!("@{} +0000", 1_700_000_000 + date * 60);
        command
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "Installed Git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

impl Repo {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("work");
        std::fs::create_dir(&root).unwrap();
        run_git(&root, &["init", "-q", "-b", "main"], None);
        Self {
            dir,
            root,
            clock: std::cell::Cell::new(0),
        }
    }

    fn git(&self, args: &[&str]) -> String {
        self.clock.set(self.clock.get() + 1);
        run_git(&self.root, args, Some(self.clock.get()))
    }

    /// Commits a distinct file so octopus merges never conflict.
    fn commit(&self, name: &str) -> String {
        std::fs::write(self.root.join(format!("{name}.txt")), name).unwrap();
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", name]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn merge(&self, args: &[&str]) {
        let mut all = vec!["merge", "-q", "--no-ff", "--no-edit"];
        all.extend_from_slice(args);
        self.git(&all);
    }

    fn worktree_status(&self) -> String {
        run_git(&self.root, &["status", "--porcelain=v1", "--branch"], None)
    }
}

/// `git log --all` in the given order, with ref labels and shallow boundaries.
fn read_dag(root: &Path, order: &str) -> Vec<LaneCommit> {
    let shallow: BTreeSet<String> = std::fs::read_to_string(root.join(".git/shallow"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect();
    let log = run_git(
        root,
        &["log", "--all", order, "--format=%H%x09%P%x09%D"],
        None,
    );
    log.lines()
        .map(|line| {
            let mut fields = line.split('\t');
            let id = fields.next().unwrap().to_owned();
            let parents = fields.next().unwrap_or("").split_whitespace();
            let labels = fields
                .next()
                .unwrap_or("")
                .split(", ")
                .filter(|label| !label.is_empty())
                .flat_map(|label| match label.strip_prefix("HEAD -> ") {
                    Some(branch) => vec!["HEAD".to_owned(), branch.to_owned()],
                    None => vec![label.trim_start_matches("tag: ").to_owned()],
                });
            let boundary = shallow.contains(&id);
            LaneCommit::new(id, parents)
                .with_labels(labels)
                .with_boundary(boundary)
        })
        .collect()
}

/// Children (and whether this is their first-parent edge) flowing down a lane, with its slot.
type Carried = (Vec<(String, bool)>, Option<usize>);

const RECEIVES: &[char] = &['│', '┿', '●', '╯', '╰', '┴', '┤', '├', '┼'];

#[derive(Default, Debug, PartialEq, Eq)]
struct Decoded {
    first: Option<String>,
    all: BTreeSet<String>,
}

/// Reads parentage back from the drawn glyphs alone, the way a person follows
/// the lines: a line entering a commit's dot or converging into its row comes
/// from a child; `├ ┤ ┼` add an edge into a passing lane, `╭ ╮ ┬` start one.
/// Also checks every line is continuous and keeps its color slot.
fn decode(commits: &[LaneCommit], rows: &[GraphRow]) -> HashMap<String, Decoded> {
    let mut carry: Vec<Option<Carried>> = Vec::new();
    let mut decoded: HashMap<String, Decoded> = HashMap::new();
    let glyphs =
        |row: &GraphRow| -> Vec<GraphCell> { row.cells.iter().step_by(2).copied().collect() };
    for (r, row) in rows.iter().enumerate() {
        let id = &commits[r].id;
        let cells = glyphs(row);
        assert!(!row.overflow);
        carry.resize(carry.len().max(cells.len()), None);
        for (k, lane) in carry.iter().enumerate() {
            let glyph = cells.get(k).map_or(' ', |cell| cell.glyph);
            match lane {
                Some((_, slot)) => {
                    assert!(
                        RECEIVES.contains(&glyph),
                        "row {r} col {k}: line lost at {glyph:?}"
                    );
                    if glyph == '│' {
                        assert_eq!(cells[k].slot, *slot, "row {r} col {k}: lane changed color");
                    }
                }
                None => assert!(
                    !RECEIVES.contains(&glyph) || glyph == '●',
                    "row {r} col {k}: line from nowhere {glyph:?}"
                ),
            }
        }
        let dots: Vec<_> = (0..cells.len())
            .filter(|k| cells[*k].glyph == '●')
            .collect();
        assert_eq!(dots, [row.column], "row {r}: one dot");
        let column = row.column;
        let mut arrive = |lane: Option<Carried>| {
            for (child, first) in lane.map(|(children, _)| children).unwrap_or_default() {
                let entry = decoded.entry(child).or_default();
                assert!(entry.all.insert(id.clone()), "duplicate edge");
                if first {
                    assert!(entry.first.replace(id.clone()).is_none());
                }
            }
        };
        if let Some((_, slot)) = &carry[column] {
            assert_eq!(Some(row.slot), *slot, "row {r}: commit lane changed color");
        }
        arrive(carry[column].take());
        for k in 0..cells.len() {
            match cells[k].glyph {
                '╯' | '╰' | '┴' => arrive(carry[k].take()),
                '┤' | '├' | '┼' => {
                    let (children, slot) = carry[k].as_mut().expect("join into a live lane");
                    assert_eq!(cells[k].slot, *slot, "join keeps the lane color");
                    children.push((id.clone(), false));
                }
                '╮' | '╭' | '┬' => {
                    carry[k] = Some((vec![(id.clone(), false)], cells[k].slot))
                }
                _ => {}
            }
        }
        // Horizontal runs connect the dot to every connector without gaps.
        let reach: Vec<usize> = (0..cells.len())
            .filter(|k| "╯╰┴┤├┼╮╭┬".contains(cells[*k].glyph))
            .collect();
        for k in reach {
            let (low, high) = (k.min(column), k.max(column));
            for gap in low..high {
                assert_eq!(row.cells[2 * gap + 1].glyph, '─', "row {r}: broken run");
            }
        }
        let continues = rows.get(r + 1).is_some_and(|next| {
            next.cells
                .get(2 * column)
                .is_some_and(|cell| RECEIVES.contains(&cell.glyph))
        });
        assert_eq!(
            continues, !row.root,
            "row {r}: lane end matches the drawing"
        );
        if continues {
            carry[column] = Some((vec![(id.clone(), true)], Some(row.slot)));
        }
    }
    assert!(
        carry.iter().all(Option::is_none),
        "complete DAGs close every lane"
    );
    decoded
}

/// Lays out the fixture in both Git display orders and compares the drawn
/// parentage with Git's, plus paging continuity at every split point.
fn assert_matches_git(repo_root: &Path) -> Vec<(Vec<LaneCommit>, Vec<GraphRow>)> {
    let mut results = Vec::new();
    for order in ["--topo-order", "--date-order"] {
        let commits = read_dag(repo_root, order);
        assert!(commits.len() > 3);
        let rows = lay(&commits);
        let decoded = decode(&commits, &rows);
        for commit in &commits {
            let got = decoded.get(&commit.id);
            if commit.boundary || commit.parents.is_empty() {
                assert!(got.is_none(), "{order}: {} has no drawn parents", commit.id);
                continue;
            }
            let got = got.unwrap_or_else(|| panic!("{order}: {} lost its parents", commit.id));
            assert_eq!(
                got.first.as_ref(),
                commit.parents.first(),
                "{order}: first parent"
            );
            let want: BTreeSet<_> = commit.parents.iter().cloned().collect();
            assert_eq!(got.all, want, "{order}: parents of {}", commit.id);
        }
        assert_paging_continuity(&commits, &rows);
        results.push((commits, rows));
    }
    results
}

fn assert_paging_continuity(commits: &[LaneCommit], full: &[GraphRow]) {
    for split in 0..=commits.len() {
        let mut layout = LaneLayout::default();
        let mut paged = layout.extend(&commits[..split]).unwrap();
        let resumed = layout.clone();
        paged.extend(layout.extend(&commits[split..]).unwrap());
        assert_eq!(paged, full, "split at {split}");
        // A snapshot taken between pages resumes identically.
        let mut again = resumed;
        assert_eq!(again.extend(&commits[split..]).unwrap(), full[split..]);
    }
    for page in [1, 2, 3, 5] {
        let mut layout = LaneLayout::default();
        let mut paged = Vec::new();
        for chunk in commits.chunks(page) {
            paged.extend(layout.extend(chunk).unwrap());
        }
        assert_eq!(paged, full, "pages of {page}");
    }
}

#[test]
fn history_lanes_v991_real_git_branches_merges_tags_and_remote_refs() {
    let repo = Repo::new();
    repo.commit("init");
    repo.commit("base");
    repo.git(&["switch", "-q", "-c", "feature"]);
    repo.commit("f1");
    repo.commit("f2");
    repo.git(&["switch", "-q", "main"]);
    repo.commit("m1");
    repo.git(&["tag", "v0.1.0"]);
    repo.git(&["switch", "-q", "-c", "topic"]);
    repo.commit("t1");
    repo.git(&["switch", "-q", "main"]);
    repo.merge(&["feature"]);
    repo.commit("m2");
    repo.git(&["switch", "-q", "-c", "unmerged", "v0.1.0"]);
    repo.commit("u1");
    repo.git(&["switch", "-q", "main"]);
    repo.merge(&["topic"]);
    let remote = repo.dir.path().join("remote.git");
    run_git(
        repo.dir.path(),
        &["init", "-q", "--bare", "remote.git"],
        None,
    );
    repo.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
    repo.git(&["push", "-q", "origin", "main", "feature"]);
    repo.git(&["fetch", "-q", "origin"]);
    repo.commit("local-only");
    let status = repo.worktree_status();
    let results = assert_matches_git(&repo.root);
    assert_eq!(
        repo.worktree_status(),
        status,
        "layout reads never touch the worktree"
    );
    let (commits, rows) = &results[0];
    assert_eq!(rows[0].label.as_deref(), Some("HEAD"));
    let unmerged = commits
        .iter()
        .position(|c| c.labels.contains(&"unmerged".to_owned()));
    let unmerged = &rows[unmerged.unwrap()];
    assert!(unmerged.tip && unmerged.column > 0);
    assert_eq!(unmerged.label.as_deref(), Some("unmerged"));
    assert!(commits
        .iter()
        .any(|c| c.labels.contains(&"origin/main".to_owned())));
    assert!(commits
        .iter()
        .any(|c| c.labels.contains(&"v0.1.0".to_owned())));
}

#[test]
fn history_lanes_v991_real_git_octopus_and_criss_cross_merges() {
    let repo = Repo::new();
    repo.commit("init");
    for branch in ["a", "b", "c"] {
        repo.git(&["switch", "-q", "-c", branch, "main"]);
        repo.commit(branch);
        repo.commit(&format!("{branch}2"));
    }
    repo.git(&["switch", "-q", "main"]);
    repo.commit("m1");
    repo.merge(&["a", "b", "c"]);
    let octopus = repo.git(&["rev-parse", "HEAD"]);
    assert_eq!(
        repo.git(&["rev-list", "--parents", "-n1", "HEAD"])
            .split(' ')
            .count(),
        5
    );
    // Criss-cross: x and y each merge the other's pre-merge commit.
    repo.git(&["switch", "-q", "-c", "x"]);
    let x1 = repo.commit("x1");
    repo.git(&["switch", "-q", "-c", "y", &octopus]);
    let y1 = repo.commit("y1");
    repo.merge(&[&x1]);
    repo.git(&["switch", "-q", "x"]);
    repo.merge(&[&y1]);
    repo.commit("x2");
    repo.git(&["switch", "-q", "y"]);
    repo.commit("y2");
    repo.git(&["switch", "-q", "main"]);
    repo.merge(&["x", "y"]);
    let results = assert_matches_git(&repo.root);
    for (commits, rows) in &results {
        let at = commits.iter().position(|c| c.id == octopus).unwrap();
        let text = rows[at].text();
        assert_eq!(
            text.chars().filter(|g| "╮┬┤┼".contains(*g)).count(),
            3,
            "octopus row draws three later-parent connectors: {text}"
        );
    }
}

#[test]
fn history_lanes_v991_real_git_unrelated_roots_and_detached_head() {
    let repo = Repo::new();
    repo.commit("init");
    let old = repo.commit("old");
    repo.commit("m1");
    repo.git(&["switch", "-q", "--orphan", "docs"]);
    repo.commit("docs-root");
    repo.commit("docs-1");
    repo.git(&["switch", "-q", "-f", "main"]);
    repo.git(&[
        "merge",
        "-q",
        "--no-edit",
        "--allow-unrelated-histories",
        "docs",
    ]);
    repo.git(&["switch", "-q", "-c", "island"]);
    repo.git(&["switch", "-q", "--orphan", "island2"]);
    repo.commit("island-root");
    repo.git(&["switch", "-q", "-f", "--detach", &old]);
    let detached = repo.commit("detached");
    let results = assert_matches_git(&repo.root);
    for (commits, rows) in &results {
        let roots = rows.iter().filter(|row| row.root).count();
        assert_eq!(roots, 3, "init, docs-root and island-root");
        let at = commits.iter().position(|c| c.id == detached).unwrap();
        assert!(rows[at].tip, "detached HEAD starts its own lane");
        assert_eq!(rows[at].label.as_deref(), Some("HEAD"));
        assert_eq!(commits[at].labels, ["HEAD"]);
    }
}

#[test]
fn history_lanes_v991_real_git_shallow_clone_boundaries() {
    let repo = Repo::new();
    repo.commit("init");
    for n in 0..4 {
        repo.commit(&format!("m{n}"));
    }
    repo.git(&["switch", "-q", "-c", "side", "HEAD~2"]);
    repo.commit("s1");
    repo.git(&["switch", "-q", "main"]);
    repo.merge(&["side"]);
    repo.commit("tip");
    let source = format!("file://{}", repo.root.display());
    run_git(
        repo.dir.path(),
        &[
            "clone",
            "-q",
            "--no-local",
            "--depth",
            "3",
            "--no-single-branch",
            &source,
            "shallow",
        ],
        None,
    );
    let shallow = repo.dir.path().join("shallow");
    let results = assert_matches_git(&shallow);
    for (commits, rows) in &results {
        let boundaries: Vec<_> = commits.iter().filter(|c| c.boundary).collect();
        assert!(!boundaries.is_empty(), "the clone has a shallow boundary");
        for (commit, row) in commits.iter().zip(rows) {
            assert_eq!(row.boundary, commit.boundary);
            if commit.boundary {
                assert!(row.root, "boundary commits end their lane");
            }
        }
    }
}
