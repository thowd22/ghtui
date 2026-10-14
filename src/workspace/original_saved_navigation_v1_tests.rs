use super::*;
use crate::local_commit_actor_origin_v1_test_support::{git, text, Real};
use std::fs;

pub(crate) struct NestedReal {
    pub real: Real,
    pub chosen: String,
    pub responsible: String,
    pub path: RawPath,
}
impl NestedReal {
    pub fn new(format: &str) -> Self {
        let real = Real::new(format);
        let content = (1..=145)
            .map(|line| format!("actual saved line {line:03}\n"))
            .collect::<String>();
        fs::write(real.root.join("history.txt"), content).unwrap();
        git(&real.root, &["add", "--", "history.txt"], None);
        git(
            &real.root,
            &[
                "commit",
                "--only",
                "--author=Actual Line Writer <line@example.invalid>",
                "-qm",
                "Original line source",
                "--",
                "history.txt",
            ],
            None,
        );
        let responsible = text(&real.root, &["rev-parse", "HEAD"]);
        fs::create_dir(real.root.join("nested")).unwrap();
        git(
            &real.root,
            &["mv", "--", "history.txt", "nested/renamed.txt"],
            None,
        );
        git(
            &real.root,
            &[
                "commit",
                "--only",
                "-qm",
                "Chosen rename source",
                "--",
                "history.txt",
                "nested/renamed.txt",
            ],
            None,
        );
        let chosen = text(&real.root, &["rev-parse", "HEAD"]);
        fs::write(
            real.root.join("nested/renamed.txt"),
            b"future line outside chosen source\n",
        )
        .unwrap();
        git(&real.root, &["add", "--", "nested/renamed.txt"], None);
        git(
            &real.root,
            &[
                "commit",
                "--only",
                "-qm",
                "Future not chosen",
                "--",
                "nested/renamed.txt",
            ],
            None,
        );
        fs::write(
            real.root.join("nested/renamed.txt"),
            b"user dirty renamed bytes\n",
        )
        .unwrap();
        Self {
            real,
            chosen,
            responsible,
            path: RawPath(b"nested/renamed.txt".to_vec()),
        }
    }
    pub fn workspace(&self) -> Workspace {
        Workspace::load(&self.real.root, RepositoryMode::Tree, &self.chosen, None)
            .unwrap()
            .unwrap()
    }
    pub fn action(&self, workspace: &mut Workspace, action: RowAction) -> usize {
        let origin = workspace.actor_origin.as_ref().unwrap().clone();
        let operation = LocalGitNavigationReadV1::begin(&origin, Default::default()).unwrap();
        self.real.clear_trace();
        workspace
            .open_original_with_operation_v1(action, &operation)
            .unwrap();
        assert!(Arc::ptr_eq(
            workspace.actor_origin.as_ref().unwrap(),
            &origin
        ));
        let children = self.real.children();
        assert_no_discovery(&children);
        assert_eq!(operation.seal().unwrap().children_started(), children.len());
        children.len()
    }
    pub fn file(&self, workspace: &mut Workspace) {
        assert_eq!(
            self.action(workspace, RowAction::Directory(RawPath(b"nested".to_vec()))),
            2
        );
        assert_eq!(
            self.action(workspace, RowAction::File(self.path.clone())),
            4
        );
        assert_eq!(workspace.preview.as_ref().unwrap().revision, self.chosen);
        assert!(workspace
            .preview
            .as_ref()
            .unwrap()
            .content
            .starts_with(b"actual saved line 001\n"));
    }
}
pub(crate) fn assert_no_discovery(children: &[serde_json::Value]) {
    for event in children {
        let args = event["argv"].as_array().unwrap();
        assert!(!args.iter().any(|arg| matches!(
            arg.as_str(),
            Some(
                "--absolute-git-dir"
                    | "--show-toplevel"
                    | "--git-common-dir"
                    | "--is-bare-repository"
            )
        )));
        assert!(!args.iter().any(|arg| matches!(
            arg.as_str(),
            Some("checkout" | "switch" | "add" | "reset" | "fetch" | "clone" | "commit")
        )));
    }
}

