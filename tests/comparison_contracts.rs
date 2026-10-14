mod support;
use ghtui::{
    comparison::{picker, CompareEndpoint, CompareIntent, ComparisonMode, ComparisonPlan},
    local_actions::LocalInventory,
    local_picker::LocalValue,
    providers::runner::CancellationToken,
    workspace::{RepositoryMode, RowAction, Workspace},
};
use std::fs;
use support::Fixture;

fn divergent() -> (Fixture, String, String, String) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("root.txt", b"root bytes\n", "starting saved version");
    let root = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file("target.txt", b"target addition\n", "target saved version");
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.git(&["switch", "main"]);
    fixture.commit_file("base.txt", b"base addition\n", "base saved version");
    let base = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    (fixture, root, base, head)
}

fn intent(fixture: &Fixture, mode: ComparisonMode) -> CompareIntent {
    CompareIntent {
        directory: fixture.repo(),
        base: CompareEndpoint::manual("main"),
        head: CompareEndpoint::manual("feature"),
        mode,
    }
}

fn independent_patch(fixture: &Fixture, base: &str, head: &str) -> Vec<u8> {
    fixture
        .git(&[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--find-renames",
            base,
            head,
            "--",
        ])
        .into_bytes()
}

#[test]
fn compare_commits_snapshot_proposed_and_directional_swap_match_independent_git_without_writes() {
    let (fixture, root, base, head) = divergent();
    let expected_snapshot = independent_patch(&fixture, &base, &head);
    let expected_proposed = independent_patch(&fixture, &root, &head);
    let expected_reverse = independent_patch(&fixture, &head, &base);
    fs::write(fixture.repo().join("root.txt"), b"important dirty bytes\n").unwrap();
    fs::write(
        fixture.repo().join("new.txt"),
        b"untracked important bytes\n",
    )
    .unwrap();
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let refs = fixture.git(&["show-ref"]);
    let mut input = intent(&fixture, ComparisonMode::Snapshot);
    let plan = ComparisonPlan::prepare(&input, CancellationToken::default()).unwrap();
    assert_eq!(
        plan.relationship.common_ancestors.as_slice(),
        std::slice::from_ref(&root)
    );
    assert_eq!((plan.relationship.ahead, plan.relationship.behind), (1, 1));
    assert!(!plan.relationship.shallow);
    let snapshot = plan.open();
    assert_eq!(
        snapshot.comparison.as_ref().unwrap().patch,
        expected_snapshot
    );
    assert_eq!(snapshot.revision, head);
    let restored = CompareIntent::from_workspace(&snapshot);
    assert_eq!(restored.base.value, base);
    assert_eq!(restored.head.value, head);
    assert_eq!(restored.mode, ComparisonMode::Snapshot);
    assert_eq!(snapshot.selected_commit.as_ref().unwrap().id, head);
    assert!(snapshot
        .rows
        .iter()
        .any(|row| row.label.contains("-base addition")));
    assert!(snapshot
        .rows
        .iter()
        .any(|row| row.label.contains("+target addition")));
    assert!(snapshot.rows.iter().filter_map(|row| row.action.as_ref()).any(|action| {
        matches!(action, RowAction::DiffLine(anchor) if anchor.base.as_ref() == Some(&base) && anchor.head.as_ref() == Some(&head))
    }));
    input.mode = ComparisonMode::ProposedChanges;
    let proposed = ComparisonPlan::prepare(&input, CancellationToken::default())
        .unwrap()
        .open();
    assert_eq!(
        proposed.comparison.as_ref().unwrap().patch,
        expected_proposed
    );
    assert!(!proposed
        .rows
        .iter()
        .any(|row| row.label.contains("-base addition")));
    assert!(proposed
        .rows
        .iter()
        .any(|row| row.label.contains("+target addition")));
    assert_eq!(
        proposed.comparison.as_ref().unwrap().diff.base.as_ref(),
        Some(&root)
    );
    input.mode = ComparisonMode::Snapshot;
    input.swap();
    let reversed = ComparisonPlan::prepare(&input, CancellationToken::default())
        .unwrap()
        .open();
    assert_eq!(
        reversed.comparison.as_ref().unwrap().patch,
        expected_reverse
    );
    assert!(reversed
        .rows
        .iter()
        .any(|row| row.label.contains("+base addition")));
    assert!(reversed
        .rows
        .iter()
        .any(|row| row.label.contains("-target addition")));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), base);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("root.txt")).unwrap(),
        b"important dirty bytes\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("new.txt")).unwrap(),
        b"untracked important bytes\n"
    );
}

