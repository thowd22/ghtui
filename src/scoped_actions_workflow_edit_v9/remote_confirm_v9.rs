//! One explicit remote Confirm, never a renewed Preview transport/source.
use super::*;
use crate::{
    drafts::{DraftClient, DraftPublicationFenceV2},
    git::workflow_edit_v9::{
        WorkflowLocalBindingV9, WorkflowMutationLockV9, WorkflowRemoteGitObservationV9,
    },
    providers::{gh::*, runner::CancellationToken},
    scoped_actions_publication_v3 as shared,
};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct WorkflowRemoteConfirmV9 {
    preview: Arc<WorkflowRemotePreviewV9>,
    cancel: CancellationToken,
    deadline: Instant,
    started: AtomicBool,
    cleanup_waiting: AtomicBool,
    ui_budget: WorkflowUiBudgetV9,
    branch: Option<Arc<WorkflowPublishedBranchV9>>,
}
impl std::fmt::Debug for WorkflowRemoteConfirmV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "WorkflowRemoteConfirmV9(<one original explicit Confirm; private inputs redacted>)",
        )
    }
}
impl WorkflowRemoteConfirmV9 {
    pub(crate) fn capture_for_form_v9(
        request: &crate::scoped_actions_workflow_edit_ui_v9::WorkflowEditRequestV9,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        use crate::scoped_actions_workflow_edit_ui_v9::{
            WorkflowEditPhaseV9 as Phase, WorkflowEditStepV9 as Step, WorkflowLocalPreviewV9,
        };
        shared::clock(cancel, deadline)?;
        let expected = match request.step() {
            Step::PublishBranch => WorkflowEditPhaseV9::PublishBranch,
            Step::CreatePr => WorkflowEditPhaseV9::CreatePr,
            _ => return Err(local_edit_refused_v9()),
        };
        let budget = WorkflowUiBudgetV9::for_form(request, Phase::Confirm, request.step())?;
        let Some(WorkflowLocalPreviewV9::Remote(preview)) = request.preview() else {
            return Err(local_edit_refused_v9());
        };
        if deadline.saturating_duration_since(Instant::now()) > std::time::Duration::from_secs(30)
            || preview.phase() != expected
            || preview.attempted_v9()
            || request
                .remote_selection()
                .is_none_or(|selection| !preview.selection().same_selection(selection))
            || match (preview.published_branch_v9(), request.published_branch_v9()) {
                (None, None) => false,
                (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
                _ => true,
            }
        {
            return Err(local_edit_refused_v9());
        }
        // H already contains the genuine held Form and its prior Preview once.
        budget.check_peak(4 * 1024 * 1024)?;
        Ok(Arc::new(Self {
            preview: preview.clone(),
            cancel: cancel.clone(),
            deadline,
            started: AtomicBool::new(false),
            cleanup_waiting: AtomicBool::new(false),
            ui_budget: budget,
            branch: request.published_branch_v9().cloned(),
        }))
    }
    pub(crate) fn preview(&self) -> &Arc<WorkflowRemotePreviewV9> {
        &self.preview
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn started_cleanup_v9(&self) -> bool {
        self.started.load(Ordering::Acquire) && self.preview.attempted_v9()
    }
    /// Observation only: the existing publication TrySend accepted this
    /// actual Confirm and its waiter has not admitted a current ACK.
    pub(crate) fn cleanup_waiting_v9(&self) -> bool {
        self.cleanup_waiting.load(Ordering::Acquire)
    }
    pub(crate) fn observe_accepted_cleanup_wait_v9(&self) -> WorkflowCleanupWaitV9<'_> {
        self.cleanup_waiting.store(true, Ordering::Release);
        WorkflowCleanupWaitV9(&self.cleanup_waiting)
    }
    pub(crate) fn held_material_bytes_v9(&self) -> usize {
        self.ui_budget.held()
    }
    pub(crate) fn published_branch_v9(&self) -> Option<&Arc<WorkflowPublishedBranchV9>> {
        self.branch.as_ref()
    }
    pub(crate) fn validate_opened_v9(&self, database: &Path) -> GithubResult<()> {
        let opened = self
            .preview
            .selection()
            .committed()
            .staged()
            .saved()
            .opened();
        match &self.branch {
            Some(branch) if self.preview.phase() == WorkflowEditPhaseV9::CreatePr => {
                opened.validate_after_own_branch_v9(database, &self.cancel, branch)
            }
            Some(_) => Err(local_edit_refused_v9()),
            None => opened.validate_local(database, &self.cancel),
        }
    }
    pub(crate) fn check_original_v9(
        &self,
        binding: &Arc<WorkflowLocalBindingV9>,
    ) -> GithubResult<()> {
        shared::clock(&self.cancel, self.deadline)?;
        if !Arc::ptr_eq(
            binding,
            self.preview
                .selection()
                .committed()
                .staged()
                .saved()
                .opened()
                .binding(),
        ) {
            return Err(local_edit_refused_v9());
        }
        Ok(())
    }
    pub(crate) fn start_v9(&self, lock: &WorkflowMutationLockV9<'_>) -> GithubResult<()> {
        let binding = self
            .preview
            .selection()
            .committed()
            .staged()
            .saved()
            .opened()
            .binding();
        self.check_original_v9(binding)?;
        if !lock.owns(binding)
            || self.started.swap(true, Ordering::AcqRel)
            || !self.preview.reserve_attempt_v9()
        {
            return Err(local_edit_refused_v9());
        }
        Ok(())
    }
    pub(crate) fn check_material_v9(&self, retained: usize, transient: usize) -> GithubResult<()> {
        self.ui_budget.check_retained(retained)?;
        self.ui_budget.check_peak(
            retained
                .checked_add(transient)
                .ok_or_else(local_edit_refused_v9)?,
        )
    }
    pub(crate) fn execute(
        &self,
        provider: &GithubProvider,
        database: &Path,
        drafts: &DraftClient,
        fence: DraftPublicationFenceV2,
    ) -> GithubResult<crate::actions_controller::WorkflowRemoteExecutionV9> {
        self.preview
            .selection()
            .committed()
            .staged()
            .saved()
            .opened()
            .binding()
            .execute_remote_v9(self, provider, database, drafts, fence)
    }
}

pub(crate) struct WorkflowCleanupWaitV9<'a>(&'a AtomicBool);
impl Drop for WorkflowCleanupWaitV9<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Local invalidation witness for this exact live Push's own Actions-alias
/// transition. Non-serializable, no source/provider/permission/clock factory.
pub(crate) struct WorkflowPublishedBranchV9 {
    preview: Arc<WorkflowRemotePreviewV9>,
    refs: Arc<WorkflowRemoteRefsV9>,
    proof: Arc<crate::actions_write_journal::ActionsOwnedWorkflowRemoteHistoryV9>,
}
impl WorkflowPublishedBranchV9 {
    fn prepare_actual_branch_v9(
        request: &WorkflowRemoteConfirmV9,
        refs: &Arc<WorkflowRemoteRefsV9>,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        claim: Arc<crate::actions_write_journal::ActionsClaimStampV3>,
        receipt: &crate::actions_write_journal::ActionsReceipt,
    ) -> GithubResult<Arc<Self>> {
        use crate::actions_write_journal::{ActionsReceiptKind, WorkflowRemotePhaseV9};
        shared::clock(request.cancellation(), request.deadline())?;
        if request.preview.phase() != WorkflowEditPhaseV9::PublishBranch
            || receipt.kind != ActionsReceiptKind::WorkflowBranchObserved
            || !request.preview.refs().same_observation_v9(refs)
            || !Arc::ptr_eq(request.preview.selection().committed(), refs.committed())
            || !request.preview.attempted_v9()
            || !request.started.load(Ordering::Acquire)
        {
            return Err(local_edit_refused_v9());
        }
        let proof = journal
            .prepare_owned_workflow_remote_history_v9(claim, receipt)
            .map_err(|_| local_edit_refused_v9())?;
        if proof.phase() != WorkflowRemotePhaseV9::Branch {
            return Err(local_edit_refused_v9());
        }
        Ok(Arc::new(Self {
            preview: request.preview.clone(),
            refs: refs.clone(),
            proof,
        }))
    }
    pub(crate) fn proof_v9(
        &self,
    ) -> &Arc<crate::actions_write_journal::ActionsOwnedWorkflowRemoteHistoryV9> {
        &self.proof
    }
    pub(crate) fn matches_source_v9(
        &self,
        source: &crate::scoped_actions_v1::ScopedActionsSourceV1,
    ) -> bool {
        std::ptr::eq(
            self.preview
                .selection()
                .committed()
                .staged()
                .saved()
                .opened()
                .intent()
                .source()
                .as_ref(),
            source,
        )
    }
    pub(crate) fn binds_opened_v9(&self, opened: &OpenedWorkflowLocalEditV9) -> bool {
        let actual = self
            .preview
            .selection()
            .committed()
            .staged()
            .saved()
            .opened();
        std::ptr::eq(actual.as_ref(), opened)
            && Arc::ptr_eq(actual.intent().source(), opened.intent().source())
            && Arc::ptr_eq(self.preview.selection().committed(), self.refs.committed())
            && self.preview.refs().same_observation_v9(&self.refs)
    }
    pub(crate) fn binds_selection_v9(&self, selection: &WorkflowRemoteSelectionV9) -> bool {
        self.preview.selection().same_selection(selection)
    }
    pub(crate) fn binds_pr_refs_v9(&self, refs: &WorkflowRemoteRefsV9) -> bool {
        refs.phase() == WorkflowEditPhaseV9::CreatePr
            && Arc::ptr_eq(refs.committed(), self.refs.committed())
            && self
                .refs
                .repositories()
                .same_observation_v9(refs.repositories())
            && self.refs.head_branch() == refs.head_branch()
            && self.refs.base_branch() == refs.base_branch()
            && self.refs.base_sha() == refs.base_sha()
            && refs.head_sha() == Some(self.refs.committed().commit().head())
    }
    pub(crate) fn validate_journal_v9(
        &self,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
    ) -> GithubResult<()> {
        use crate::actions_write_journal::WorkflowRemotePhaseV9;
        if self.proof.phase() != WorkflowRemotePhaseV9::Branch
            || !self.binds_opened_v9(self.refs.committed().staged().saved().opened())
        {
            return Err(local_edit_refused_v9());
        }
        journal
            .validate_owned_workflow_remote_history_v9(&self.proof)
            .map_err(|_| local_edit_refused_v9())
    }
    pub(crate) fn matches_pr_origin_v9(
        &self,
        target: &crate::pr_write_journal::PrWriteTarget,
        database: &Path,
        file: (u64, u64),
    ) -> bool {
        self.proof.phase() == crate::actions_write_journal::WorkflowRemotePhaseV9::Branch
            && self.proof.matches_pr_origin_v9(target, database, file)
    }
    pub(crate) fn retained_bytes_v9(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + self.proof.retained_bytes()
            + self.preview.retained_bytes()
            + self.refs.owned_bytes_v9()
            + self.refs.repositories().owned_bytes_v9()
    }
}