#[test]
fn original_saved_navigation_nested_tree_file_sha1_sha256_same_origin_and_actual_children() {
    for format in ["sha1", "sha256"] {
        let fixture = NestedReal::new(format);
        let before = fixture.real.snapshot();
        let dirty = fs::read(fixture.real.root.join("nested/renamed.txt")).unwrap();
        let mut workspace = fixture.workspace();
        fixture.file(&mut workspace);
        assert_ne!(
            workspace.revision,
            text(&fixture.real.root, &["rev-parse", "HEAD"])
        );
        assert_eq!(fixture.real.snapshot(), before);
        assert_eq!(
            fs::read(fixture.real.root.join("nested/renamed.txt")).unwrap(),
            dirty
        );
    }
}
#[test]
fn original_saved_navigation_history_blame_responsible_roles_not_checkout_or_blame_name_guess() {
    let fixture = NestedReal::new("sha1");
    let before = fixture.real.snapshot();
    let mut workspace = fixture.workspace();
    fixture.file(&mut workspace);
    assert_eq!(
        fixture.action(
            &mut workspace,
            RowAction::FileProvenance(FileAction::History)
        ),
        6
    );
    assert_eq!(
        workspace.file_provenance.as_ref().unwrap().revision,
        fixture.chosen
    );
    assert_eq!(
        fixture.action(&mut workspace, RowAction::FileProvenance(FileAction::Blame)),
        11
    );
    assert_eq!(
        fixture.action(
            &mut workspace,
            RowAction::FileProvenance(FileAction::BlameLine(2))
        ),
        6
    );
    let selected = workspace.selected_commit.as_ref().unwrap();
    assert_eq!(selected.id, fixture.responsible);
    assert_eq!(selected.author.name, "Actual Line Writer");
    assert_eq!(selected.committer.name, "Current Local Recorder");
    assert_eq!(
        workspace
            .file_provenance
            .as_ref()
            .unwrap()
            .selection
            .as_ref()
            .unwrap()
            .path
            .as_ref()
            .unwrap()
            .0,
        b"history.txt"
    );
    for role in [GitRole::Author, GitRole::Committer] {
        assert_eq!(fixture.action(&mut workspace, RowAction::Actor(role)), 0);
        let profile =
            crate::repository_commit_actor_profile_v1::LocalActorProfileV1::prepare(&workspace)
                .unwrap();
        assert_eq!(profile.sha(), fixture.responsible);
        assert_eq!(profile.role(), role);
    }
    assert_eq!(fixture.real.snapshot(), before);
}
#[test]
fn original_saved_navigation_provenance_graph_diff_file_and_back_keep_chosen_and_responsible_source(
) {
    let fixture = NestedReal::new("sha256");
    let before = fixture.real.snapshot();
    let mut workspace = fixture.workspace();
    fixture.file(&mut workspace);
    fixture.action(
        &mut workspace,
        RowAction::FileProvenance(FileAction::BlameAt(2)),
    );
    for action in [
        FileAction::Graph,
        FileAction::Diff,
        FileAction::OriginalFile,
    ] {
        let count = fixture.action(&mut workspace, RowAction::FileProvenance(action));
        assert!(
            count > 0,
            "Exact destination is actual Git read, not fabricated prepared body"
        );
        assert_eq!(
            workspace.selected_commit.as_ref().unwrap().id,
            fixture.responsible
        );
        assert_eq!(
            workspace.file_provenance.as_ref().unwrap().revision,
            fixture.chosen
        );
        fixture.action(&mut workspace, RowAction::FileProvenance(FileAction::Back));
        assert_eq!(workspace.revision, fixture.chosen);
        assert_eq!(
            workspace.file_provenance.as_ref().unwrap().view,
            ProvenanceView::Blame
        );
        fixture.action(
            &mut workspace,
            RowAction::FileProvenance(FileAction::BlameLine(2)),
        );
    }
    assert_eq!(fixture.real.snapshot(), before);
}
#[test]
fn original_saved_navigation_replacement_same_path_same_sha_rejects_before_nested_child() {
    let fixture = NestedReal::new("sha1");
    let mut workspace = fixture.workspace();
    let moved = fixture.real.directory.path().join("retained-original");
    fs::rename(&fixture.real.root, &moved).unwrap();
    git(
        fixture.real.directory.path(),
        &[
            "clone",
            "--quiet",
            "--no-hardlinks",
            moved.to_str().unwrap(),
            fixture.real.root.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(
        text(&fixture.real.root, &["rev-parse", "HEAD~1"]),
        fixture.chosen
    );
    fs::write(fixture.real.root.join("tracked"), b"replacement dirty\n").unwrap();
    fs::write(
        fixture.real.root.join("scratch"),
        b"replacement untracked\n",
    )
    .unwrap();
    let before = fixture.real.snapshot();
    let operation = LocalGitNavigationReadV1::begin(
        workspace.actor_origin.as_ref().unwrap(),
        Default::default(),
    )
    .unwrap();
    fixture.real.clear_trace();
    assert!(workspace
        .open_original_with_operation_v1(
            RowAction::Directory(RawPath(b"nested".to_vec())),
            &operation
        )
        .unwrap_err()
        .contains("source/locator changed"));
    assert!(fixture.real.children().is_empty());
    assert_eq!(operation.accounting().unwrap().children_started(), 0);
    assert_eq!(fixture.real.snapshot(), before);
    assert_eq!(
        fs::read(moved.join("nested/renamed.txt")).unwrap(),
        b"user dirty renamed bytes\n"
    );
}
#[test]
fn original_saved_navigation_live_bounds_cannot_strip_completed_bounds_keep_actual_origin_for_fresh_read(
) {
    let fixture = NestedReal::new("sha1");
    let ordinary = fixture.workspace();
    let old = LocalGitNavigationReadV1::begin(
        ordinary.actor_origin.as_ref().unwrap(),
        Default::default(),
    )
    .unwrap();
    let handle = fixture
        .real
        .repository
        .clone()
        .with_navigation_read_window_v1(old.window());
    let origin = crate::git::OriginalLocalActorOriginV1::capture(&handle).unwrap();
    let mut workspace = ordinary;
    workspace.actor_origin = Some(origin.clone());
    let new = LocalGitNavigationReadV1::begin(&origin, Default::default()).unwrap();
    fixture.real.clear_trace();
    assert!(workspace
        .open_original_with_operation_v1(RowAction::Directory(RawPath(b"nested".to_vec())), &new)
        .is_err());
    assert!(fixture.real.children().is_empty());
    old.seal().unwrap();
    workspace
        .open_original_with_operation_v1(RowAction::Directory(RawPath(b"nested".to_vec())), &new)
        .unwrap();
    assert_eq!(fixture.real.children().len(), 2);
    assert!(Arc::ptr_eq(
        workspace.actor_origin.as_ref().unwrap(),
        &origin
    ));
    assert_eq!(old.accounting().unwrap().children_started(), 0);
    new.seal().unwrap();
    drop(old);
    fixture.real.clear_trace();
    let raw = origin
        .read_original(
            &fixture.responsible,
            &Default::default(),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(raw.sha(), fixture.responsible);
    assert_eq!(fixture.real.children().len(), 2);
}
#[test]
fn original_saved_navigation_cancelled_new_request_and_missing_origin_do_not_rediscover() {
    let fixture = NestedReal::new("sha1");
    let mut workspace = fixture.workspace();
    let token = CancellationToken::default();
    let operation =
        LocalGitNavigationReadV1::begin(workspace.actor_origin.as_ref().unwrap(), token.clone())
            .unwrap();
    token.cancel();
    fixture.real.clear_trace();
    assert!(workspace
        .open_original_with_operation_v1(
            RowAction::Directory(RawPath(b"nested".to_vec())),
            &operation
        )
        .is_err());
    let next = LocalGitNavigationReadV1::begin(
        workspace.actor_origin.as_ref().unwrap(),
        Default::default(),
    )
    .unwrap();
    workspace.actor_origin = None;
    assert!(workspace
        .open_original_with_operation_v1(RowAction::Directory(RawPath(b"nested".to_vec())), &next)
        .is_err());
    assert!(fixture.real.children().is_empty());
}
#[test]
fn original_saved_navigation_tree_revision_and_uncached_commit_require_original_exact_worker() {
    let fixture = NestedReal::new("sha1");
    let mut workspace = fixture.workspace();
    workspace.tree.as_mut().unwrap().revision = fixture.responsible.clone();
    let operation = LocalGitNavigationReadV1::begin(
        workspace.actor_origin.as_ref().unwrap(),
        Default::default(),
    )
    .unwrap();
    fixture.real.clear_trace();
    assert!(workspace
        .open_original_with_operation_v1(RowAction::File(fixture.path.clone()), &operation)
        .is_err());
    assert!(fixture.real.children().is_empty());
    workspace
        .open_original_with_operation_v1(RowAction::Commit(fixture.responsible.clone()), &operation)
        .unwrap();
    assert_eq!(
        workspace.selected_commit.as_ref().unwrap().id,
        fixture.responsible
    );
    assert_eq!(fixture.real.children().len(), 3);
}

#[test]
fn original_saved_navigation_no_remote_working_diff_keeps_failed_charge_policy_and_remote_fingerprint(
) {
    let fixture = NestedReal::new("sha1");
    let before = fixture.real.snapshot();
    let workspace = fixture.workspace();
    let original = workspace.actor_origin.as_ref().unwrap();
    let fingerprint = fixture.real.repository.fingerprint().unwrap();
    let operation = LocalGitNavigationReadV1::begin(original, Default::default()).unwrap();
    fixture.real.clear_trace();
    let working = workspace
        .load_original_with_operation_v1(RepositoryMode::Working, &workspace.revision, &operation)
        .unwrap();
    assert!(working
        .rows
        .iter()
        .any(|row| row.label.contains("user dirty renamed bytes")));
    assert_eq!(
        working.comparison.as_ref().unwrap().observed,
        Some(fingerprint)
    );
    let children = fixture.real.children();
    assert_no_discovery(&children);
    let config_reads = children
        .iter()
        .filter(|event| {
            event["argv"].as_array().unwrap().windows(3).any(|args| {
                args.iter().map(|arg| arg.as_str()).eq([
                    Some("config"),
                    Some("--null"),
                    Some("--list"),
                ])
            })
        })
        .count();
    assert_eq!(
        config_reads, 2,
        "both comparison fingerprints read real successful empty remote sets"
    );
    let accounting = operation.seal().unwrap();
    assert_eq!(accounting.failed_reserved_bytes(), 0);
    assert_eq!(accounting.children_started(), children.len());
    assert!(accounting.stdout_bytes() > 0);
    assert_eq!(fixture.real.snapshot(), before);
    git(
        &fixture.real.root,
        &["config", "custom.unrelated", "not-a-remote"],
        None,
    );
    assert_eq!(fixture.real.repository.fingerprint().unwrap(), fingerprint);
    git(
        &fixture.real.root,
        &["config", "remote.origin.url", "https://example.invalid/one"],
        None,
    );
    let first_remote = fixture.real.repository.fingerprint().unwrap();
    assert_ne!(first_remote, fingerprint);
    git(
        &fixture.real.root,
        &["config", "remote.origin.url", "https://example.invalid/two"],
        None,
    );
    assert_ne!(fixture.real.repository.fingerprint().unwrap(), first_remote);
}
