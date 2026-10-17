//! Exact local Save through a closed explicit Confirm and SAME common-dir
//! mutation lock. Preview owns display/preimage, never a renewable API lease.
use super::*;
use crate::{
    git::{
        workflow_edit_v9::{WorkflowLocalBindingV9, WorkflowMutationLockV9, WorkflowWorkingFileV9},
        WorkspaceFingerprint,
    },
    prepared_form_text_v1::PreparedFormTextV1,
    providers::{gh::*, runner::CancellationToken},
    scoped_actions_publication_v3 as shared,
};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct WorkflowSavePreviewV9 {
    opened: Arc<OpenedWorkflowLocalEditV9>,
    values: Arc<WorkflowEditValuesV9>,
    preimage: Arc<WorkflowWorkingFileV9>,
    observed: WorkspaceFingerprint,
    display: Arc<PreparedFormTextV1>,
    retained: usize,
    attempted: AtomicBool,
}
impl std::fmt::Debug for WorkflowSavePreviewV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowSavePreviewV9(<exact private preimage/content; display only>)")
    }
}
impl WorkflowSavePreviewV9 {
    pub(crate) fn opened(&self) -> &Arc<OpenedWorkflowLocalEditV9> {
        &self.opened
    }
    pub(crate) fn display(&self) -> &Arc<PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn values(&self) -> &Arc<WorkflowEditValuesV9> {
        &self.values
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        self.retained
    }
    #[cfg(test)]
    pub(crate) fn prepare(
        opened: Arc<OpenedWorkflowLocalEditV9>,
        values: Arc<WorkflowEditValuesV9>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        Self::prepare_inner(
            opened,
            values,
            provider,
            database,
            cancel,
            deadline,
            WorkflowUiBudgetV9::standalone(),
        )
    }
    pub(crate) fn prepare_for_form_v9(
        request: &crate::scoped_actions_workflow_edit_ui_v9::WorkflowEditRequestV9,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        use crate::scoped_actions_workflow_edit_ui_v9::{WorkflowEditPhaseV9, WorkflowEditStepV9};
        let budget = WorkflowUiBudgetV9::for_form(
            request,
            WorkflowEditPhaseV9::Preview,
            WorkflowEditStepV9::Save,
        )?;
        Self::prepare_inner(
            request.opened().clone(),
            request.values().clone(),
            provider,
            database,
            cancel,
            deadline,
            budget,
        )
    }
    fn prepare_inner(
        opened: Arc<OpenedWorkflowLocalEditV9>,
        values: Arc<WorkflowEditValuesV9>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        budget: WorkflowUiBudgetV9,
    ) -> GithubResult<Arc<Self>> {
        bounded_clock(cancel, deadline)?;
        budget.before_native_prepare(
            opened
                .retained_bytes()
                .checked_add(values.retained_bytes())
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        let core_inputs = budget.core_opened_v9(&opened, &values)?;
        let core_policy =
            WorkflowCoreJsonV9::for_local_v9(provider, &core_inputs, cancel, deadline)?;
        values
            .validate_yaml_worker()
            .map_err(|_| local_edit_refused_v9())?;
        let intent = opened.intent();
        let phase = provider.clone().with_catalogue_read_deadline(deadline);
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
            WorkflowEditPhaseV9::Bind,
            1,
            cancel,
        )?;
        if post.is_some() {
            return Err(local_edit_refused_v9());
        }
        let effective = effective_phase_deadline_v9(cancel, deadline, window.deadline())?;
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
        let repository = opened
            .binding()
            .current_repository_v9(measured.remote_git_read_window_v1(cancel)?, cancel)
            .map_err(|_| local_edit_refused_v9())?;
        let (observed, config) = opened
            .binding()
            .check_save_repository_v9(&repository)
            .map_err(|_| local_edit_refused_v9())?;
        drop(config);
        let preimage = opened
            .binding()
            .observe_working_file_v9(cancel, effective)
            .map_err(|_| local_edit_refused_v9())?;
        if preimage.content() == values.yaml().as_bytes() {
            return Err(local_edit_refused_v9());
        }
        // Borrowed bounds BEFORE copying a source line or allocating indexes.
        let old_text =
            std::str::from_utf8(preimage.content()).map_err(|_| local_edit_refused_v9())?;
        if old_text.contains('\0') {
            return Err(local_edit_refused_v9());
        }
        let source_rows = values
            .yaml()
            .split('\n')
            .count()
            .checked_add(old_text.split('\n').count())
            .ok_or_else(local_edit_refused_v9)?;
        if source_rows > 65_500 {
            return Err(local_edit_refused_v9());
        }
        let base = opened
            .retained_bytes()
            .checked_add(values.retained_bytes())
            .and_then(|n| n.checked_add(preimage.retained_bytes()))
            .and_then(|n| {
                n.checked_add(std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>())
            })
            .ok_or_else(local_edit_refused_v9)?;
        let header_bound = opened
            .binding()
            .root_display_bound_v9()
            .and_then(|n| n.checked_add(intent.definition().workflow.path.len()))
            .and_then(|n| n.checked_add(2048))
            .ok_or_else(local_edit_refused_v9)?;
        let lines_bound = values
            .yaml()
            .len()
            .checked_add(preimage.content().len())
            .ok_or_else(local_edit_refused_v9)?
            .checked_mul(2)
            .and_then(|n| n.checked_add((source_rows + 10) * (std::mem::size_of::<String>() + 8)))
            .and_then(|n| n.checked_add(header_bound))
            .ok_or_else(local_edit_refused_v9)?;
        let before_lines = base
            .checked_add(lines_bound)
            .ok_or_else(local_edit_refused_v9)?;
        budget.check_retained(before_lines)?;
        let mut lines = Vec::with_capacity(source_rows + 10);
        lines.extend([
            "SAVE EXACT WORKFLOW FILE • no Git/index/remote change".into(),
            format!("Exact local root: {}", opened.binding().root_label()),
            format!("Exact path: {}", intent.definition().workflow.path),
            format!("Selected original saved C: {}", intent.definition().commit_sha),
            "Replaces the complete current regular file with these exact edited YAML bytes after explicit Confirm.".into(),
            "No automatic retry/rollback. External filesystem writers are not serialized by the application lock.".into(),
            "Complete current WORKING bytes below; they are NOT necessarily the saved blob C:".into(),
        ]);
        lines.extend(old_text.split('\n').map(str::to_owned));
        lines.push(
            "Complete edited replacement YAML below; original trailing newline/bytes are retained:"
                .into(),
        );
        lines.extend(values.yaml().split('\n').map(str::to_owned));
        let lines_bytes = lines.capacity() * std::mem::size_of::<String>()
            + lines.iter().map(String::capacity).sum::<usize>();
        if lines_bytes > lines_bound {
            return Err(local_edit_refused_v9());
        }
        let prepared_base = base
            .checked_add(lines_bytes)
            .ok_or_else(local_edit_refused_v9)?;
        let display = PreparedFormTextV1::prepare_with_remaining_v9(
            &lines,
            cancel,
            effective,
            budget.retained_remaining(prepared_base)?,
            budget.peak_remaining(prepared_base)?,
        )
        .map_err(|_| local_edit_refused_v9())?;
        let retained = base
            .checked_add(display.retained_bytes())
            .ok_or_else(local_edit_refused_v9)?;
        drop(lines); // No duplicate private row body retained beside the prepared Doc.
        preimage
            .validate(cancel, effective)
            .map_err(|_| local_edit_refused_v9())?;
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
        let closed =
            session.close_with_guard(cancel, || opened.validate_local(database, cancel))?;
        shared::scope(intent.source(), closed.identity())?;
        shared::clock(cancel, effective)?;
        Ok(Arc::new(Self {
            opened,
            values,
            preimage,
            observed,
            display,
            retained,
            attempted: AtomicBool::new(false),
        }))
    }
}
pub(crate) struct WorkflowSaveConfirmV9 {
    preview: Arc<WorkflowSavePreviewV9>,
    cancel: CancellationToken,
    deadline: Instant,
    started: AtomicBool,
    ui_budget: WorkflowUiBudgetV9,
}
impl std::fmt::Debug for WorkflowSaveConfirmV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowSaveConfirmV9(<one explicit original attempt>)")
    }
}
fn bounded_clock(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    shared::clock(cancel, deadline)?;
    if deadline.saturating_duration_since(Instant::now()) > std::time::Duration::from_secs(30) {
        return Err(local_edit_refused_v9());
    }
    Ok(())
}
impl WorkflowSaveConfirmV9 {
    pub(crate) fn capture(
        preview: Arc<WorkflowSavePreviewV9>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        bounded_clock(cancel, deadline)?;
        if preview.attempted.load(Ordering::Acquire) {
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
            WorkflowEditStepV9::Save,
        )?;
        let Some(WorkflowLocalPreviewV9::Save(preview)) = request.preview() else {
            return Err(local_edit_refused_v9());
        };
        budget.check_peak(
            preview
                .retained_bytes()
                .checked_add(4 * YAML_BYTES_V9 + 64 * 1024)
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
    ) -> GithubResult<WorkflowSaveOutcomeV9> {
        self.ui_budget.check_peak(
            self.preview
                .retained_bytes()
                .checked_add(4 * YAML_BYTES_V9 + 64 * 1024)
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        self.preview
            .opened
            .binding()
            .execute_save_v9(self, provider, database)
    }
    pub(crate) fn check_original_v9(
        &self,
        binding: &Arc<WorkflowLocalBindingV9>,
    ) -> GithubResult<()> {
        bounded_clock(&self.cancel, self.deadline)?;
        if !Arc::ptr_eq(binding, self.preview.opened.binding()) {
            return Err(local_edit_refused_v9());
        }
        Ok(())
    }
    pub(crate) fn execute_locked_v9(
        &self,
        provider: &GithubProvider,
        database: &Path,
        lock: &WorkflowMutationLockV9<'_>,
    ) -> GithubResult<WorkflowSaveOutcomeV9> {
        self.check_original_v9(self.preview.opened.binding())?;
        if !lock.owns(self.preview.opened.binding())
            || self.started.swap(true, Ordering::AcqRel)
            || self.preview.attempted.swap(true, Ordering::AcqRel)
        {
            return Err(local_edit_refused_v9());
        }
        let opened = &self.preview.opened;
        let intent = opened.intent();
        let cancel = &self.cancel;
        let core_inputs = self.ui_budget.core_save_confirm_v9(&self.preview)?;
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
            WorkflowEditPhaseV9::Save,
            1,
            cancel,
        )?;
        let mut post = post.ok_or_else(local_edit_refused_v9)?;
        let effective = effective_phase_deadline_v9(cancel, self.deadline, post.deadline())?;
        if window.deadline() != effective {
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
        let repository = opened
            .binding()
            .current_repository_v9(measured.remote_git_read_window_v1(cancel)?, cancel)
            .map_err(|_| local_edit_refused_v9())?;
        let (now, config) = opened
            .binding()
            .check_save_repository_v9(&repository)
            .map_err(|_| local_edit_refused_v9())?;
        drop(config);
        if now != self.preview.observed {
            return Err(local_edit_refused_v9());
        }
        self.preview
            .preimage
            .validate(cancel, effective)
            .map_err(|_| local_edit_refused_v9())?;
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
        // Validate the SAME actual measured Arc while the reservation is
        // PENDING. Core close seals it; the later fixed cursor is CLOSED.
        post.validate_prewrite(&measured, cancel)?;
        // All source/mapping/SQL checks precede final UID while SAME lock is
        // still held. Pool is sealed here; no post-Git read window is revived.
        let closed =
            session.close_with_guard(cancel, || opened.validate_local(database, cancel))?;
        shared::scope(intent.source(), closed.identity())?;
        let release = WorkflowSaveReleaseV9 {
            request: self,
            lock,
            closed,
            effective,
        };
        let saved = self.preview.preimage.save_confirmed_v9(&release);
        let principal =
            post.final_principal_with_heap_v9(provider, release.closed.identity(), &heap, cancel);
        Ok(match (saved, principal) {
            (Ok(file), Ok(identity)) => {
                let receipt = WorkflowSavedLocalV9 {
                    opened: self.preview.opened.clone(),
                    values: self.preview.values.clone(),
                    file,
                    identity,
                };
                if receipt
                    .retained_bytes_v9()
                    .is_none_or(|bytes| self.ui_budget.check_retained(bytes).is_err())
                {
                    WorkflowSaveOutcomeV9::InspectionRequired
                } else {
                    WorkflowSaveOutcomeV9::Saved(Arc::new(receipt))
                }
            }
            (Err(false), Ok(_)) => WorkflowSaveOutcomeV9::NotSaved,
            _ => WorkflowSaveOutcomeV9::InspectionRequired,
        })
    }
}
/// Construction occurs ONLY above, after actual explicit-once Confirm,
/// fresh Core/SQL/final UID, while the borrowed SAME mutex remains held.
pub(crate) struct WorkflowSaveReleaseV9<'a> {
    request: &'a WorkflowSaveConfirmV9,
    lock: &'a WorkflowMutationLockV9<'a>,
    closed: ClosedCatalogueAccess,
    effective: Instant,
}
impl WorkflowSaveReleaseV9<'_> {
    pub(crate) fn check(&self, binding: &Arc<WorkflowLocalBindingV9>) -> GithubResult<()> {
        self.request.check_original_v9(binding)?;
        effective_phase_deadline_v9(&self.request.cancel, self.request.deadline, self.effective)?;
        if !self.lock.owns(binding)
            || !self.request.started.load(Ordering::Acquire)
            || self
                .closed
                .observation(binding.intent().source().reference().id)
                .is_none()
            || self
                .request
                .preview
                .retained
                .checked_add(4 * YAML_BYTES_V9)
                .and_then(|n| n.checked_add(self.request.ui_budget.held()))
                .and_then(|n| n.checked_add(64 * 1024))
                .is_none_or(|n| n > 16 * 1024 * 1024)
        {
            return Err(local_edit_refused_v9());
        }
        shared::scope(binding.intent().source(), self.closed.identity())
    }
    pub(crate) fn content(&self) -> &[u8] {
        self.request.preview.values.yaml().as_bytes()
    }
    pub(crate) fn preimage(&self) -> &Arc<WorkflowWorkingFileV9> {
        &self.request.preview.preimage
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.request.cancel
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.effective
    }
}
pub(crate) struct WorkflowSavedLocalV9 {
    opened: Arc<OpenedWorkflowLocalEditV9>,
    values: Arc<WorkflowEditValuesV9>,
    file: Arc<WorkflowWorkingFileV9>,
    identity: HostIdentity,
}
impl WorkflowSavedLocalV9 {
    pub(crate) fn retained_bytes_v9(&self) -> Option<usize> {
        self.opened
            .retained_bytes()
            .checked_add(self.values.retained_bytes())?
            .checked_add(self.file.retained_bytes())?
            .checked_add(std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>())?
            .checked_add(identity_bytes_v9(&self.identity))
    }
    pub(crate) fn opened(&self) -> &Arc<OpenedWorkflowLocalEditV9> {
        &self.opened
    }
    pub(crate) fn values(&self) -> &Arc<WorkflowEditValuesV9> {
        &self.values
    }
    pub(crate) fn file(&self) -> &Arc<WorkflowWorkingFileV9> {
        &self.file
    }
    #[cfg(test)]
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
}
impl std::fmt::Debug for WorkflowSavedLocalV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowSavedLocalV9(<actual saved bytes; no Stage/GitHub permission>)")
    }
}
pub(crate) enum WorkflowSaveOutcomeV9 {
    Saved(Arc<WorkflowSavedLocalV9>),
    NotSaved,
    /// Save may have occurred; never retry, roll back or infer no effect.
    InspectionRequired,
}
