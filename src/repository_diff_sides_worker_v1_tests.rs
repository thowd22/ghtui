use super::*;
use crate::{
    git::{Comparison, OriginalComparisonIntentV1, OriginalComparisonSideV1 as Side},
    original_diff_sides_v1_test_support as support,
    repository_diff_sides_v1::OriginalDiffActionV1,
};
fn fixture() -> (
    crate::local_commit_actor_origin_v1_test_support::Real,
    Workspace,
    Option<OriginalDiffPickerV1>,
    Arc<OriginalDiffWorkerOwnerV1>,
) {
    let (real, a, b, _) = support::fixture("sha1");
    let workspace = support::workspace(&real, Comparison::BranchChanges { base: a, head: b });
    let mut picker =
        OriginalDiffPickerV1::new(OriginalComparisonIntentV1::capture(&workspace).unwrap());
    let request = picker.request().unwrap();
    let owner = OriginalDiffWorkerOwnerV1::begin(
        &workspace,
        Some(&picker),
        Arc::new(OriginalDiffWorkV1::Query(request)),
        Default::default(),
    )
    .unwrap();
    (real, workspace, Some(picker), owner)
}
#[test]
fn original_diff_sides_worker_same_original_query_sealed_actual_nine_reads_arc_publish() {
    let (real, mut workspace, mut picker, owner) = fixture();
    let before = real.snapshot();
    real.clear_trace();
    let result = owner.prepare().unwrap();
    assert!(result.matches(&owner));
    assert_eq!(owner.accounting().unwrap().children_started(), 9);
    assert_eq!(result.publish(&mut workspace, &mut picker).unwrap(), None);
    assert!(workspace.rows.is_empty());
    assert!(workspace.comparison.is_none());
    assert_eq!(
        workspace
            .original_comparison_v1
            .as_ref()
            .unwrap()
            .members()
            .len(),
        4
    );
    assert_eq!(real.snapshot(), before);
}
#[test]
fn original_diff_sides_worker_old_query_both_success_and_error_refused() {
    let (_real, mut workspace, mut picker, owner) = fixture();
    let result = owner.prepare().unwrap();
    picker
        .as_mut()
        .unwrap()
        .apply(OriginalDiffActionV1::Insert("shared".into()))
        .unwrap();
    assert!(!owner.can_admit(&workspace, picker.as_ref(), true));
    assert!(!owner.can_admit(&workspace, picker.as_ref(), false));
    assert!(result.publish(&mut workspace, &mut picker).is_err());
    assert!(!owner.failed(picker.as_mut().unwrap(), "old caught panic/error"));
}
#[test]
fn original_diff_sides_worker_cancel_before_start_has_zero_children_no_rebase() {
    let (real, workspace, picker, owner) = fixture();
    real.clear_trace();
    owner.cancellation().cancel();
    assert!(owner.prepare().is_err());
    assert!(real.children().is_empty());
    assert_eq!(owner.accounting().unwrap().children_started(), 0);
    assert!(!owner.can_admit(&workspace, picker.as_ref(), false));
}
#[test]
fn original_diff_sides_worker_foreign_result_and_closed_picker_cannot_admit() {
    let (_real, mut workspace, _, owner) = fixture();
    let result = owner.prepare().unwrap();
    let (_foreign_real, _, _, foreign) = fixture();
    assert!(!result.matches(&foreign));
    let mut picker = None;
    assert!(result.publish(&mut workspace, &mut picker).is_err());
    assert!(!owner.can_admit(&workspace, None, false));
}
#[test]
fn original_diff_sides_worker_exact_side8_and_back0_keep_one_origin_actual_queue_pool() {
    let (real, mut workspace, mut picker, owner) = fixture();
    owner
        .prepare()
        .unwrap()
        .publish(&mut workspace, &mut picker)
        .unwrap();
    let frame = picker.as_ref().unwrap().frame().unwrap();
    let visible = (0..frame.len())
        .find(|index| {
            frame
                .member(*index)
                .unwrap()
                .path(Side::Old)
                .is_some_and(|path| path.0 == b"shared")
        })
        .unwrap();
    let ticket = frame.ticket(visible, Side::Old).unwrap();
    let owner = OriginalDiffWorkerOwnerV1::begin(
        &workspace,
        picker.as_ref(),
        Arc::new(OriginalDiffWorkV1::Side(ticket)),
        Default::default(),
    )
    .unwrap();
    real.clear_trace();
    let prepared = owner.prepare().unwrap();
    assert_eq!(owner.accounting().unwrap().children_started(), 8);
    prepared.publish(&mut workspace, &mut picker).unwrap();
    assert!(picker.is_none());
    let leaf = workspace.original_diff_side_v1.as_ref().unwrap().clone();
    let origin = workspace.actor_origin.as_ref().unwrap().clone();
    let back = OriginalDiffWorkerOwnerV1::begin(
        &workspace,
        None,
        Arc::new(OriginalDiffWorkV1::Back(leaf)),
        Default::default(),
    )
    .unwrap();
    real.clear_trace();
    back.prepare()
        .unwrap()
        .publish(&mut workspace, &mut picker)
        .unwrap();
    assert_eq!(back.accounting().unwrap().children_started(), 0);
    assert!(real.children().is_empty());
    assert!(Arc::ptr_eq(
        workspace.actor_origin.as_ref().unwrap(),
        &origin
    ));
}
