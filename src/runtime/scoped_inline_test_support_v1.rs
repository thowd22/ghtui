//! Test-only inspection/faulting of the ACTUAL Background completion channel.
//! No source/owner/receipt constructor and no alternate queue or fake publisher.
use super::*;
pub(crate) fn generation(background: &Background) -> Option<u64> {
    background
        .current
        .get(&View::Inbox.index())
        .map(|(generation, _)| *generation)
}
pub(crate) fn fault_next(background: &mut Background, mode: u8) {
    let mut completion = background
        .completions
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("Actual queued inline worker must complete");
    assert!(completion
        .remote_scope_owner
        .as_ref()
        .is_some_and(|owner| owner.inline_v1.is_some()));
    match mode {
        0 => {
            let prepared = completion
                .result
                .as_mut()
                .expect("actual successful source worker");
            prepared.status = "PRIVATE_MIXED_STATUS_POISON".into();
            prepared.clear_private = true;
        }
        1 => completion.remote_scope_owner = None,
        2 => completion.result = Err("PRIVATE_ERROR_POISON".into()),
        3 => assert!(
            completion.result.is_err(),
            "Must be actual worker-caught panic, not manufactured Err"
        ),
        4 => assert!(
            completion.result.is_ok(),
            "Actual successful completed source worker"
        ),
        5 | 6 => {
            let mut real = crate::repository_stash_recovery_v1::tests::Real::new();
            let owner = real.owner();
            if mode == 5 {
                completion
                    .result
                    .as_mut()
                    .unwrap()
                    .original_stash_recovery_v1 = Some(owner.prepare().unwrap());
            } else {
                completion.original_stash_recovery_owner_v1 = Some(owner);
            }
        }
        _ => panic!("Unknown bounded test fault"),
    }
    background
        .inline_completion_sender_v1
        .send(completion)
        .expect("Same original completion receiver");
}

pub(crate) fn reject_inline_in_original_stash_slot(background: &mut Background) {
    let inline = background
        .completions
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("Actual original inline worker completed")
        .result
        .unwrap()
        .scoped_inline_v1
        .unwrap();
    let mut real = crate::repository_stash_recovery_v1::tests::Real::new();
    crate::repository_stash_recovery_v1::dispatch(
        &mut real.app,
        crate::repository_stash_recovery_v1::StashRecoveryActionV1::Inspect,
    );
    let intent = real.app.original_stash_recovery_request_v1.take().unwrap();
    let mut stash_background = Background::new();
    assert!(stash_background.submit_stash_recovery_v1(&mut real.app, intent));
    let mut completion = stash_background
        .completions
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("Actual original stash worker completed");
    completion.result.as_mut().unwrap().scoped_inline_v1 = Some(inline);
    let generation = stash_background.current[&View::Repository.index()].0;
    real.app.status = "UNCHANGED_ORIGINAL_STASH_STATUS".into();
    assert_eq!(
        stash_background.publish_stash_recovery_v1(&mut real.app, &mut completion),
        Some(false)
    );
    assert_eq!(
        stash_background.current[&View::Repository.index()].0,
        generation
    );
    assert_eq!(real.app.status, "UNCHANGED_ORIGINAL_STASH_STATUS");
    assert!(real.app.loading[View::Repository.index()]);
    assert!(real.app.form.is_some());
    assert!(stash_background.cancel_current_stash_recovery_v1());
}
