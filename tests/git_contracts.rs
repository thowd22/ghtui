#[path = "support/fixture_shell_publication_v1.rs"]
mod fixture_shell_publication_v1;
mod git_support;
#[path = "git_contracts/graph_paging_v1.rs"]
mod graph_paging_v1;
#[path = "git_contracts/provider_special_read_v1.rs"]
mod provider_special_read_v1;
#[path = "support/raw_git_argv_v1.rs"]
mod raw_git_argv_v1;
#[path = "support/raw_git_argv_v1_tests.rs"]
mod raw_git_argv_v1_tests;

use ghtui::git::{
    Comparison, GitRepository, Operation, PullStrategy, RawPath, ResetMode, Sequencer,
};
use ghtui::providers::runner::CancellationToken;
use git_support::{git, try_git, Fixture};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

fn execute(fixture: &Fixture, operation: Operation) {
    let plan = fixture.repository.plan(operation).unwrap();
    fixture.repository.execute(&plan, true).unwrap();
}

#[test]
fn compare_commits_relationship_preserves_direction_all_best_bases_and_unrelated_history() {
    let fixture = Fixture::divergent();
    let head = fixture.head();
    let index = std::fs::read(fixture.repository.git_dir.join("index")).unwrap();
    let common = fixture.repository.resolve_original("main~1").unwrap();
    let related = fixture.repository.relationship("main", "topic").unwrap();
    assert_eq!((related.ahead, related.behind), (1, 1));
    assert_eq!(
        related.common_ancestors.as_slice(),
        std::slice::from_ref(&common)
    );
    assert_eq!(
        related.base,
        fixture.repository.resolve_original("main").unwrap()
    );
    assert_eq!(
        related.head,
        fixture.repository.resolve_original("topic").unwrap()
    );
    assert!(!related.shallow);
    let tree = String::from_utf8(git(&fixture.root, ["rev-parse", "HEAD^{tree}"]).stdout).unwrap();
    let tree = tree.trim();
    let commit = |parents: &[&str], message: &str| {
        let mut args = vec!["commit-tree", tree];
        for parent in parents {
            args.extend(["-p", parent]);
        }
        args.extend(["-m", message]);
        String::from_utf8(git(&fixture.root, args).stdout)
            .unwrap()
            .trim()
            .to_owned()
    };
    let unrelated = commit(&[], "Independent root");
    let independent = fixture.repository.relationship("main", &unrelated).unwrap();
    assert!(independent.common_ancestors.is_empty());
    assert_eq!((independent.ahead, independent.behind), (1, 3));
    // Two independent merges of both parents have two equally best common starting points.
    let x = commit(&[&common], "Cross X");
    let y = commit(&[&common], "Cross Y");
    let a = commit(&[&x, &y], "Merge X/Y");
    let b = commit(&[&y, &x], "Merge Y/X");
    let cross = fixture.repository.relationship(&a, &b).unwrap();
    assert_eq!((cross.ahead, cross.behind), (1, 1));
    let mut best = vec![x, y];
    best.sort();
    assert_eq!(cross.common_ancestors, best);
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        index
    );
    assert!(fixture.repository.status().unwrap().clean());
}

#[test]
fn immutable_overrides_non_utf8_refs_preserve_raw_identity_and_never_write_unicode_sibling() {
    let fixture = Fixture::linear();
    let head = fixture.head();
    let root = fixture.repository.resolve_original("HEAD~1").unwrap();
    let invalid = b"refs/heads/na\xff";
    let unicode = "refs/heads/na\u{fffd}";
    git(&fixture.root, ["update-ref", unicode, &head]);
    let packed = fixture.repository.common_dir.join("packed-refs");
    let mut bytes = format!("# pack-refs with: peeled fully-peeled sorted\n{root} ").into_bytes();
    bytes.extend_from_slice(invalid);
    bytes.push(b'\n');
    std::fs::write(&packed, &bytes).unwrap();
    let references = fixture.repository.references().unwrap();
    let raw = references
        .iter()
        .find(|reference| reference.raw_name == invalid)
        .unwrap();
    let valid = references
        .iter()
        .find(|reference| reference.raw_name == unicode.as_bytes())
        .unwrap();
    assert_eq!(raw.id, root);
    assert_eq!(valid.id, head);
    assert_ne!(raw.name, valid.name);
    assert!(raw.name.contains(r"\xff"));
    assert!(raw.name.starts_with("[non-UTF8 ref:"));
    assert!(fixture
        .repository
        .resolve_original(&raw.name)
        .unwrap_err()
        .0
        .contains("Named non-UTF8"));
    assert!(fixture
        .repository
        .plan(Operation::DeleteBranch {
            name: raw.name.clone(),
            force: true
        })
        .unwrap_err()
        .0
        .contains("Named non-UTF8"));
    assert_eq!(fixture.repository.resolve_original(unicode).unwrap(), head);
    assert_eq!(fixture.repository.resolve_original(&raw.id).unwrap(), root);
    assert_eq!(std::fs::read(&packed).unwrap(), bytes);
    let mut symbolic = b"ref: ".to_vec();
    symbolic.extend_from_slice(invalid);
    symbolic.push(b'\n');
    std::fs::write(fixture.repository.git_dir.join("HEAD"), &symbolic).unwrap();
    let status = fixture.repository.status().unwrap();
    assert_eq!(status.raw_branch.as_deref(), Some(b"na\xff".as_slice()));
    assert_ne!(status.branch.as_deref(), Some("na\u{fffd}"));
    assert_eq!(status.head.as_deref(), Some(root.as_str()));
    let log = fixture.repository.log(10).unwrap();
    assert!(log[0]
        .raw_refs
        .iter()
        .any(|reference| reference.windows(3).any(|part| part == b"na\xff")));
    assert!(log[0].refs.iter().any(|label| label.contains(r"na\xff")));
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("HEAD")).unwrap(),
        symbolic
    );
    assert_eq!(fixture.repository.resolve_original(unicode).unwrap(), head);
}

#[test]
fn git_provider_raw_path_labels_distinguish_invalid_bytes_unicode_and_literal_escapes() {
    let fixture = Fixture::unborn();
    let paths = [
        RawPath::new(b"na\xff".to_vec()),
        RawPath::new("na\u{fffd}".as_bytes().to_vec()),
        RawPath::new(b"line\n".to_vec()),
        RawPath::new(br"line\n".to_vec()),
        RawPath::new(br"na\xff".to_vec()),
        RawPath::new("über.rs".as_bytes().to_vec()),
    ];
    for (index, path) in paths.iter().enumerate() {
        fixture.index_blob(&path.0, format!("unique file {index}\n").as_bytes());
    }
    let head = fixture.commit_index("Distinct raw path identities");
    let index = std::fs::read(fixture.repository.git_dir.join("index")).unwrap();
    let labels = paths
        .iter()
        .map(RawPath::display)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(labels.len(), paths.len());
    assert_eq!(paths[0].display(), r"na\xff");
    assert_eq!(paths[1].display(), "na\u{fffd}");
    assert_eq!(paths[2].display(), r"line\n");
    assert_eq!(paths[3].display(), r"line\\n");
    assert_eq!(paths[4].display(), r"na\\xff");
    assert_eq!(paths[5].display(), "über.rs");
    for (index, path) in paths.iter().enumerate() {
        assert_eq!(
            fixture.repository.blob(&head, path).unwrap(),
            format!("unique file {index}\n").as_bytes()
        );
    }
    let workspace = ghtui::workspace::Workspace::load(
        &fixture.root,
        ghtui::workspace::RepositoryMode::Tree,
        &head,
        None,
    )
    .unwrap()
    .unwrap();
    let visible_labels = workspace
        .rows
        .iter()
        .map(|row| row.label.as_str())
        .collect::<Vec<_>>();
    for label in labels {
        assert!(visible_labels.iter().any(|row| row.contains(&label)));
    }
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        index
    );
}

