//! Test-only actual completion-channel inspection, never a source/owner/result
//! constructor, extra queue, fake provider or alternate publication path.
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
        .expect("Actual queued review worker must complete");
    assert!(completion
        .remote_scope_owner
        .as_ref()
        .is_some_and(|owner| owner.reviews_v2.is_some()));
    match mode {
        0 => {
            let prepared = completion
                .result
                .as_mut()
                .expect("actual successful closed source worker");
            prepared.status = "PRIVATE_MIXED_REVIEW_STATUS_POISON".into();
            prepared.clear_private = true;
        }
        1 => completion.remote_scope_owner = None,
        2 => assert!(
            completion.result.is_err(),
            "actual caught worker panic, never a manufactured Err"
        ),
        3 => assert!(
            completion.result.is_ok(),
            "actual successful closed source result"
        ),
        4 => {
            let mut real = crate::repository_stash_recovery_v1::tests::Real::new();
            completion.original_stash_recovery_owner_v1 = Some(real.owner());
        }
        _ => panic!("Unknown bounded review test fault"),
    }
    background
        .inline_completion_sender_v1
        .send(completion)
        .expect("SAME actual original completion receiver");
}
