use super::*;
use crate::{git::Comparison, original_diff_sides_v1_test_support as support};
#[test]
fn original_diff_sides_real_old_c_blob_roles_eight_children_no_checkout_or_discovery() {
    let (real, a, b, c) = support::fixture("sha256");
    let workspace = support::workspace(&real, Comparison::BranchChanges { base: a, head: b });
    let picker = support::state(&workspace);
    let before = real.snapshot();
    real.clear_trace();
    let leaf = support::open(&workspace, &picker, b"shared", Side::Old);
    assert_eq!(leaf.sha(), c);
    assert_eq!(leaf.preview().content, b"COMMON old bytes\n");
    assert!(leaf
        .rows()
        .iter()
        .any(|row| row.label.contains("Common Writer")));
    assert!(leaf
        .rows()
        .iter()
        .any(|row| row.label.contains("Common Recorder")));
    assert_eq!(real.children().len(), 8);
    support::assert_no_discovery(&real);
    assert_eq!(real.snapshot(), before);
}
#[test]
fn original_diff_sides_symlink_and_submodule_never_follow_clone_or_fetch() {
    let (real, _, _, c) = support::fixture("sha1");
    let b = support::commit(
        &real,
        &support::tree(
            &real,
            &[
                (b"external-link", b"/outside/secret", "120000"),
                (b"module", c.as_bytes(), "160000"),
            ],
        ),
        Some(&c),
        "Special",
    );
    let workspace = support::workspace(&real, Comparison::Snapshots { base: c, head: b });
    let picker = support::state(&workspace);
    let before = real.snapshot();
    real.clear_trace();
    let link = support::open(&workspace, &picker, b"external-link", Side::New);
    assert_eq!(
        link.preview().kind,
        crate::repository_view::BlobPreviewKind::Symlink
    );
    assert_eq!(link.preview().content, b"/outside/secret");
    assert_eq!(real.children().len(), 8);
    real.clear_trace();
    let module = support::open(&workspace, &picker, b"module", Side::New);
    assert_eq!(
        module.preview().kind,
        crate::repository_view::BlobPreviewKind::Submodule
    );
    assert!(module.preview().content.is_empty());
    assert_eq!(real.children().len(), 6);
    support::assert_no_discovery(&real);
    assert_eq!(real.snapshot(), before);
}
#[test]
fn original_diff_sides_binary_and_lfs_pointer_are_honest_metadata_no_network() {
    let (real, _, _, c) = support::fixture("sha256");
    let pointer = format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize 99999\n",
        "a".repeat(64)
    );
    let b = support::commit(
        &real,
        &support::tree(
            &real,
            &[
                (b"binary", b"\0actualbinary", "100644"),
                (b"pointer", pointer.as_bytes(), "100644"),
            ],
        ),
        Some(&c),
        "Kinds",
    );
    let workspace = support::workspace(&real, Comparison::Snapshots { base: c, head: b });
    let picker = support::state(&workspace);
    real.clear_trace();
    let binary = support::open(&workspace, &picker, b"binary", Side::New);
    assert_eq!(
        binary.preview().kind,
        crate::repository_view::BlobPreviewKind::Binary
    );
    assert!(binary.rows().iter().all(|row| row.action.is_none()));
    let lfs = support::open(&workspace, &picker, b"pointer", Side::New);
    assert!(matches!(
        lfs.preview().kind,
        crate::repository_view::BlobPreviewKind::LfsPointer { size: 99999, .. }
    ));
    assert_eq!(lfs.preview().content, pointer.as_bytes());
    assert_eq!(real.children().len(), 16);
}
#[test]
fn original_diff_sides_large_blob_size_refuses_before_object_read() {
    let (real, _, _, c) = support::fixture("sha1");
    let bytes = vec![0; 2 * 1024 * 1024 + 1];
    let b = support::commit(
        &real,
        &support::tree(&real, &[(b"large", &bytes, "100644")]),
        Some(&c),
        "Large",
    );
    let workspace = support::workspace(&real, Comparison::Snapshots { base: c, head: b });
    let picker = support::state(&workspace);
    let frame = picker.frame().unwrap();
    let index = (0..frame.len())
        .find(|index| {
            frame
                .member(*index)
                .unwrap()
                .path(Side::New)
                .is_some_and(|path| path.0 == b"large")
        })
        .unwrap();
    let ticket = frame.ticket(index, Side::New).unwrap();
    let op = LocalGitNavigationReadV1::begin(
        workspace.actor_origin.as_ref().unwrap(),
        Default::default(),
    )
    .unwrap();
    real.clear_trace();
    assert!(PreparedOriginalDiffSideV1::open(&ticket, &op)
        .unwrap_err()
        .contains("exceeds2MiB"));
    assert_eq!(real.children().len(), 3);
    assert_eq!(op.accounting().unwrap().children_started(), 3);
    assert!(!real.children().iter().any(|event| event["argv"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "blob")));
}
#[test]
fn original_diff_sides_missing_blob_type_fails_honestly_no_head_or_worktree_substitute() {
    let (real, a, b, c) = support::fixture("sha1");
    let workspace = support::workspace(&real, Comparison::Snapshots { base: a, head: b });
    let picker = support::state(&workspace);
    let object = support::object_id(&real, &c, "shared");
    let held = support::remove_object(&real.root, &object);
    let op = LocalGitNavigationReadV1::begin(
        workspace.actor_origin.as_ref().unwrap(),
        Default::default(),
    )
    .unwrap();
    let frame = picker.frame().unwrap();
    let visible = (0..frame.len())
        .find(|index| {
            frame
                .member(*index)
                .unwrap()
                .path(Side::New)
                .is_some_and(|path| path.0 == b"shared")
        })
        .unwrap();
    let ticket = frame.ticket(visible, Side::Old).unwrap();
    // Snapshot Old is A, so remove its ACTUAL blob, not the common C blob.
    let old_object = support::object_id(&real, ticket.source().intent().content_base(), "shared");
    let old_held = support::remove_object(&real.root, &old_object);
    real.clear_trace();
    assert!(PreparedOriginalDiffSideV1::open(&ticket, &op).is_err());
    assert_eq!(real.children().len(), 3);
    assert!(!real.children().iter().any(|event| event["argv"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "blob")));
    std::fs::rename(
        old_held,
        real.root
            .join(".git/objects")
            .join(&old_object[..2])
            .join(&old_object[2..]),
    )
    .unwrap();
    std::fs::rename(
        held,
        real.root
            .join(".git/objects")
            .join(&object[..2])
            .join(&object[2..]),
    )
    .unwrap();
}
#[test]
fn original_diff_sides_back_pins_actual_content_basis_zero_git_and_arc_visible_precedence() {
    let (real, a, b, c) = support::fixture("sha256");
    let mut workspace = support::workspace(
        &real,
        Comparison::BranchChanges {
            base: a.clone(),
            head: b.clone(),
        },
    );
    let picker = support::state(&workspace);
    let leaf = support::open(&workspace, &picker, b"shared", Side::Old);
    let commit = leaf.prepared_commit();
    leaf.clone().install(&mut workspace, commit).unwrap();
    assert!(leaf.visible_for(&workspace));
    assert_eq!(workspace.revision, c);
    assert!(workspace.comparison_v1().is_none());
    let op = LocalGitNavigationReadV1::begin(
        workspace.actor_origin.as_ref().unwrap(),
        Default::default(),
    )
    .unwrap();
    real.clear_trace();
    let back = leaf.prepare_back(&op).unwrap();
    op.seal().unwrap();
    back.install(&mut workspace).unwrap();
    assert!(real.children().is_empty());
    assert_eq!(workspace.revision, b);
    assert!(workspace
        .original_comparison_v1
        .as_ref()
        .unwrap()
        .visible_for(&workspace));
    assert_eq!(workspace.comparison_v1().unwrap().base, Some(a));
    assert_eq!(workspace.comparison_v1().unwrap().diff.base, Some(c));
    assert!(workspace.original_diff_side_v1.is_none());
    assert!(workspace.rows.is_empty());
}
