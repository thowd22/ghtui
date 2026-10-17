//! Borrowed allocation provenance ONLY. No arbitrary numeric budget setter,
//! source constructor, transport, saved authority, or additional ownership.
use super::*;
pub(crate) struct WorkflowLocalCoreInputsV9<'a> {
    intent: &'a Arc<WorkflowEditIntentV9>,
    held: usize,
}
impl WorkflowLocalCoreInputsV9<'_> {
    pub(crate) fn intent(&self) -> &Arc<WorkflowEditIntentV9> {
        self.intent
    }
    pub(crate) fn held_bytes(&self) -> usize {
        self.held
    }
}
impl WorkflowUiBudgetV9 {
    fn core_held_v9(&self, standalone: usize) -> crate::providers::gh::GithubResult<usize> {
        // A native budget was captured from the actual immutable request and
        // ALREADY includes its prior Preview once. Standalone has no Form.
        let held = if self.held() == 0 {
            standalone
        } else {
            self.held()
        };
        self.check_retained(standalone)?;
        self.check_peak(standalone)?;
        Ok(held)
    }
    pub(super) fn core_open_v9<'a>(
        &self,
        intent: &'a Arc<WorkflowEditIntentV9>,
    ) -> crate::providers::gh::GithubResult<WorkflowLocalCoreInputsV9<'a>> {
        Ok(WorkflowLocalCoreInputsV9 {
            intent,
            held: self.core_held_v9(
                std::mem::size_of::<WorkflowEditIntentV9>()
                    + 2 * std::mem::size_of::<usize>()
                    + intent.key().len(),
            )?,
        })
    }
    pub(super) fn core_opened_v9<'a>(
        &self,
        opened: &'a Arc<OpenedWorkflowLocalEditV9>,
        values: &WorkflowEditValuesV9,
    ) -> crate::providers::gh::GithubResult<WorkflowLocalCoreInputsV9<'a>> {
        let material = opened
            .retained_bytes()
            .checked_add(values.retained_bytes())
            .ok_or_else(local_edit_refused_v9)?;
        Ok(WorkflowLocalCoreInputsV9 {
            intent: opened.intent(),
            held: self.core_held_v9(material)?,
        })
    }
    pub(super) fn core_saved_v9<'a>(
        &self,
        saved: &'a Arc<WorkflowSavedLocalV9>,
    ) -> crate::providers::gh::GithubResult<WorkflowLocalCoreInputsV9<'a>> {
        let material = saved
            .opened()
            .retained_bytes()
            .checked_add(saved.values().retained_bytes())
            .and_then(|n| n.checked_add(saved.file().retained_bytes()))
            .and_then(|n| {
                n.checked_add(
                    std::mem::size_of::<WorkflowSavedLocalV9>() + 2 * std::mem::size_of::<usize>(),
                )
            })
            .ok_or_else(local_edit_refused_v9)?;
        Ok(WorkflowLocalCoreInputsV9 {
            intent: saved.opened().intent(),
            held: self.core_held_v9(material)?,
        })
    }
    pub(super) fn core_preview_v9<'a>(
        &self,
        preview: &'a WorkflowLocalPlanPreviewV9,
    ) -> crate::providers::gh::GithubResult<WorkflowLocalCoreInputsV9<'a>> {
        Ok(WorkflowLocalCoreInputsV9 {
            intent: preview.saved().opened().intent(),
            held: self.core_held_v9(preview.retained_bytes())?,
        })
    }
    pub(super) fn core_save_confirm_v9<'a>(
        &self,
        preview: &'a WorkflowSavePreviewV9,
    ) -> crate::providers::gh::GithubResult<WorkflowLocalCoreInputsV9<'a>> {
        Ok(WorkflowLocalCoreInputsV9 {
            intent: preview.opened().intent(),
            held: self.core_held_v9(preview.retained_bytes())?,
        })
    }
    pub(super) fn core_worktree_confirm_v9<'a>(
        &self,
        preview: &'a WorkflowWorktreePreviewV9,
    ) -> crate::providers::gh::GithubResult<WorkflowLocalCoreInputsV9<'a>> {
        Ok(WorkflowLocalCoreInputsV9 {
            intent: preview.target().opened().intent(),
            held: self.core_held_v9(preview.retained_bytes())?,
        })
    }
}
