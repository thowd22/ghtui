//! Explicit-once new worktree Confirm, same original Core/ledger/common-dir
//! mutex, followed only by the fixed paid receipt. No automatic cleanup/retry.
use super::*;
use crate::{
    git::{
        workflow_edit_v9::{
            WorkflowLocalBindingV9, WorkflowMutationLockV9, WorkflowPreparedWorktreeV9,
            WorkflowWorkingFileV9, WorkflowWorktreePurposeV9,
        },
        RepoStatus,
    },
    prepared_form_text_v1::PreparedFormTextV1,
    providers::{gh::*, runner::CancellationToken},
    scoped_actions_publication_v3 as shared,
};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct WorkflowWorktreeConfirmV9 {
    preview: Arc<WorkflowWorktreePreviewV9>,
    cancel: CancellationToken,
    deadline: Instant,
    started: AtomicBool,
    ui_budget: WorkflowUiBudgetV9,
}
impl std::fmt::Debug for WorkflowWorktreeConfirmV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowWorktreeConfirmV9(<one genuine explicit isolation attempt>)")
    }
}
impl WorkflowWorktreeConfirmV9 {
    pub(crate) fn capture(
        preview: Arc<WorkflowWorktreePreviewV9>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        shared::clock(cancel, deadline)?;
        if deadline.saturating_duration_since(Instant::now()) > std::time::Duration::from_secs(30)
            || preview.attempted_v9()
        {
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
    pub(crate) fn preview(&self) -> &Arc<WorkflowWorktreePreviewV9> {
        &self.preview
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    /// Restriction-only debit for the actual command/marker, before copying
    /// its bounded argv/environment. Native H is always the genuine request.
    pub(crate) fn check_command_material_v9(&self, borrowed: usize) -> GithubResult<()> {
        if borrowed > 64 * 1024 {
            return Err(local_edit_refused_v9());
        }
        let bytes = self
            .preview
            .retained_bytes()
            .checked_add(borrowed)
            .and_then(|n| n.checked_add(64 * 1024))
            .ok_or_else(local_edit_refused_v9)?;
        self.ui_budget.check_retained(bytes)?;
        self.ui_budget.check_peak(
            bytes
                .checked_add(4 * 1024 * 1024)
                .ok_or_else(local_edit_refused_v9)?,
        )
    }
    pub(crate) fn check_original_v9(
        &self,
        binding: &Arc<WorkflowLocalBindingV9>,
    ) -> GithubResult<()> {
        shared::clock(&self.cancel, self.deadline)?;
        if !Arc::ptr_eq(binding, self.preview.target().opened().binding()) {
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
            WorkflowEditStepV9::Isolate,
        )?;
        let Some(WorkflowLocalPreviewV9::Worktree(preview)) = request.preview() else {
            return Err(local_edit_refused_v9());
        };
        budget.check_peak(
            preview
                .retained_bytes()
                .checked_add(4 * 1024 * 1024)
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
    ) -> GithubResult<WorkflowWorktreeOutcomeV9> {
        self.ui_budget.check_peak(
            self.preview
                .retained_bytes()
                .checked_add(4 * 1024 * 1024)
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        self.preview
            .target()
            .opened()
            .binding()
            .execute_worktree_v9(self, provider, database)
    }
    pub(crate) fn execute_locked_v9(
        &self,
        provider: &GithubProvider,
        database: &Path,
        lock: &WorkflowMutationLockV9<'_>,
    ) -> GithubResult<WorkflowWorktreeOutcomeV9> {
        let target = self.preview.target();
        let opened = target.opened();
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
        let core_inputs = self.ui_budget.core_worktree_confirm_v9(&self.preview)?;
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
            WorkflowEditPhaseV9::Worktree,
            1,
            cancel,
        )?;
        let mut post = post.ok_or_else(local_edit_refused_v9)?;
        let effective = effective_phase_deadline_v9(cancel, self.deadline, post.deadline())?;
        if window.deadline() != effective
            || self
                .preview
                .retained_bytes()
                .checked_add(post.reserved_bytes())
                .and_then(|n| n.checked_add(self.ui_budget.held()))
                .and_then(|n| n.checked_add(4 * 1024 * 1024))
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
        let purpose = WorkflowWorktreePurposeV9::capture_v9(target.clone(), cancel, effective)
            .map_err(|_| local_edit_refused_v9())?;
        let (status, observed) = purpose
            .check_fresh_plan_repository_v9(&repository)
            .map_err(|_| local_edit_refused_v9())?;
        if observed != self.preview.plan().mutation.observed {
            return Err(local_edit_refused_v9());
        }
        let command =
            WorkflowPreparedWorktreeV9::prepare_v9(self, &repository, &status, effective)?;
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
        let closed = session.close_with_guard(cancel, || {
            opened.validate_local(database, cancel)?;
            target
                .validate_absent_v9(cancel, effective)
                .map_err(|_| local_edit_refused_v9())
        })?;
        shared::scope(intent.source(), closed.identity())?;
        let release = WorkflowWorktreeReleaseV9 {
            request: self,
            lock,
            closed,
            effective,
        };
        let write = post.worktree_write_v9(provider, command.release_v9(&release)?, cancel);
        let prepared = (|| -> GithubResult<_> {
            let (binding, file, status) = target
                .observe_after_attempt_v9(provider, &mut post, cancel)?
                .into_parts_v9();
            let lines = vec!["Isolated linked worktree actually observed • local effects only".into(),
                format!("Actual worktree: {}", binding.root_label()),
                format!("Actual NEW branch: {}", target.values().branch()),
                format!("Actual HEAD C: {}", intent.definition().commit_sha),
                "Directory, shared branch and linked metadata exist; remote branch/PR remain unchanged.".into(),
                "Save, Stage, Commit, create-only push and PR each require their own explicit Preview/Confirm.".into(),
                "No automatic worktree deletion, branch deletion, rollback or retry is authorized.".into()];
            let identity = release.closed.identity().clone();
            let base = self
                .preview
                .retained_bytes()
                .checked_add(binding.retained_bytes())
                .and_then(|n| n.checked_add(file.retained_bytes()))
                .and_then(|n| n.checked_add(identity_bytes_v9(&identity)))
                .and_then(|n| {
                    n.checked_add(
                        lines.capacity() * std::mem::size_of::<String>()
                            + lines.iter().map(String::capacity).sum::<usize>(),
                    )
                })
                .and_then(|n| {
                    n.checked_add(
                        std::mem::size_of::<OpenedWorkflowLocalEditV9>()
                            + std::mem::size_of::<WorkflowIsolatedLocalV9>()
                            + 4 * std::mem::size_of::<usize>(),
                    )
                })
                .and_then(|n| n.checked_add(status_heap_v9(&status)))
                .ok_or_else(local_edit_refused_v9)?;
            let display = PreparedFormTextV1::prepare_with_remaining_v9(
                &lines,
                cancel,
                effective,
                self.ui_budget.retained_remaining(base)?,
                self.ui_budget.peak_remaining(
                    base.checked_add(post.reserved_bytes())
                        .ok_or_else(local_edit_refused_v9)?,
                )?,
            )
            .map_err(|_| local_edit_refused_v9())?;
            let retained = base
                .checked_add(display.retained_bytes())
                .ok_or_else(local_edit_refused_v9)?;
            opened.validate_local(database, cancel)?;
            file.validate(cancel, effective)
                .map_err(|_| local_edit_refused_v9())?;
            let derived = OpenedWorkflowLocalEditV9 {
                intent: intent.clone(),
                binding,
                local: opened.local.clone(),
                identity,
                lines,
                display,
                retained,
            };
            Ok((derived, file, status))
        })();
        let principal =
            post.final_principal_with_heap_v9(provider, release.closed.identity(), &heap, cancel);
        Ok(match (write, prepared, principal) {
            (Ok(()), Ok((mut derived, file, status)), Ok(identity)) => {
                derived.retained -= identity_bytes_v9(&derived.identity);
                derived.identity = identity;
                derived.retained += identity_bytes_v9(&derived.identity);
                if self.ui_budget.check_retained(derived.retained).is_err() {
                    return Ok(WorkflowWorktreeOutcomeV9::InspectionRequired);
                }
                WorkflowWorktreeOutcomeV9::Isolated(Arc::new(WorkflowIsolatedLocalV9 {
                    preview: self.preview.clone(),
                    opened: Arc::new(derived),
                    _retained_file: file,
                    _retained_status: status,
                }))
            }
            _ => WorkflowWorktreeOutcomeV9::InspectionRequired,
        })
    }
}

pub(super) fn status_heap_v9(status: &RepoStatus) -> usize {
    status.head.as_ref().map_or(0, String::capacity)
        + status.branch.as_ref().map_or(0, String::capacity)
        + status.raw_branch.as_ref().map_or(0, Vec::capacity)
        + status.upstream.as_ref().map_or(0, String::capacity)
        + status.raw_upstream.as_ref().map_or(0, Vec::capacity)
        + status.entries.capacity() * std::mem::size_of::<crate::git::StatusEntry>()
        + status
            .entries
            .iter()
            .map(|entry| {
                entry.path.0.capacity()
                    + entry
                        .original_path
                        .as_ref()
                        .map_or(0, |path| path.0.capacity())
            })
            .sum::<usize>()
}
pub(crate) struct WorkflowWorktreeReleaseV9<'a> {
    request: &'a WorkflowWorktreeConfirmV9,
    lock: &'a WorkflowMutationLockV9<'a>,
    closed: ClosedCatalogueAccess,
    effective: Instant,
}
impl WorkflowWorktreeReleaseV9<'_> {
    pub(crate) fn check(
        &self,
        binding: &Arc<WorkflowLocalBindingV9>,
        preview: &Arc<WorkflowWorktreePreviewV9>,
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
pub(crate) struct WorkflowIsolatedLocalV9 {
    preview: Arc<WorkflowWorktreePreviewV9>,
    opened: Arc<OpenedWorkflowLocalEditV9>,
    _retained_file: Arc<WorkflowWorkingFileV9>,
    _retained_status: RepoStatus,
}
impl WorkflowIsolatedLocalV9 {
    pub(crate) fn preview(&self) -> &Arc<WorkflowWorktreePreviewV9> {
        &self.preview
    }
    pub(crate) fn opened(&self) -> &Arc<OpenedWorkflowLocalEditV9> {
        &self.opened
    }
    #[cfg(test)]
    pub(crate) fn file(&self) -> &Arc<WorkflowWorkingFileV9> {
        &self._retained_file
    }
    #[cfg(test)]
    pub(crate) fn status(&self) -> &RepoStatus {
        &self._retained_status
    }
}
pub(crate) enum WorkflowWorktreeOutcomeV9 {
    Isolated(Arc<WorkflowIsolatedLocalV9>),
    InspectionRequired,
}
