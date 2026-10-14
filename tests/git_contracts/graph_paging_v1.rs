//! Undeclared installed-Git graph contracts; requires Root parent activation.
use super::*;
use ghtui::{
    repository_graph_v1::{GraphActionV1, GraphCoverageV1, GraphViewV1},
    workspace::{RepositoryMode, RowAction, Workspace},
};

fn text(f: &Fixture, args: &[&str]) -> String {
    String::from_utf8(git(&f.root, args).stdout)
        .unwrap()
        .trim()
        .into()
}
fn commit(f: &Fixture, tree: &str, parents: &[String], subject: &str) -> String {
    let mut args = vec!["commit-tree", tree];
    for parent in parents {
        args.extend(["-p", parent]);
    }
    args.extend(["-m", subject]);
    text(f, &args)
}
fn loaded(f: &Fixture, tip: &str) -> Workspace {
    Workspace::load(&f.root, RepositoryMode::Graph, tip, None)
        .unwrap()
        .unwrap()
}
fn snapshot(f: &Fixture) -> Vec<Vec<u8>> {
    // Independent read oracles must not refresh the index or create/remove an
    // optional lock after the graph captured its absent-shallow parent stamp.
    vec![
        git(&f.root, ["--no-optional-locks", "rev-parse", "HEAD"]).stdout,
        git(&f.root, ["--no-optional-locks", "show-ref", "--head"]).stdout,
        std::fs::read(f.root.join(".git/index")).unwrap(),
        git(
            &f.root,
            ["--no-optional-locks", "status", "--porcelain=v2", "-z"],
        )
        .stdout,
        git(&f.root, ["--no-optional-locks", "diff", "--binary"]).stdout,
        git(
            &f.root,
            ["--no-optional-locks", "diff", "--cached", "--binary"],
        )
        .stdout,
        std::fs::read(f.root.join(".git/config")).unwrap(),
    ]
}
fn chain(f: &Fixture, count: usize) -> String {
    let tree = text(f, &["rev-parse", "HEAD^{tree}"]);
    let mut parent = f.head();
    for n in 0..count {
        parent = commit(f, &tree, &[parent], &format!("Paged real version {n:03}"));
    }
    parent
}
#[test]
fn commit_graph_v1_real_pages_keep_original_walk_after_selected_parent_and_readonly_bytes() {
    let f = Fixture::linear();
    let tip = chain(&f, 410);
    let before = snapshot(&f);
    let mut w = loaded(&f, &tip);
    let first = w.graph_state_v1.as_ref().unwrap().clone();
    assert_eq!(first.loaded(), 200);
    let selected = first.last_page().commits()[100].commit().id.clone();
    w.open(RowAction::Commit(selected.clone())).unwrap();
    w.open(RowAction::Graph(GraphActionV1::More)).unwrap();
    assert_eq!(w.revision, selected);
    let next = w.graph_state_v1.as_ref().unwrap();
    assert_eq!(next.walk_tip(), tip);
    assert_eq!(next.loaded(), 400);
    for original in first.last_page().commits() {
        let retained = next.observed(&original.commit().id).unwrap();
        assert_eq!(retained.commit(), original.commit());
        assert_eq!(retained.message(), original.message());
    }
    assert_eq!(next.coverage(), &GraphCoverageV1::MoreAvailable);
    w.open(RowAction::Graph(GraphActionV1::More)).unwrap();
    assert_eq!(w.graph_state_v1.as_ref().unwrap().loaded(), 412);
    assert_eq!(
        w.graph_state_v1.as_ref().unwrap().coverage(),
        &GraphCoverageV1::AvailableWalkExhausted
    );
    assert!(w.open(RowAction::Graph(GraphActionV1::More)).is_err());
    assert_eq!(snapshot(&f), before);
}
#[test]
fn commit_graph_v1_actual_octopus_each_parent_comparison_preserves_raw_order_and_direction() {
    let f = Fixture::linear();
    let base = f.head();
    let tree = text(&f, &["rev-parse", "HEAD^{tree}"]);
    let parents = (0..4)
        .map(|n| {
            commit(
                &f,
                &tree,
                std::slice::from_ref(&base),
                &format!("Parent {n}"),
            )
        })
        .collect::<Vec<_>>();
    let tip = commit(&f, &tree, &parents, "Four actual histories");
    let before = snapshot(&f);
    let mut w = loaded(&f, &tip);
    assert_eq!(w.selected_commit.as_ref().unwrap().parents, parents);
    for (index, parent) in parents.iter().enumerate() {
        w.open(RowAction::Graph(GraphActionV1::Parents)).unwrap();
        assert!(w
            .display_rows_v1()
            .iter()
            .any(|row| row.label == format!("Parent {}: {}", index + 1, parent)));
        w.open(RowAction::Graph(GraphActionV1::CompareParent(index)))
            .unwrap();
        let comparison = w.comparison.as_ref().unwrap();
        assert_eq!(comparison.base.as_deref(), Some(parent.as_str()));
        assert_eq!(comparison.head.as_deref(), Some(tip.as_str()));
        assert!(comparison.diff.files.is_empty());
        w.open(RowAction::Graph(GraphActionV1::Back)).unwrap();
        assert_eq!(w.revision, tip);
    }
    assert_eq!(snapshot(&f), before);
}
#[test]
fn commit_graph_v1_actual_depth1_clone_retains_parent_but_refuses_unknown_parent_diff() {
    let f = Fixture::linear();
    let shallow = f.directory.path().join("shallow");
    let url = format!("file://{}", f.root.display());
    git(
        f.directory.path(),
        ["clone", "--depth", "1", &url, shallow.to_str().unwrap()],
    );
    let mut w = Workspace::load(&shallow, RepositoryMode::Graph, "HEAD", None)
        .unwrap()
        .unwrap();
    assert_eq!(
        w.graph_state_v1.as_ref().unwrap().coverage(),
        &GraphCoverageV1::ShallowBoundary
    );
    assert_eq!(w.selected_commit.as_ref().unwrap().parents.len(), 1);
    w.open(RowAction::Graph(GraphActionV1::Parents)).unwrap();
    assert!(w
        .display_rows_v1()
        .iter()
        .any(|row| row.label.contains("Shallow parent")));
    assert!(w
        .open(RowAction::Graph(GraphActionV1::CompareParent(0)))
        .unwrap_err()
        .contains("Shallow"));
    assert!(w.comparison.is_none());
    assert!(shallow.join(".git/shallow").is_file());
}
#[test]
fn commit_graph_v1_actual_sha256_root_detached_names_and_roles_are_not_guessed_users() {
    let f = Fixture::unborn();
    git(&f.root, ["config", "core.repositoryformatversion", "1"]);
    git(&f.root, ["config", "extensions.objectFormat", "sha256"]);
    f.write("root.txt", b"root sha256 bytes\n");
    let root = f.commit("Root SHA256 saved");
    assert_eq!(root.len(), 64);
    git(&f.root, ["checkout", "--detach", &root]);
    git(&f.root, ["tag", "actual-tag", &root]);
    let before = snapshot(&f);
    let mut w = loaded(&f, &root);
    assert_eq!(
        w.selected_commit.as_ref().unwrap().author.name,
        "Fixture Author"
    );
    assert_eq!(
        w.selected_commit.as_ref().unwrap().committer.name,
        "Fixture Author"
    );
    assert!(w.selected_commit.as_ref().unwrap().parents.is_empty());
    w.open(RowAction::Graph(GraphActionV1::Parents)).unwrap();
    assert!(w
        .display_rows_v1()
        .iter()
        .any(|row| row.label.contains("Root commit")));
    w.open(RowAction::Graph(GraphActionV1::Details)).unwrap();
    assert!(w
        .display_rows_v1()
        .iter()
        .any(|row| row.label.contains("refs/tags/actual-tag")));
    assert_eq!(snapshot(&f), before);
}
#[test]
fn commit_graph_v1_canceled_or_changed_shallow_continuation_cannot_replace_original_arc() {
    let f = Fixture::linear();
    let tip = chain(&f, 205);
    let mut w = loaded(&f, &tip);
    let original = w.graph_state_v1.as_ref().unwrap().clone();
    let token = CancellationToken::default();
    token.cancel();
    assert!(w
        .open_with_cancellation(RowAction::Graph(GraphActionV1::More), token)
        .is_err());
    assert!(std::sync::Arc::ptr_eq(
        w.graph_state_v1.as_ref().unwrap(),
        &original
    ));
    std::fs::write(f.root.join(".git/shallow"), format!("{tip}\n")).unwrap();
    assert!(w
        .open(RowAction::Graph(GraphActionV1::More))
        .unwrap_err()
        .contains("shallow"));
    assert!(std::sync::Arc::ptr_eq(
        w.graph_state_v1.as_ref().unwrap(),
        &original
    ));
    assert_eq!(w.graph_state_v1.as_ref().unwrap().loaded(), 200);
}
#[test]
fn commit_graph_v1_ephemeral_back_fresh_original_read_preserves_walk_page_and_selected_view() {
    let f = Fixture::linear();
    let tip = chain(&f, 230);
    let mut w = loaded(&f, &tip);
    let selected = w.graph_state_v1.as_ref().unwrap().last_page().commits()[40]
        .commit()
        .id
        .clone();
    w.open(RowAction::Commit(selected.clone())).unwrap();
    w.open(RowAction::Graph(GraphActionV1::Details)).unwrap();
    let visit = ghtui::navigation_local_source_v1::OriginalSavedVisitV1::capture(&w).unwrap();
    let before = snapshot(&f);
    let operation =
        ghtui::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1::begin(
            visit.origin(),
            Default::default(),
        )
        .unwrap();
    let restored = visit.restore(&operation).unwrap();
    operation.seal().unwrap();
    assert_eq!(restored.revision, selected);
    assert_eq!(restored.graph_state_v1.as_ref().unwrap().walk_tip(), tip);
    assert_eq!(restored.graph_state_v1.as_ref().unwrap().loaded(), 200);
    assert_eq!(restored.graph_view_v1, GraphViewV1::Details);
    assert!(restored
        .display_rows_v1()
        .iter()
        .any(|row| row.label == format!("Saved commit: {selected}")));
    assert_eq!(snapshot(&f), before);
}