#[test]
fn compare_commits_typed_picker_revalidates_stale_refs_and_edit_swap_retains_exact_pins() {
    let (fixture, _, _, _) = divergent();
    let inventory = LocalInventory::load(&fixture.repo(), CancellationToken::default()).unwrap();
    let picker = picker(1, &inventory).unwrap();
    let choice = picker
        .options
        .iter()
        .find(|choice| choice.label == "refs/heads/feature")
        .unwrap();
    assert_eq!(choice.field, "head");
    assert!(matches!(choice.value, LocalValue::Reference { .. }));
    let mut input = intent(&fixture, ComparisonMode::Snapshot);
    input.select(1, choice.value.clone()).unwrap();
    assert_eq!(input.head.value, choice.editor_text);
    input.edit(1, choice.editor_text.clone()).unwrap();
    assert_eq!(input.head.selection, Some(choice.value.clone()));
    input.swap();
    assert_eq!(input.base.selection, Some(choice.value.clone()));
    input.swap();
    ComparisonPlan::prepare(&input, CancellationToken::default()).unwrap();
    fixture.git(&["branch", "-f", "feature", "main"]);
    let current = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    assert!(
        ComparisonPlan::prepare(&input, CancellationToken::default())
            .unwrap_err()
            .contains("Selected reference changed")
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), current);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    input.edit(1, "feature".into()).unwrap();
    assert!(input.head.selection.is_none());
    let fresh = ComparisonPlan::prepare(&input, CancellationToken::default())
        .unwrap()
        .open();
    assert!(fresh.comparison.unwrap().diff.files.is_empty());
    assert!(input.select(2, choice.value.clone()).is_err());
    assert!(input
        .select(
            1,
            LocalValue::Remote {
                name: "origin".into()
            }
        )
        .is_err());
}

#[test]
fn compare_commits_unrelated_roots_allow_snapshot_but_explain_missing_proposed_base() {
    let (fixture, _, base, _) = divergent();
    fixture.git(&["checkout", "--orphan", "unrelated"]);
    fixture.git(&["rm", "-f", "--", "base.txt", "root.txt"]);
    fixture.commit_file("other.txt", b"unrelated root\n", "independent root");
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let expected = independent_patch(&fixture, &base, &head);
    let mut input = CompareIntent {
        directory: fixture.repo(),
        base: CompareEndpoint::manual(base),
        head: CompareEndpoint::manual(head.clone()),
        mode: ComparisonMode::Snapshot,
    };
    let plan = ComparisonPlan::prepare(&input, CancellationToken::default()).unwrap();
    assert!(plan.relationship.common_ancestors.is_empty());
    assert!(plan
        .preview
        .iter()
        .any(|line| line.starts_with("No common starting point")));
    assert_eq!(plan.open().comparison.unwrap().patch, expected);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    input.mode = ComparisonMode::ProposedChanges;
    assert!(
        ComparisonPlan::prepare(&input, CancellationToken::default())
            .unwrap_err()
            .contains("no common starting point")
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("other.txt")).unwrap(),
        b"unrelated root\n"
    );
}

#[test]
fn compare_commits_root_defaults_missing_invalid_and_cancelled_endpoints_are_explicit() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("root", b"saved root\n", "root");
    let workspace = Workspace::load(&fixture.repo(), RepositoryMode::Overview, "HEAD", None)
        .unwrap()
        .unwrap();
    let mut input = CompareIntent::from_workspace(&workspace);
    let plan = ComparisonPlan::prepare(&input, CancellationToken::default()).unwrap();
    assert_eq!(plan.relationship.ahead, 0);
    assert_eq!(plan.relationship.behind, 0);
    assert!(plan.open().comparison.unwrap().diff.files.is_empty());
    for bad in ["", "--all", "missing-ref", "HEAD\n"] {
        input.head = CompareEndpoint::manual(bad);
        assert!(ComparisonPlan::prepare(&input, CancellationToken::default()).is_err());
    }
    input.head = CompareEndpoint::manual("HEAD");
    let token = CancellationToken::default();
    token.cancel();
    assert!(ComparisonPlan::prepare(&input, token).is_err());
    assert!(ComparisonMode::parse("unknown").is_err());
    assert_eq!(
        ComparisonMode::parse("proposed").unwrap(),
        ComparisonMode::ProposedChanges
    );
}

