use super::*;
use crate::{git::Comparison, original_diff_sides_v1_test_support as support};
#[test]
fn original_diff_sides_saved_foreign_genuine_origin_refused_before_any_child() {
    let (real, a, b, _) = support::fixture("sha1");
    let mut workspace = support::workspace(&real, Comparison::Snapshots { base: a, head: b });
    let source = support::source(&workspace);
    source.install(&mut workspace).unwrap();
    let route = OriginalDiffSavedRouteV1::capture(&workspace)
        .unwrap()
        .unwrap();
    let (other, a, b, _) = support::fixture("sha1");
    let foreign = support::workspace(&other, Comparison::Snapshots { base: a, head: b });
    let origin = foreign.actor_origin.as_ref().unwrap().clone();
    let op = LocalGitNavigationReadV1::begin(&origin, Default::default()).unwrap();
    real.clear_trace();
    other.clear_trace();
    assert!(route
        .restore(origin, &op, &mut workspace)
        .unwrap_err()
        .contains("another genuine source"));
    assert!(real.children().is_empty());
    assert!(other.children().is_empty());
    assert_eq!(op.accounting().unwrap().children_started(), 0);
}
#[test]
fn original_diff_sides_saved_route_retains_actual_c_basis_not_requested_a_on_new_read() {
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
    leaf.install(&mut workspace, commit).unwrap();
    let route = OriginalDiffSavedRouteV1::capture(&workspace)
        .unwrap()
        .unwrap();
    let key = route.identity_bytes().unwrap();
    assert!(key.len() < 16 * 1024);
    let origin = workspace.actor_origin.as_ref().unwrap().clone();
    let op = LocalGitNavigationReadV1::begin(&origin, Default::default()).unwrap();
    let mut restored = support::workspace(&real, Comparison::Snapshots { base: a, head: b });
    restored.actor_origin = Some(origin.clone());
    real.clear_trace();
    route.restore(origin, &op, &mut restored).unwrap();
    op.seal().unwrap();
    let side = restored.original_diff_side_v1.as_ref().unwrap();
    assert_eq!(side.sha(), c);
    assert_eq!(side.preview().content, b"COMMON old bytes\n");
    assert!(key.windows(c.len()).any(|bytes| bytes == c.as_bytes()));
    support::assert_no_discovery(&real);
}
#[test]
fn original_diff_sides_saved_basis_change_refuses_instead_of_recomputed_common_start() {
    let (real, a, b, c) = support::fixture("sha1");
    let mut workspace = support::workspace(&real, Comparison::BranchChanges { base: a, head: b });
    let source = support::source(&workspace);
    source.install(&mut workspace).unwrap();
    let mut route = OriginalDiffSavedRouteV1::capture(&workspace)
        .unwrap()
        .unwrap();
    assert_eq!(route.old, c);
    route.old = real.original.clone();
    let op = LocalGitNavigationReadV1::begin(
        workspace.actor_origin.as_ref().unwrap(),
        Default::default(),
    )
    .unwrap();
    let origin = workspace.actor_origin.as_ref().unwrap().clone();
    assert!(route
        .restore(origin, &op, &mut workspace)
        .unwrap_err()
        .contains("basis changed"));
}
#[test]
fn original_diff_sides_saved_leaf_expected_oid_and_mode_are_not_display_name_authority() {
    let (real, a, b, _) = support::fixture("sha1");
    let mut workspace = support::workspace(&real, Comparison::Snapshots { base: a, head: b });
    let picker = support::state(&workspace);
    let side = support::open(&workspace, &picker, b"shared", Side::New);
    let commit = side.prepared_commit();
    side.install(&mut workspace, commit).unwrap();
    let mut route = OriginalDiffSavedRouteV1::capture(&workspace)
        .unwrap()
        .unwrap();
    route.member.as_mut().unwrap().object = real.original.clone();
    let origin = workspace.actor_origin.as_ref().unwrap().clone();
    let op = LocalGitNavigationReadV1::begin(&origin, Default::default()).unwrap();
    assert!(route
        .restore(origin, &op, &mut workspace)
        .unwrap_err()
        .contains("object/mode changed"));
}
