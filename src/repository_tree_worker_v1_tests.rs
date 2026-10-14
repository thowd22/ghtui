use super::*;
use crate::{
    git::{original_tree_inventory_v1::tests::fixture_tree, OriginalTreeIntentV1},
    local_commit_actor_origin_v1_test_support::Real,
    repository_tree_jump_v1::OriginalTreeJumpActionV1,
};
fn fixture() -> (
    Real,
    Workspace,
    Option<OriginalTreeJumpV1>,
    Arc<OriginalTreeWorkerOwnerV1>,
) {
    let real = Real::new("sha1");
    let workspace = fixture_tree(&real, &[b"saved", b"source"]);
    let mut jump = OriginalTreeJumpV1::new(OriginalTreeIntentV1::capture(&workspace).unwrap());
    let request = jump.request().unwrap();
    let owner = OriginalTreeWorkerOwnerV1::begin(
        &workspace,
        Some(&jump),
        Arc::new(OriginalTreeWorkV1::Query(request)),
        Default::default(),
    )
    .unwrap();
    (real, workspace, Some(jump), owner)
}
#[test]
fn original_tree_jump_worker_same_original_request_sealed_single_actual_child_and_arc_publish() {
    let (real, mut workspace, mut jump, owner) = fixture();
    let before = real.snapshot();
    real.clear_trace();
    let result = owner.prepare().unwrap();
    assert!(result.matches(&owner));
    assert_eq!(owner.accounting().unwrap().children_started(), 1);
    assert_eq!(result.publish(&mut workspace, &mut jump).unwrap(), None);
    assert_eq!(jump.as_ref().unwrap().frame().unwrap().matches().len(), 2);
    assert_eq!(real.children().len(), 1);
    assert_eq!(real.snapshot(), before);
}
#[test]
fn original_tree_jump_worker_old_query_success_and_error_refused_even_same_original_id_matches() {
    let (_real, mut workspace, mut jump, owner) = fixture();
    let result = owner.prepare().unwrap();
    jump.as_mut()
        .unwrap()
        .apply(OriginalTreeJumpActionV1::Insert("s".into()))
        .unwrap();
    assert!(!owner.can_admit(&workspace, jump.as_ref(), true));
    assert!(!owner.can_admit(&workspace, jump.as_ref(), false));
    assert!(result.publish(&mut workspace, &mut jump).is_err());
    assert!(!owner.failed(jump.as_mut().unwrap(), "old panic/error"));
}
#[test]
fn original_tree_jump_worker_cancellation_before_start_has_zero_original_children_no_late_model() {
    let (real, workspace, jump, owner) = fixture();
    real.clear_trace();
    owner.cancellation().cancel();
    assert!(owner.prepare().is_err());
    assert_eq!(owner.accounting().unwrap().children_started(), 0);
    assert!(real.children().is_empty());
    assert!(!owner.can_admit(&workspace, jump.as_ref(), false));
}
#[test]
fn original_tree_jump_worker_close_or_source_selection_change_rejects_success_and_caught_error() {
    let (_real, mut workspace, mut jump, owner) = fixture();
    let result = owner.prepare().unwrap();
    assert!(
        jump.take().is_some(),
        "Close removes an actual open owned chooser"
    );
    assert!(!owner.can_admit(&workspace, None, false));
    assert!(result.publish(&mut workspace, &mut jump).is_err());
    let (_real, mut workspace, jump, owner) = fixture();
    workspace.mode = crate::workspace::RepositoryMode::Working;
    assert!(!owner.can_admit(&workspace, jump.as_ref(), true));
    assert!(!owner.can_admit(&workspace, jump.as_ref(), false));
}
#[test]
fn original_tree_jump_worker_foreign_owner_does_not_mint_prepared_success() {
    let (_real, _workspace, _jump, owner) = fixture();
    let result = owner.prepare().unwrap();
    let (_other, _workspace, _jump, foreign) = fixture();
    assert!(!result.matches(&foreign));
}
