//! TASK-99.1: History lanes over real temporary Git repositories. Every drawn
//! edge is traced back through the rendered lane cells and compared with the
//! parentage `git log --graph` reports for the same branches, remotes, tags and HEAD.
use super::*;
use crate::{
    git::{GitRepository, OriginalGraphReadV1},
    history::lanes::GraphRow,
    local_commit_actor_origin_v1_test_support::{git, text},
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
};
use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
};

fn init(directory: &Path, name: &str) -> PathBuf {
    let root = directory.join(name);
    std::fs::create_dir(&root).unwrap();
    git(&root, &["init", "-q", "--initial-branch=main"], None);
    git(&root, &["config", "user.name", "Lane Writer"], None);
    git(
        &root,
        &["config", "user.email", "lanes@example.invalid"],
        None,
    );
    git(&root, &["config", "commit.gpgsign", "false"], None);
    root
}
fn commit(root: &Path, subject: &str) -> String {
    git(
        root,
        &["commit", "-q", "--allow-empty", "-m", subject],
        None,
    );
    text(root, &["rev-parse", "HEAD"])
}
/// Every page of the all-branches walk opened at `tip`.
fn walk(root: &Path, tip: &str) -> Arc<OriginalGraphStateV1> {
    let repository = GitRepository::discover(root).unwrap().unwrap();
    let origin = OriginalLocalActorOriginV1::capture(&repository).unwrap();
    let operation = LocalGitNavigationReadV1::begin(&origin, Default::default()).unwrap();
    let state = origin
        .with_saved_navigation_read_v1(&operation, |handle| {
            let page = handle
                .original_graph_page_v1(OriginalGraphReadV1::Start(tip), operation.deadline())
                .map_err(|error| error.to_string())?;
            let mut state = OriginalGraphStateV1::start(origin.clone(), page)?;
            while state.last_page().has_more() {
                let page = handle
                    .original_graph_page_v1(
                        OriginalGraphReadV1::Continue(state.last_page()),
                        operation.deadline(),
                    )
                    .map_err(|error| error.to_string())?;
                state = OriginalGraphStateV1::append(&state, page)?;
            }
            Ok(state)
        })
        .unwrap();
    operation.seal().unwrap();
    state
}
/// `id -> parents` and the id order from `git log --graph` over the same tips.
fn git_graph(root: &Path) -> (Vec<String>, HashMap<String, Vec<String>>) {
    let raw = text(
        root,
        &[
            "log",
            "--graph",
            "--format=%H %P",
            "--branches",
            "--remotes",
            "--tags",
            "HEAD",
        ],
    );
    let mut order = Vec::new();
    let mut parents = HashMap::new();
    for line in raw.lines() {
        let ids = line
            .split(' ')
            .filter(|word| word.len() >= 40 && word.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if let Some((id, rest)) = ids.split_first() {
            order.push(id.clone());
            parents.insert(id.clone(), rest.to_vec());
        }
    }
    (order, parents)
}
fn glyph(row: &GraphRow, lane: usize) -> char {
    row.cells.get(2 * lane).map_or(' ', |cell| cell.glyph)
}
/// Follows lane `lane` down from `rows[from]` to the commit it reaches.
fn follow(rows: &[GraphRow], ids: &[String], lane: usize, from: usize) -> Option<String> {
    for (index, row) in rows.iter().enumerate().skip(from) {
        match glyph(row, lane) {
            '●' if row.column == lane => return Some(ids[index].clone()),
            '╯' | '╰' | '┴' => return Some(ids[index].clone()),
            '│' | '┤' | '├' | '┼' => {}
            other => panic!(
                "lane {lane} breaks at row {index} ({other:?}): {}",
                row.text()
            ),
        }
    }
    None
}
/// Parents as drawn: the first parent continues the dot's lane; later
/// parents leave on the same row through `╮ ╭ ┬` (new lane) or `┤ ├ ┼` (join).
fn drawn_parents(rows: &[GraphRow], ids: &[String]) -> HashMap<String, Vec<String>> {
    let mut drawn = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        let mut parents = Vec::new();
        if !row.root {
            parents.extend(follow(rows, ids, row.column, index + 1));
        }
        for lane in 0..row.cells.len().div_ceil(2) {
            if lane != row.column && matches!(glyph(row, lane), '╮' | '╭' | '┬' | '┤' | '├' | '┼')
            {
                parents.extend(follow(rows, ids, lane, index + 1));
            }
        }
        drawn.insert(ids[index].clone(), parents);
    }
    drawn
}
fn ids(state: &OriginalGraphStateV1) -> Vec<String> {
    (0..state.loaded())
        .map(|index| state.member_at_v2(index).unwrap().id.clone())
        .collect()
}
/// A lane keeps its color from one row to the next: every `│` and every
/// non-tip dot continues a cell of the same palette slot directly above it.
fn assert_continuous(rows: &[GraphRow]) {
    for pair in rows.windows(2) {
        let (above, below) = (&pair[0], &pair[1]);
        for lane in 0..below.cells.len().div_ceil(2) {
            let cell = below.cells[2 * lane];
            let continues = cell.glyph == '│' || (cell.glyph == '●' && !below.tip);
            if !continues {
                continue;
            }
            let over = above.cells.get(2 * lane).copied();
            assert!(
                over.is_some_and(|over| over.glyph != ' ' && over.glyph != '─'),
                "lane {lane} has a gap between\n{}\n{}",
                above.text(),
                below.text()
            );
            assert_eq!(
                over.unwrap().slot,
                cell.slot,
                "lane {lane} changes color between\n{}\n{}",
                above.text(),
                below.text()
            );
        }
    }
}
fn assert_matches_git(root: &Path, state: &OriginalGraphStateV1) -> Vec<GraphRow> {
    let rows = state.lane_rows_v991(0, state.loaded()).unwrap();
    let ids = ids(state);
    let (order, expected) = git_graph(root);
    assert_eq!(
        ids.iter().collect::<BTreeSet<_>>(),
        order.iter().collect::<BTreeSet<_>>(),
        "History walks exactly the commits of git log --branches --remotes --tags HEAD"
    );
    let drawn = drawn_parents(&rows, &ids);
    for id in &ids {
        let commit = state.selected_commit(id).unwrap();
        let boundary = state.last_page().epoch().is_shallow_commit(id);
        let wanted = expected[id].clone();
        if boundary {
            // Git grafts the shallow boundary to no parents; the object keeps them.
            assert!(wanted.is_empty(), "git log --graph ends at {id}");
        } else {
            assert_eq!(commit.parents, wanted, "object parents of {id}");
        }
        assert_eq!(
            drawn[id].iter().collect::<BTreeSet<_>>(),
            wanted.iter().collect::<BTreeSet<_>>(),
            "drawn parents of {} ({})",
            &id[..8],
            commit.subject
        );
        if let Some(first) = wanted.first() {
            assert_eq!(
                drawn[id].first(),
                Some(first),
                "first parent keeps the lane"
            );
        }
    }
    assert_continuous(&rows);
    rows
}
fn row_of<'a>(state: &OriginalGraphStateV1, rows: &'a [GraphRow], id: &str) -> &'a GraphRow {
    &rows[state.loaded_member_v2(id).unwrap().node as usize]
}
fn read_only_state(root: &Path) -> Vec<String> {
    vec![
        text(root, &["rev-parse", "HEAD"]),
        text(root, &["show-ref", "--head"]),
        text(root, &["status", "--porcelain=v2", "--branch"]),
        text(root, &["config", "--list", "--local"]),
    ]
}