#[test]
fn immutable_overrides_replacement_blob_commit_and_tag_keep_original_saved_identity() {
    let fixture = Fixture::linear();
    let head = fixture.head();
    let original = fixture.repository.log(10).unwrap();
    let root = original[1].id.clone();
    let entry = fixture
        .repository
        .tree_entry(&head, &RawPath::new(b"readme.txt".to_vec()))
        .unwrap();
    let replacement_file = fixture.directory.path().join("replacement-content");
    std::fs::write(
        &replacement_file,
        b"replacement bytes, not the original object\n",
    )
    .unwrap();
    let replaced_blob = String::from_utf8(
        git(
            &fixture.root,
            [
                OsStr::new("hash-object"),
                OsStr::new("-w"),
                replacement_file.as_os_str(),
            ],
        )
        .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    let root_tree =
        String::from_utf8(git(&fixture.root, ["rev-parse", &format!("{root}^{{tree}}")]).stdout)
            .unwrap()
            .trim()
            .to_owned();
    let replaced_commit = String::from_utf8(
        git(
            &fixture.root,
            ["commit-tree", &root_tree, "-m", "Replacement root history"],
        )
        .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    git(
        &fixture.root,
        ["tag", "-a", "saved", &head, "-m", "Original saved tag"],
    );
    git(
        &fixture.root,
        [
            "tag",
            "-a",
            "alternate",
            &root,
            "-m",
            "Different tag target",
        ],
    );
    let tag = String::from_utf8(git(&fixture.root, ["rev-parse", "saved^{tag}"]).stdout)
        .unwrap()
        .trim()
        .to_owned();
    let alternate_tag =
        String::from_utf8(git(&fixture.root, ["rev-parse", "alternate^{tag}"]).stdout)
            .unwrap()
            .trim()
            .to_owned();
    git(&fixture.root, ["replace", &entry.id, &replaced_blob]);
    git(&fixture.root, ["replace", &head, &replaced_commit]);
    git(&fixture.root, ["replace", &tag, &alternate_tag]);
    fixture.write("readme.txt", b"user working data retained\n");
    let index = std::fs::read(fixture.repository.git_dir.join("index")).unwrap();
    let refs = git(&fixture.root, ["show-ref"]).stdout;
    assert_eq!(
        git(&fixture.root, ["cat-file", "blob", &entry.id]).stdout,
        b"replacement bytes, not the original object\n"
    );
    assert_eq!(
        fixture.repository.resolve("saved").unwrap(),
        root,
        "Ordinary mutation resolution keeps installed Git semantics"
    );
    assert_eq!(fixture.repository.resolve_original("saved").unwrap(), head);
    assert_eq!(
        fixture
            .repository
            .blob("saved", &RawPath::new(b"readme.txt".to_vec()))
            .unwrap(),
        b"first\nchanged\n"
    );
    assert_eq!(
        fixture.repository.object(&entry.id).unwrap(),
        b"first\nchanged\n"
    );
    assert_eq!(
        fixture
            .repository
            .object_sizes(std::slice::from_ref(&entry.id))
            .unwrap(),
        vec![14]
    );
    let commits = fixture.repository.log_page("saved", 0, 10).unwrap();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].parents, vec![root.clone()]);
    assert_eq!(commits[0].subject, "Update readme");
    assert!(String::from_utf8(
        git(
            &fixture.root,
            ["log", "--max-count=1", "--format=%P", &head]
        )
        .stdout
    )
    .unwrap()
    .trim()
    .is_empty());
    let status = fixture.repository.status().unwrap();
    assert_eq!(status.replacement_refs, 3);
    assert!(!status.grafted);
    assert!(status.has_history_overrides());
    let relationship = fixture.repository.relationship(&root, &head).unwrap();
    assert_eq!((relationship.ahead, relationship.behind), (1, 0));
    assert_eq!(
        relationship.common_ancestors.as_slice(),
        std::slice::from_ref(&root)
    );
    let comparison = fixture
        .repository
        .compare(&Comparison::Snapshots {
            base: root.clone(),
            head: "saved".into(),
        })
        .unwrap();
    assert_eq!(comparison.head, Some(head.clone()));
    assert!(comparison
        .patch
        .windows(b"+changed".len())
        .any(|bytes| bytes == b"+changed"));
    let introduced = fixture
        .repository
        .compare(&Comparison::CommitChanges {
            commit: "saved".into(),
            parent: 0,
        })
        .unwrap();
    assert_eq!(introduced.base.as_deref(), Some(root.as_str()));
    assert_eq!(introduced.head.as_deref(), Some(head.as_str()));
    assert_eq!(introduced.patch, comparison.patch);
    assert!(fixture
        .repository
        .plan(Operation::Commit {
            message: "Do not change history".into(),
            amend: false
        })
        .unwrap_err()
        .0
        .contains("History writes are guarded"));
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        index
    );
    assert_eq!(git(&fixture.root, ["show-ref"]).stdout, refs);
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"user working data retained\n"
    );
    execute(
        &fixture,
        Operation::Stage(vec![RawPath::new(b"readme.txt".to_vec())]),
    );
    assert_eq!(
        git(&fixture.root, ["show", ":readme.txt"]).stdout,
        b"user working data retained\n"
    );
    assert_eq!(fixture.head(), head);
}

#[test]
fn immutable_overrides_legacy_grafts_do_not_rewrite_original_ancestry_or_mutations() {
    let fixture = Fixture::linear();
    let head = fixture.head();
    let original = fixture.repository.log(10).unwrap();
    let root = original[1].id.clone();
    let graft = fixture.repository.common_dir.join("info/grafts");
    std::fs::write(&graft, format!("{head}\n")).unwrap();
    let index = std::fs::read(fixture.repository.git_dir.join("index")).unwrap();
    let status = fixture.repository.status().unwrap();
    assert_eq!(status.replacement_refs, 0);
    assert!(status.grafted && status.has_history_overrides());
    assert!(String::from_utf8(
        git(
            &fixture.root,
            ["log", "--format=%P", "--max-count=1", "HEAD"]
        )
        .stdout
    )
    .unwrap()
    .trim()
    .is_empty());
    let commits = fixture.repository.log(10).unwrap();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].parents, vec![root.clone()]);
    assert_eq!(
        fixture.repository.ahead_behind(&root, &head).unwrap(),
        (1, 0)
    );
    let relationship = fixture.repository.relationship(&root, &head).unwrap();
    assert_eq!(
        relationship.common_ancestors.as_slice(),
        std::slice::from_ref(&root)
    );
    assert_eq!((relationship.ahead, relationship.behind), (1, 0));
    let comparison = fixture
        .repository
        .compare(&Comparison::BranchChanges {
            base: root,
            head: head.clone(),
        })
        .unwrap();
    assert!(comparison
        .patch
        .windows(b"+changed".len())
        .any(|bytes| bytes == b"+changed"));
    let introduced = fixture
        .repository
        .compare(&Comparison::CommitChanges {
            commit: head.clone(),
            parent: 0,
        })
        .unwrap();
    assert_eq!(introduced.patch, comparison.patch);
    assert!(fixture
        .repository
        .plan(Operation::CreateBranch {
            name: "blocked".into(),
            revision: "HEAD".into()
        })
        .unwrap_err()
        .0
        .contains("History writes are guarded"));
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(graft).unwrap(),
        format!("{head}\n").as_bytes()
    );
}

