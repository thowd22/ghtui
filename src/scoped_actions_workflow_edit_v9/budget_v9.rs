//! A genuine captured presentation can only REDUCE the existing material
//! budgets. It is never a source, provider window, permission or clock.
use super::*;
use crate::scoped_actions_workflow_edit_ui_v9::{
    WorkflowEditPhaseV9, WorkflowEditRequestV9, WorkflowEditStepV9,
};

#[derive(Clone, Copy)]
pub(super) struct WorkflowUiBudgetV9 {
    held: usize,
    environment_retained: usize,
    environment_peak: usize,
}
impl WorkflowUiBudgetV9 {
    pub(super) fn for_live_native_open_v9(
        intent: &WorkflowEditIntentV9,
        old: Option<&crate::scoped_actions_workflow_edit_ui_v9::ClosedWorkflowFormV9>,
        presentation:&crate::scoped_actions_native_v1::publication_v3::workflow_edit_v9::WorkflowNativePresentationV9,
    ) -> crate::providers::gh::GithubResult<Self> {
        if !presentation.matches_intent(intent)
            || old.is_some_and(|old| !old.matches_intent(intent))
        {
            return Err(local_edit_refused_v9());
        }
        let mut budget = match old {
            Some(old) => Self::for_reopen(old)?,
            None => Self::standalone(),
        };
        budget.held = budget
            .held
            .checked_add(presentation.held_bytes())
            .filter(|n| *n <= 8 * 1024 * 1024)
            .ok_or_else(local_edit_refused_v9)?;
        budget.check_retained(0)?;
        budget.check_peak(0)?;
        Ok(budget)
    }
    pub(super) fn standalone() -> Self {
        let (environment_retained, environment_peak) =
            Self::environment_bound_v9().unwrap_or((usize::MAX, usize::MAX));
        Self {
            held: 0,
            environment_retained,
            environment_peak,
        }
    }
    fn environment_bound_v9() -> Option<(usize, usize)> {
        let snapshot = crate::providers::runner::WorkflowProcessEnvironmentV9::current_v9()?;
        let retained = snapshot.retained_bytes_v9().checked_add(64 * 1024)?;
        Some((
            retained,
            retained.checked_add(snapshot.fixed_git_child_bound_v9()?)?,
        ))
    }
    pub(super) fn for_reopen(
        old: &crate::scoped_actions_workflow_edit_ui_v9::ClosedWorkflowFormV9,
    ) -> crate::providers::gh::GithubResult<Self> {
        let headers = std::mem::size_of::<
            crate::scoped_actions_workflow_edit_ui_v9::ClosedWorkflowFormV9,
        >() + std::mem::size_of::<crate::scoped_actions_v1::ScopedActionsWorkV1>()
            + std::mem::size_of::<crate::scoped_actions_native_v1::ScopedActionsOwnerV1>()
            + 12 * std::mem::size_of::<usize>();
        let held = old
            .held_bytes()
            .checked_add(headers)
            .filter(|n| *n <= 8 * 1024 * 1024)
            .ok_or_else(local_edit_refused_v9)?;
        let (environment_retained, environment_peak) =
            Self::environment_bound_v9().ok_or_else(local_edit_refused_v9)?;
        Ok(Self {
            held,
            environment_retained,
            environment_peak,
        })
    }
    pub(super) fn for_form(
        request: &WorkflowEditRequestV9,
        phase: WorkflowEditPhaseV9,
        step: WorkflowEditStepV9,
    ) -> crate::providers::gh::GithubResult<Self> {
        if request.phase() != phase
            || request.step() != step
            || request.owned_bytes() > 8 * 1024 * 1024
        {
            return Err(local_edit_refused_v9());
        }
        let (environment_retained, environment_peak) =
            Self::environment_bound_v9().ok_or_else(local_edit_refused_v9)?;
        Ok(Self {
            held: request.owned_bytes(),
            environment_retained,
            environment_peak,
        })
    }
    pub(super) fn held(&self) -> usize {
        self.held
    }
    pub(super) fn retained_remaining(
        &self,
        base: usize,
    ) -> crate::providers::gh::GithubResult<usize> {
        (8 * 1024 * 1024usize)
            .checked_sub(base)
            .and_then(|n| n.checked_sub(self.held))
            .and_then(|n| n.checked_sub(self.environment_retained))
            .ok_or_else(local_edit_refused_v9)
    }
    pub(super) fn peak_remaining(&self, base: usize) -> crate::providers::gh::GithubResult<usize> {
        (16 * 1024 * 1024usize)
            .checked_sub(base)
            .and_then(|n| n.checked_sub(self.held))
            .and_then(|n| n.checked_sub(self.environment_peak))
            .ok_or_else(local_edit_refused_v9)
    }
    pub(super) fn check_retained(&self, base: usize) -> crate::providers::gh::GithubResult<()> {
        self.retained_remaining(base).map(|_| ())
    }
    pub(super) fn check_peak(&self, base: usize) -> crate::providers::gh::GithubResult<()> {
        self.peak_remaining(base).map(|_| ())
    }
    /// Native-only conservative scratch refusal BEFORE local parsing/target/
    /// model preparation. Standalone wrappers retain their historical guard.
    /// This headroom is NOT a measured heap/RSS or final parser derivation.
    pub(super) fn before_native_prepare(
        &self,
        base: usize,
    ) -> crate::providers::gh::GithubResult<()> {
        self.check_retained(base)?;
        self.check_peak(base)?;
        if self.held != 0 {
            self.check_peak(
                base.checked_add(12 * 1024 * 1024)
                    .ok_or_else(local_edit_refused_v9)?,
            )?;
        }
        Ok(())
    }
}
