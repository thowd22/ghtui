//! Exact Form identity only. No body copy, authority, generic Form getter or
//! caller-created saved source. Minted after borrowed whole-workspace preflight.
use super::*;
pub(crate) struct WorkflowFormIdentityV9 {
    ticket: Arc<()>,
    presentation: Arc<()>,
    revision: u64,
    input_epoch: u64,
}
impl WorkflowFormIdentityV9 {
    pub(crate) fn capture(
        form: &Form,
        page: &Arc<crate::scoped_actions_v1::ScopedActionsPageV1>,
    ) -> Result<Self, String> {
        let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
            return Err(refused());
        };
        let (presentation, epoch) = form.presentation_v4();
        if !Arc::ptr_eq(state.intent().page(), page)
            || !Arc::ptr_eq(state.intent().source(), page.source())
        {
            return Err(refused());
        }
        let input_epoch = epoch.ok_or_else(refused)?;
        Ok(Self {
            ticket: state.ticket.clone(),
            presentation: presentation.clone(),
            revision: state.revision,
            input_epoch,
        })
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
            return false;
        };
        let (presentation, epoch) = form.presentation_v4();
        Arc::ptr_eq(&self.ticket, &state.ticket)
            && Arc::ptr_eq(&self.presentation, presentation)
            && self.revision == state.revision
            && epoch == Some(self.input_epoch)
    }
    /// Retirement only: Esc has already observed an input epoch. This exact
    /// Form/ticket/revision cannot admit a result, source or another request.
    pub(crate) fn matches_retirement_v9(&self, form: &Form) -> bool {
        let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
            return false;
        };
        Arc::ptr_eq(&self.ticket, &state.ticket)
            && Arc::ptr_eq(&self.presentation, form.presentation_v4().0)
            && self.revision == state.revision
    }
}
