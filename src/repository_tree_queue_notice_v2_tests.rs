use super::*;
use crate::{
    git::OriginalTreeIntentV1, repository_tree_jump_v1::OriginalTreeJumpEffectV1,
    repository_tree_viewport_v2_test_support::ready,
    repository_tree_worker_v1::OriginalTreeWorkerOwnerV1,
};
fn pending(workspace: &Workspace) -> (OriginalTreeJumpV1, Arc<OriginalTreeWorkV1>) {
    let mut jump = OriginalTreeJumpV1::new(OriginalTreeIntentV1::capture(workspace).unwrap());
    let request = jump.request().unwrap();
    (jump, Arc::new(OriginalTreeWorkV1::Query(request)))
}
#[test]
fn original_tree_viewport_queue_full_exact_owner_clears_only_pending_and_paints_owned_reason() {
    let (real, workspace, _loaded) = ready("sha1", &[b"actual.rs"]);
    let (mut jump, work) = pending(&workspace);
    let owner =
        OriginalTreeWorkerOwnerV1::begin(&workspace, Some(&jump), work.clone(), Default::default())
            .unwrap();
    let original = jump.pending().unwrap().clone();
    let failure = OriginalTreeQueueFailureV2::reject(
        &workspace,
        &mut jump,
        &work,
        Some(&owner),
        "Original tree queue full",
    )
    .unwrap();
    assert!(jump.pending().is_none());
    assert!(jump.frame().is_none());
    assert!(jump.status.contains("Original tree queue full"));
    assert!(jump.status.contains("Retry is explicit"));
    assert!(Arc::ptr_eq(&failure.request, &original));
    assert!(failure.is_current(&workspace, &jump));
    assert_eq!(owner.accounting().unwrap().children_started(), 0);
    assert_eq!(failure.request_epoch_v2(), original.epoch());
    let mut app = crate::app::App {
        view: crate::app::View::Repository,
        workspace: Some(workspace),
        original_tree_jump_v1: Some(jump),
        refresh_requested: false,
        ..Default::default()
    };
    app.original_tree_viewport_v2.failure = Some(failure.clone());
    let mut previous = None;
    for expected_epoch in [original.epoch(), original.epoch() + 1] {
        let label = format!("Request #{expected_epoch} rejected");
        for width in [40, 80, 120] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, 24)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut app))
                .unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content
                .chunks(usize::from(width))
                .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                text.contains(&label),
                "Exact rejected request is visible at width{width}: {text}"
            );
            assert!(
                text.contains("Original tree queue full") && text.contains("Retry is explicit")
            );
            assert!(text.contains("[Retry]") && text.contains("[Esc Close]"));
            if let Some(previous) = &previous {
                assert!(
                    !text.contains(previous),
                    "Old rejection cannot acknowledge the new request"
                );
            }
        }
        if previous.is_none() {
            crate::repository_tree_viewport_native_v2::dispatch(
                &mut app,
                crate::repository_tree_viewport_v2::TreeViewportActionV2::Retry(failure.clone()),
            );
            let next = app.original_tree_work_v1.take().unwrap();
            let next_failure = OriginalTreeQueueFailureV2::reject(
                app.workspace.as_ref().unwrap(),
                app.original_tree_jump_v1.as_mut().unwrap(),
                &next,
                None,
                "Original tree queue full",
            )
            .unwrap();
            assert_eq!(next_failure.request_epoch_v2(), expected_epoch + 1);
            app.original_tree_viewport_v2.failure = Some(next_failure);
        }
        previous = Some(label);
    }
    assert!(real.children().is_empty());
}
#[test]
fn original_tree_viewport_begin_failure_matches_current_request_without_minting_owner() {
    let (real, workspace, _loaded) = ready("sha256", &[b"actual.rs"]);
    let (mut jump, work) = pending(&workspace);
    let failure = OriginalTreeQueueFailureV2::reject(
        &workspace,
        &mut jump,
        &work,
        None,
        "Original owner unavailable",
    )
    .unwrap();
    assert!(failure.is_current(&workspace, &jump));
    assert!(jump.status.contains("Original owner unavailable"));
    assert!(jump.pending().is_none());
    assert!(real.children().is_empty());
}
#[test]
fn original_tree_viewport_new_pending_cannot_be_cleared_by_prior_or_foreign_owned_failure() {
    let (real, workspace, _loaded) = ready("sha1", &[b"actual.rs"]);
    let (mut jump, old_work) = pending(&workspace);
    let old_owner = OriginalTreeWorkerOwnerV1::begin(
        &workspace,
        Some(&jump),
        old_work.clone(),
        Default::default(),
    )
    .unwrap();
    let new = jump.request().unwrap();
    assert!(OriginalTreeQueueFailureV2::reject(
        &workspace,
        &mut jump,
        &old_work,
        Some(&old_owner),
        "late old failure"
    )
    .is_none());
    assert!(Arc::ptr_eq(jump.pending().unwrap(), &new));
    let new_work = Arc::new(OriginalTreeWorkV1::Query(new));
    assert!(OriginalTreeQueueFailureV2::reject(
        &workspace,
        &mut jump,
        &new_work,
        Some(&old_owner),
        "foreign owner"
    )
    .is_none());
    assert!(jump.pending().is_some());
    assert!(jump.status.contains("Matching"));
    assert!(real.children().is_empty());
}
#[test]
fn original_tree_viewport_explicit_retry_uses_new_actual_token_not_canceled_original_operation() {
    let (real, workspace, _loaded) = ready("sha1", &[b"actual.rs"]);
    let before = real.snapshot();
    let (mut jump, work) = pending(&workspace);
    let old =
        OriginalTreeWorkerOwnerV1::begin(&workspace, Some(&jump), work.clone(), Default::default())
            .unwrap();
    let failure =
        OriginalTreeQueueFailureV2::reject(&workspace, &mut jump, &work, Some(&old), "queue full")
            .unwrap();
    old.cancellation().cancel();
    let retry = failure.retry(&workspace, &mut jump).unwrap();
    assert!(!Arc::ptr_eq(&work, &retry));
    assert!(!failure.is_current(&workspace, &jump));
    let new = OriginalTreeWorkerOwnerV1::begin(&workspace, Some(&jump), retry, Default::default())
        .unwrap();
    assert!(!new.cancellation().is_cancelled());
    let prepared = new.prepare().unwrap();
    assert_eq!(new.accounting().unwrap().children_started(), 1);
    assert_eq!(old.accounting().unwrap().children_started(), 0);
    assert!(prepared.matches(&new));
    let mut target = workspace.clone();
    let mut chooser = Some(jump);
    assert!(prepared
        .publish(&mut target, &mut chooser)
        .unwrap()
        .is_none());
    assert!(chooser.as_ref().unwrap().pending().is_none());
    assert!(chooser.as_ref().unwrap().frame().is_some());
    assert_eq!(real.snapshot(), before);
}
#[test]
fn original_tree_viewport_jump_rejection_preserves_original_ready_frame_and_oversize_is_bounded() {
    let (real, workspace, mut jump) = ready("sha1", &[b"actual.rs"]);
    let frame = jump.frame().unwrap().clone();
    let ticket = frame.ticket(0).unwrap();
    let work = Arc::new(OriginalTreeWorkV1::Jump(ticket));
    let owner =
        OriginalTreeWorkerOwnerV1::begin(&workspace, Some(&jump), work.clone(), Default::default())
            .unwrap();
    let notice = reject_navigation_v2(
        &workspace,
        Some(&mut jump),
        &work,
        Some(&owner),
        &"x".repeat(5000),
    )
    .unwrap();
    assert!(notice.contains("diagnostic omitted"));
    assert!(Arc::ptr_eq(jump.frame().unwrap(), &frame));
    assert!(jump.pending().is_none());
    assert!(matches!(
        jump.apply(crate::repository_tree_jump_v1::OriginalTreeJumpActionV1::Choose)
            .unwrap(),
        OriginalTreeJumpEffectV1::Open(_)
    ));
    assert_eq!(owner.accounting().unwrap().children_started(), 0);
    assert!(real.children().is_empty());
}
