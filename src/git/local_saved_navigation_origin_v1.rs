//! Borrow the SAME admitted origin for a NEW explicit bounded local operation.
//! No discovery/path constructor or new source observation is minted here.
use super::{GitRepository, OriginalLocalActorOriginV1};
use crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1;

impl OriginalLocalActorOriginV1 {
    pub(crate) fn with_saved_navigation_read_v1<T>(
        &self,
        operation: &LocalGitNavigationReadV1,
        read: impl FnOnce(&GitRepository) -> Result<T, String>,
    ) -> Result<T, String> {
        operation.check()?;
        self.recheck(operation.cancellation())
            .map_err(|error| error.to_string())?;
        operation.check()?;
        // Only a LATER explicit operation uses this seam. The original loading
        // capture/recheck retain their live remote/local window unchanged.
        let repository = self
            .original_repository_v1()
            .clone()
            .strip_navigation_read_window_v1()
            .map_err(|error| error.to_string())?
            .with_cancellation(operation.cancellation().clone())
            .with_navigation_read_window_v1(operation.window());
        let result = read(&repository);
        // Source/clock loss wins over both successful data AND ordinary errors.
        // No rebase, replacement path discovery or prepared host report reuse.
        self.recheck(operation.cancellation())
            .map_err(|error| error.to_string())?;
        operation.check()?;
        result
    }
}