#[test]
fn git_provider_discovery_immutable_objects_preserve_index_and_working_tree() {
    let fixture = Fixture::linear();
    std::fs::create_dir(fixture.root.join("nested")).unwrap();
    let repository = GitRepository::discover(fixture.root.join("nested"))
        .unwrap()
        .unwrap();
    assert_eq!(repository.root, fixture.root);
    fixture.write("readme.txt", b"uncommitted work\n");
    let index = std::fs::read(repository.git_dir.join("index")).unwrap();
    let head = fixture.head();
    assert_eq!(
        repository
            .blob("HEAD~1", &RawPath::new(b"readme.txt".to_vec()))
            .unwrap(),
        b"first\nsecond\n"
    );
    assert_eq!(repository.resolve("HEAD").unwrap(), head);
    assert_eq!(repository.log(10).unwrap().len(), 2);
    assert_eq!(
        std::fs::read(repository.git_dir.join("index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"uncommitted work\n"
    );
    assert_eq!(fixture.head(), head);
    assert!(GitRepository::discover(fixture.directory.path())
        .unwrap()
        .is_none());
}

#[test]
fn git_provider_raw_paths_and_literal_pathspecs() {
    let fixture = Fixture::linear();
    let unusual = git_support::unusual_worktree_path();
    for raw in [unusual.as_slice(), b"-danger", b":(glob)*"] {
        fixture.write(OsStr::from_bytes(raw), b"unusual\n");
        execute(&fixture, Operation::Stage(vec![RawPath::new(raw.to_vec())]));
    }
    execute(
        &fixture,
        Operation::Commit {
            message: "Byte paths".into(),
            amend: false,
        },
    );
    for raw in [unusual.as_slice(), b"-danger", b":(glob)*"] {
        assert!(fixture
            .repository
            .tree("HEAD")
            .unwrap()
            .iter()
            .any(|entry| entry.path.0 == raw));
        assert_eq!(
            fixture
                .repository
                .blob("HEAD", &RawPath::new(raw.to_vec()))
                .unwrap(),
            b"unusual\n"
        );
    }
    assert!(fixture.repository.status().unwrap().clean());
}

#[test]
fn git_provider_unborn_detached_shallow_and_worktrees() {
    let fixture = Fixture::unborn();
    let status = fixture.repository.status().unwrap();
    assert_eq!(status.head, None);
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert!(fixture.repository.log(20).unwrap().is_empty());
    fixture.write("file", b"content\n");
    fixture.commit("Root");
    git(&fixture.root, ["switch", "--detach"]);
    assert!(fixture.repository.status().unwrap().detached);
    let clone = fixture.directory.path().join("shallow");
    git(
        fixture.directory.path(),
        [
            OsStr::new("clone"),
            OsStr::new("--depth=1"),
            OsStr::new(&format!("file://{}", fixture.root.display())),
            clone.as_os_str(),
        ],
    );
    let repository = GitRepository::discover(&clone).unwrap().unwrap();
    assert!(repository.status().unwrap().shallow);
    let relation = repository.relationship("HEAD", "HEAD").unwrap();
    assert!(relation.shallow);
    assert_eq!((relation.ahead, relation.behind), (0, 0));
    assert_eq!(
        relation.common_ancestors.as_slice(),
        std::slice::from_ref(&relation.head)
    );
    let linked = fixture.directory.path().join("linked");
    git(
        &fixture.root,
        [
            OsStr::new("worktree"),
            OsStr::new("add"),
            OsStr::new("-b"),
            OsStr::new("linked"),
            linked.as_os_str(),
        ],
    );
    let repository = GitRepository::discover(&linked).unwrap().unwrap();
    assert_ne!(repository.git_dir, repository.common_dir);
    assert_eq!(
        repository.status().unwrap().branch.as_deref(),
        Some("linked")
    );
}

#[test]
fn git_provider_cancellation_stops_hook_children_and_preserves_commit_state() {
    let fixture = Fixture::linear();
    fixture.write("new", b"ready for commit\n");
    execute(
        &fixture,
        Operation::Stage(vec![RawPath::new(b"new".to_vec())]),
    );
    let head = fixture.head();
    let hook = fixture.repository.git_dir.join("hooks/pre-commit");
    std::fs::write(
        &hook,
        b"#!/bin/sh\nprintf ready > cancel-ready\nsleep 30\nprintf unexpected > cancel-finished\n",
    )
    .unwrap();
    std::fs::set_permissions(hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let token = CancellationToken::default();
    let repository = fixture.repository.clone().with_cancellation(token.clone());
    let plan = fixture
        .repository
        .plan(Operation::Commit {
            message: "Should cancel".into(),
            amend: false,
        })
        .unwrap();
    let worker = std::thread::spawn(move || repository.execute(&plan, true));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fixture.root.join("cancel-ready").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let hook_started = fixture.root.join("cancel-ready").exists();
    token.cancel();
    let result = worker.join().unwrap();
    assert!(
        hook_started,
        "The actual hook must have started before cancellation"
    );
    assert!(result.unwrap_err().0.contains("cancelled"));
    assert_eq!(fixture.head(), head);
    assert!(!fixture.root.join("cancel-finished").exists());
    assert_eq!(
        git(&fixture.root, ["show", ":new"]).stdout,
        b"ready for commit\n"
    );
    assert!(GitRepository::discover_with_cancellation(&fixture.root, token).is_err());
}

#[test]
fn commit_graph_ordered_parents_refs_and_page_continuity() {
    let fixture = Fixture::divergent();
    git(&fixture.root, ["merge", "--no-ff", "--no-edit", "topic"]);
    git(&fixture.root, ["tag", "release"]);
    let commits = fixture.repository.log(50).unwrap();
    assert_eq!(commits.len(), 5);
    assert_eq!(commits[0].parents.len(), 2);
    assert!(commits[0].refs.iter().any(|name| name.contains("release")));
    assert!(commits[0].refs.iter().any(|name| name.contains("HEAD")));
    let parents =
        String::from_utf8(git(&fixture.root, ["show", "--format=%P", "--no-patch", "HEAD"]).stdout)
            .unwrap();
    assert_eq!(commits[0].parents.join(" "), parents.trim());
    let first = fixture.repository.log_page("HEAD", 0, 2).unwrap();
    let second = fixture.repository.log_page("HEAD", 2, 3).unwrap();
    assert_eq!([first, second].concat(), commits);
    assert!(commits.last().unwrap().parents.is_empty());
}

#[test]
fn commit_graph_shallow_leaf_retains_unloaded_parent_identity() {
    let fixture = Fixture::linear();
    let expected_parent = fixture.repository.resolve("HEAD~1").unwrap();
    let clone = fixture.directory.path().join("shallow-linear");
    git(
        fixture.directory.path(),
        [
            OsStr::new("clone"),
            OsStr::new("--depth=1"),
            OsStr::new(&format!("file://{}", fixture.root.display())),
            clone.as_os_str(),
        ],
    );
    let repository = GitRepository::discover(&clone).unwrap().unwrap();
    let commits = repository.log(20).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].parents, vec![expected_parent.clone()]);
    let mut graph = ghtui::repository_view::CommitGraph::default();
    graph.append(&commits);
    assert_eq!(graph.boundaries(), &[expected_parent]);
    assert!(!graph.rows[0].explanation.contains("root commit"));
}

#[test]
fn commit_changes_root_first_and_second_merge_parent_have_exact_distinct_saved_diffs() {
    let fixture = Fixture::divergent();
    let root = fixture.repository.resolve_original("main~2").unwrap();
    let starting = fixture
        .repository
        .compare(&Comparison::CommitChanges {
            commit: root.clone(),
            parent: 0,
        })
        .unwrap();
    assert!(starting.description.contains("Root starting content"));
    assert_eq!(starting.head, Some(root.clone()));
    assert_eq!(starting.diff.files.len(), 1);
    assert_eq!(starting.diff.files[0].old_path, None);
    assert_eq!(
        starting.diff.files[0].new_path.as_deref(),
        Some(b"readme.txt".as_slice())
    );
    assert!(String::from_utf8(starting.patch)
        .unwrap()
        .contains("+second"));
    assert!(fixture
        .repository
        .compare(&Comparison::CommitChanges {
            commit: root,
            parent: 1
        })
        .unwrap_err()
        .0
        .contains("root commit has no parent"));
    git(
        &fixture.root,
        ["merge", "--no-ff", "topic", "-m", "Join histories"],
    );
    let head = fixture.head();
    let parents = fixture.repository.log_page(&head, 0, 1).unwrap()[0]
        .parents
        .clone();
    assert_eq!(parents.len(), 2);
    let index = std::fs::read(fixture.repository.git_dir.join("index")).unwrap();
    fixture.write("readme.txt", b"working data retained\n");
    for (parent, introduced_file) in [(0, b"topic.txt".as_slice()), (1, b"main.txt".as_slice())] {
        let result = fixture
            .repository
            .compare(&Comparison::CommitChanges {
                commit: head.clone(),
                parent,
            })
            .unwrap();
        assert_eq!(result.base.as_deref(), Some(parents[parent].as_str()));
        assert_eq!(result.head.as_deref(), Some(head.as_str()));
        assert_eq!(result.diff.base, result.base);
        assert_eq!(result.diff.head, result.head);
        assert_eq!(result.diff.files.len(), 1);
        assert_eq!(
            result.diff.files[0].new_path.as_deref(),
            Some(introduced_file)
        );
        assert!(result.description.contains(if parent == 0 {
            "first parent"
        } else {
            "parent 2"
        }));
        assert_eq!(
            result.patch,
            git(
                &fixture.root,
                [
                    "--no-replace-objects",
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-color",
                    "--find-renames",
                    &parents[parent],
                    &head,
                    "--"
                ]
            )
            .stdout
        );
    }
    assert!(fixture
        .repository
        .compare(&Comparison::CommitChanges {
            commit: head.clone(),
            parent: 2
        })
        .unwrap_err()
        .0
        .contains("requested parent 3 is unavailable"));
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"working data retained\n"
    );
}

#[test]
fn compare_commits_snapshot_and_directional_branch_semantics() {
    let fixture = Fixture::divergent();
    let snapshots = fixture
        .repository
        .compare(&Comparison::Snapshots {
            base: "main".into(),
            head: "topic".into(),
        })
        .unwrap();
    let branch = fixture
        .repository
        .compare(&Comparison::BranchChanges {
            base: "main".into(),
            head: "topic".into(),
        })
        .unwrap();
    assert_eq!(snapshots.diff.files.len(), 2);
    assert_eq!(branch.diff.files.len(), 1);
    assert_eq!(
        branch.diff.files[0].new_path.as_deref(),
        Some(b"topic.txt".as_slice())
    );
    assert_eq!(
        branch.common_ancestor.as_deref(),
        Some(fixture.repository.resolve("main~1").unwrap().as_str())
    );
    let swapped = fixture
        .repository
        .compare(&Comparison::BranchChanges {
            base: "topic".into(),
            head: "main".into(),
        })
        .unwrap();
    assert_eq!(
        swapped.diff.files[0].new_path.as_deref(),
        Some(b"main.txt".as_slice())
    );
    assert_eq!(
        snapshots.patch,
        git(
            &fixture.root,
            [
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--find-renames",
                "main",
                "topic",
                "--"
            ]
        )
        .stdout
    );
}

