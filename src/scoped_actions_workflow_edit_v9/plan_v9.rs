//! Worker-only SAME LocalPlan/canonical parser over an actual saved receipt.
//! A prepared local plan is display, never an executable read-window permit.
use super::*;
use crate::{
    git::{
        workflow_edit_v9::{WorkflowLocalStepV9, WorkflowPlanPurposeV9},
        Operation,
    },
    local_actions::{LocalPlan, WorkflowReviewedPatchV9},
    prepared_form_text_v1::PreparedFormTextV1,
    providers::{gh::*, runner::CancellationToken},
    scoped_actions_publication_v3 as shared,
};

pub(crate) struct WorkflowLocalPlanPreviewV9 {
    saved: Arc<WorkflowSavedLocalV9>,
    step: WorkflowLocalStepV9,
    plan: Arc<LocalPlan>,
    reviewed: Option<WorkflowReviewedPatchV9>,
    display: Arc<PreparedFormTextV1>,
    retained: usize,
    attempted: std::sync::atomic::AtomicBool,
}
impl std::fmt::Debug for WorkflowLocalPlanPreviewV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "WorkflowLocalPlanPreviewV9(<exact installed-Git plan; private content omitted>)",
        )
    }
}
pub(super) fn plan_bytes(plan: &LocalPlan) -> GithubResult<usize> {
    let status = &plan.context.status;
    let mut n = std::mem::size_of::<LocalPlan>()
        + 2 * std::mem::size_of::<usize>()
        + plan.context.directory.capacity()
        + plan.context.revision.capacity()
        + plan
            .context
            .selected_path
            .as_ref()
            .map_or(0, |path| path.0.capacity())
        + status.head.as_ref().map_or(0, String::capacity)
        + status.branch.as_ref().map_or(0, String::capacity)
        + status.raw_branch.as_ref().map_or(0, Vec::capacity)
        + status.upstream.as_ref().map_or(0, String::capacity)
        + status.raw_upstream.as_ref().map_or(0, Vec::capacity)
        + status.entries.capacity() * std::mem::size_of::<crate::git::StatusEntry>()
        + plan.preview.capacity() * std::mem::size_of::<String>()
        + plan.mutation.target.capacity()
        + plan.mutation.effect.capacity();
    for entry in &status.entries {
        n = n
            .checked_add(entry.path.0.capacity())
            .and_then(|n| {
                n.checked_add(
                    entry
                        .original_path
                        .as_ref()
                        .map_or(0, |path| path.0.capacity()),
                )
            })
            .ok_or_else(local_edit_refused_v9)?;
    }
    for row in &plan.preview {
        n = n
            .checked_add(row.capacity())
            .ok_or_else(local_edit_refused_v9)?;
    }
    n = n
        .checked_add(match &plan.mutation.operation {
            Operation::Stage(paths) => {
                paths.capacity() * std::mem::size_of::<crate::git::RawPath>()
                    + paths.iter().map(|path| path.0.capacity()).sum::<usize>()
            }
            Operation::Commit {
                message,
                amend: false,
            } => message.capacity(),
            Operation::AddWorktreeBranchAtV9 {
                path,
                branch,
                commit,
            } => path.capacity() + branch.capacity() + commit.capacity(),
            _ => return Err(local_edit_refused_v9()),
        })
        .ok_or_else(local_edit_refused_v9)?;
    Ok(n)
}
impl WorkflowLocalPlanPreviewV9 {
    pub(crate) fn saved(&self) -> &Arc<WorkflowSavedLocalV9> {
        &self.saved
    }
    pub(crate) fn step(&self) -> WorkflowLocalStepV9 {
        self.step
    }
    pub(crate) fn plan(&self) -> &Arc<LocalPlan> {
        &self.plan
    }
    pub(super) fn reviewed_v9(&self) -> Option<&WorkflowReviewedPatchV9> {
        self.reviewed.as_ref()
    }
    pub(crate) fn display(&self) -> &Arc<PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        self.retained
    }
    pub(super) fn reserve_attempt_v9(&self) -> bool {
        !self
            .attempted
            .swap(true, std::sync::atomic::Ordering::AcqRel)
    }
    pub(super) fn attempted_v9(&self) -> bool {
        self.attempted.load(std::sync::atomic::Ordering::Acquire)
    }
    #[cfg(test)]
    pub(crate) fn prepare(
        saved: Arc<WorkflowSavedLocalV9>,
        step: WorkflowLocalStepV9,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        Self::prepare_inner(
            saved,
            step,
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
        let step = match request.step() {
            WorkflowEditStepV9::Stage => WorkflowLocalStepV9::Stage,
            WorkflowEditStepV9::Commit => WorkflowLocalStepV9::Commit,
            _ => return Err(local_edit_refused_v9()),
        };
        let budget =
            WorkflowUiBudgetV9::for_form(request, WorkflowEditPhaseV9::Preview, request.step())?;
        Self::prepare_inner(
            request.saved().ok_or_else(local_edit_refused_v9)?.clone(),
            step,
            provider,
            database,
            cancel,
            deadline,
            budget,
        )
    }
    fn prepare_inner(
        saved: Arc<WorkflowSavedLocalV9>,
        step: WorkflowLocalStepV9,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        budget: WorkflowUiBudgetV9,
    ) -> GithubResult<Arc<Self>> {
        budget.before_native_prepare(
            saved
                .opened()
                .retained_bytes()
                .checked_add(saved.values().retained_bytes())
                .and_then(|n| n.checked_add(saved.file().retained_bytes()))
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        shared::clock(cancel, deadline)?;
        if deadline.saturating_duration_since(Instant::now()) > std::time::Duration::from_secs(30) {
            return Err(local_edit_refused_v9());
        }
        let opened = saved.opened();
        let intent = opened.intent();
        let core_inputs = budget.core_saved_v9(&saved)?;
        let core_policy =
            WorkflowCoreJsonV9::for_local_v9(provider, &core_inputs, cancel, deadline)?;
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
        let repo = opened
            .binding()
            .current_repository_v9(measured.remote_git_read_window_v1(cancel)?, cancel)
            .map_err(|_| local_edit_refused_v9())?;
        let purpose = WorkflowPlanPurposeV9::from_saved_v9(saved.clone(), step, cancel, effective)
            .map_err(|_| local_edit_refused_v9())?;
        let (plan, reviewed) = match step {
            WorkflowLocalStepV9::Stage => (
                LocalPlan::prepare_workflow_edit_v9(&repo, &purpose)
                    .map_err(|_| local_edit_refused_v9())?,
                None,
            ),
            WorkflowLocalStepV9::Commit => {
                LocalPlan::prepare_workflow_edit_with_review_v9(&repo, &purpose)
                    .map_err(|_| local_edit_refused_v9())?
            }
        };
        let plan = Arc::new(plan);
        let base = opened
            .retained_bytes()
            .checked_add(saved.values().retained_bytes())
            .and_then(|n| n.checked_add(saved.file().retained_bytes()))
            .and_then(|n| n.checked_add(plan_bytes(&plan).ok()?))
            .and_then(|n| {
                n.checked_add(
                    reviewed
                        .as_ref()
                        .map_or(0, WorkflowReviewedPatchV9::retained_heap_bytes_v9),
                )
            })
            .and_then(|n| {
                n.checked_add(std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>())
            })
            .ok_or_else(local_edit_refused_v9)?;
        let display = PreparedFormTextV1::prepare_with_remaining_v9(
            &plan.preview,
            cancel,
            effective,
            budget.retained_remaining(base)?,
            budget.peak_remaining(base)?,
        )
        .map_err(|_| local_edit_refused_v9())?;
        let retained = base
            .checked_add(display.retained_bytes())
            .ok_or_else(local_edit_refused_v9)?;
        purpose
            .validate(&repo)
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
            saved,
            step,
            plan,
            reviewed,
            display,
            retained,
            attempted: std::sync::atomic::AtomicBool::new(false),
        }))
    }
}
