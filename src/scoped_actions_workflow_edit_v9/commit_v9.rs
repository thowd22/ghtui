//! Exact actual Stage receipt→canonical LocalPlan→explicit Commit. Original
//! hook/signing behavior is retained; any ambiguous result is inspect-only.
use super::*;
use crate::{
    git::{
        workflow_edit_v9::{
            WorkflowLocalBindingV9, WorkflowLocalStepV9, WorkflowMutationLockV9,
            WorkflowObservedCommitV9, WorkflowPlanPurposeV9, WorkflowPreparedCommitV9,
        },
        RepoStatus, WorkspaceFingerprint,
    },
    providers::{gh::*, runner::CancellationToken},
    scoped_actions_publication_v3 as shared,
};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct WorkflowCommitConfirmV9 {
    staged: Arc<WorkflowStagedLocalV9>,
    preview: Arc<WorkflowLocalPlanPreviewV9>,
    cancel: CancellationToken,
    deadline: Instant,
    started: AtomicBool,
    ui_budget: WorkflowUiBudgetV9,
}
impl std::fmt::Debug for WorkflowCommitConfirmV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowCommitConfirmV9(<one actual staged exact commit attempt>)")
    }
}
fn clock(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    shared::clock(cancel, deadline)?;
    if deadline.saturating_duration_since(Instant::now()) > std::time::Duration::from_secs(30) {
        return Err(local_edit_refused_v9());
    }
    Ok(())
}
impl WorkflowCommitConfirmV9 {
    pub(crate) fn capture(
        staged: Arc<WorkflowStagedLocalV9>,
        preview: Arc<WorkflowLocalPlanPreviewV9>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        clock(cancel, deadline)?;
        if preview.step() != WorkflowLocalStepV9::Commit
            || preview.attempted_v9()
            || !Arc::ptr_eq(staged.saved(), preview.saved())
            || staged.fingerprint() != preview.plan().mutation.observed
            || preview.reviewed_v9().is_none_or(|reviewed| {
                !reviewed.binds_plan_v9(
                    preview.plan(),
                    &staged.saved().opened().intent().definition().commit_sha,
                )
            })
        {
            return Err(local_edit_refused_v9());
        }
        Ok(Arc::new(Self {
            staged,
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
        clock(&self.cancel, self.deadline)?;
        if !Arc::ptr_eq(binding, self.staged.saved().opened().binding())
            || !Arc::ptr_eq(self.staged.saved(), self.preview.saved())
            || self.preview.step() != WorkflowLocalStepV9::Commit
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
            WorkflowEditStepV9::Commit,
        )?;
        let Some(WorkflowLocalPreviewV9::Commit(preview)) = request.preview() else {
            return Err(local_edit_refused_v9());
        };
        budget.check_peak(
            preview
                .retained_bytes()
                .checked_add(12 * 1024 * 1024)
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        let mut captured = Self::capture(
            request.staged().ok_or_else(local_edit_refused_v9)?.clone(),
            preview.clone(),
            cancel,
            deadline,
        )?;
        Arc::get_mut(&mut captured)
            .ok_or_else(local_edit_refused_v9)?
            .ui_budget = budget;
        Ok(captured)
    }
    pub(crate) fn execute(
        &self,
        provider: &GithubProvider,
        database: &Path,
    ) -> GithubResult<WorkflowCommitOutcomeV9> {
        self.ui_budget.check_peak(
            self.preview
                .retained_bytes()
                .checked_add(12 * 1024 * 1024)
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        self.staged
            .saved()
            .opened()
            .binding()
            .execute_commit_v9(self, provider, database)
    }
    pub(crate) fn execute_locked_v9(
        &self,
        provider: &GithubProvider,
        database: &Path,
        lock: &WorkflowMutationLockV9<'_>,
    ) -> GithubResult<WorkflowCommitOutcomeV9> {
        let saved = self.staged.saved();
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
            WorkflowEditPhaseV9::Commit,
            1,
            cancel,
        )?;
        let mut post = post.ok_or_else(local_edit_refused_v9)?;
        let effective = effective_phase_deadline_v9(cancel, self.deadline, post.deadline())?;
        if window.deadline() != effective {
            return Err(local_edit_refused_v9());
        }
        // A downward-only refusal for the original 16MiB phase envelope;
        // final pinned canonical/status allocation proof remains a review gate.
        if self
            .preview
            .retained_bytes()
            .checked_add(post.reserved_bytes())
            .and_then(|n| n.checked_add(self.ui_budget.held()))
            .and_then(|n| n.checked_add(12 * 1024 * 1024))
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
        let (status, now, config) = binding
            .check_fresh_plan_repository_v9(&repository)
            .map_err(|_| local_edit_refused_v9())?;
        drop(config);
        if now != self.preview.plan().mutation.observed || now != self.staged.fingerprint() {
            return Err(local_edit_refused_v9());
        }
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
        let command =
            WorkflowPreparedCommitV9::prepare(binding, self, &repository, &status, effective)?;
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
        let release = WorkflowCommitReleaseV9 {
            request: self,
            lock,
            closed,
            effective,
        };
        let write = post.local_write_v9(provider, command.release(&release)?, cancel);
        let observed = (|| -> GithubResult<_> {
            let commit = binding.post_commit_v9(provider, &mut post, cancel)?;
            let exact_review = self
                .preview
                .reviewed_v9()
                .is_some_and(|reviewed| reviewed.matches_post_v9(commit.comparison()));
            let status = binding.post_status_v9(provider, &mut post, cancel)?;
            let fingerprint = binding.post_fingerprint_v9(provider, &mut post, cancel)?;
            if !exact_review
                || !binding.branch_matches_v9(&status)
                || status.head.as_deref() != Some(commit.head())
                || status.operation.is_some()
                || status.conflicts() != 0
                || status.has_history_overrides()
                || status.sparse
                || status.entries.iter().any(|entry| entry.staged())
                || status.entries.iter().any(|entry| {
                    entry.path.0 == intent.definition().workflow.path.as_bytes()
                        && entry.worktree != '.'
                })
            {
                return Err(local_edit_refused_v9());
            }
            saved
                .file()
                .validate(cancel, effective)
                .map_err(|_| local_edit_refused_v9())?;
            Ok((commit, status, fingerprint))
        })();
        let principal =
            post.final_principal_with_heap_v9(provider, release.closed.identity(), &heap, cancel);
        Ok(match (write, observed, principal) {
            (Ok(()), Ok((commit, status, fingerprint)), Ok(identity)) => {
                let receipt = WorkflowCommittedLocalV9 {
                    staged: self.staged.clone(),
                    commit,
                    status,
                    fingerprint,
                    identity,
                };
                if receipt
                    .retained_bytes_v9()
                    .is_none_or(|bytes| self.ui_budget.check_retained(bytes).is_err())
                {
                    WorkflowCommitOutcomeV9::InspectionRequired
                } else {
                    WorkflowCommitOutcomeV9::Committed(Arc::new(receipt))
                }
            }
            _ => WorkflowCommitOutcomeV9::InspectionRequired,
        })
    }
}
pub(crate) struct WorkflowCommitReleaseV9<'a> {
    request: &'a WorkflowCommitConfirmV9,
    lock: &'a WorkflowMutationLockV9<'a>,
    closed: ClosedCatalogueAccess,
    effective: Instant,
}
impl WorkflowCommitReleaseV9<'_> {
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
pub(crate) struct WorkflowCommittedLocalV9 {
    staged: Arc<WorkflowStagedLocalV9>,
    commit: WorkflowObservedCommitV9,
    status: RepoStatus,
    fingerprint: WorkspaceFingerprint,
    identity: HostIdentity,
}
impl WorkflowCommittedLocalV9 {
    pub(crate) fn retained_bytes_v9(&self) -> Option<usize> {
        self.staged
            .retained_bytes_v9()?
            .checked_add(std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>())?
            .checked_add(self.commit.retained_bytes_v9()?)?
            .checked_add(super::isolation_confirm_v9::status_heap_v9(&self.status))?
            .checked_add(identity_bytes_v9(&self.identity))
    }
    pub(crate) fn staged(&self) -> &Arc<WorkflowStagedLocalV9> {
        &self.staged
    }
    pub(crate) fn commit(&self) -> &WorkflowObservedCommitV9 {
        &self.commit
    }
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
impl std::fmt::Debug for WorkflowCommittedLocalV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "WorkflowCommittedLocalV9(<actual D/canonical C..D; no remote association/approval>)",
        )
    }
}
pub(crate) enum WorkflowCommitOutcomeV9 {
    Committed(Arc<WorkflowCommittedLocalV9>),
    InspectionRequired,
}