#[test]
fn revision_tree_and_file_history_follow_renames_without_checkout() {
    let fixture = Fixture::linear();
    git(&fixture.root, ["mv", "readme.txt", "renamed.txt"]);
    fixture.commit("Rename readme");
    let head = fixture.head();
    let old_tree = fixture.repository.tree("HEAD~1").unwrap();
    assert!(old_tree.iter().any(|entry| entry.path.0 == b"readme.txt"));
    let history = fixture
        .repository
        .file_history("HEAD", &RawPath::new(b"renamed.txt".to_vec()), 20)
        .unwrap();
    assert_eq!(history.len(), 3);
    let blame = fixture
        .repository
        .blame("HEAD", &RawPath::new(b"renamed.txt".to_vec()))
        .unwrap();
    assert_eq!(blame.len(), 2);
    assert_eq!(blame[0].text, b"first");
    assert_eq!(blame[1].text, b"changed");
    assert_eq!(blame[0].author, "Fixture Author");
    assert_eq!(blame[0].email, "fixture@example.test");
    assert_eq!(fixture.head(), head);
    assert!(fixture.root.join("renamed.txt").exists());
    assert!(!fixture.root.join("readme.txt").exists());
}

#[test]
fn local_workspace_distinguishes_working_staged_and_saved_changes() {
    let fixture = Fixture::linear();
    fixture.write("readme.txt", b"first\nstaged\n");
    execute(
        &fixture,
        Operation::Stage(vec![RawPath::new(b"readme.txt".to_vec())]),
    );
    fixture.write("readme.txt", b"first\nworking\n");
    let status = fixture.repository.status().unwrap();
    assert_eq!(
        (status.entries[0].index, status.entries[0].worktree),
        ('M', 'M')
    );
    let working = fixture.repository.compare(&Comparison::Working).unwrap();
    let staged = fixture.repository.compare(&Comparison::Staged).unwrap();
    let uncommitted = fixture
        .repository
        .compare(&Comparison::Uncommitted)
        .unwrap();
    assert!(String::from_utf8_lossy(&working.patch).contains("-staged"));
    assert!(String::from_utf8_lossy(&staged.patch).contains("+staged"));
    assert!(String::from_utf8_lossy(&uncommitted.patch).contains("+working"));
}

#[test]
fn native_diff_binary_mode_changes_and_raw_paths_have_canonical_identities() {
    let fixture = Fixture::linear();
    fixture.write("binary space", b"before\0binary");
    fixture.write("executable", b"content\n");
    fixture.commit("Files");
    fixture.write("binary space", b"after\0binary");
    std::fs::set_permissions(
        fixture.root.join("executable"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let diff = fixture
        .repository
        .compare(&Comparison::Working)
        .unwrap()
        .diff;
    assert_eq!(diff.files.len(), 2);
    let binary = diff.files.iter().find(|file| file.binary).unwrap();
    assert_eq!(binary.old_path.as_deref(), Some(b"binary space".as_slice()));
    assert_eq!(binary.new_path.as_deref(), Some(b"binary space".as_slice()));
    let mode = diff
        .files
        .iter()
        .find(|file| file.new_path.as_deref() == Some(b"executable".as_slice()))
        .unwrap();
    assert!(mode.hunks.is_empty());
    assert!(mode
        .metadata
        .iter()
        .any(|line| line.starts_with(b"new mode")));
}

#[test]
fn revision_tree_lazy_directory_and_batch_objects_preserve_bytes() {
    let fixture = Fixture::linear();
    fixture.write("directory/child", b"child bytes\0");
    fixture.write("directory/nested/grandchild", b"nested bytes\n");
    fixture.commit("Nested tree");
    let top = fixture.repository.tree_children("HEAD", None).unwrap();
    assert!(top
        .iter()
        .any(|entry| entry.path.0 == b"directory" && entry.kind == "tree"));
    assert!(!top.iter().any(|entry| entry.path.0 == b"directory/child"));
    let children = fixture
        .repository
        .tree_children("HEAD", Some(&RawPath::new(b"directory".to_vec())))
        .unwrap();
    assert_eq!(children.len(), 2);
    let child = children
        .iter()
        .find(|entry| entry.path.0 == b"directory/child")
        .unwrap();
    let readme = top
        .iter()
        .find(|entry| entry.path.0 == b"readme.txt")
        .unwrap();
    let objects = fixture
        .repository
        .objects(&[child.id.clone(), readme.id.clone()])
        .unwrap();
    assert_eq!(
        objects,
        vec![b"child bytes\0".to_vec(), b"first\nchanged\n".to_vec()]
    );
    assert!(fixture.repository.objects(&["0".repeat(40)]).is_err());
}

#[test]
fn local_workspace_unborn_uncommitted_contains_staged_root_content() {
    let fixture = Fixture::unborn();
    fixture.write("file", b"staged root\n");
    execute(
        &fixture,
        Operation::Stage(vec![RawPath::new(b"file".to_vec())]),
    );
    fixture.write("file", b"working root\n");
    let result = fixture
        .repository
        .compare(&Comparison::Uncommitted)
        .unwrap();
    assert_eq!(result.diff.files.len(), 1);
    assert!(String::from_utf8_lossy(&result.patch).contains("+working root"));
    assert_eq!(result.diff.files[0].old_path, None);
}

#[test]
fn staging_and_unstaging_preserve_files_and_reject_stale_content() {
    let fixture = Fixture::linear();
    let path = RawPath::new(b"readme.txt".to_vec());
    fixture.write("readme.txt", b"selected changes\n");
    let plan = fixture
        .repository
        .plan(Operation::Stage(vec![path.clone()]))
        .unwrap();
    fixture.write("readme.txt", b"new edits after preview\n");
    assert!(fixture
        .repository
        .execute(&plan, true)
        .unwrap_err()
        .0
        .contains("changed since"));
    assert_eq!(
        git(&fixture.root, ["show", ":readme.txt"]).stdout,
        b"first\nchanged\n"
    );
    execute(&fixture, Operation::Stage(vec![path.clone()]));
    assert_eq!(
        git(&fixture.root, ["show", ":readme.txt"]).stdout,
        b"new edits after preview\n"
    );
    execute(&fixture, Operation::Unstage(vec![path]));
    assert_eq!(
        git(&fixture.root, ["show", ":readme.txt"]).stdout,
        b"first\nchanged\n"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"new edits after preview\n"
    );
}

#[test]
fn staging_concurrent_confirmed_plans_do_not_apply_stale_selection() {
    let fixture = Fixture::linear();
    fixture.write("a", b"one\n");
    fixture.write("b", b"two\n");
    let first = fixture
        .repository
        .plan(Operation::Stage(vec![RawPath::new(b"a".to_vec())]))
        .unwrap();
    let second = fixture
        .repository
        .plan(Operation::Stage(vec![RawPath::new(b"b".to_vec())]))
        .unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let spawn = |plan| {
        let repository = fixture.repository.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            repository.execute(&plan, true)
        })
    };
    let first = spawn(first);
    let second = spawn(second);
    barrier.wait();
    let outcomes = [first.join().unwrap(), second.join().unwrap()];
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert!(outcomes
        .iter()
        .filter_map(|outcome| outcome.as_ref().err())
        .all(|error| error.0.contains("changed since")));
    let status = fixture.repository.status().unwrap();
    assert_eq!(
        status.entries.iter().filter(|entry| entry.staged()).count(),
        1
    );
    assert_eq!(
        status
            .entries
            .iter()
            .filter(|entry| entry.untracked())
            .count(),
        1
    );
}

