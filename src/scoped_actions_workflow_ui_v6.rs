//! Typed workflow state in the ONE existing Form renderer and input semantics.
use crate::{
    app::{App, Overlay, View},
    forms::{Form, FormAction, FormPurpose},
    scoped_actions_workflow_publication_v6::{
        OpenedWorkflowV6, WorkflowHistoryV6, WorkflowPreviewV6,
    },
};
use std::sync::Arc;
#[derive(Clone)]
pub struct WorkflowComposerStateV6 {
    opened: Arc<OpenedWorkflowV6>,
    ticket: Arc<()>,
    revision: u64,
    preview: Option<Arc<WorkflowPreviewV6>>,
    input_visible: bool,
    confirmation_visible: bool,
    history: Option<Arc<WorkflowHistoryV6>>,
    owned_history: Option<Arc<crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6>>,
    // Display annotation only, issued by an actual admitted typed outcome.
    // Never serialized or consulted for publication/ACK/current-source checks.
    outcome_feedback_v25: Option<WorkflowOutcomeFeedbackV25>,
    pub(crate) inspect_requested: bool,
    pub(crate) ack_requested: bool,
}
impl std::fmt::Debug for WorkflowComposerStateV6 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowComposerStateV6(<actual workflow, not approval>)")
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkflowPhaseV6 {
    Preview,
    Confirm,
    Inspect,
    Acknowledge,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkflowOutcomeFeedbackV25 {
    Confirmed,
    AcceptedPending,
}
impl WorkflowOutcomeFeedbackV25 {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Confirmed => "HTTP204 accepted • exact same workflow state observed and durably recorded • causation not claimed • no retry",
            Self::AcceptedPending => "HTTP204 accepted • desired workflow state NOT observed • protected result retained • inspect, never resend",
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Confirmed => "Confirmed",
            Self::AcceptedPending => "Accepted; still waiting",
        }
    }
    pub(crate) fn token(self) -> crate::theme::SemanticToken {
        match self {
            Self::Confirmed => crate::theme::SemanticToken::Success,
            Self::AcceptedPending => crate::theme::SemanticToken::Pending,
        }
    }
}
pub(crate) fn account_caption_v25(account_id: Option<u64>) -> String {
    account_id
        .filter(|id| *id > 0)
        .map_or_else(|| "ID unavailable".into(), |id| format!("#{id}"))
}
/// The typed tag is issued ONLY by the native actual-result match. Exact text
/// equality merely detects a later unrelated local error, never creates success.
pub(crate) fn feedback_v25(form: &Form) -> Option<WorkflowOutcomeFeedbackV25> {
    let FormPurpose::ScopedActionsWorkflowV6(state) = &form.purpose else {
        return None;
    };
    let feedback = state.outcome_feedback_v25?;
    if form.submitting || form.error != feedback.message() {
        return None;
    }
    let reference = form.actions_publication.as_ref()?;
    state
        .owned_history
        .as_ref()
        .is_some_and(|history| &history.reference() == reference)
        .then_some(feedback)
}
pub(crate) fn result_heading_v42(form: &Form) -> &'static str {
    match feedback_v25(form) {
        Some(WorkflowOutcomeFeedbackV25::Confirmed) => {
            "Confirmed • exact workflow state observed; inspect original"
        }
        Some(WorkflowOutcomeFeedbackV25::AcceptedPending) => {
            "Accepted; state not yet observed • inspect original; never resend"
        }
        None => "Original workflow result • inspect only; never resend",
    }
}
pub(crate) struct WorkflowRequestV6 {
    opened: Arc<OpenedWorkflowV6>,
    ticket: Arc<()>,
    revision: u64,
    preview: Option<Arc<WorkflowPreviewV6>>,
    phase: WorkflowPhaseV6,
    reference: Option<crate::actions_write_journal::ActionsPublicationReference>,
    history: Option<Arc<WorkflowHistoryV6>>,
    owned_history: Option<Arc<crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6>>,
}
impl WorkflowComposerStateV6 {
    pub(crate) fn opened(&self) -> &Arc<OpenedWorkflowV6> {
        &self.opened
    }
    pub fn identity(&self) -> &crate::providers::gh::HostIdentity {
        self.opened.identity()
    }
    pub fn context(&self) -> String {
        format!(
            "@{} • {} • account {} • {} #{}",
            self.identity().login,
            self.identity().host,
            account_caption_v25(self.identity().account_id),
            self.opened.intent().command().label(),
            self.opened.target().model().id
        )
    }
    pub fn plan_lines(&self) -> &[String] {
        if let Some(history) = &self.history {
            return history.lines();
        }
        self.preview
            .as_ref()
            .map_or(self.opened.lines(), |preview| &preview.plan().preview)
    }
    pub(crate) fn acknowledgement_ready(&self) -> bool {
        self.input_visible
            && self
                .history
                .as_ref()
                .is_some_and(|history| history.acknowledge_available())
    }
    pub fn ready(&self) -> bool {
        self.input_visible
            && self.confirmation_visible
            && self.preview.as_ref().is_some_and(|p| p.available())
    }
    pub fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.preview = None;
        self.confirmation_visible = false;
        self.inspect_requested = false;
        self.ack_requested = false;
        self.outcome_feedback_v25 = None;
    }
    pub(crate) fn resize(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.input_visible = false;
        self.confirmation_visible = false;
        self.inspect_requested = false;
        self.ack_requested = false;
    }
}
impl WorkflowRequestV6 {
    pub(crate) fn opened(&self) -> &Arc<OpenedWorkflowV6> {
        &self.opened
    }
    pub(crate) fn preview(&self) -> Option<&Arc<WorkflowPreviewV6>> {
        self.preview.as_ref()
    }
    pub(crate) fn phase(&self) -> WorkflowPhaseV6 {
        self.phase
    }
    pub(crate) fn reference(
        &self,
    ) -> Option<&crate::actions_write_journal::ActionsPublicationReference> {
        self.reference.as_ref()
    }
    pub(crate) fn history(&self) -> Option<&Arc<WorkflowHistoryV6>> {
        self.history.as_ref()
    }
    pub(crate) fn owned_history(
        &self,
    ) -> Option<&Arc<crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6>> {
        self.owned_history.as_ref()
    }
    pub(crate) fn matches_form(&self, form: &Form) -> bool {
        matches!(&form.purpose,FormPurpose::ScopedActionsWorkflowV6(state)
        if Arc::ptr_eq(&state.ticket,&self.ticket)&&Arc::ptr_eq(&state.opened,&self.opened)&&state.revision==self.revision
            &&form.fields.is_empty()&&form.actions_publication==self.reference
            &&match(&state.history,&self.history){(Some(a),Some(b))=>Arc::ptr_eq(a,b),(None,None)=>true,_=>false}
            &&match(&state.owned_history,&self.owned_history){(Some(a),Some(b))=>Arc::ptr_eq(a,b),(None,None)=>true,_=>false}
            &&match(&state.preview,&self.preview){(Some(a),Some(b))=>Arc::ptr_eq(a,b),(None,None)=>true,_=>false})
    }
    pub(crate) fn owns_presentation(&self, form: &Form) -> bool {
        matches!(&form.purpose,FormPurpose::ScopedActionsWorkflowV6(state)
        if Arc::ptr_eq(&state.ticket,&self.ticket)&&Arc::ptr_eq(&state.opened,&self.opened))
    }
    pub(crate) fn retire_busy(&self, form: &mut Form) {
        if self.owns_presentation(form) {
            form.submitting = false;
            form.invalidate_plan();
        }
    }
}
pub(crate) fn form(opened: Arc<OpenedWorkflowV6>) -> Result<Form, String> {
    let mut form = opened.take_seed()?;
    form.title = format!(
        "{} • original whole workflow",
        opened.intent().command().label()
    );
    form.purpose = FormPurpose::ScopedActionsWorkflowV6(Box::new(WorkflowComposerStateV6 {
        opened: opened.clone(),
        ticket: Arc::new(()),
        revision: 0,
        preview: None,
        history: None,
        owned_history: None,
        outcome_feedback_v25: None,
        input_visible: false,
        confirmation_visible: false,
        inspect_requested: false,
        ack_requested: false,
    }));
    form.install_prepared_preview_display_v1(opened.display().clone());
    Ok(form)
}
pub(crate) fn capture(
    form: &Form,
    phase: WorkflowPhaseV6,
) -> Result<Arc<WorkflowRequestV6>, String> {
    let FormPurpose::ScopedActionsWorkflowV6(state) = &form.purpose else {
        return Err("Not original workflow Form".into());
    };
    if form.submitting || !state.input_visible || !form.fields.is_empty() {
        return Err("Inspect useful actual workflow Form before work".into());
    }
    if phase == WorkflowPhaseV6::Confirm
        && (!form.preview
            || !state.ready()
            || form.actions_publication.is_some()
            || form.preview_display_v1.geometry_rejected)
    {
        return Err("Fresh useful original workflow Preview required; no PUT".into());
    }
    if matches!(
        phase,
        WorkflowPhaseV6::Inspect | WorkflowPhaseV6::Acknowledge
    ) && form.actions_publication.is_none()
    {
        return Err("Select exact original protected workflow result".into());
    }
    Ok(Arc::new(WorkflowRequestV6 {
        opened: state.opened.clone(),
        ticket: state.ticket.clone(),
        revision: state.revision,
        preview: state.preview.clone(),
        phase,
        reference: form.actions_publication.clone(),
        history: state.history.clone(),
        owned_history: state.owned_history.clone(),
    }))
}
pub(crate) fn begin_draw(form: &mut Form, usable: bool) {
    if let FormPurpose::ScopedActionsWorkflowV6(state) = &mut form.purpose {
        state.input_visible = usable;
        state.confirmation_visible = false;
    }
}
pub(crate) fn painted_preview(form: &mut Form, useful: bool) {
    if let FormPurpose::ScopedActionsWorkflowV6(state) = &mut form.purpose {
        state.confirmation_visible =
            useful && state.input_visible && form.preview && state.preview.is_some();
    }
}
pub(crate) fn install_preview(
    form: &mut Form,
    request: &WorkflowRequestV6,
    preview: Arc<WorkflowPreviewV6>,
) -> bool {
    if !request.matches_form(form)
        || request.phase != WorkflowPhaseV6::Preview
        || !Arc::ptr_eq(request.opened(), preview.opened())
    {
        return false;
    }
    let display = preview.display().clone();
    let FormPurpose::ScopedActionsWorkflowV6(state) = &mut form.purpose else {
        return false;
    };
    state.preview = Some(preview);
    state.outcome_feedback_v25 = None;
    state.confirmation_visible = false;
    form.preview = true;
    form.focus = form.fields.len();
    form.submitting = false;
    form.preview_scroll = 0;
    form.error.clear();
    form.install_prepared_preview_display_v1(display);
    true
}
pub(crate) fn install_attempt(
    form: &mut Form,
    request: &WorkflowRequestV6,
    reference: crate::actions_write_journal::ActionsPublicationReference,
    message: String,
    feedback: Option<WorkflowOutcomeFeedbackV25>,
    history: Option<Arc<crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6>>,
) -> bool {
    if !request.matches_form(form) || request.phase != WorkflowPhaseV6::Confirm {
        return false;
    }
    let FormPurpose::ScopedActionsWorkflowV6(state) = &mut form.purpose else {
        return false;
    };
    if history
        .as_ref()
        .is_none_or(|proof| proof.reference() != reference)
    {
        return false;
    }
    state.owned_history = history;
    state.outcome_feedback_v25 = feedback;
    state.confirmation_visible = false;
    form.actions_publication = Some(reference);
    form.submitting = false;
    form.preview = true;
    form.preview_requested = false;
    form.submit_requested = false;
    form.title = "Original workflow publication • inspect only; no resend".into();
    form.error = message;
    true
}
pub(crate) fn install_history(
    form: &mut Form,
    request: &WorkflowRequestV6,
    history: Arc<WorkflowHistoryV6>,
) -> bool {
    if !request.matches_form(form)
        || !matches!(
            request.phase,
            WorkflowPhaseV6::Inspect | WorkflowPhaseV6::Acknowledge
        )
        || request.reference.as_ref() != Some(history.reference())
    {
        return false;
    }
    let display = history.display().clone();
    let FormPurpose::ScopedActionsWorkflowV6(state) = &mut form.purpose else {
        return false;
    };
    state.owned_history = history.owned_history().cloned();
    state.outcome_feedback_v25 = None;
    state.preview = None;
    state.history = Some(history);
    state.confirmation_visible = false;
    form.preview = true;
    form.submitting = false;
    form.error.clear();
    form.preview_scroll = 0;
    form.focus = form.fields.len();
    form.install_prepared_preview_display_v1(display);
    true
}
pub(crate) fn footer(form: &Form) -> Option<Vec<(&'static str, FormAction)>> {
    if !matches!(form.purpose, FormPurpose::ScopedActionsWorkflowV6(_)) {
        return None;
    }
    Some(if form.actions_publication.is_some() {
        vec![
            ("[View original]", FormAction::Submit),
            ("[F4 Acknowledge locally]", FormAction::AcknowledgeActionsV3),
            (
                if form.submitting {
                    "[Esc Cancel]"
                } else {
                    "[Keep draft / Close]"
                },
                FormAction::Close,
            ),
        ]
    } else {
        vec![
            ("[Preview]", FormAction::Preview),
            ("[Confirm action]", FormAction::Submit),
            (
                if form.submitting {
                    "[Esc Cancel]"
                } else {
                    "[Keep draft / Close]"
                },
                FormAction::Close,
            ),
        ]
    })
}
pub(crate) fn dispatch(form: &mut Form, action: &FormAction) -> bool {
    let FormPurpose::ScopedActionsWorkflowV6(state) = &mut form.purpose else {
        return false;
    };
    match action {
        FormAction::NewIntent | FormAction::Editor => {
            form.error="Workflow state has no editable body; protected history is never new approval. Close and reopen actual workflow.".into();
            true
        }
        FormAction::Submit if form.actions_publication.is_some() => {
            if state.input_visible && !form.submitting {
                state.inspect_requested = true;
                form.submit_requested = true;
            } else {
                form.error = "Resize/wait to inspect original workflow result".into();
            }
            true
        }
        FormAction::AcknowledgeActionsV3 => {
            if state.acknowledgement_ready()
                && !form.submitting
                && form.actions_publication.is_some()
                && !form.preview_display_v1.geometry_rejected
            {
                state.ack_requested = true;
                form.submit_requested = true;
            } else {
                form.error = "Inspect exact original workflow state before ACK".into();
            }
            true
        }
        FormAction::AcknowledgeActionsAcceptanceV5 => {
            form.error =
                "Workflow state needs an actual observed transition; no acceptance-only release"
                    .into();
            true
        }
        FormAction::Submit
            if !state.ready() || !form.preview || form.preview_display_v1.geometry_rejected =>
        {
            form.error = "Fresh useful exact workflow Preview required; nothing sent".into();
            true
        }
        FormAction::Preview if !state.input_visible || form.actions_publication.is_some() => {
            form.error = "Resize to inspect actual workflow; protected result is read only".into();
            true
        }
        _ => false,
    }
}
pub(crate) fn retire_invalid_form(app: &mut App) -> bool {
    if app.overlay != Some(Overlay::Form) || app.view != View::Actions {
        return false;
    }
    let invalid=app.form.as_ref().is_some_and(|form|matches!(&form.purpose,FormPurpose::ScopedActionsWorkflowV6(state)
        if !crate::scoped_actions_native_v1::publication_v3::workflow_v6::form_source_current(app,state.opened.intent())));
    if invalid {
        app.form = None;
        app.overlay = None;
        app.hits.clear();
        app.status = "Original workflow source changed • no old approval restored".into();
    }
    invalid
}
