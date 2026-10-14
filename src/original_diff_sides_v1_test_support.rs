//! NEW source-only actual Git fixtures; no constructed origin/member proof.
use crate::{
    git::{Comparison, RawPath},
    local_commit_actor_origin_v1_test_support::{git, text, Real},
    workspace::{RepositoryMode, Workspace},
};
use std::{fs, path::Path, sync::Arc};
pub(crate) fn tree(real: &Real, files: &[(&[u8], &[u8], &str)]) -> String {
    let mut records = vec![];
    for (path, content, mode) in files {
        let id = if *mode == "160000" {
            std::str::from_utf8(content).unwrap().to_owned()
        } else {
            String::from_utf8(git(
                &real.root,
                &["hash-object", "-w", "--stdin"],
                Some(content),
            ))
            .unwrap()
            .trim()
            .to_owned()
        };
        records.extend_from_slice(
            format!(
                "{mode} {} {id}\t",
                if *mode == "160000" { "commit" } else { "blob" }
            )
            .as_bytes(),
        );
        records.extend_from_slice(path);
        records.push(0);
    }
    String::from_utf8(git(&real.root, &["mktree", "-z"], Some(&records)))
        .unwrap()
        .trim()
        .to_owned()
}
pub(crate) fn commit(real: &Real, tree: &str, parent: Option<&str>, name: &str) -> String {
    let mut raw = format!("tree {tree}\n");
    if let Some(parent) = parent {
        raw.push_str(&format!("parent {parent}\n"));
    }
    raw.push_str(&format!("author {name} Writer <{name}-writer@example.invalid> 1700000000 +0000\ncommitter {name} Recorder <{name}-recorder@example.invalid> 1700000100 +0000\n\n{name} saved side\n"));
    String::from_utf8(git(
        &real.root,
        &["hash-object", "-t", "commit", "-w", "--stdin"],
        Some(raw.as_bytes()),
    ))
    .unwrap()
    .trim()
    .to_owned()
}
pub(crate) fn fixture(format: &str) -> (Real, String, String, String) {
    let real = Real::new(format);
    let c = commit(
        &real,
        &tree(
            &real,
            &[
                (b"shared", b"COMMON old bytes\n", "100644"),
                (b"rename old", b"stable renamed bytes\n", "100644"),
                (b"deleted", b"deleted saved bytes\n", "100644"),
            ],
        ),
        None,
        "Common",
    );
    let a = commit(
        &real,
        &tree(
            &real,
            &[
                (b"shared", b"REQUESTED A bytes must not be old\n", "100644"),
                (b"rename old", b"stable renamed bytes\n", "100644"),
                (b"deleted", b"deleted saved bytes\n", "100644"),
            ],
        ),
        Some(&c),
        "Requested",
    );
    let b = commit(
        &real,
        &tree(
            &real,
            &[
                (b"shared", b"TARGET new bytes\n", "100644"),
                (b"rename new", b"stable renamed bytes\n", "100644"),
                (b"added", b"new added bytes\n", "100644"),
            ],
        ),
        Some(&c),
        "Target",
    );
    git(&real.root, &["tag", "requested-A", &a], None);
    git(&real.root, &["tag", "target-B", &b], None);
    (real, a, b, c)
}
pub(crate) fn workspace(real: &Real, question: Comparison) -> Workspace {
    let head = match &question {
        Comparison::Snapshots { head, .. } | Comparison::BranchChanges { head, .. } => head,
        Comparison::CommitChanges { commit, .. } => commit,
        _ => panic!("immutable fixture only"),
    };
    let mut workspace = Workspace::load(&real.root, RepositoryMode::Tree, head, None)
        .unwrap()
        .unwrap();
    let result = real.repository.compare(&question).unwrap();
    workspace.mode = RepositoryMode::CommitChanges;
    workspace.tree = None;
    workspace.preview = None;
    workspace.rows.clear();
    workspace.comparison = Some(result);
    workspace
}
pub(crate) fn source(workspace: &Workspace) -> Arc<crate::git::OriginalComparisonSourceV1> {
    let intent = crate::git::OriginalComparisonIntentV1::capture(workspace).unwrap();
    let op = crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1::begin(
        intent.origin(),
        Default::default(),
    )
    .unwrap();
    let source = crate::git::OriginalComparisonSourceV1::read(intent, &op).unwrap();
    op.seal().unwrap();
    source
}
pub(crate) fn state(
    workspace: &Workspace,
) -> crate::repository_diff_sides_v1::OriginalDiffPickerV1 {
    let mut picker = crate::repository_diff_sides_v1::OriginalDiffPickerV1::new(
        crate::git::OriginalComparisonIntentV1::capture(workspace).unwrap(),
    );
    let request = picker.request().unwrap();
    let op = crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1::begin(
        request.intent().origin(),
        Default::default(),
    )
    .unwrap();
    let frame =
        crate::repository_diff_sides_v1::OriginalDiffFrameV1::prepare(request, &op).unwrap();
    op.seal().unwrap();
    assert!(picker.accept(frame));
    picker
}
pub(crate) fn open(
    workspace: &Workspace,
    picker: &crate::repository_diff_sides_v1::OriginalDiffPickerV1,
    path: &[u8],
    side: crate::git::OriginalComparisonSideV1,
) -> Arc<crate::workspace::PreparedOriginalDiffSideV1> {
    let frame = picker.frame().unwrap();
    let visible = (0..frame.len())
        .find(|index| frame.member(*index).unwrap().path(side) == Some(&RawPath(path.to_vec())))
        .unwrap();
    let ticket = frame.ticket(visible, side).unwrap();
    let op = crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1::begin(
        workspace.actor_origin.as_ref().unwrap(),
        Default::default(),
    )
    .unwrap();
    let leaf = crate::workspace::PreparedOriginalDiffSideV1::open(&ticket, &op).unwrap();
    op.seal().unwrap();
    leaf
}
pub(crate) fn assert_no_discovery(real: &Real) {
    for event in real.children() {
        assert!(
            !event["argv"].as_array().unwrap().iter().any(|arg| matches!(
                arg.as_str(),
                Some(
                    "--show-toplevel"
                        | "--absolute-git-dir"
                        | "--git-common-dir"
                        | "--is-bare-repository"
                )
            ))
        );
    }
}
pub(crate) fn remove_object(root: &Path, id: &str) -> std::path::PathBuf {
    let path = root.join(".git/objects").join(&id[..2]).join(&id[2..]);
    let preserved = path.with_extension("preserved-test-object");
    fs::rename(&path, &preserved).unwrap();
    preserved
}
pub(crate) fn object_id(real: &Real, sha: &str, path: &str) -> String {
    text(&real.root, &["rev-parse", &format!("{sha}:{path}")])
}
