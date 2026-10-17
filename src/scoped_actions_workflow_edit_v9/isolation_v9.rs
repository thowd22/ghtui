//! Isolated-worktree Preview is display material only. Its target is a real
//! absent private child, not a directory created by Preview or a source mint.
use super::*;
use crate::{
    git::workflow_edit_v9::{WorkflowWorktreePurposeV9, WorkflowWorktreeTargetV9},
    local_actions::LocalPlan,
    prepared_form_text_v1::PreparedFormTextV1,
    providers::{gh::*, runner::CancellationToken},
    scoped_actions_publication_v3 as shared,
};

pub(crate) struct WorkflowWorktreePreviewV9 {
    target: Arc<WorkflowWorktreeTargetV9>,
    plan: Arc<LocalPlan>,
    display: Arc<PreparedFormTextV1>,
    retained: usize,
    attempted: std::sync::atomic::AtomicBool,
}
impl std::fmt::Debug for WorkflowWorktreePreviewV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowWorktreePreviewV9(<exact absent target/branch/C; not approval>)")
    }
}
impl WorkflowWorktreePreviewV9 {
    pub(crate) fn target(&self) -> &Arc<WorkflowWorktreeTargetV9> {
        &self.target
    }
    pub(crate) fn plan(&self) -> &Arc<LocalPlan> {
        &self.plan
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
            WorkflowEditStepV9::Isolate,
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
        budget.before_native_prepare(
            opened
                .retained_bytes()
                .checked_add(values.retained_bytes())
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        shared::clock(cancel, deadline)?;
        if deadline.saturating_duration_since(Instant::now()) > std::time::Duration::from_secs(30) {
            return Err(local_edit_refused_v9());
        }
        values
            .validate_yaml_worker()
            .map_err(|_| local_edit_refused_v9())?;
        let intent = opened.intent();
        let core_inputs = budget.core_opened_v9(&opened, &values)?;
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
        let repository = opened
            .binding()
            .current_repository_v9(measured.remote_git_read_window_v1(cancel)?, cancel)
            .map_err(|_| local_edit_refused_v9())?;
        let target = WorkflowWorktreeTargetV9::capture_default_v9(
            opened.clone(),
            values.clone(),
            database,
            cancel,
            effective,
        )?;
        let purpose = WorkflowWorktreePurposeV9::capture_v9(target.clone(), cancel, effective)
            .map_err(|_| local_edit_refused_v9())?;
        let plan = Arc::new(
            LocalPlan::prepare_workflow_worktree_v9(&repository, &purpose)
                .map_err(|_| local_edit_refused_v9())?,
        );
        let base = opened
            .retained_bytes()
            .checked_add(values.retained_bytes())
            .and_then(|n| n.checked_add(target.retained_bytes()))
            .and_then(|n| n.checked_add(plan_v9::plan_bytes(&plan).ok()?))
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
        let closed = session.close_with_guard(cancel, || {
            opened.validate_local(database, cancel)?;
            target
                .validate_absent_v9(cancel, effective)
                .map_err(|_| local_edit_refused_v9())
        })?;
        shared::scope(intent.source(), closed.identity())?;
        shared::clock(cancel, effective)?;
        Ok(Arc::new(Self {
            target,
            plan,
            display,
            retained,
            attempted: std::sync::atomic::AtomicBool::new(false),
        }))
    }
}