#[test]
fn remote_git_rejects_changed_remote_target_and_ref_identity() {
    let fixture = Fixture::linear();
    let first = fixture.directory.path().join("first.git");
    let second = fixture.directory.path().join("second.git");
    for bare in [&first, &second] {
        git(
            fixture.directory.path(),
            [OsStr::new("init"), OsStr::new("--bare"), bare.as_os_str()],
        );
    }
    git(
        &fixture.root,
        [
            OsStr::new("remote"),
            OsStr::new("add"),
            OsStr::new("origin"),
            first.as_os_str(),
        ],
    );
    let plan = fixture
        .repository
        .plan(Operation::Push {
            remote: "origin".into(),
            branch: "main".into(),
            set_upstream: false,
        })
        .unwrap();
    assert!(plan.target.contains(&first.display().to_string()));
    git(
        &fixture.root,
        [
            OsStr::new("remote"),
            OsStr::new("set-url"),
            OsStr::new("origin"),
            second.as_os_str(),
        ],
    );
    assert!(fixture
        .repository
        .execute(&plan, true)
        .unwrap_err()
        .0
        .contains("changed since"));
    assert!(
        !try_git(&second, ["rev-parse", "--verify", "refs/heads/main"])
            .status
            .success()
    );
    git(&fixture.root, ["branch", "topic", "HEAD~1"]);
    let plan = fixture
        .repository
        .plan(Operation::CreateTag {
            name: "planned".into(),
            revision: "topic".into(),
        })
        .unwrap();
    git(
        &fixture.root,
        ["update-ref", "refs/heads/topic", &fixture.head()],
    );
    assert!(fixture
        .repository
        .execute(&plan, true)
        .unwrap_err()
        .0
        .contains("changed since"));
    assert!(!try_git(
        &fixture.root,
        ["rev-parse", "--verify", "refs/tags/planned"]
    )
    .status
    .success());
}

#[test]
fn local_workspace_prepared_graph_tree_and_diff_keep_saved_identity() {
    use ghtui::workspace::{RepositoryMode, RowAction, Workspace};
    let fixture = Fixture::linear();
    fixture.write("readme.txt", b"local work to keep\n");
    let mut graph = Workspace::load(&fixture.root, RepositoryMode::Graph, "HEAD", None)
        .unwrap()
        .unwrap();
    let old = fixture.repository.resolve("HEAD~1").unwrap();
    graph.open(RowAction::Commit(old.clone())).unwrap();
    assert_eq!(graph.revision, old);
    let mut tree = Workspace::load(&fixture.root, RepositoryMode::Tree, &graph.revision, None)
        .unwrap()
        .unwrap();
    tree.open(RowAction::File(RawPath::new(b"readme.txt".to_vec())))
        .unwrap();
    assert!(tree.rows.iter().any(|row| row.label.contains("second")));
    assert!(!tree
        .rows
        .iter()
        .any(|row| row.label.contains("local work to keep")));
    let working = Workspace::load(&fixture.root, RepositoryMode::Working, "HEAD", None)
        .unwrap()
        .unwrap();
    assert!(working
        .rows
        .iter()
        .any(|row| row.label.contains("+local work to keep")));
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"local work to keep\n"
    );
}

#[test]
fn staging_unborn_unstage_and_exact_hunk_patch() {
    let fixture = Fixture::unborn();
    fixture.write("file", b"one\ntwo\nthree\n");
    execute(
        &fixture,
        Operation::Stage(vec![RawPath::new(b"file".to_vec())]),
    );
    execute(
        &fixture,
        Operation::Unstage(vec![RawPath::new(b"file".to_vec())]),
    );
    assert_eq!(fixture.repository.status().unwrap().entries[0].index, '?');
    fixture.commit("Root");
    fixture.write("file", b"ONE\ntwo\nTHREE\n");
    execute(&fixture, Operation::ApplyPatch { patch: b"diff --git a/file b/file\n--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n-one\n+ONE\n two\n three\n".to_vec(), reverse: false });
    assert_eq!(
        git(&fixture.root, ["show", ":file"]).stdout,
        b"ONE\ntwo\nthree\n"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("file")).unwrap(),
        b"ONE\ntwo\nTHREE\n"
    );
}

#[test]
fn staging_selected_lines_ignore_earlier_unselected_hunks_and_reverse_exactly() {
    let fixture = Fixture::linear();
    let original = (1..=30)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    fixture.write("lines", original.as_bytes());
    fixture.commit("Many lines");
    let selected = original.replace("line 20\n", "line TWENTY\n");
    let working = format!("INSERT\n{selected}");
    fixture.write("lines", working.as_bytes());
    let diff = fixture
        .repository
        .compare(&Comparison::Working)
        .unwrap()
        .diff;
    let file = &diff.files[0];
    assert_eq!(file.hunks.len(), 2);
    let selection = file.hunks[1]
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            matches!(
                line.kind,
                ghtui::diff::LineKind::Added | ghtui::diff::LineKind::Removed
            )
        })
        .map(|(index, _)| (1, index))
        .collect::<Vec<_>>();
    let patch = diff.selected_patch(0, &selection).unwrap();
    execute(
        &fixture,
        Operation::ApplyPatch {
            patch: patch.clone(),
            reverse: false,
        },
    );
    assert_eq!(
        git(&fixture.root, ["show", ":lines"]).stdout,
        selected.as_bytes()
    );
    assert_eq!(
        std::fs::read(fixture.root.join("lines")).unwrap(),
        working.as_bytes()
    );
    execute(
        &fixture,
        Operation::ApplyPatch {
            patch,
            reverse: true,
        },
    );
    assert_eq!(
        git(&fixture.root, ["show", ":lines"]).stdout,
        original.as_bytes()
    );
}

#[test]
fn staging_selected_patch_preserves_quoted_bytes_and_no_newline_markers() {
    let fixture = Fixture::linear();
    let raw = git_support::unusual_worktree_path();
    let raw = raw.as_slice();
    fixture.write(OsStr::from_bytes(raw), b"old no newline");
    fixture.commit("Byte text");
    fixture.write(OsStr::from_bytes(raw), b"new no newline");
    let diff = fixture
        .repository
        .compare(&Comparison::Working)
        .unwrap()
        .diff;
    let selection = diff.files[0].hunks[0]
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            matches!(
                line.kind,
                ghtui::diff::LineKind::Added | ghtui::diff::LineKind::Removed
            )
        })
        .map(|(index, _)| (0, index))
        .collect::<Vec<_>>();
    let patch = diff.selected_patch(0, &selection).unwrap();
    execute(
        &fixture,
        Operation::ApplyPatch {
            patch,
            reverse: false,
        },
    );
    let mut object = b":".to_vec();
    object.extend_from_slice(raw);
    assert_eq!(
        git(
            &fixture.root,
            [OsStr::new("show"), OsStr::from_bytes(&object)]
        )
        .stdout,
        b"new no newline"
    );
}

#[test]
fn staging_selected_diff_snapshot_rejects_edits_before_confirmation_preview() {
    let fixture = Fixture::linear();
    fixture.write("readme.txt", b"first\nchosen\n");
    let result = fixture.repository.compare(&Comparison::Working).unwrap();
    let selected = result.diff.files[0].hunks[0]
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            matches!(
                line.kind,
                ghtui::diff::LineKind::Added | ghtui::diff::LineKind::Removed
            )
        })
        .map(|(index, _)| (0, index))
        .collect::<Vec<_>>();
    fixture.write("readme.txt", b"first\nedited again\n");
    assert!(fixture
        .repository
        .plan_selected_patch(&result, 0, &selected, false)
        .unwrap_err()
        .0
        .contains("stale"));
    assert_eq!(
        git(&fixture.root, ["show", ":readme.txt"]).stdout,
        b"first\nchanged\n"
    );
    let result = fixture.repository.compare(&Comparison::Working).unwrap();
    let plan = fixture
        .repository
        .plan_selected_patch(&result, 0, &selected, false)
        .unwrap();
    fixture.repository.execute(&plan, true).unwrap();
    assert_eq!(
        git(&fixture.root, ["show", ":readme.txt"]).stdout,
        b"first\nedited again\n"
    );
    let staged = fixture.repository.compare(&Comparison::Staged).unwrap();
    let plan = fixture
        .repository
        .plan_selected_patch(&staged, 0, &selected, true)
        .unwrap();
    fixture.repository.execute(&plan, true).unwrap();
    assert_eq!(
        git(&fixture.root, ["show", ":readme.txt"]).stdout,
        b"first\nchanged\n"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"first\nedited again\n"
    );
}

#[test]
fn staging_new_unicode_space_and_control_path_headers_parse_and_unstage_exactly() {
    for quote in ["true", "false"] {
        let fixture = Fixture::linear();
        git(&fixture.root, ["config", "core.quotePath", quote]);
        let paths = [
            RawPath::new("new 日本語.txt".as_bytes().to_vec()),
            RawPath::new(
                [
                    b"quote\" ".as_slice(),
                    &git_support::unusual_worktree_path(),
                ]
                .concat(),
            ),
        ];
        let head = fixture.head();
        let original_index = git(&fixture.root, ["ls-files", "--stage", "-z"]).stdout;
        for (index, path) in paths.iter().enumerate() {
            fixture.write(
                std::path::Path::new(path.as_os_str()),
                format!("new source bytes {index}\n").as_bytes(),
            );
        }
        execute(&fixture, Operation::Stage(paths.to_vec()));
        let patch = git(&fixture.root, ["diff", "--cached", "--no-color"]).stdout;
        assert!(
            patch.windows(2).any(|bytes| bytes == b"\t\n"),
            "Installed Git emits an empty tab header delimiter for space-bearing paths"
        );
        let staged = fixture.repository.compare(&Comparison::Staged).unwrap();
        assert_eq!(staged.diff.files.len(), 2);
        for path in &paths {
            assert!(staged
                .diff
                .files
                .iter()
                .any(|file| file.old_path.is_none() && file.new_path.as_ref() == Some(&path.0)));
        }
        execute(&fixture, Operation::Unstage(paths.to_vec()));
        assert_eq!(
            git(&fixture.root, ["ls-files", "--stage", "-z"]).stdout,
            original_index
        );
        assert_eq!(fixture.head(), head);
        for (index, path) in paths.iter().enumerate() {
            assert_eq!(
                std::fs::read(fixture.root.join(path.as_os_str())).unwrap(),
                format!("new source bytes {index}\n").as_bytes()
            );
        }
    }
}

