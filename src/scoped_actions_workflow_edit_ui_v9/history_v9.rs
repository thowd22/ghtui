//! Retained actual Form READ origin; NOT a Preview, Confirm or saved source.
use super::*;
use crate::{
    actions_write_journal::{ActionsOwnedWorkflowRemoteHistoryV9, ActionsPublicationReference},
    scoped_actions_native_v1::journal_v5::ClosedJournalViewV5,
};
pub(crate) struct WorkflowHistoryFormV9 {
    initial: Arc<OpenedWorkflowLocalEditV9>,
    opened: Arc<OpenedWorkflowLocalEditV9>,
    ticket: Arc<()>,
    revision: u64,
    presentation: Arc<()>,
    input_epoch: u64,
    reference: ActionsPublicationReference,
    proof: Arc<ActionsOwnedWorkflowRemoteHistoryV9>,
    held: usize,
}
impl WorkflowHistoryFormV9 {
    pub(crate) fn intent(&self) -> &Arc<WorkflowEditIntentV9> {
        self.initial.intent()
    }
    pub(crate) fn reference(&self) -> &ActionsPublicationReference {
        &self.reference
    }
    pub(crate) fn proof(&self) -> &Arc<ActionsOwnedWorkflowRemoteHistoryV9> {
        &self.proof
    }
    pub(crate) fn held_bytes(&self) -> usize {
        self.held
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
            return false;
        };
        let (presentation, epoch) = form.presentation_v4();
        Arc::ptr_eq(&state.ticket, &self.ticket)
            && state.revision == self.revision
            && Arc::ptr_eq(state.opened(), &self.opened)
            && state
                .initial
                .as_ref()
                .is_some_and(|p| Arc::ptr_eq(p, &self.initial))
            && Arc::ptr_eq(presentation, &self.presentation)
            && epoch == Some(self.input_epoch)
            && state.remote_reference.as_ref() == Some(&self.reference)
            && state
                .remote_history
                .as_ref()
                .is_some_and(|p| Arc::ptr_eq(p, &self.proof))
    }
    pub(crate) fn retire_busy(&self, form: &mut Form) {
        if !self.matches(form) {
            return;
        }
        form.submitting = false;
        form.submit_requested = false;
        form.preview_requested = false;
        if let FormPurpose::ScopedWorkflowEditV9(state) = &mut form.purpose {
            state.inspect_requested_v9 = false;
            state.ack_requested_v9 = false;
        }
    }
}
pub(crate) fn history_requested_v9(form: &Form) -> Option<bool> {
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return None;
    };
    (state.inspect_requested_v9 || state.ack_requested_v9).then_some(state.ack_requested_v9)
}
pub(crate) fn capture_history_v9(form: &Form) -> Result<Arc<WorkflowHistoryFormV9>, String> {
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return Err(refused());
    };
    if form.submitting
        || !state.input_visible
        || form.editor_requested
        || form.preview_requested
        || state.preview.is_some()
        || form.actions_publication.is_some()
    {
        return Err(refused());
    }
    let reference = state.remote_reference.as_ref().ok_or_else(refused)?;
    let proof = state
        .remote_history
        .as_ref()
        .filter(|p| p.reference() == *reference)
        .ok_or_else(refused)?;
    let initial = state.initial.as_ref().ok_or_else(refused)?;
    let held = history_presentation_bound_v9(form)?;
    let (presentation, epoch) = form.presentation_v4();
    // Capture capacities BEFORE the first Arc/string clone. Existing actual
    // source/opened/proof identities are retained, never reconstructed.
    Ok(Arc::new(WorkflowHistoryFormV9 {
        initial: initial.clone(),
        opened: state.opened().clone(),
        ticket: state.ticket.clone(),
        revision: state.revision,
        presentation: presentation.clone(),
        input_epoch: epoch.ok_or_else(refused)?,
        reference: reference.clone(),
        proof: proof.clone(),
        held,
    }))
}
pub(crate) fn history_presentation_bound_v9(form: &Form) -> Result<usize, String> {
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return Err(refused());
    };
    let reference = state.remote_reference.as_ref().ok_or_else(refused)?;
    presentation_bytes_v9(form)?
        .checked_add(
            std::mem::size_of::<WorkflowHistoryFormV9>()
                + reference.fingerprint_sha256.capacity()
                + 8 * std::mem::size_of::<usize>(),
        )
        .filter(|n| *n <= 8 * 1024 * 1024)
        .ok_or_else(refused)
}
pub(crate) fn history_previous_v9(form: &Form) -> Option<Arc<ClosedJournalViewV5>> {
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return None;
    };
    state.history_view_v9.clone()
}
pub(crate) fn history_refused_v9(form: &mut Form) {
    let FormPurpose::ScopedWorkflowEditV9(state) = &mut form.purpose else {
        return;
    };
    state.inspect_requested_v9 = false;
    state.ack_requested_v9 = false;
    form.submit_requested = false;
    form.error =
        "Original W9 history unavailable; retained reference protected, no request repeated".into();
}
pub(crate) fn history_ack_ready_v9(form: &Form) -> bool {
    matches!(&form.purpose,FormPurpose::ScopedWorkflowEditV9(state) if !form.submitting&&state.input_visible&&state.history_visible_v9
        &&!form.preview_display_v1.geometry_rejected&&state.history_view_v9.as_ref().is_some_and(|view|view.workflow_remote_ack_v9()))
}
pub(crate) fn history_view_available_v9(form: &Form) -> bool {
    matches!(&form.purpose,FormPurpose::ScopedWorkflowEditV9(state) if !form.submitting&&state.input_visible&&state.remote_reference.is_some()&&state.remote_history.is_some())
}
/// Key routing follows the actual footer shape, not ACK readiness. An owned
/// history that is not ready still receives its existing refusal explanation;
/// editable fields and the Branch next-step footer keep real F6 field paging.
pub(crate) fn history_ack_key_v9(form: &Form) -> bool {
    matches!(&form.purpose, FormPurpose::ScopedWorkflowEditV9(state)
        if state.remote_reference.is_some()
            && !(state.published_branch.is_some()
                && !state.inspection_required
                && state.history_view_v9.is_none()))
}
pub(super) fn footer(form: &Form) -> Option<Vec<(&'static str, FormAction)>> {
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return None;
    };
    state.remote_reference.as_ref()?;
    let close = if form.submitting {
        "[Esc Cancel]"
    } else {
        "[Keep draft / Close]"
    };
    Some(if !history_ack_key_v9(form) {
        vec![
            ("[Preview next step]", FormAction::Preview),
            ("[Confirm step]", FormAction::Submit),
            ("[F4 View]", FormAction::AcknowledgeActionsV3),
            (close, FormAction::Close),
        ]
    } else {
        vec![
            ("[F4 View]", FormAction::AcknowledgeActionsV3),
            ("[F6 ACK local]", FormAction::AcknowledgeActionsAcceptanceV5),
            (close, FormAction::Close),
        ]
    })
}
pub(super) fn dispatch(form: &mut Form, action: &FormAction) -> bool {
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return false;
    };
    if state.remote_reference.is_none() {
        return false;
    }
    if !matches!(
        action,
        FormAction::AcknowledgeActionsV3 | FormAction::AcknowledgeActionsAcceptanceV5
    ) {
        return false;
    }
    let ack = *action == FormAction::AcknowledgeActionsAcceptanceV5;
    if form.submitting
        || !state.input_visible
        || state.remote_history.is_none()
        || ack && !history_ack_ready_v9(form)
    {
        form.error="Inspect the exact current original remote facts before explicit local ACK; effect remains UNVERIFIED and no request is resent".into();
        return true;
    }
    if let FormPurpose::ScopedWorkflowEditV9(state) = &mut form.purpose {
        state.inspect_requested_v9 = !ack;
        state.ack_requested_v9 = ack;
    }
    form.submit_requested = true; // SAME existing newer-Form intent fence.
    true
}
pub(crate) fn install_history_v9(
    form: &mut Form,
    request: &WorkflowHistoryFormV9,
    history: Arc<ClosedJournalViewV5>,
    document: Arc<PreparedFormTextV1>,
    successor: Option<Arc<ActionsOwnedWorkflowRemoteHistoryV9>>,
) -> bool {
    if !request.matches(form) {
        return false;
    }
    let FormPurpose::ScopedWorkflowEditV9(state) = &mut form.purpose else {
        return false;
    };
    state.summary = document.clone();
    state.history_view_v9 = Some(history);
    if let Some(proof) = successor {
        state.remote_history = Some(proof)
    }
    state.inspection_required = true;
    state.preview = None;
    state.confirmation_visible = false;
    state.inspect_requested_v9 = false;
    state.ack_requested_v9 = false;
    state.history_visible_v9 = false;
    form.submitting = false;
    form.submit_requested = false;
    form.preview_requested = false;
    form.preview = true;
    form.focus = form.fields.len();
    form.error.clear();
    form.install_prepared_preview_display_v1(document);
    true
}
