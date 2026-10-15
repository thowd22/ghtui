//! Genuine same-provider job-log capture, not a DTO/PR-capture constructor.
use super::*;
use std::sync::Arc;
pub struct ScopedActionsLogSnapshotV2 {
    run: Arc<ScopedActionsRunTargetV1>,
    job: Arc<ScopedActionsJobTargetV1>,
    text: String,
    observed_at: u64,
}
impl ScopedActionsLogSnapshotV2 {
    pub fn run(&self) -> &Arc<ScopedActionsRunTargetV1> {
        &self.run
    }
    pub fn job(&self) -> &Arc<ScopedActionsJobTargetV1> {
        &self.job
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn observed_at(&self) -> u64 {
        self.observed_at
    }
    pub(crate) fn owned_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>() + self.text.capacity()
    }
}
impl GithubProvider {
    /// Only the actual bounded CLI response constructs this snapshot. Both
    /// opaque targets must be actual current exact-attempt siblings; raw model
    /// data merely supplies the SAME existing run_logs_at command parameters.
    pub(super) fn scoped_actions_capture_logs_v2(
        &self,
        current: &CurrentCatalogueRepository,
        run: Arc<ScopedActionsRunTargetV1>,
        job: Arc<ScopedActionsJobTargetV1>,
        model: &ActionsJob,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<ScopedActionsLogSnapshotV2>> {
        if !run.current_resource_v2(self, current)
            || !Arc::ptr_eq(job.run(), &run)
            || model.id != job.id()
            || model.run_id != run.run().id
            || model.attempt != run.run().attempt
            || model.head_sha != run.run().head_sha
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Actual sibling job/run/head/attempt required before scoped log capture",
            ));
        }
        let text = self.run_logs_at(
            &current.repository().full_name,
            run.run(),
            Some(model),
            cancel,
        )?;
        if text.len() > 512 * 1024
            || text.lines().count() > 5000
            || text.capacity() > 1024 * 1024
            || cancel.is_cancelled()
        {
            return Err(error(GithubErrorKind::Unsupported,"Original log capture exceeds512KiB/5000lines/1MiB capacity or canceled; no partial snapshot"));
        }
        Ok(Arc::new(ScopedActionsLogSnapshotV2 {
            run,
            job,
            text,
            observed_at: crate::storage::now(),
        }))
    }
}