#[test]
fn staging_exact_nested_untracked_preview_rejects_same_size_edits_and_directory_targets() {
    let fixture = Fixture::linear();
    fixture.write("new/deep/file", b"first");
    let path = RawPath::new(b"new/deep/file".to_vec());
    let status = fixture.repository.status().unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == path && entry.untracked()));
    assert!(!status.entries.iter().any(|entry| entry.path.0 == b"new/"));
    let plan = fixture
        .repository
        .plan(Operation::Stage(vec![path.clone()]))
        .unwrap();
    fixture.write("new/deep/file", b"other");
    assert!(fixture
        .repository
        .execute(&plan, true)
        .unwrap_err()
        .0
        .contains("Repository changed since the preview"));
    assert!(!try_git(
        &fixture.root,
        ["ls-files", "--error-unmatch", "new/deep/file"]
    )
    .status
    .success());
    let failure = fixture
        .repository
        .plan(Operation::Stage(vec![RawPath::new(b"new".to_vec())]))
        .unwrap_err();
    assert!(failure
        .0
        .contains("Directory staging is refused: select exact files"));
    execute(&fixture, Operation::Stage(vec![path]));
    assert_eq!(
        git(&fixture.root, ["show", ":new/deep/file"]).stdout,
        b"other"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("new/deep/file")).unwrap(),
        b"other"
    );
}

#[test]
fn staging_untracked_aggregate_budget_refuses_without_index_or_file_changes() {
    let fixture = Fixture::linear();
    let head = fixture.head();
    let index = std::fs::read(fixture.repository.git_dir.join("index")).unwrap();
    for name in ["untracked-a", "untracked-b"] {
        std::fs::File::create(fixture.root.join(name))
            .unwrap()
            .set_len(17 * 1024 * 1024)
            .unwrap();
    }
    let failure = fixture
        .repository
        .plan(Operation::Stage(vec![RawPath::new(
            b"untracked-a".to_vec(),
        )]))
        .unwrap_err();
    assert!(failure
        .0
        .contains("safe mutation preview byte budget (32MiB)"));
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        index
    );
    for name in ["untracked-a", "untracked-b"] {
        assert_eq!(
            std::fs::metadata(fixture.root.join(name)).unwrap().len(),
            17 * 1024 * 1024
        );
    }
}

#[test]
fn staging_selected_unstage_preserves_neighboring_additions_and_removals_in_index() {
    for remove in [false, true] {
        let fixture = Fixture::unborn();
        let baseline = if remove {
            b"top\nalpha\nbeta\nkeep\nbottom\nmore\n".as_slice()
        } else {
            b"top\nkeep\nbottom\nmore\n".as_slice()
        };
        fixture.write("range.txt", baseline);
        fixture.commit("Range starting version");
        let staged = if remove {
            b"top\nkeep\nbottom\nmore\n".as_slice()
        } else {
            b"top\nalpha\nbeta\nkeep\nbottom\nmore\n".as_slice()
        };
        fixture.write("range.txt", staged);
        git(&fixture.root, ["add", "range.txt"]);
        let working = b"top\nalpha\nbeta\ngamma\nkeep\nbottom\nmore\n";
        fixture.write("range.txt", working);
        let head = fixture.head();
        let comparison = fixture.repository.compare(&Comparison::Staged).unwrap();
        let selected = comparison.diff.files[0]
            .hunks
            .iter()
            .enumerate()
            .flat_map(|(hunk, rows)| {
                rows.lines
                    .iter()
                    .enumerate()
                    .filter(|(_, line)| line.text == b"beta")
                    .map(move |(line, _)| (hunk, line))
            })
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 1);
        let plan = fixture
            .repository
            .plan_selected_patch(&comparison, 0, &selected, true)
            .unwrap();
        fixture.repository.execute(&plan, true).unwrap();
        let expected = if remove {
            b"top\nbeta\nkeep\nbottom\nmore\n".as_slice()
        } else {
            b"top\nalpha\nkeep\nbottom\nmore\n".as_slice()
        };
        assert_eq!(git(&fixture.root, ["show", ":range.txt"]).stdout, expected);
        assert_eq!(
            std::fs::read(fixture.root.join("range.txt")).unwrap(),
            working
        );
        assert_eq!(fixture.head(), head);
        assert!(
            fixture
                .repository
                .plan_selected_patch(&comparison, 0, &selected, true)
                .is_err(),
            "The old selection is stale after index changes"
        );
    }
}

#[test]
fn stash_worktrees_ignored_local_bytes_are_guarded_before_installed_git_would_delete_them() {
    let fixture = Fixture::linear();
    fixture.write(".gitignore", b"ignored.env\n");
    fixture.commit("Ignore local environment data");
    let linked = fixture.directory.path().join("ignored-linked");
    git(
        &fixture.root,
        [
            OsStr::new("worktree"),
            OsStr::new("add"),
            OsStr::new("-b"),
            OsStr::new("ignored-linked"),
            linked.as_os_str(),
        ],
    );
    std::fs::write(
        linked.join("ignored.env"),
        b"fixture ignored bytes retained\n",
    )
    .unwrap();
    assert!(git(&linked, ["status", "--porcelain=v2", "-z"])
        .stdout
        .is_empty());
    let failure = fixture
        .repository
        .plan(Operation::RemoveWorktree(linked.clone()))
        .unwrap_err();
    assert!(failure
        .0
        .contains("Worktree removal refused: ignored local files would be deleted"));
    assert_eq!(
        std::fs::read(linked.join("ignored.env")).unwrap(),
        b"fixture ignored bytes retained\n"
    );
    assert!(fixture
        .repository
        .worktrees()
        .unwrap()
        .iter()
        .any(|worktree| worktree.path == linked));
    // Independent negative control uses only generated fixture bytes and confirms why the guard
    // is necessary: ordinary Git considers this clean enough and irreversibly removes ignored data.
    git(
        &fixture.root,
        [
            OsStr::new("worktree"),
            OsStr::new("remove"),
            linked.as_os_str(),
        ],
    );
    assert!(!linked.exists());
}