/// The actual Branch object exists before final UID, but no operational
/// accessor exists until the same exact cleanup transaction is acknowledged.
pub(crate) struct PreparedWorkflowBranchV9 {
    branch: Arc<WorkflowPublishedBranchV9>,
    cleanup_ticket: Option<std::sync::Weak<()>>,
}
impl PreparedWorkflowBranchV9 {
    pub(crate) fn prepare_actual_branch_v9(
        request: &WorkflowRemoteConfirmV9,
        refs: &Arc<WorkflowRemoteRefsV9>,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        claim: Arc<crate::actions_write_journal::ActionsClaimStampV3>,
        receipt: &crate::actions_write_journal::ActionsReceipt,
    ) -> GithubResult<Self> {
        Ok(Self {
            branch: WorkflowPublishedBranchV9::prepare_actual_branch_v9(
                request, refs, journal, claim, receipt,
            )?,
            cleanup_ticket: None,
        })
    }
    pub(crate) fn proof_v9(
        &self,
    ) -> &Arc<crate::actions_write_journal::ActionsOwnedWorkflowRemoteHistoryV9> {
        self.branch.proof_v9()
    }
    pub(crate) fn bind_cleanup_v9(
        &mut self,
        prepared: &crate::storage::workflow_cleanup_v9::PreparedWorkflowCleanupV9,
    ) -> GithubResult<()> {
        if self.cleanup_ticket.is_some()
            || !Arc::ptr_eq(self.branch.proof_v9(), prepared.history_v9())
        {
            return Err(local_edit_refused_v9());
        }
        self.cleanup_ticket = Some(Arc::downgrade(prepared.ticket_v9()));
        Ok(())
    }
    pub(crate) fn release_after_cleanup_v9(
        self,
        committed: &crate::storage::workflow_cleanup_v9::CommittedWorkflowCleanupV9,
    ) -> GithubResult<Arc<WorkflowPublishedBranchV9>> {
        if self
            .cleanup_ticket
            .as_ref()
            .is_none_or(|ticket| !committed.matches_v9(self.branch.proof_v9(), ticket))
        {
            return Err(local_edit_refused_v9());
        }
        Ok(self.branch)
    }
}