#[test]
fn history_lanes_v991_branches_merge_tag_remote_and_detached_head_match_git_log_graph() {
    let directory = tempfile::tempdir().unwrap();
    let root = init(directory.path(), "repo");
    let first = commit(&root, "Lane root");
    let fork = commit(&root, "Lane main one");
    git(&root, &["switch", "-q", "-c", "feature"], None);
    let feature = [
        commit(&root, "Lane feature one"),
        commit(&root, "Lane feature two"),
    ];
    git(&root, &["switch", "-q", "main"], None);
    let main_two = commit(&root, "Lane main two");
    git(&root, &["tag", "v0.1.0", &main_two], None);
    git(
        &root,
        &[
            "merge",
            "-q",
            "--no-ff",
            "-m",
            "Lane merge feature",
            "feature",
        ],
        None,
    );
    let merge = text(&root, &["rev-parse", "HEAD"]);
    // A local bare remote with a branch that exists only as origin/remote-only.
    let bare = directory.path().join("remote.git");
    git(
        directory.path(),
        &["init", "-q", "--bare", bare.to_str().unwrap()],
        None,
    );
    git(
        &root,
        &["remote", "add", "origin", bare.to_str().unwrap()],
        None,
    );
    git(
        &root,
        &["switch", "-q", "-c", "remote-only", &main_two],
        None,
    );
    let remote_only = commit(&root, "Lane remote only");
    git(
        &root,
        &["push", "-q", "origin", "main", "remote-only"],
        None,
    );
    git(&root, &["fetch", "-q", "origin"], None);
    git(&root, &["switch", "-q", "main"], None);
    git(&root, &["branch", "-q", "-D", "remote-only"], None);
    // Detached HEAD on a commit no branch or tag names.
    git(&root, &["switch", "-q", "--detach", &merge], None);
    let detached = commit(&root, "Lane detached head");
    std::fs::write(root.join("scratch"), b"untracked bytes\n").unwrap();
    let before = read_only_state(&root);

    let state = walk(&root, &detached);
    assert_eq!(state.loaded(), 8);
    for id in [
        &first,
        &fork,
        &feature[0],
        &feature[1],
        &main_two,
        &merge,
        &remote_only,
        &detached,
    ] {
        assert!(state.selected_commit(id).is_some(), "{id} is in the walk");
    }
    let rows = assert_matches_git(&root, &state);
    // The merge joins its second parent on its own row: `●─╮`, never a separate connector row.
    let merge_text = row_of(&state, &rows, &merge).text();
    let tail = merge_text.split_once('●').unwrap().1;
    let link = tail.trim_start_matches(['─', '│']).chars().next();
    assert!(
        tail.starts_with('─') && matches!(link, Some('╮' | '┬' | '┤' | '┼')),
        "merge row joins on the same row: {merge_text:?}"
    );
    // The fork point closes the feature lane on its own row: `●─╯`.
    let fork_text = row_of(&state, &rows, &fork).text();
    assert!(fork_text.contains(['╯', '╰', '┴']), "{fork_text:?}");
    // The timeline has exactly one row per commit, plus the summary and coverage.
    let shown = GraphPresentationV1::prepare(&state, &detached, GraphViewV1::Timeline).unwrap();
    let commits = shown
        .rows()
        .iter()
        .filter(|row| matches!(row.action, Some(RowAction::Commit(_))))
        .count();
    assert_eq!(commits, 8);
    assert_eq!(shown.rows().len(), 8 + 2);
    assert!(shown.rows().iter().all(|row| !row.label.contains("↳")));
    // Messages start in one column: every graph column is padded to the window width.
    let widths = (0..shown.rows().len())
        .filter_map(|index| shown.paint(index))
        .filter(|paint| paint.len() > 1)
        .map(|paint| {
            paint
                .iter()
                .filter(|span| span.color.is_some())
                .map(|span| span.text.chars().count())
                .sum::<usize>()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(widths.len(), 1, "one graph column width: {widths:?}");
    assert_eq!(read_only_state(&root), before, "History reads never write");
}

#[test]
fn history_lanes_v991_octopus_and_unrelated_roots_match_git_log_graph() {
    let directory = tempfile::tempdir().unwrap();
    let root = init(directory.path(), "repo");
    let base = commit(&root, "Octopus base");
    let mut heads = Vec::new();
    for n in 0..3 {
        git(
            &root,
            &["switch", "-q", "-c", &format!("leg-{n}"), &base],
            None,
        );
        heads.push(commit(&root, &format!("Octopus leg {n}")));
    }
    git(&root, &["switch", "-q", "main"], None);
    commit(&root, "Octopus main leg");
    git(
        &root,
        &[
            "merge",
            "-q",
            "--no-ff",
            "-m",
            "Octopus merge",
            "leg-0",
            "leg-1",
            "leg-2",
        ],
        None,
    );
    let octopus = text(&root, &["rev-parse", "HEAD"]);
    assert_eq!(
        text(&root, &["rev-list", "--parents", "-n", "1", &octopus])
            .split(' ')
            .count(),
        5
    );
    // An unrelated history with its own root.
    git(&root, &["switch", "-q", "--orphan", "pages"], None);
    let orphan_root = commit(&root, "Orphan root");
    let orphan_tip = commit(&root, "Orphan tip");
    git(&root, &["switch", "-q", "main"], None);
    let before = read_only_state(&root);

    let state = walk(&root, &octopus);
    let rows = assert_matches_git(&root, &state);
    let octopus_row = row_of(&state, &rows, &octopus).text();
    // Three new lanes leave the octopus on its own row: `●─┬─┬─╮`.
    assert_eq!(
        octopus_row.matches(['┬', '╮', '┼', '┤']).count(),
        3,
        "{octopus_row:?}"
    );
    for id in [&base, &orphan_root] {
        let row = row_of(&state, &rows, id);
        assert!(row.root, "{} is a root row", &id[..8]);
    }
    assert!(row_of(&state, &rows, &orphan_tip).tip);
    assert_eq!(read_only_state(&root), before);
}

#[test]
fn history_lanes_v991_shallow_boundary_ends_its_lane_without_inventing_a_parent() {
    let directory = tempfile::tempdir().unwrap();
    let seed = init(directory.path(), "seed");
    for n in 0..4 {
        commit(&seed, &format!("Shallow seed {n}"));
    }
    git(&seed, &["switch", "-q", "-c", "side", "HEAD~1"], None);
    commit(&seed, "Shallow side");
    git(&seed, &["switch", "-q", "main"], None);
    let url = format!("file://{}", seed.display());
    git(
        directory.path(),
        &[
            "clone",
            "-q",
            "--no-single-branch",
            "--depth",
            "2",
            &url,
            "clone",
        ],
        None,
    );
    let root = directory.path().join("clone");
    let head = text(&root, &["rev-parse", "HEAD"]);
    let shallow = std::fs::read_to_string(root.join(".git/shallow")).unwrap();
    assert!(!shallow.trim().is_empty());
    let before = read_only_state(&root);

    let state = walk(&root, &head);
    let rows = assert_matches_git(&root, &state);
    for boundary in shallow.lines() {
        let row = row_of(&state, &rows, boundary);
        assert!(row.boundary && row.root, "boundary row ends its lane");
        // Git still names the real parent; the graph does not draw a guessed one.
        assert!(!state.selected_commit(boundary).unwrap().parents.is_empty());
    }
    assert_eq!(read_only_state(&root), before);
}

#[test]
fn history_lanes_v991_paging_keeps_lane_columns_and_colors_stable() {
    let directory = tempfile::tempdir().unwrap();
    let root = init(directory.path(), "repo");
    commit(&root, "Paged root");
    // A side branch merged back every round: 281 commits, past the 200-row page.
    for round in 0..70 {
        git(&root, &["switch", "-q", "-C", "side"], None);
        commit(&root, &format!("Paged side {round}a"));
        commit(&root, &format!("Paged side {round}b"));
        git(&root, &["switch", "-q", "main"], None);
        commit(&root, &format!("Paged main {round}"));
        git(
            &root,
            &[
                "merge",
                "-q",
                "--no-ff",
                "-m",
                &format!("Paged merge {round}"),
                "side",
            ],
            None,
        );
    }
    let head = text(&root, &["rev-parse", "HEAD"]);
    let repository = GitRepository::discover(&root).unwrap().unwrap();
    let origin = OriginalLocalActorOriginV1::capture(&repository).unwrap();
    let operation = LocalGitNavigationReadV1::begin(&origin, Default::default()).unwrap();
    let (first, all) = origin
        .with_saved_navigation_read_v1(&operation, |handle| {
            let page = handle
                .original_graph_page_v1(OriginalGraphReadV1::Start(&head), operation.deadline())
                .map_err(|error| error.to_string())?;
            let first = OriginalGraphStateV1::start(origin.clone(), page)?;
            let page = handle
                .original_graph_page_v1(
                    OriginalGraphReadV1::Continue(first.last_page()),
                    operation.deadline(),
                )
                .map_err(|error| error.to_string())?;
            let all = OriginalGraphStateV1::append(&first, page)?;
            Ok((first, all))
        })
        .unwrap();
    operation.seal().unwrap();
    assert_eq!(first.loaded(), 200);
    assert!(all.loaded() > 200);
    assert_eq!(
        first.last_page().walk_fingerprint(),
        all.last_page().walk_fingerprint()
    );
    // Loading the next page never moves or recolors a row already shown.
    assert_eq!(
        first.lane_rows_v991(0, 200).unwrap(),
        all.lane_rows_v991(0, 200).unwrap()
    );
    // The second window continues the first one's lanes without a gap.
    let seam = all.lane_rows_v991(199, 201).unwrap();
    assert_continuous(&seam);
    let window = all.lane_rows_v991(200, all.loaded()).unwrap();
    assert_eq!(
        window[..],
        all.lane_rows_v991(0, all.loaded()).unwrap()[200..]
    );
    assert_matches_git(&root, &all);
    // The projected Older window has one row per commit and no connector rows.
    let older = all.member_at_v2(200).unwrap().id.clone();
    let shown =
        GraphPresentationV1::prepare_window_v2(&all, &older, GraphViewV1::Timeline, 200).unwrap();
    assert_eq!(shown.rows().len(), all.loaded() - 200 + 2);
}
