//! Fault only an ACTUAL completed original worker, reusing its existing queue.
use super::*;
pub(crate) fn fault_next(background: &mut Background, mode: u8) {
    let mut actual = background
        .completions
        .recv_timeout(std::time::Duration::from_secs(8))
        .expect("Actual original thread worker completes");
    assert!(actual
        .remote_scope_owner
        .as_ref()
        .is_some_and(|owner| owner.threads_v1.is_some()));
    match mode {
        0 => {
            let prepared = actual.result.as_mut().expect("actual source worker");
            prepared.status = "PRIVATE_MIXED_THREAD_STATUS".into();
            prepared.clear_private = true;
        }
        1 => actual.remote_scope_owner = None,
        2 => actual.result = Err("PRIVATE_THREAD_ERROR_POISON".into()),
        3 => assert!(actual.result.is_ok()),
        _ => panic!("Unknown bounded fault"),
    }
    background
        .inline_completion_sender_v1
        .send(actual)
        .expect("SAME completion receiver");
}
pub(crate) fn generation(background: &Background) -> Option<u64> {
    background
        .current
        .get(&View::Inbox.index())
        .map(|value| value.0)
}
/// Wait for the actual original operation without replacing its result. The
/// assertion distinguishes caught worker panic from a closed provider refusal;
/// both retain their genuine original owner and phase on the SAME channel.
pub(crate) fn actual_completion(background: &mut Background, caught_panic: bool) {
    let actual = background
        .completions
        .recv_timeout(std::time::Duration::from_secs(8))
        .expect("Actual original thread worker completes");
    assert!(actual
        .remote_scope_owner
        .as_ref()
        .is_some_and(|owner| owner.threads_v1.is_some()));
    if caught_panic {
        assert!(
            actual.result.is_err(),
            "Background catches the actual panic"
        );
    } else {
        let prepared = actual
            .result
            .as_ref()
            .expect("Closed provider refusal result");
        let result = prepared.scoped_threads_v1.as_ref().unwrap();
        assert!(crate::scoped_pr_observed_tabs_v1::resources::threads_v1::native_v1::needs_private_retirement(result));
    }
    background
        .inline_completion_sender_v1
        .send(actual)
        .expect("SAME completion receiver; original result unmodified");
}
