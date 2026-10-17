//! Genuine ordinary Personal queue/page factories and installed gh scripts;
//! no constructed source/parent/DTO or direct publication substitutes.
use super::*;
use crate::{
    pr_actions::{PrCommand, PrOperation},
    pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
    scoped_pr_collaboration_source_v17 as source,
};
use std::collections::BTreeMap;
#[path = "scoped_pr_metadata_base_v32_tests.rs"]
mod metadata_base_v32;
#[path = "scoped_pr_metadata_members_v50_tests.rs"]
pub(crate) mod metadata_members_v50;
#[path = "scoped_pr_metadata_original_v25_tests.rs"]
mod metadata_v25;
#[path = "scoped_pr_collaboration_protocol_v17_tests.rs"]
pub(crate) mod protocol;
#[path = "scoped_pr_collaboration_refresh_v19_tests.rs"]
pub(crate) mod refresh_v19;
#[path = "scoped_pr_state_source_v68_tests.rs"]
mod state_v68;
fn parent_phase() -> Vec<GhStep> {
    let mut calls = principal();
    calls.push(get(&format!("repos/{REPO}"), repository()));
    calls.push(get(&format!("repos/{REPO}/pulls/7"), parent()));
    calls.push(get(&format!("repos/{REPO}/pulls/7"), parent()));
    calls.push(get(&format!("repos/{REPO}"), repository()));
    calls.extend(principal());
    assert_eq!(calls.len(), 8);
    calls
}
fn allocations(app: &App) -> (*const u8, *const crate::github_workspace::PrRow) {
    let p = app.github.as_ref().unwrap().pr.as_ref().unwrap();
    (p.details.summary.pr.body.as_ptr(), p.rows.as_ptr())
}
#[test]
fn conversation_comments_v17_actual_personal_parent_source_same_core_and_retained_page_allocations()
{
    let setup = setup(parent_phase());
    let page = allocations(&setup.app);
    let original =
        PrCollaborationOpenIntentV17::capture(&setup.app, &database(&setup.fixture)).unwrap();
    let before = fs::read(setup.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert!(original.matches_capture(&setup.app));
    assert_eq!(allocations(&setup.app), page);
    assert_eq!(
        fs::read(setup.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        before
    );
    let bounded = setup
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let opened = source::open(
        original.clone(),
        PrCommand::NewComment,
        &bounded,
        &database(&setup.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    assert!(Arc::ptr_eq(opened.source.opening(), &original));
    assert_eq!(opened.source.parent().database_id(), 1007);
    assert_eq!(opened.source.parent().repository_id(), 10000);
    assert_eq!(opened.source.current().unwrap().repository().id, 10000);
    assert_eq!(opened.source.identity().account_id, Some(42));
    assert!(opened.source.owns_presentation(&setup.app));
    assert_eq!(allocations(&setup.app), page);
    exact(&setup, setup.steps.len());
}
#[test]
fn conversation_comments_v17_actual_conversation_factory_rotates_ticket_without_summary_fallback_or_extra_io(
) {
    let mut setup = setup(parent_phase());
    let old = PrCollaborationOpenIntentV17::capture(&setup.app, &database(&setup.fixture)).unwrap();
    let before = fs::read(setup.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let workspace = setup
        .app
        .github
        .take()
        .unwrap()
        .load_tab(
            PrTab::Conversation,
            &setup.api,
            &CancellationToken::default(),
        )
        .unwrap();
    setup.app.github = Some(workspace);
    assert!(!old.same_presentation(&setup.app));
    let original =
        PrCollaborationOpenIntentV17::capture(&setup.app, &database(&setup.fixture)).unwrap();
    assert_eq!(original.tab(), PrTab::Conversation);
    assert_eq!(
        fs::read(setup.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        before
    );
    let bounded = setup
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let result = source::open(
        original.clone(),
        PrCommand::NewComment,
        &bounded,
        &database(&setup.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    assert_eq!(result.source.opening().tab(), PrTab::Conversation);
    exact(&setup, setup.steps.len());
}
#[test]
fn conversation_comments_v17_missing_issuer_window_and_actual_cancel_refuse_before_any_child_or_page_move(
) {
    let setup = setup(Vec::new());
    let original =
        PrCollaborationOpenIntentV17::capture(&setup.app, &database(&setup.fixture)).unwrap();
    let page = allocations(&setup.app);
    assert!(source::open(
        original.clone(),
        PrCommand::NewComment,
        &setup.api,
        &database(&setup.fixture),
        original.cancellation(),
        original.deadline()
    )
    .is_err());
    let bounded = setup
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    original.cancellation().cancel();
    assert!(source::open(
        original.clone(),
        PrCommand::NewComment,
        &bounded,
        &database(&setup.fixture),
        original.cancellation(),
        original.deadline()
    )
    .is_err());
    assert_eq!(allocations(&setup.app), page);
    exact(&setup, setup.steps.len());
}
#[test]
fn conversation_comments_v17_original_expiry_and_foreign_token_cannot_renew_window_or_start_child()
{
    let setup = setup(Vec::new());
    let original =
        PrCollaborationOpenIntentV17::capture(&setup.app, &database(&setup.fixture)).unwrap();
    let bounded = setup
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    assert!(source::open(
        original.clone(),
        PrCommand::NewComment,
        &bounded,
        &database(&setup.fixture),
        &CancellationToken::default(),
        original.deadline()
    )
    .is_err());
    let expired = Instant::now()
        .checked_sub(Duration::from_millis(1))
        .unwrap();
    assert!(source::open(
        original.clone(),
        PrCommand::NewComment,
        &bounded,
        &database(&setup.fixture),
        original.cancellation(),
        expired
    )
    .is_err());
    exact(&setup, setup.steps.len());
}
#[test]
fn conversation_comments_v17_shared_operation_policy_refuses_oversized_body_missing_comment_and_toggle(
) {
    let valid = BTreeMap::from([("body".into(), "actual shared body".into())]);
    let op =
        PrOperation::from_values_v17(PrCommand::NewComment, None, &[], Some(42), &valid).unwrap();
    assert_eq!(op.command_v17(), PrCommand::NewComment);
    let invalid = BTreeMap::from([("body".into(), "x".repeat(65_537))]);
    assert!(
        PrOperation::from_values_v17(PrCommand::NewComment, None, &[], Some(42), &invalid).is_err()
    );
    assert!(PrOperation::from_values_v17(
        PrCommand::DeleteComment,
        None,
        &[],
        Some(42),
        &BTreeMap::new()
    )
    .is_err());
    assert!(PrOperation::from_values_v17(
        PrCommand::Subscription,
        None,
        &[],
        Some(42),
        &BTreeMap::from([("subscription".into(), "toggle".into())])
    )
    .is_err());
}
