use super::*;
use crate::repository_stash_recovery_v1::tests::Real;
use std::time::Duration;
fn completion(
    real: &mut Real,
    kind: u8,
) -> (Background, Completion, Arc<OriginalStashRecoveryOwnerV1>) {
    let owner = real.owner();
    let mut background = Background::new();
    let generation = 7;
    background.current.insert(
        View::Repository.index(),
        (generation, owner.cancellation().clone()),
    );
    background.current_stash_recovery_v1 = Some((generation, owner.clone()));
    let work = owner.clone();
    background
        .requests
        .as_ref()
        .unwrap()
        .try_send(Request {
            original_stash_recovery_owner_v1: Some(owner.clone()),
            view: View::Repository,
            generation,
            cancellation: owner.cancellation().clone(),
            catalogue_owner: None,
            catalogue_recovery: None,
            remote_scope_owner: None,
            local_actor_owner: None,
            remote_repository_owner: None,
            local_navigation_owner: None,
            original_tree_owner_v1: None,
            original_diff_owner_v1: None,
            work: Box::new(move |_| {
                let report = work.prepare()?;
                match kind {
                    0 => Ok(PreparedView {
                        original_stash_recovery_v1: Some(report),
                        ..Default::default()
                    }),
                    1 => Err("Owned error after genuine bounded read".into()),
                    _ => panic!("Owned stash worker panic after genuine bounded read"),
                }
            }),
        })
        .unwrap();
    let completion = background
        .completions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(completion.result.is_ok(), kind == 0);
    (background, completion, owner)
}
fn newer_intents(kind: u8) {
    for input in 0..10 {
        let mut real = Real::new();
        let graph_request = if input == 5 {
            let origin = real
                .app
                .workspace
                .as_ref()
                .unwrap()
                .actor_origin
                .clone()
                .unwrap();
            let operation =
                crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1::begin(
                    &origin,
                    Default::default(),
                )
                .unwrap();
            let graph = origin
                .with_saved_navigation_read_v1(&operation, |repo| {
                    crate::workspace::Workspace::load_from_repository_v1(
                        repo,
                        crate::workspace::RepositoryMode::Graph,
                        "HEAD",
                        None,
                        Some(origin.clone()),
                    )
                })
                .unwrap()
                .unwrap();
            operation.seal().unwrap();
            real.app.workspace = Some(graph);
            Some(
                crate::repository_graph_reread_v3::GraphRereadPreviewV3::capture(
                    real.app.workspace.as_ref().unwrap(),
                    &mut real.app.original_saved_visits,
                )
                .unwrap(),
            )
        } else {
            None
        };
        let (mut background, completion, owner) = completion(&mut real, kind);
        match input {
            0 => real.app.repository_load_requested = true,
            1 => real.app.original_stash_recovery_cancel_requested_v1 = true,
            2 => real.app.original_tree_cancel_v1 = true,
            3 => real.app.original_diff_cancel_v1 = true,
            4 => real.app.related_pr_requested_v1 = true,
            5 => real.app.graph_reread_request_v3 = graph_request,
            6 => real.app.cancel_requested = true,
            7 => real.app.refresh_requested = true,
            8 => real.app.local_actor_requested = true,
            _ => real.app.compare_requested = true,
        }
        real.app.status = "New intent owns visible state".into();
        real.app.loading[View::Repository.index()] = true;
        let origin = real
            .app
            .workspace
            .as_ref()
            .unwrap()
            .actor_origin
            .clone()
            .unwrap();
        let (sender, receiver) = mpsc::sync_channel(1);
        sender.send(completion).unwrap();
        background.completions = receiver;
        assert!(!background.poll(&mut real.app));
        assert_eq!(real.app.status, "New intent owns visible state");
        assert!(real.app.loading[View::Repository.index()]);
        assert!(Arc::ptr_eq(
            &origin,
            real.app
                .workspace
                .as_ref()
                .unwrap()
                .actor_origin
                .as_ref()
                .unwrap()
        ));
        assert!(background
            .current_stash_recovery_v1
            .as_ref()
            .is_some_and(|(_, current)| Arc::ptr_eq(current, &owner)));
        assert!(background.cancel_current_stash_recovery_v1());
        assert!(owner.cancellation().is_cancelled());
    }
}
#[test]
fn stash_recovery_before_poll_real_success_respects_every_newer_intent() {
    newer_intents(0)
}
#[test]
fn stash_recovery_before_poll_real_error_respects_every_newer_intent() {
    newer_intents(1)
}
#[test]
fn stash_recovery_before_poll_actual_caught_worker_panic_respects_every_newer_intent() {
    newer_intents(2)
}
#[test]
fn stash_recovery_before_poll_mixed_result_refused_and_close_never_cancels_new_repo_generation() {
    let mut real = Real::new();
    let (mut background, mut completed, owner) = completion(&mut real, 0);
    completed.result.as_mut().unwrap().clear_private = true;
    real.app.status = "Untouched original status".into();
    assert_eq!(
        background.publish_stash_recovery_v1(&mut real.app, &mut completed),
        Some(false)
    );
    assert_eq!(real.app.status, "Untouched original status");
    let foreign = CancellationToken::default();
    background
        .current
        .insert(View::Repository.index(), (8, foreign.clone()));
    assert!(!background.cancel_current_stash_recovery_v1());
    assert!(!foreign.is_cancelled());
    assert!(!owner.cancellation().is_cancelled());
}