#[test]
fn local_commit_successful_hook_progress_is_bounded_redacted_without_altering_saved_content() {
    let fixture = Fixture::linear();
    fixture.write(
        "sensitive-content",
        b"ghp_originalFixtureSource https://user:password@example.invalid/raw\n",
    );
    execute(
        &fixture,
        Operation::Stage(vec![RawPath::new(b"sensitive-content".to_vec())]),
    );
    let hook = fixture.repository.git_dir.join("hooks/pre-commit");
    std::fs::write(&hook, b"#!/bin/sh\nprintf 'hook progress ghp_fixtureSecret https://user:password@example.invalid/private?token=secret\\n'\nprintf 'safe warning ghp_stderrSecret\\n' >&2\nexit 0\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let plan = fixture
        .repository
        .plan(Operation::Commit {
            message: "Successful safe progress".into(),
            amend: false,
        })
        .unwrap();
    let output = fixture.repository.execute(&plan, true).unwrap();
    assert!(output.contains("hook progress [REDACTED] [REMOTE URL REDACTED]"));
    assert!(output.contains("safe warning [REDACTED]"));
    for secret in [
        "fixtureSecret",
        "stderrSecret",
        "password",
        "example.invalid",
    ] {
        assert!(!output.contains(secret));
    }
    assert_eq!(
        fixture
            .repository
            .blob("HEAD", &RawPath::new(b"sensitive-content".to_vec()))
            .unwrap(),
        b"ghp_originalFixtureSource https://user:password@example.invalid/raw\n"
    );
}

#[test]
fn local_commit_hooks_and_amend_recovery_preserve_staged_changes() {
    let fixture = Fixture::linear();
    fixture.write("new", b"next\n");
    execute(
        &fixture,
        Operation::Stage(vec![RawPath::new(b"new".to_vec())]),
    );
    let hook = fixture.repository.git_dir.join("hooks/pre-commit");
    std::fs::write(&hook, b"#!/bin/sh\nprintf 'reject fixture\n' >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let head = fixture.head();
    let plan = fixture
        .repository
        .plan(Operation::Commit {
            message: "Rejected".into(),
            amend: false,
        })
        .unwrap();
    assert!(fixture.repository.execute(&plan, true).is_err());
    assert_eq!(fixture.head(), head);
    assert_eq!(git(&fixture.root, ["show", ":new"]).stdout, b"next\n");
    fixture.repository.reconcile_write().unwrap();
    std::fs::remove_file(hook).unwrap();
    execute(
        &fixture,
        Operation::Commit {
            message: "Accepted".into(),
            amend: false,
        },
    );
    assert_eq!(fixture.repository.log(1).unwrap()[0].subject, "Accepted");
    let plan = fixture
        .repository
        .plan(Operation::Commit {
            message: "Amended".into(),
            amend: true,
        })
        .unwrap();
    assert!(plan.destructive);
    assert!(fixture.repository.execute(&plan, false).is_err());
    fixture.repository.execute(&plan, true).unwrap();
    assert_eq!(fixture.repository.log(1).unwrap()[0].subject, "Amended");
}

#[test]
fn branches_tags_switch_rename_and_dirty_guard() {
    let fixture = Fixture::linear();
    execute(
        &fixture,
        Operation::CreateBranch {
            name: "topic".into(),
            revision: "HEAD~1".into(),
        },
    );
    execute(
        &fixture,
        Operation::RenameBranch {
            old: "topic".into(),
            new: "renamed".into(),
        },
    );
    execute(
        &fixture,
        Operation::CreateTag {
            name: "release".into(),
            revision: "HEAD".into(),
        },
    );
    let refs = fixture.repository.references().unwrap();
    assert!(refs
        .iter()
        .any(|reference| reference.name == "refs/heads/renamed"));
    assert!(refs
        .iter()
        .any(|reference| reference.name == "refs/tags/release"));
    fixture.write("readme.txt", b"dirty\n");
    assert!(fixture
        .repository
        .plan(Operation::SwitchBranch("renamed".into()))
        .is_err());
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"dirty\n"
    );
    git(&fixture.root, ["restore", "readme.txt"]);
    execute(&fixture, Operation::SwitchBranch("renamed".into()));
    assert_eq!(
        fixture.repository.status().unwrap().branch.as_deref(),
        Some("renamed")
    );
    assert!(fixture
        .repository
        .plan(Operation::CreateBranch {
            name: "-unsafe".into(),
            revision: "HEAD".into()
        })
        .is_err());
}

#[test]
fn remote_git_push_fetch_and_upstream_with_local_bare_remote() {
    let fixture = Fixture::linear();
    let bare = fixture.directory.path().join("remote.git");
    git(
        fixture.directory.path(),
        [OsStr::new("init"), OsStr::new("--bare"), bare.as_os_str()],
    );
    git(
        &fixture.root,
        [
            OsStr::new("remote"),
            OsStr::new("add"),
            OsStr::new("origin"),
            bare.as_os_str(),
        ],
    );
    execute(
        &fixture,
        Operation::Push {
            remote: "origin".into(),
            branch: "main".into(),
            set_upstream: true,
        },
    );
    assert_eq!(
        git(&bare, ["rev-parse", "refs/heads/main"]).stdout,
        git(&fixture.root, ["rev-parse", "HEAD"]).stdout
    );
    let status = fixture.repository.status().unwrap();
    assert_eq!(status.upstream.as_deref(), Some("origin/main"));
    assert_eq!((status.ahead, status.behind), (0, 0));
    fixture.write("local", b"local\n");
    fixture.commit("Ahead");
    assert_eq!(fixture.repository.status().unwrap().ahead, 1);
    execute(
        &fixture,
        Operation::Fetch {
            remote: "origin".into(),
        },
    );
    assert_eq!(fixture.repository.status().unwrap().ahead, 1);
    assert!(fixture
        .repository
        .plan(Operation::Fetch {
            remote: "https://unexpected.example/repo".into()
        })
        .is_err());
}

#[test]
fn stash_worktrees_save_apply_add_and_dirty_remove_guard() {
    let fixture = Fixture::linear();
    fixture.write("readme.txt", b"stashed changes\n");
    execute(
        &fixture,
        Operation::StashSave {
            message: "Save work".into(),
            untracked: true,
        },
    );
    let stashes = fixture.repository.stashes().unwrap();
    assert_eq!(stashes.len(), 1);
    assert!(fixture.repository.status().unwrap().clean());
    execute(
        &fixture,
        Operation::StashApply {
            id: stashes[0].id.clone(),
            pop: false,
        },
    );
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"stashed changes\n"
    );
    assert_eq!(fixture.repository.stashes().unwrap().len(), 1);
    git(&fixture.root, ["restore", "readme.txt"]);
    execute(
        &fixture,
        Operation::CreateBranch {
            name: "linked".into(),
            revision: "HEAD".into(),
        },
    );
    let linked = fixture.directory.path().join("linked");
    execute(
        &fixture,
        Operation::AddWorktree {
            path: linked.clone(),
            branch: "linked".into(),
        },
    );
    assert_eq!(fixture.repository.worktrees().unwrap().len(), 2);
    std::fs::write(linked.join("untracked"), b"keep this").unwrap();
    let plan = fixture
        .repository
        .plan(Operation::RemoveWorktree(linked.clone()))
        .unwrap();
    assert!(fixture.repository.execute(&plan, true).is_err());
    assert_eq!(
        std::fs::read(linked.join("untracked")).unwrap(),
        b"keep this"
    );
}

#[test]
fn git_sequencers_merge_conflict_stages_and_abort_restore() {
    let fixture = Fixture::linear();
    git(&fixture.root, ["switch", "-c", "topic"]);
    fixture.write("readme.txt", b"topic conflict\n");
    fixture.commit("Topic conflicting change");
    git(&fixture.root, ["switch", "main"]);
    fixture.write("readme.txt", b"main conflict\n");
    fixture.commit("Main conflicting change");
    let original = fixture.head();
    let plan = fixture
        .repository
        .plan(Operation::Merge("topic".into()))
        .unwrap();
    let failure = fixture.repository.execute(&plan, true).unwrap_err();
    assert!(failure
        .0
        .contains("CONFLICT (content): Merge conflict in readme.txt"));
    assert!(failure.0.contains("fix conflicts and then commit"));
    let status = fixture.repository.status().unwrap();
    assert_eq!(status.operation, Some(Sequencer::Merge));
    assert_eq!(status.conflicts(), 1);
    let stages = fixture.repository.conflict_stages().unwrap();
    assert_eq!(
        stages.iter().map(|stage| stage.stage).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(
        fixture.repository.object(&stages[1].id).unwrap(),
        b"main conflict\n"
    );
    assert_eq!(
        fixture.repository.object(&stages[2].id).unwrap(),
        b"topic conflict\n"
    );
    assert!(fixture.repository.plan(Operation::Skip).is_err());
    fixture.repository.reconcile_write().unwrap();
    execute(&fixture, Operation::Abort);
    assert_eq!(fixture.head(), original);
    assert!(fixture.repository.status().unwrap().clean());
    assert_eq!(fixture.repository.sequencer(), None);
}

#[test]
fn git_sequencers_conflict_stage_resolution_is_inspected_then_staged_and_continued() {
    let fixture = Fixture::linear();
    git(&fixture.root, ["switch", "-c", "topic"]);
    fixture.write("readme.txt", b"topic resolution\n");
    fixture.commit("Topic");
    git(&fixture.root, ["switch", "main"]);
    fixture.write("readme.txt", b"main resolution\n");
    fixture.commit("Main");
    let plan = fixture
        .repository
        .plan(Operation::Merge("topic".into()))
        .unwrap();
    assert!(fixture.repository.execute(&plan, true).is_err());
    fixture.repository.reconcile_write().unwrap();
    let plan = fixture
        .repository
        .plan(Operation::ResolveConflict {
            path: RawPath::new(b"readme.txt".to_vec()),
            stage: 3,
        })
        .unwrap();
    assert!(plan.destructive);
    fixture.repository.execute(&plan, true).unwrap();
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"topic resolution\n"
    );
    assert_eq!(
        fixture.repository.status().unwrap().conflicts(),
        1,
        "Choosing content does not silently stage a resolution"
    );
    execute(
        &fixture,
        Operation::Stage(vec![RawPath::new(b"readme.txt".to_vec())]),
    );
    assert_eq!(fixture.repository.status().unwrap().conflicts(), 0);
    execute(&fixture, Operation::Continue);
    assert_eq!(fixture.repository.sequencer(), None);
    assert!(fixture.repository.status().unwrap().clean());
    assert_eq!(fixture.repository.log(1).unwrap()[0].parents.len(), 2);
    assert_eq!(
        git(&fixture.root, ["show", "HEAD:readme.txt"]).stdout,
        b"topic resolution\n"
    );
}

