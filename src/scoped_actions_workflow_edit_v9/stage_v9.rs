//! Real Stage Confirm after a genuine saved-file LocalPlan Preview. This
//! closed kernel cannot execute a generic operation or revive the read pool.
use super::*;
use crate::{
    git::{
        workflow_edit_v9::{
            WorkflowLocalBindingV9, WorkflowLocalStepV9, WorkflowMutationLockV9,
            WorkflowPlanPurposeV9, WorkflowPreparedStageV9,
        },
        RepoStatus, WorkspaceFingerprint,
    },
    providers::{gh::*, runner::CancellationToken},
    scoped_actions_publication_v3 as shared,
};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct WorkflowStageConfirmV9 {
    preview: Arc<WorkflowLocalPlanPreviewV9>,
    cancel: CancellationToken,
    deadline: Instant,
    started: AtomicBool,
    ui_budget: WorkflowUiBudgetV9,
}
impl std::fmt::Debug for WorkflowStageConfirmV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowStageConfirmV9(<one explicit exact Stage attempt>)")
    }
}
fn check_clock(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    shared::clock(cancel, deadline)?;
    if deadline.saturating_duration_since(Instant::now()) > std::time::Duration::from_secs(30) {
        return Err(local_edit_refused_v9());
    }
    Ok(())
}
impl WorkflowStageConfirmV9 {
    pub(crate) fn capture(
        preview: Arc<WorkflowLocalPlanPreviewV9>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        check_clock(cancel, deadline)?;
        if preview.step() != WorkflowLocalStepV9::Stage || preview.attempted_v9() {
            return Err(local_edit_refused_v9());
        }
        Ok(Arc::new(Self {
            preview,
            cancel: cancel.clone(),
            deadline,
            started: AtomicBool::new(false),
            ui_budget: WorkflowUiBudgetV9::standalone(),
        }))
    }
    pub(crate) fn preview(&self) -> &Arc<WorkflowLocalPlanPreviewV9> {
        &self.preview
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn check_original_v9(
        &self,
        binding: &Arc<WorkflowLocalBindingV9>,
    ) -> GithubResult<()> {
        check_clock(&self.cancel, self.deadline)?;
        if !Arc::ptr_eq(binding, self.preview.saved().opened().binding())
            || self.preview.step() != WorkflowLocalStepV9::Stage
        {
            return Err(local_edit_refused_v9());
        }
        Ok(())
    }
    pub(crate) fn capture_for_form_v9(
        request: &crate::scoped_actions_workflow_edit_ui_v9::WorkflowEditRequestV9,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        use crate::scoped_actions_workflow_edit_ui_v9::{
            WorkflowEditPhaseV9, WorkflowEditStepV9, WorkflowLocalPreviewV9,
        };
        let budget = WorkflowUiBudgetV9::for_form(
            request,
            WorkflowEditPhaseV9::Confirm,
            WorkflowEditStepV9::Stage,
        )?;
        let Some(WorkflowLocalPreviewV9::Stage(preview)) = request.preview() else {
            return Err(local_edit_refused_v9());
        };
        budget.check_peak(
            preview
                .retained_bytes()
                .checked_add(8 * 1024 * 1024)
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        let mut captured = Self::capture(preview.clone(), cancel, deadline)?;
        Arc::get_mut(&mut captured)
            .ok_or_else(local_edit_refused_v9)?
            .ui_budget = budget;
        Ok(captured)
    }
    pub(crate) fn execute(
        &self,
        provider: &GithubProvider,
        database: &Path,
    ) -> GithubResult<WorkflowStageOutcomeV9> {
        self.ui_budget.check_peak(
            self.preview
                .retained_bytes()
                .checked_add(8 * 1024 * 1024)
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        self.preview
            .saved()
            .opened()
            .binding()
            .execute_stage_v9(self, provider, database)
    }
    pub(crate) fn execute_locked_v9(
        &self,
        provider: &GithubProvider,
        database: &Path,
        lock: &WorkflowMutationLockV9<'_>,
    ) -> GithubResult<WorkflowStageOutcomeV9> {
        let saved = self.preview.saved();
        let opened = saved.opened();
        let binding = opened.binding();
        self.check_original_v9(binding)?;
        if !lock.owns(binding)
            || self.started.swap(true, Ordering::AcqRel)
            || !self.preview.reserve_attempt_v9()
        {
            return Err(local_edit_refused_v9());
        }
        let cancel = &self.cancel;
        let intent = opened.intent();
        let core_inputs = self.ui_budget.core_preview_v9(&self.preview)?;
        let core_policy =
            WorkflowCoreJsonV9::for_local_v9(provider, &core_inputs, cancel, self.deadline)?;
        let phase = provider.clone().with_catalogue_read_deadline(self.deadline);
        let mut session = phase.begin_workflow_edit_access_v9(
            intent.source().original().source(),
            &shared::access_limits(),
            &core_policy,
            cancel,
        )?;
        shared::scope(intent.source(), session.identity())?;
        session.verify(intent.source().reference(), cancel)?;
        opened.validate_local(database, cancel)?;
        let (window, post) = session.reserve_workflow_edit_transport_v9(
            intent.original(),
            WorkflowEditPhaseV9::Stage,
            1,
            cancel,
        )?;
        let mut post = post.ok_or_else(local_edit_refused_v9)?;
        let effective = effective_phase_deadline_v9(cancel, self.deadline, post.deadline())?;
        if window.deadline() != effective {
            return Err(local_edit_refused_v9());
        }
        // Keep post buffers/parser results and the real runner's copies inside
        // the ORIGINAL 16MiB phase envelope, without widening any child cap.
        // Eight MiB is reserved scratch headroom, not measured RSS or a larger
        // source/HTTP allowance. Final pinned-parser derivation is a gate.
        if self
            .preview
            .retained_bytes()
            .checked_add(post.reserved_bytes())
            .and_then(|n| n.checked_add(self.ui_budget.held()))
            .and_then(|n| n.checked_add(8 * 1024 * 1024))
            .is_none_or(|n| n > 16 * 1024 * 1024)
        {
            return Err(local_edit_refused_v9());
        }
        let measured = phase.clone().with_measured_read_window(window)?;
        let heap = core_policy.bind_measured_v9(&measured, cancel)?;
        let current = session
            .observed_original_v1(intent.source().reference().id)
            .ok_or_else(local_edit_refused_v9)?;
        if let Some(pr) = intent.source().pr_origin_v2() {
            pr.fresh_workflow_edit_parent_v9(
                &measured,
                session.identity(),
                current,
                &heap,
                cancel,
            )?;
        }
        let first = measured.scoped_actions_workflow_snapshot_heap_v9(
            current,
            intent.original(),
            true,
            &heap,
            cancel,
        )?;
        if !first.same_pin_v6(intent.original(), true) {
            return Err(local_edit_refused_v9());
        }
        let repository = binding
            .current_repository_v9(measured.remote_git_read_window_v1(cancel)?, cancel)
            .map_err(|_| local_edit_refused_v9())?;
        // SAME mutation freshness/status/planner as the actual LocalPlan.
        let (status, now, config) = binding
            .check_fresh_plan_repository_v9(&repository)
            .map_err(|_| local_edit_refused_v9())?;
        drop(config);
        if now != self.preview.plan().mutation.observed {
            return Err(local_edit_refused_v9());
        }
        // Move the actual fresh status from the SAME 4+5 preflight; never
        // replace this current check with cached Preview status or an extra
        // hidden status4 child group outside the original ledger.
        let command =
            WorkflowPreparedStageV9::prepare(binding, self, &repository, &status, effective)?;
        let last = measured.scoped_actions_workflow_snapshot_heap_v9(
            current,
            intent.original(),
            true,
            &heap,
            cancel,
        )?;
        if !last.same_pin_v6(intent.original(), true) {
            return Err(local_edit_refused_v9());
        }
        if let Some(pr) = intent.source().pr_origin_v2() {
            pr.fresh_workflow_edit_parent_v9(
                &measured,
                session.identity(),
                current,
                &heap,
                cancel,
            )?;
        }
        session.verify(intent.source().reference(), cancel)?;
        post.validate_prewrite(&measured, cancel)?;
        let closed =
            session.close_with_guard(cancel, || opened.validate_local(database, cancel))?;
        shared::scope(intent.source(), closed.identity())?;
        let release = WorkflowStageReleaseV9 {
            request: self,
            lock,
            closed,
            effective,
        };
        let command = command.release(&release)?;
        let write = post.local_write_v9(provider, command, cancel);
        // An attempted child may have changed the index even on error. No
        // retry/clean-success/rollback claim is made by any following failure.
        let observed = (|| -> GithubResult<(RepoStatus, WorkspaceFingerprint)> {
            let status = binding.post_status_v9(provider, &mut post, cancel)?;
            let fingerprint = binding.post_fingerprint_v9(provider, &mut post, cancel)?;
            let purpose = WorkflowPlanPurposeV9::from_saved_v9(
                saved.clone(),
                WorkflowLocalStepV9::Commit,
                cancel,
                effective,
            )
            .map_err(|_| local_edit_refused_v9())?;
            purpose
                .validate_status(&status)
                .map_err(|_| local_edit_refused_v9())?;
            Ok((status, fingerprint))
        })();
        let principal =
            post.final_principal_with_heap_v9(provider, release.closed.identity(), &heap, cancel);
        Ok(match (write, observed, principal) {
            (Ok(()), Ok((status, fingerprint)), Ok(identity)) => {
                let receipt = WorkflowStagedLocalV9 {
                    saved: saved.clone(),
                    status,
                    fingerprint,
                    identity,
                };
                if receipt
                    .retained_bytes_v9()
                    .is_none_or(|bytes| self.ui_budget.check_retained(bytes).is_err())
                {
                    WorkflowStageOutcomeV9::InspectionRequired
                } else {
                    WorkflowStageOutcomeV9::Staged(Arc::new(receipt))
                }
            }
            _ => WorkflowStageOutcomeV9::InspectionRequired,
        })
    }
}

/// This borrowed witness exists ONLY while the SAME common-dir mutex remains
/// held, after the actual original Core has closed and final UID succeeded.
pub(crate) struct WorkflowStageReleaseV9<'a> {
    request: &'a WorkflowStageConfirmV9,
    lock: &'a WorkflowMutationLockV9<'a>,
    closed: ClosedCatalogueAccess,
    effective: Instant,
}
impl WorkflowStageReleaseV9<'_> {
    pub(crate) fn check(
        &self,
        binding: &Arc<WorkflowLocalBindingV9>,
        preview: &Arc<WorkflowLocalPlanPreviewV9>,
    ) -> GithubResult<()> {
        self.request.check_original_v9(binding)?;
        effective_phase_deadline_v9(&self.request.cancel, self.request.deadline, self.effective)?;
        if !self.lock.owns(binding)
            || !Arc::ptr_eq(preview, &self.request.preview)
            || !self.request.started.load(Ordering::Acquire)
            || !preview.attempted_v9()
            || self
                .closed
                .observation(binding.intent().source().reference().id)
                .is_none()
        {
            return Err(local_edit_refused_v9());
        }
        shared::scope(binding.intent().source(), self.closed.identity())
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.effective
    }
}
pub(crate) struct WorkflowStagedLocalV9 {
    saved: Arc<WorkflowSavedLocalV9>,
    status: RepoStatus,
    fingerprint: WorkspaceFingerprint,
    identity: HostIdentity,
}
impl WorkflowStagedLocalV9 {
    pub(crate) fn retained_bytes_v9(&self) -> Option<usize> {
        self.saved
            .retained_bytes_v9()?
            .checked_add(std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>())?
            .checked_add(super::isolation_confirm_v9::status_heap_v9(&self.status))?
            .checked_add(identity_bytes_v9(&self.identity))
    }
    pub(crate) fn saved(&self) -> &Arc<WorkflowSavedLocalV9> {
        &self.saved
    }
    #[cfg(test)]
    pub(crate) fn status(&self) -> &RepoStatus {
        &self.status
    }
    pub(crate) fn fingerprint(&self) -> WorkspaceFingerprint {
        self.fingerprint
    }
    #[cfg(test)]
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
}
impl std::fmt::Debug for WorkflowStagedLocalV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "WorkflowStagedLocalV9(<actual exact saved path staged; no Commit/remote approval>)",
        )
    }
}
pub(crate) enum WorkflowStageOutcomeV9 {
    Staged(Arc<WorkflowStagedLocalV9>),
    InspectionRequired,
}
