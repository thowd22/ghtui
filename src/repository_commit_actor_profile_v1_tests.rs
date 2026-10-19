use super::*;
use crate::local_commit_actor_origin_v1_test_support as fixture;
use crate::workspace::{DisplayRow, RepositoryMode, RowAction};
use fixture::Real;

fn workspace(real: &Real) -> Workspace {
    Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None)
        .unwrap()
        .unwrap()
}
#[test]
fn git_actor_profile_real_selected_saved_sha_not_checkout_head_and_explicit_both_roles() {
    for format in ["sha1", "sha256"] {
        let real = Real::new(format);
        assert!(real
            .root
            .starts_with(std::fs::canonicalize(real.directory.path()).unwrap()));
        assert_eq!(
            fixture::git(&real.root, &["cat-file", "commit", &real.original], None),
            real.raw
        );
        let mut workspace = workspace(&real);
        let origin = workspace.actor_origin.as_ref().unwrap().clone();
        assert_eq!(
            workspace.selected_commit.as_ref().unwrap().id,
            real.original
        );
        assert_eq!(workspace.status.head.as_deref(), Some(real.head.as_str()));
        assert!(LocalActorProfileV1::prepare(&workspace).is_err());
        real.clear_trace();
        workspace.open(RowAction::Actor(GitRole::Author)).unwrap();
        let author = LocalActorProfileV1::prepare(&workspace).unwrap();
        assert_eq!(author.sha(), real.original);
        assert_eq!(author.role(), GitRole::Author);
        workspace
            .open(RowAction::Actor(GitRole::Committer))
            .unwrap();
        let committer = LocalActorProfileV1::prepare(&workspace).unwrap();
        assert_eq!(committer.role(), GitRole::Committer);
        assert!(author.same_saved_source(&committer));
        assert!(Arc::ptr_eq(committer.origin(), &origin));
        assert!(
            real.children().is_empty(),
            "Pure role preparation starts no Git read"
        );
    }
}
#[test]
fn git_actor_profile_real_same_sha_role_change_retires_pending_but_not_local_data() {
    let real = Real::new("sha1");
    let mut workspace = workspace(&real);
    workspace.open(RowAction::Actor(GitRole::Author)).unwrap();
    let before = real.snapshot();
    let mut state = LocalActorProfileStateV1::default();
    assert_eq!(
        state.install(&workspace).unwrap(),
        LocalActorProfileChangeV1::SavedSource
    );
    let owner = state.begin(Default::default()).unwrap();
    assert!(state.owns(&owner));
    workspace
        .open(RowAction::Actor(GitRole::Committer))
        .unwrap();
    assert_eq!(
        state.install(&workspace).unwrap(),
        LocalActorProfileChangeV1::RoleOnly
    );
    assert!(!state.owns(&owner));
    assert!(owner.cancellation().is_cancelled());
    assert!(owner.guard().is_err());
    assert_eq!(
        workspace.selected_commit.as_ref().unwrap().author.name,
        "Original Writer"
    );
    assert_eq!(real.snapshot(), before);
}
#[test]
fn git_actor_profile_real_new_origin_same_metadata_cannot_reuse_old_owner() {
    let real = Real::new("sha1");
    let mut workspace = workspace(&real);
    workspace.open(RowAction::Actor(GitRole::Author)).unwrap();
    let mut state = LocalActorProfileStateV1::default();
    state.install(&workspace).unwrap();
    let old = state.begin(Default::default()).unwrap();
    let replacement = OriginalLocalActorOriginV1::capture(&real.repository).unwrap();
    assert_eq!(
        replacement.source(),
        workspace.actor_origin.as_ref().unwrap().source()
    );
    workspace.actor_origin = Some(replacement);
    assert_eq!(
        state.install(&workspace).unwrap(),
        LocalActorProfileChangeV1::SavedSource
    );
    assert!(!state.owns(&old));
    assert!(old.cancellation().is_cancelled());
}
#[test]
fn git_actor_profile_real_same_original_role_reselection_preserves_pending_epoch() {
    let real = Real::new("sha1");
    let mut workspace = workspace(&real);
    workspace.open(RowAction::Actor(GitRole::Author)).unwrap();
    let mut state = LocalActorProfileStateV1::default();
    state.install(&workspace).unwrap();
    let profile = state.current().unwrap().clone();
    let owner = state.begin(Default::default()).unwrap();
    workspace.open(RowAction::Actor(GitRole::Author)).unwrap();
    assert_eq!(
        state.install(&workspace).unwrap(),
        LocalActorProfileChangeV1::Unchanged
    );
    assert!(Arc::ptr_eq(state.current().unwrap(), &profile));
    assert!(state.owns(&owner));
}
#[test]
fn git_actor_profile_real_completed_request_role_switch_does_not_cancel_admitted_token() {
    let real = Real::new("sha1");
    let mut workspace = workspace(&real);
    workspace.open(RowAction::Actor(GitRole::Author)).unwrap();
    let mut state = LocalActorProfileStateV1::default();
    state.install(&workspace).unwrap();
    let owner = state.begin(Default::default()).unwrap();
    let token = owner
        .profile()
        .origin()
        .read_original(
            owner.profile().sha(),
            &Default::default(),
            owner.cancellation(),
        )
        .unwrap();
    assert!(owner.matches_token(&token));
    assert!(state.finish(&owner));
    assert!(!state.owns(&owner));
    assert!(
        !state.finish(&owner),
        "A completion cannot be accepted twice"
    );
    workspace
        .open(RowAction::Actor(GitRole::Committer))
        .unwrap();
    assert_eq!(
        state.install(&workspace).unwrap(),
        LocalActorProfileChangeV1::RoleOnly
    );
    assert!(!owner.cancellation().is_cancelled());
    assert!(token.remaining(&Default::default()).is_ok());
}
#[test]
fn git_actor_profile_real_rows_mode_head_and_revision_never_infer_role_or_source() {
    let real = Real::new("sha1");
    let mut workspace = workspace(&real);
    workspace.rows.push(DisplayRow {
        label: "Actor profile — Git author".into(),
        role: crate::repository_view::SemanticRole::Reference,
        action: None,
        cells: None,
    });
    assert!(LocalActorProfileV1::prepare(&workspace).is_err());
    workspace.open(RowAction::Actor(GitRole::Author)).unwrap();
    workspace.actor_origin = None;
    assert!(LocalActorProfileV1::prepare(&workspace)
        .unwrap_err()
        .contains("source"));
    workspace.actor_origin = Some(OriginalLocalActorOriginV1::capture(&real.repository).unwrap());
    workspace.selected_commit.as_mut().unwrap().id = "HEAD".into();
    assert!(LocalActorProfileV1::prepare(&workspace)
        .unwrap_err()
        .contains("SHA"));
}
#[test]
fn git_actor_profile_real_retirement_cancel_and_new_source_hide_old_owners_without_mutating_git() {
    let real = Real::new("sha1");
    let mut workspace = workspace(&real);
    workspace.open(RowAction::Actor(GitRole::Author)).unwrap();
    let before = real.snapshot();
    let mut state = LocalActorProfileStateV1::default();
    state.install(&workspace).unwrap();
    let old = state.begin(Default::default()).unwrap();
    let canceled = CancellationToken::default();
    canceled.cancel();
    assert!(state.begin(canceled).is_err());
    assert!(!state.owns(&old));
    assert!(old.guard().is_err());
    let next = state.begin(Default::default()).unwrap();
    state.retire();
    assert!(state.current().is_none());
    assert!(!state.owns(&next));
    assert!(next.cancellation().is_cancelled());
    assert!(state.begin(Default::default()).is_err());
    assert_eq!(real.snapshot(), before);
}
#[test]
fn git_actor_profile_real_changed_saved_commit_and_epoch_exhaustion_fail_closed() {
    let real = Real::new("sha1");
    let mut workspace = workspace(&real);
    workspace.open(RowAction::Actor(GitRole::Author)).unwrap();
    let mut state = LocalActorProfileStateV1::default();
    state.install(&workspace).unwrap();
    let old = state.begin(Default::default()).unwrap();
    workspace.selected_commit.as_mut().unwrap().id = real.head.clone();
    assert_eq!(
        state.install(&workspace).unwrap(),
        LocalActorProfileChangeV1::SavedSource
    );
    assert!(!state.owns(&old));
    state.epoch = u64::MAX;
    assert!(state.begin(Default::default()).is_err());
    assert!(state.current().is_none());
    assert!(state.install(&workspace).is_err());
}
