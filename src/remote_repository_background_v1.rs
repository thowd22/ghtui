//! Independent browser slot. Owned errors/panics are gated before ANY effects.
use super::*;
use crate::remote_repository_native_v1::RemoteRepositoryRequestOwnerV1;
const REMOTE_BROWSER_SLOT_V1: usize = 6;
impl Background {
    pub fn cancel_remote_repository_v1(&mut self) {
        if let Some((_, token)) = self.current.remove(&REMOTE_BROWSER_SLOT_V1) {
            token.cancel();
        }
    }
    pub fn submit_remote_repository_v1(
        &mut self,
        app: &mut App,
        owner: Arc<RemoteRepositoryRequestOwnerV1>,
        work: impl FnOnce(CancellationToken) -> Result<PreparedView, String> + Send + 'static,
    ) -> bool {
        if !owner.owns(app) {
            return false;
        }
        if owner.expired() {
            crate::remote_repository_native_v1::worker_failed(app);
            return false;
        }
        self.cancel_remote_repository_v1();
        self.generation += 1;
        let generation = self.generation;
        let cancellation = CancellationToken::default();
        let request = Request {
            view: View::Repository,
            generation,
            cancellation: cancellation.clone(),
            catalogue_owner: None,
            catalogue_recovery: None,
            remote_scope_owner: None,
            local_actor_owner: None,
            local_navigation_owner: None,
            original_tree_owner_v1: None,
            remote_repository_owner: Some(owner),
            work: Box::new(work),
        };
        match self
            .requests
            .as_ref()
            .expect("active worker queue")
            .try_send(request)
        {
            Ok(()) => {
                self.current
                    .insert(REMOTE_BROWSER_SLOT_V1, (generation, cancellation));
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                crate::remote_repository_native_v1::worker_failed(app);
                false
            }
        }
    }
}

#[cfg(test)]
#[path = "remote_repository_background_v1_tests.rs"]
mod tests;
