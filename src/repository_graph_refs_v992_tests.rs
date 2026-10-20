//! TASK-99.2: typed ref chips from the original walk over real temporary repositories. Every
//! branch, remote branch and tag `git for-each-ref` lists appears on its commit, HEAD is a
//! typed chip (never a look-alike name), synced pairs merge, refs past the 4096-name cap are
//! counted, and reading writes nothing.
use super::*;
use crate::{
    git::{GitRepository, HistoryRefKindV1 as Kind, OriginalGraphReadV1},
    history::refs,
    local_commit_actor_origin_v1_test_support::{git, text},
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
};
use std::path::{Path, PathBuf};

fn init(directory: &Path, name: &str) -> PathBuf {
    let root = directory.join(name);
    std::fs::create_dir(&root).unwrap();
    git(&root, &["init", "-q", "--initial-branch=main"], None);
    git(&root, &["config", "user.name", "Ref Writer"], None);
    git(
        &root,
        &["config", "user.email", "refs@example.invalid"],
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
/// HEAD, refs, status, the index and config: what a read-only walk must leave alone.
fn snapshot(root: &Path) -> Vec<Vec<u8>> {
    vec![
        std::fs::read(root.join(".git/HEAD")).unwrap(),
        git(root, &["for-each-ref"], None),
        git(root, &["status", "--porcelain=v2", "--branch", "-z"], None),
        std::fs::read(root.join(".git/index")).unwrap_or_default(),
        std::fs::read(root.join(".git/config")).unwrap(),
    ]
}
fn labels(state: &OriginalGraphStateV1, id: &str) -> Vec<String> {
    refs::chips(state.observed(id).unwrap().chips())
        .into_iter()
        .map(|chip| chip.label)
        .collect()
}

/// main (ahead of origin/main), feature with a merge, release synced with origin, tags of
/// both kinds, a remote-only branch, origin/HEAD and a stash.
fn repository(directory: &Path) -> (PathBuf, std::collections::HashMap<&'static str, String>) {
    let root = init(directory, "refs");
    let mut ids = std::collections::HashMap::new();
    std::fs::write(root.join("notes.txt"), "notes\n").unwrap();
    git(&root, &["add", "notes.txt"], None);
    git(&root, &["commit", "-q", "-m", "Root notes"], None);
    ids.insert("root", text(&root, &["rev-parse", "HEAD"]));
    git(&root, &["tag", "v0.1.0"], None);
    git(&root, &["switch", "-q", "-c", "feature"], None);
    ids.insert("feature", commit(&root, "Feature work"));
    git(&root, &["switch", "-q", "main"], None);
    ids.insert("main1", commit(&root, "Main work"));
    git(
        &root,
        &["merge", "-q", "--no-ff", "-m", "Merge feature", "feature"],
        None,
    );
    ids.insert("merge", text(&root, &["rev-parse", "HEAD"]));
    git(&root, &["branch", "release/1.0"], None);
    git(
        &root,
        &["tag", "-a", "-m", "Release 1.0", "v1.0.0", "release/1.0"],
        None,
    );
    let bare = directory.join("origin.git");
    git(
        directory,
        &["init", "-q", "--bare", bare.to_str().unwrap()],
        None,
    );
    git(
        &root,
        &["remote", "add", "origin", bare.to_str().unwrap()],
        None,
    );
    git(&root, &["switch", "-q", "-c", "hotfix", "v0.1.0"], None);
    ids.insert("hotfix", commit(&root, "Hotfix only on the remote"));
    git(
        &root,
        &[
            "push",
            "-q",
            "origin",
            "main",
            "release/1.0",
            "feature",
            "hotfix",
        ],
        None,
    );
    git(&root, &["remote", "set-head", "origin", "main"], None);
    git(&root, &["switch", "-q", "main"], None);
    git(&root, &["branch", "-q", "-D", "hotfix"], None);
    // main moves one commit past origin/main; feature tracks origin/feature by upstream.
    ids.insert("tip", commit(&root, "Unpushed tweak"));
    git(
        &root,
        &["branch", "--set-upstream-to=origin/feature", "feature"],
        None,
    );
    // A stash ref names a commit outside the walk; it is never a chip.
    std::fs::write(root.join("notes.txt"), "dirty\n").unwrap();
    git(&root, &["stash", "-q"], None);
    (root, ids)
}

#[test]
fn history_refs_v992_every_branch_remote_and_tag_is_a_typed_chip_on_its_commit() {
    let directory = tempfile::tempdir().unwrap();
    let (root, ids) = repository(directory.path());
    let before = snapshot(&root);
    let state = walk(&root, &ids["tip"]);
    // Every ref `git for-each-ref` lists (peeled to its commit) is a chip of that commit,
    // except the symbolic origin/HEAD, which only points at origin/main.
    let listed = text(
        &root,
        &[
            "for-each-ref",
            "--format=%(refname) %(symref) %(*objectname) %(objectname)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    );
    let mut checked = 0;
    for line in listed.lines() {
        let words = line.split(' ').collect::<Vec<_>>();
        let (name, target) = match words[..] {
            [name, "", "", object] => (name, object),
            [name, "", peeled, _] => (name, peeled),
            [name, ..] => {
                assert_eq!(name, "refs/remotes/origin/HEAD");
                continue;
            }
            _ => unreachable!(),
        };
        let observed = state
            .observed(target)
            .unwrap_or_else(|| panic!("{name} target {target} is not loaded"));
        let chip = observed
            .chips()
            .iter()
            .find(|chip| chip.raw_name == name.as_bytes())
            .unwrap_or_else(|| panic!("{name} is not a chip of {target}"));
        let kind = if name.starts_with("refs/heads/") {
            Kind::Local
        } else if name.starts_with("refs/remotes/") {
            Kind::Remote
        } else {
            Kind::Tag
        };
        assert_eq!(chip.kind, kind, "{name}");
        assert_eq!(chip.target, target);
        checked += 1;
    }
    assert_eq!(checked, 9, "{listed}");
    // HEAD is attached to main: a typed current flag, no look-alike name anywhere.
    let tip = state.observed(&ids["tip"]).unwrap();
    assert!(tip
        .chips()
        .iter()
        .any(|chip| chip.kind == Kind::Local && chip.current && chip.name == "main"));
    assert!(tip.chips().iter().all(|chip| chip.kind != Kind::Head));
    assert!(state
        .pages
        .iter()
        .all(|page| page.commits().iter().all(|row| row
            .commit()
            .refs
            .iter()
            .all(|name| !name.contains("graph load")))));
    assert_eq!(labels(&state, &ids["tip"]), ["HEAD", "main"]);
    // origin/main stays behind, separate; release and feature are synced pairs; the
    // remote-only branch and both tag kinds keep their own chips.
    assert_eq!(
        labels(&state, &ids["merge"]),
        ["release/1.0 ⇅", "origin/main", "v1.0.0"]
    );
    assert_eq!(labels(&state, &ids["feature"]), ["feature ⇅"]);
    assert_eq!(labels(&state, &ids["hotfix"]), ["origin/hotfix"]);
    assert_eq!(labels(&state, &ids["root"]), ["v0.1.0"]);
    // The stash names a commit only in Details' names; it starts no walk and is no chip.
    let stash = text(&root, &["rev-parse", "refs/stash"]);
    assert!(state.observed(&stash).is_none());
    // The row label lists every name, typed.
    let label = short_names_v1(state.observed(&ids["merge"]).unwrap());
    assert_eq!(label, " (release/1.0 ⇅, origin/main, tag v1.0.0)");
    assert_eq!(state.last_page().ref_overflow(), 0);
    assert_eq!(snapshot(&root), before, "the walk wrote nothing");
}

#[test]
fn history_refs_v992_detached_head_is_a_head_chip_before_the_tags() {
    let directory = tempfile::tempdir().unwrap();
    let (root, ids) = repository(directory.path());
    git(&root, &["switch", "-q", "--detach", "v1.0.0"], None);
    let before = snapshot(&root);
    let state = walk(&root, &ids["merge"]);
    assert_eq!(
        labels(&state, &ids["merge"]),
        ["HEAD", "release/1.0 ⇅", "origin/main", "v1.0.0"]
    );
    let head = state.observed(&ids["merge"]).unwrap();
    assert_eq!(head.chips()[0].kind, Kind::Head);
    assert_eq!(
        head_label_v992(head).unwrap(),
        "HEAD at graph load: detached at this commit"
    );
    assert!(head_label_v992(state.observed(&ids["tip"]).unwrap()).is_none());
    assert_eq!(labels(&state, &ids["tip"]), ["main"]);
    assert_eq!(snapshot(&root), before);
}

#[test]
fn history_refs_v992_refs_past_the_4096_cap_are_counted_not_failed() {
    let directory = tempfile::tempdir().unwrap();
    let root = init(directory.path(), "many");
    let tip = commit(&root, "Many tags");
    let input = (0..4100)
        .map(|index| format!("create refs/tags/t{index:05} {tip}\n"))
        .collect::<String>();
    git(&root, &["update-ref", "--stdin"], Some(input.as_bytes()));
    let before = snapshot(&root);
    let state = walk(&root, &tip);
    // main sorts first; 4095 tags fit; the last five are counted and reported.
    assert_eq!(state.last_page().ref_overflow(), 5);
    let observed = state.observed(&tip).unwrap();
    assert_eq!(observed.chips().len(), 4096, "main and 4095 tags");
    let display = GraphPresentationV1::prepare(&state, &tip, GraphViewV1::Timeline).unwrap();
    let summary = &display.rows()[0].label;
    assert!(
        summary.contains("5 refs past the 4096-ref cap not shown or walked"),
        "{summary}"
    );
    assert!(
        summary.starts_with("Read-only · 1 commit loaded"),
        "{summary}"
    );
    // Every name stays reachable in the label up to its bound, then an explicit count.
    let label = &display.rows()[1].label;
    assert!(label.contains(", +"), "{label}");
    assert_eq!(snapshot(&root), before);
}