/// Private fields; the only factory consumes this attempt's actual closed Core
/// while the SAME common-dir mutation mutex is still borrowed.
pub(crate) struct WorkflowRemoteReleaseV9<'a> {
    request: &'a WorkflowRemoteConfirmV9,
    lock: &'a WorkflowMutationLockV9<'a>,
    closed: ClosedCatalogueAccess,
    git: Arc<WorkflowRemoteGitObservationV9>,
    refs: Arc<WorkflowRemoteRefsV9>,
    effective: Instant,
}
impl<'a> WorkflowRemoteReleaseV9<'a> {
    pub(crate) fn from_closed_v9(
        request: &'a WorkflowRemoteConfirmV9,
        lock: &'a WorkflowMutationLockV9<'a>,
        closed: ClosedCatalogueAccess,
        git: Arc<WorkflowRemoteGitObservationV9>,
        refs: Arc<WorkflowRemoteRefsV9>,
        effective: Instant,
    ) -> GithubResult<Self> {
        let release = Self {
            request,
            lock,
            closed,
            git,
            refs,
            effective,
        };
        release.check_v9(&release.git)?;
        Ok(release)
    }
    pub(crate) fn check_v9(&self, git: &Arc<WorkflowRemoteGitObservationV9>) -> GithubResult<()> {
        let opened = self
            .request
            .preview
            .selection()
            .committed()
            .staged()
            .saved()
            .opened();
        self.request.check_original_v9(opened.binding())?;
        effective_phase_deadline_v9(&self.request.cancel, self.request.deadline, self.effective)?;
        if !self.request.started.load(Ordering::Acquire)
            || !self.request.preview.attempted_v9()
            || !self.lock.owns(opened.binding())
            || !Arc::ptr_eq(git, &self.git)
            || self.git.deadline() != self.effective
            || !self.request.preview.git().same_observation(git)
            || !self.request.preview.refs().same_observation_v9(&self.refs)
            || self
                .closed
                .observation(opened.intent().source().reference().id)
                .is_none()
        {
            return Err(local_edit_refused_v9());
        }
        shared::scope(opened.intent().source(), self.closed.identity())
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.effective
    }
    pub(crate) fn check_refs_v9(&self, refs: &Arc<WorkflowRemoteRefsV9>) -> GithubResult<()> {
        if !Arc::ptr_eq(refs, &self.refs) {
            return Err(local_edit_refused_v9());
        }
        self.check_v9(&self.git)
    }
}
