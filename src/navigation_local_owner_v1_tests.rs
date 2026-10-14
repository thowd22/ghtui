use super::*;
use crate::{
    local_commit_actor_origin_v1_test_support::Real,
    workspace::{RepositoryMode, RowAction},
};

fn workspace(real: &Real) -> Workspace {
    Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None)
        .unwrap()
        .unwrap()
}
#[test]
fn original_saved_navigation_owner_one_token_queue_clock_and_owned_success_error() {
    let real = Real::new("sha1");
    let workspace = workspace(&real);
    let token = CancellationToken::default();
    let owner = OriginalLocalNavigationOwnerV1::begin(&workspace, token.clone()).unwrap();
    let initial_deadline = owner.operation().deadline();
    assert!(owner.can_admit(&workspace, true));
    assert!(owner.can_admit(&workspace, false));
    let mut result = workspace.clone();
    result
        .open_original_with_operation_v1(
            RowAction::Actor(crate::identity::GitRole::Author),
            owner.operation(),
        )
        .unwrap();
    assert!(
        !owner.admits_workspace(&result),
        "Actual operation must seal first"
    );
    owner.operation().seal().unwrap();
    assert!(owner.admits_workspace(&result));
    assert_eq!(owner.operation().deadline(), initial_deadline);
    token.cancel();
    assert!(!owner.can_admit(&workspace, false));
    assert!(!owner.can_admit(&workspace, true));
    assert!(!owner.admits_workspace(&result));
}
#[test]
fn original_saved_navigation_owner_equal_source_text_new_arc_or_changed_sha_refuses_both_paths() {
    let real = Real::new("sha256");
    let workspace = workspace(&real);
    let owner = OriginalLocalNavigationOwnerV1::begin(&workspace, Default::default()).unwrap();
    let mut other = workspace.clone();
    other.actor_origin =
        Some(crate::git::OriginalLocalActorOriginV1::capture(&real.repository).unwrap());
    assert!(!owner.can_admit(&other, true));
    assert!(!owner.can_admit(&other, false));
    other.actor_origin = workspace.actor_origin.clone();
    other.revision = real.head.clone();
    assert!(!owner.can_admit(&other, true));
    assert!(!owner.can_admit(&other, false));
}
#[test]
fn original_saved_navigation_owner_no_origin_or_display_head_cannot_mint_saved_owner() {
    let real = Real::new("sha1");
    let mut workspace = workspace(&real);
    workspace.actor_origin = None;
    assert!(OriginalLocalNavigationOwnerV1::begin(&workspace, Default::default()).is_err());
    workspace.actor_origin =
        Some(crate::git::OriginalLocalActorOriginV1::capture(&real.repository).unwrap());
    workspace.revision = "HEAD".into();
    assert!(OriginalLocalNavigationOwnerV1::begin(&workspace, Default::default()).is_err());
    real.clear_trace();
    assert!(real.children().is_empty());
}