#[test]
fn reflog_recovery_reset_modes_and_exact_discard_require_confirmation() {
    let fixture = Fixture::linear();
    let old = fixture.repository.resolve("HEAD~1").unwrap();
    let original = fixture.head();
    fixture.write("readme.txt", b"dirty to preserve\n");
    fixture.write("untracked", b"keep untracked\n");
    let plan = fixture
        .repository
        .plan(Operation::Reset {
            revision: old.clone(),
            mode: ResetMode::Soft,
        })
        .unwrap();
    assert!(plan.destructive);
    assert!(fixture.repository.execute(&plan, false).is_err());
    assert_eq!(fixture.head(), original);
    fixture.repository.execute(&plan, true).unwrap();
    assert_eq!(fixture.head(), old);
    assert_eq!(
        git(&fixture.root, ["show", ":readme.txt"]).stdout,
        b"first\nchanged\n"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"dirty to preserve\n"
    );
    execute(
        &fixture,
        Operation::Reset {
            revision: old.clone(),
            mode: ResetMode::Mixed,
        },
    );
    assert_eq!(
        git(&fixture.root, ["show", ":readme.txt"]).stdout,
        b"first\nsecond\n"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"dirty to preserve\n"
    );
    let plan = fixture
        .repository
        .plan(Operation::DiscardFiles(vec![RawPath::new(
            b"readme.txt".to_vec(),
        )]))
        .unwrap();
    assert!(plan.destructive);
    assert!(fixture.repository.execute(&plan, false).is_err());
    fixture.repository.execute(&plan, true).unwrap();
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"first\nsecond\n"
    );
    fixture.write("readme.txt", b"dirty for explicit hard reset\n");
    execute(
        &fixture,
        Operation::Reset {
            revision: original,
            mode: ResetMode::Hard,
        },
    );
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"first\nchanged\n"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("untracked")).unwrap(),
        b"keep untracked\n"
    );
}

#[test]
fn remote_git_force_push_uses_exact_remote_lease_and_pull_strategy() {
    let fixture = Fixture::linear();
    let remote = fixture.directory.path().join("lease.git");
    git(
        fixture.directory.path(),
        [OsStr::new("init"), OsStr::new("--bare"), remote.as_os_str()],
    );
    git(
        &fixture.root,
        [
            OsStr::new("remote"),
            OsStr::new("add"),
            OsStr::new("origin"),
            remote.as_os_str(),
        ],
    );
    execute(
        &fixture,
        Operation::Push {
            remote: "origin".into(),
            branch: "main".into(),
            set_upstream: true,
        },
    );
    let expected = fixture.head();
    execute(
        &fixture,
        Operation::Commit {
            message: "Rewrite tip".into(),
            amend: true,
        },
    );
    assert!(fixture
        .repository
        .plan(Operation::ForcePushLease {
            remote: "origin".into(),
            branch: "main".into(),
            expected_remote: "0".repeat(40)
        })
        .is_err());
    let plan = fixture
        .repository
        .plan(Operation::ForcePushLease {
            remote: "origin".into(),
            branch: "main".into(),
            expected_remote: expected,
        })
        .unwrap();
    assert!(plan.destructive);
    assert!(plan.effect.contains("--force-with-lease="));
    assert!(fixture.repository.execute(&plan, false).is_err());
    fixture.repository.execute(&plan, true).unwrap();
    assert_eq!(
        git(&remote, ["rev-parse", "refs/heads/main"]).stdout,
        git(&fixture.root, ["rev-parse", "HEAD"]).stdout
    );
    execute(
        &fixture,
        Operation::PullWithStrategy {
            remote: "origin".into(),
            branch: "main".into(),
            strategy: PullStrategy::FastForwardOnly,
        },
    );
    assert!(fixture.repository.status().unwrap().clean());
}

#[test]
fn remote_git_divergent_pull_merge_and_rebase_preserve_both_histories() {
    for strategy in [PullStrategy::Merge, PullStrategy::Rebase] {
        let fixture = Fixture::linear();
        let remote = fixture.directory.path().join("divergent.git");
        let peer = fixture.directory.path().join("peer");
        git(
            fixture.directory.path(),
            [OsStr::new("init"), OsStr::new("--bare"), remote.as_os_str()],
        );
        git(
            &fixture.root,
            [
                OsStr::new("remote"),
                OsStr::new("add"),
                OsStr::new("origin"),
                remote.as_os_str(),
            ],
        );
        execute(
            &fixture,
            Operation::Push {
                remote: "origin".into(),
                branch: "main".into(),
                set_upstream: true,
            },
        );
        git(
            fixture.directory.path(),
            [
                OsStr::new("clone"),
                OsStr::new("--branch"),
                OsStr::new("main"),
                remote.as_os_str(),
                peer.as_os_str(),
            ],
        );
        git(&peer, ["config", "user.name", "Remote Author"]);
        git(&peer, ["config", "user.email", "remote@example.test"]);
        git(&peer, ["config", "commit.gpgsign", "false"]);
        std::fs::write(peer.join("remote-file"), b"remote saved work\n").unwrap();
        git(&peer, ["add", "remote-file"]);
        git(&peer, ["commit", "-m", "Peer commit"]);
        git(&peer, ["push", "origin", "main"]);
        fixture.write("local-file", b"local saved work\n");
        fixture.commit("Local commit");
        execute(
            &fixture,
            Operation::PullWithStrategy {
                remote: "origin".into(),
                branch: "main".into(),
                strategy,
            },
        );
        assert!(fixture.repository.status().unwrap().clean());
        assert_eq!(fixture.repository.sequencer(), None);
        assert_eq!(
            std::fs::read(fixture.root.join("remote-file")).unwrap(),
            b"remote saved work\n"
        );
        assert_eq!(
            std::fs::read(fixture.root.join("local-file")).unwrap(),
            b"local saved work\n"
        );
        let commits = fixture.repository.log(20).unwrap();
        assert!(commits.iter().any(|commit| commit.subject == "Peer commit"));
        assert!(commits
            .iter()
            .any(|commit| commit.subject == "Local commit"));
        assert_eq!(
            commits[0].parents.len(),
            if strategy == PullStrategy::Merge {
                2
            } else {
                1
            }
        );
    }
}

#[test]
fn reflog_recovery_creates_branch_without_discarding_dirty_work() {
    let fixture = Fixture::linear();
    let original = fixture.head();
    execute(
        &fixture,
        Operation::Commit {
            message: "Replacement subject".into(),
            amend: true,
        },
    );
    fixture.write("readme.txt", b"dirty work\n");
    let entries = fixture.repository.reflog(20).unwrap();
    assert!(entries.iter().any(|entry| entry.id == original));
    execute(
        &fixture,
        Operation::RecoverBranch {
            name: "rescue".into(),
            revision: original.clone(),
        },
    );
    assert_eq!(fixture.repository.resolve("rescue").unwrap(), original);
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"dirty work\n"
    );
}

#[test]
fn guided_bisect_finds_culprit_and_restores_original_head() {
    let fixture = Fixture::linear();
    let good = fixture.head();
    fixture.write("flag", b"good");
    fixture.commit("Another good commit");
    fixture.write("flag", b"bad");
    let culprit = fixture.commit("Introduce failure");
    fixture.write("other", b"later");
    let original = fixture.commit("Later commit");
    execute(
        &fixture,
        Operation::BisectStart {
            good,
            bad: original.clone(),
        },
    );
    assert_eq!(fixture.repository.sequencer(), Some(Sequencer::Bisect));
    let mut found = false;
    let mut outputs = Vec::new();
    for _ in 0..8 {
        let bad = std::fs::read(fixture.root.join("flag"))
            .map(|contents| contents == b"bad")
            .unwrap_or(false);
        let plan = fixture
            .repository
            .plan(Operation::BisectMark { good: !bad })
            .unwrap();
        let output = fixture.repository.execute(&plan, true).unwrap();
        outputs.push(output.clone());
        if output.contains("is the first bad commit") {
            assert!(output.contains(&culprit));
            found = true;
            break;
        }
    }
    assert!(
        found,
        "Bisect must converge to the known culprit: {outputs:#?}"
    );
    execute(&fixture, Operation::BisectReset);
    assert_eq!(fixture.head(), original);
    assert_eq!(
        fixture.repository.status().unwrap().branch.as_deref(),
        Some("main")
    );
    assert!(fixture.repository.status().unwrap().clean());
    assert!(
        !try_git(&fixture.root, ["rev-parse", "--verify", "BISECT_START"])
            .status
            .success()
    );
}