#[test]
fn compare_commits_shallow_boundaries_do_not_claim_complete_ancestry_or_fetch_implicitly() {
    let (fixture, _, base, head) = divergent();
    let directory = fixture.root.join("shallow");
    let git_at = |path: &std::path::Path, args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    // Only this fixture explicitly fetches history. The controller must not.
    git_at(
        &fixture.root,
        &[
            "clone",
            "--depth=1",
            "--branch=main",
            "--no-local",
            &format!("file://{}", fixture.repo().display()),
            directory.to_str().unwrap(),
        ],
    );
    git_at(
        &directory,
        &["fetch", "--depth=1", "origin", "feature:refs/heads/feature"],
    );
    let shallow = fs::read(directory.join(".git/shallow")).unwrap();
    let refs = git_at(&directory, &["show-ref"]);
    let index = fs::read(directory.join(".git/index")).unwrap();
    let mut input = CompareIntent {
        directory: directory.clone(),
        base: CompareEndpoint::manual(base.clone()),
        head: CompareEndpoint::manual(head),
        mode: ComparisonMode::Snapshot,
    };
    let plan = ComparisonPlan::prepare(&input, CancellationToken::default()).unwrap();
    assert!(plan.relationship.shallow);
    assert!(plan.relationship.common_ancestors.is_empty());
    assert!(plan
        .preview
        .iter()
        .any(|line| line.contains("available shallow history")));
    assert!(plan
        .preview
        .iter()
        .any(|line| line.contains("No automatic fetch")));
    assert_eq!(plan.open().comparison.unwrap().diff.files.len(), 2);
    input.mode = ComparisonMode::ProposedChanges;
    assert!(
        ComparisonPlan::prepare(&input, CancellationToken::default())
            .unwrap_err()
            .contains("shallow history")
    );
    assert_eq!(git_at(&directory, &["rev-parse", "HEAD"]).trim(), base);
    assert_eq!(git_at(&directory, &["show-ref"]), refs);
    assert_eq!(fs::read(directory.join(".git/shallow")).unwrap(), shallow);
    assert_eq!(fs::read(directory.join(".git/index")).unwrap(), index);
}

#[test]
fn compare_commits_multiple_best_bases_are_visible_with_exact_chosen_diff_base() {
    let (fixture, _, base, head) = divergent();
    let tree_a = fixture
        .git(&["rev-parse", &format!("{base}^{{tree}}")])
        .trim()
        .to_owned();
    let tree_b = fixture
        .git(&["rev-parse", &format!("{head}^{{tree}}")])
        .trim()
        .to_owned();
    let merge_a = fixture
        .git(&[
            "commit-tree",
            &tree_a,
            "-p",
            &base,
            "-p",
            &head,
            "-m",
            "first criss-cross merge",
        ])
        .trim()
        .to_owned();
    let merge_b = fixture
        .git(&[
            "commit-tree",
            &tree_b,
            "-p",
            &head,
            "-p",
            &base,
            "-m",
            "second criss-cross merge",
        ])
        .trim()
        .to_owned();
    let mut best = fixture
        .git(&["merge-base", "--all", &merge_a, &merge_b])
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    best.sort();
    assert_eq!(best.len(), 2);
    let selected = fixture
        .git(&["merge-base", &merge_a, &merge_b])
        .trim()
        .to_owned();
    let expected = independent_patch(&fixture, &selected, &merge_b);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let input = CompareIntent {
        directory: fixture.repo(),
        base: CompareEndpoint::manual(merge_a),
        head: CompareEndpoint::manual(merge_b),
        mode: ComparisonMode::ProposedChanges,
    };
    let plan = ComparisonPlan::prepare(&input, CancellationToken::default()).unwrap();
    assert_eq!(plan.relationship.common_ancestors, best);
    assert!(plan
        .preview
        .iter()
        .any(|line| line.contains("Multiple best common starting points: 2")));
    assert!(plan
        .preview
        .iter()
        .any(|line| line.contains(&format!("Exact content compared: {selected}"))));
    let workspace = plan.open();
    assert_eq!(workspace.comparison.as_ref().unwrap().patch, expected);
    assert_eq!(workspace.comparison.unwrap().diff.base, Some(selected));
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), base);
}
