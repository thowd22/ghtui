//! SAME shared Form controls over a genuine original Actions intent. No GH,
//! SQL, workspace/body clone or authority renewal in input/draw.
use crate::{
    app::{App, Overlay, View},
    forms::{Form, FormAction, FormPurpose},
    providers::gh::HostIdentity,
    scoped_actions_publication_v3::{
        OpenedActionsComposerV3, ScopedActionsHistoryV3, ScopedActionsPreviewV3, ScopedRunOptionsV4,
    },
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposerPhaseV3 {
    Preview,
    Confirm,
    Inspect,
    Acknowledge,
    AcknowledgeAcceptanceV5,
}

#[derive(Clone)]
pub struct ScopedActionsComposerStateV3 {
    opened: Arc<OpenedActionsComposerV3>,
    ticket: Arc<()>,
    revision: u64,
    preview: Option<Arc<ScopedActionsPreviewV3>>,
    history: Option<Arc<ScopedActionsHistoryV3>>,
    owned_history_v8: Option<Arc<crate::actions_write_journal::ActionsOwnedRunHistoryV8>>,
    input_visible: bool,
    confirmation_visible: bool,
    pub(crate) inspection_requested: bool,
    pub(crate) acknowledge_requested: bool,
    pub(crate) acceptance_requested_v5: bool,
}
impl std::fmt::Debug for ScopedActionsComposerStateV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedActionsComposerStateV3(<original display; not approval>)")
    }
}
pub struct ScopedActionsComposerRequestV3 {
    opened: Arc<OpenedActionsComposerV3>,
    ticket: Arc<()>,
    revision: u64,
    preview: Option<Arc<ScopedActionsPreviewV3>>,
    phase: ComposerPhaseV3,
    reference: Option<crate::actions_write_journal::ActionsPublicationReference>,
    history: Option<Arc<ScopedActionsHistoryV3>>,
    owned_history_v8: Option<Arc<crate::actions_write_journal::ActionsOwnedRunHistoryV8>>,
    options: ScopedRunOptionsV4,
}
fn options_v4(form: &Form) -> Result<ScopedRunOptionsV4, String> {
    let FormPurpose::ScopedActionsV3(state) = &form.purpose else {
        return Err("Not original Actions Form".into());
    };
    let command = state.opened.intent().command();
    if form.actions_publication.is_some()
        || matches!(
            command,
            crate::actions_workspace::ActionsCommand::Cancel
                | crate::actions_workspace::ActionsCommand::DeleteRun
                | crate::actions_workspace::ActionsCommand::DeleteLogs
        )
    {
        return if form.fields.is_empty() {
            Ok(Default::default())
        } else {
            Err("Protected/Cancel form has no editable fields".into())
        };
    }
    let [field] = form.fields.as_slice() else {
        return Err("Exact debug choice schema required".into());
    };
    let force = command == crate::actions_workspace::ActionsCommand::ForceCancel;
    if field.key != if force { "force_ack" } else { "debug" }
        || field.secret
        || field.multiline
        || field.choices != ["false", "true"]
    {
        return Err("Original debug schema changed; no source or approval granted".into());
    }
    let choice = match field.editor.text.as_str() {
        "false" => false,
        "true" => true,
        _ => return Err("Choose exact false/true debug value".into()),
    };
    Ok(ScopedRunOptionsV4 {
        debug_logging: !force && choice,
        force_acknowledged: force && choice,
    })
}
/// The existing generic field renderer paints exactly label/value/choices.
/// Reserve those SAME rows before painting the borrowed prepared intent, so
/// debug inputs cannot overwrite its actual target/repository. No new layout
/// grammar, source, matching or approval is produced here.
pub(crate) fn input_rows_v4(form: &Form) -> u16 {
    if !form.preview
        && form.actions_publication.is_none()
        && form.fields.len() == 1
        && options_v4(form).is_ok()
    {
        3
    } else {
        0
    }
}
impl std::fmt::Debug for ScopedActionsComposerRequestV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedActionsComposerRequestV3(<exact original Form revision>)")
    }
}
impl ScopedActionsComposerStateV3 {
    #[cfg(test)]
    pub(crate) fn prepared_preview(&self) -> Option<&Arc<ScopedActionsPreviewV3>> {
        self.preview.as_ref()
    }
    pub(crate) fn opened(&self) -> &Arc<OpenedActionsComposerV3> {
        &self.opened
    }
    pub fn identity(&self) -> &HostIdentity {
        self.opened.identity()
    }
    pub fn context(&self) -> String {
        let command =
            if self.opened.intent().command() == crate::actions_workspace::ActionsCommand::Cancel {
                "Cancel"
            } else {
                self.history
                    .as_ref()
                    .map_or(self.opened.intent().command().label(), |history| {
                        history.command_label_v4()
                    })
            };
        format!(
            "Original @{} / {} / numeric UID {:?} / {} run {} attempt {}",
            self.identity().login,
            self.identity().host,
            self.identity().account_id,
            command,
            self.opened.intent().original().run().id,
            self.opened.intent().original().run().attempt
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
    pub fn ready(&self) -> bool {
        self.input_visible
            && self.confirmation_visible
            && self
                .preview
                .as_ref()
                .is_some_and(|preview| preview.available())
    }
    pub fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.preview = None;
        self.confirmation_visible = false;
        self.inspection_requested = false;
        self.acknowledge_requested = false;
        self.acceptance_requested_v5 = false;
    }
    pub(crate) fn resize(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.input_visible = false;
        self.confirmation_visible = false;
        self.inspection_requested = false;
        self.acknowledge_requested = false;
        self.acceptance_requested_v5 = false;
    }
}
impl ScopedActionsComposerRequestV3 {
    pub(crate) fn options_v4(&self) -> ScopedRunOptionsV4 {
        self.options
    }
    pub(crate) fn opened(&self) -> &Arc<OpenedActionsComposerV3> {
        &self.opened
    }
    pub(crate) fn preview(&self) -> Option<&Arc<ScopedActionsPreviewV3>> {
        self.preview.as_ref()
    }
    pub(crate) fn phase(&self) -> ComposerPhaseV3 {
        self.phase
    }
    pub(crate) fn reference(
        &self,
    ) -> Option<&crate::actions_write_journal::ActionsPublicationReference> {
        self.reference.as_ref()
    }
    pub(crate) fn history(&self) -> Option<&Arc<ScopedActionsHistoryV3>> {
        self.history.as_ref()
    }
    pub(crate) fn owned_history_v8(
        &self,
    ) -> Option<&Arc<crate::actions_write_journal::ActionsOwnedRunHistoryV8>> {
        self.owned_history_v8.as_ref()
    }
    pub(crate) fn matches_form(&self, form: &Form) -> bool {
        matches!(&form.purpose,FormPurpose::ScopedActionsV3(state)
        if Arc::ptr_eq(&state.ticket,&self.ticket)
            &&Arc::ptr_eq(&state.opened,&self.opened)
            &&state.revision==self.revision
            &&options_v4(form).ok()==Some(self.options)
            &&form.actions_publication==self.reference
            &&match(&state.owned_history_v8,&self.owned_history_v8){(Some(a),Some(b))=>Arc::ptr_eq(a,b),(None,None)=>true,_=>false}
            &&match(&state.history,&self.history){(Some(a),Some(b))=>Arc::ptr_eq(a,b),(None,None)=>true,_=>false}
            &&match(&state.preview,&self.preview){
                (Some(a),Some(b))=>Arc::ptr_eq(a,b),(None,None)=>true,_=>false,
            })
    }
    pub(crate) fn owns_presentation(&self, form: &Form) -> bool {
        matches!(&form.purpose,FormPurpose::ScopedActionsV3(state)
            if Arc::ptr_eq(&state.ticket,&self.ticket)&&Arc::ptr_eq(&state.opened,&self.opened))
    }
    pub(crate) fn retire_busy(&self, form: &mut Form) {
        if self.owns_presentation(form) {
            form.submitting = false;
            form.invalidate_plan();
        }
    }
}
/// Worker-only constructor. The tiny workspace is freshly source-produced;
/// generic Form initialization is reused, then no workspace survives in state.
pub(crate) fn form(opened: Arc<OpenedActionsComposerV3>) -> Result<Form, String> {
    let mut form = opened.take_form_seed()?;
    form.title = if opened.intent().command() == crate::actions_workspace::ActionsCommand::Cancel {
        "Cancel original Actions run".into()
    } else {
        format!("{} • original Actions", opened.intent().command().label())
    };
    form.purpose = FormPurpose::ScopedActionsV3(Box::new(ScopedActionsComposerStateV3 {
        opened: opened.clone(),
        ticket: Arc::new(()),
        revision: 0,
        preview: None,
        history: opened.history().cloned(),
        owned_history_v8: None,
        input_visible: false,
        confirmation_visible: false,
        inspection_requested: false,
        acknowledge_requested: false,
        acceptance_requested_v5: false,
    }));
    form.install_prepared_preview_display_v1(opened.display().clone());
    if let Some(history) = opened.history() {
        form.title = format!(
            "Original {} publication • inspect only; no resend",
            history.command_label_v4()
        );
        form.actions_publication = Some(history.reference());
        form.preview = true;
    }
    Ok(form)
}
pub(crate) fn capture(
    form: &Form,
    phase: ComposerPhaseV3,
) -> Result<Arc<ScopedActionsComposerRequestV3>, String> {
    let FormPurpose::ScopedActionsV3(state) = &form.purpose else {
        return Err("Not the original Actions form".into());
    };
    let options = options_v4(form)?;
    if phase == ComposerPhaseV3::Preview
        && state.opened.intent().command() == crate::actions_workspace::ActionsCommand::ForceCancel
        && !options.force_acknowledged
    {
        return Err("Choose force_ack=true explicitly before Preview; normal Cancel is safer and existing barriers remain".into());
    }
    if form.submitting || !state.input_visible {
        return Err("Resize/wait to inspect the actual original Actions form".into());
    }
    if phase == ComposerPhaseV3::Confirm
        && (!form.preview
            || !state.ready()
            || form.actions_publication.is_some()
            || state
                .preview
                .as_ref()
                .is_none_or(|preview| preview.options() != options)
            || form.preview_display_v1.geometry_rejected)
    {
        return Err(
            "Fresh useful exact original Preview required; history/hidden rows grant no approval"
                .into(),
        );
    }
    if matches!(
        phase,
        ComposerPhaseV3::Inspect
            | ComposerPhaseV3::Acknowledge
            | ComposerPhaseV3::AcknowledgeAcceptanceV5
    ) && (form.actions_publication.is_none()
        || !form.preview
        || form.preview_display_v1.geometry_rejected)
    {
        return Err("No actual original protected Actions reference selected".into());
    }
    Ok(Arc::new(ScopedActionsComposerRequestV3 {
        opened: state.opened.clone(),
        ticket: state.ticket.clone(),
        revision: state.revision,
        preview: state.preview.clone(),
        phase,
        reference: form.actions_publication.clone(),
        history: state.history.clone(),
        owned_history_v8: state.owned_history_v8.clone(),
        options,
    }))
}
pub(crate) fn install_preview(
    form: &mut Form,
    request: &ScopedActionsComposerRequestV3,
    preview: Arc<ScopedActionsPreviewV3>,
) -> bool {
    if !request.matches_form(form)
        || request.phase != ComposerPhaseV3::Preview
        || !Arc::ptr_eq(request.opened(), preview.opened())
        || request.options != preview.options()
    {
        return false;
    }
    let display = preview.display().clone();
    let FormPurpose::ScopedActionsV3(state) = &mut form.purpose else {
        return false;
    };
    state.preview = Some(preview);
    state.confirmation_visible = false;
    form.preview = true;
    // Inputs are hidden in Preview. Focus the first actual visible control,
    // including entry via Ctrl-P from the sole debug field.
    form.focus = form.fields.len();
    form.submitting = false;
    form.preview_scroll = 0;
    form.error.clear();
    form.install_prepared_preview_display_v1(display);
    true
}
pub(crate) fn install_history(
    form: &mut Form,
    request: &ScopedActionsComposerRequestV3,
    history: Arc<ScopedActionsHistoryV3>,
) -> bool {
    if !request.matches_form(form)
        || !matches!(
            request.phase,
            ComposerPhaseV3::Inspect
                | ComposerPhaseV3::Acknowledge
                | ComposerPhaseV3::AcknowledgeAcceptanceV5
        )
    {
        return false;
    }
    let display = history.display().clone();
    let reference = history.reference();
    let FormPurpose::ScopedActionsV3(state) = &mut form.purpose else {
        return false;
    };
    state.preview = None;
    state.owned_history_v8 = history.owned_history_v8().cloned();
    state.history = Some(history);
    state.confirmation_visible = false;
    form.actions_publication = Some(reference);
    form.fields.clear();
    form.preview = true;
    form.submitting = false;
    form.error.clear();
    form.preview_scroll = 0;
    form.focus = form.fields.len();
    form.install_prepared_preview_display_v1(display);
    true
}
pub(crate) fn install_attempt(
    form: &mut Form,
    request: &ScopedActionsComposerRequestV3,
    reference: crate::actions_write_journal::ActionsPublicationReference,
    message: String,
    owned_history_v8: Option<Arc<crate::actions_write_journal::ActionsOwnedRunHistoryV8>>,
) -> bool {
    if !request.matches_form(form) || request.phase != ComposerPhaseV3::Confirm {
        return false;
    }
    if owned_history_v8.as_ref().is_none_or(|proof| {
        proof.reference() != reference
            || request
                .preview()
                .is_none_or(|preview| !proof.original().same_watch_pin_v2(preview.target()))
    }) {
        return false;
    }
    let FormPurpose::ScopedActionsV3(state) = &mut form.purpose else {
        return false;
    };
    state.confirmation_visible = false;
    state.owned_history_v8 = owned_history_v8;
    form.actions_publication = Some(reference);
    form.fields.clear();
    form.submitting = false;
    form.preview = true;
    form.submit_requested = false;
    form.preview_requested = false;
    form.title = if request.opened().intent().command()
        == crate::actions_workspace::ActionsCommand::Cancel
    {
        "Original Cancel publication • inspect only; no resend".into()
    } else {
        format!(
            "Original {} publication • inspect only; no resend",
            request.opened().intent().command().label()
        )
    };
    form.error = message;
    true
}
pub(crate) fn begin_draw(form: &mut Form, usable: bool) {
    crate::scoped_actions_dispatch_ui_v7::begin_draw(form, usable);
    crate::scoped_actions_workflow_ui_v6::begin_draw(form, usable);
    if let FormPurpose::ScopedActionsV3(state) = &mut form.purpose {
        state.input_visible = usable;
        state.confirmation_visible = false;
    }
}
pub(crate) fn painted_preview(form: &mut Form, useful: bool) {
    crate::scoped_actions_dispatch_ui_v7::painted_preview(form, useful);
    crate::scoped_actions_workflow_ui_v6::painted_preview(form, useful);
    if let FormPurpose::ScopedActionsV3(state) = &mut form.purpose {
        state.confirmation_visible =
            useful && state.input_visible && form.preview && state.preview.is_some();
    }
}
pub(crate) fn is_original(form: &Form) -> bool {
    matches!(
        form.purpose,
        FormPurpose::ScopedActionsV3(_)
            | FormPurpose::ScopedActionsWorkflowV6(_)
            | FormPurpose::ScopedActionsDispatchV7(_)
            | FormPurpose::ScopedWorkflowEditV9(_)
    )
}
pub(crate) fn acknowledgement_ready(form: &Form) -> bool {
    if matches!(form.purpose, FormPurpose::ScopedWorkflowEditV9(_)) {
        return crate::scoped_actions_workflow_edit_ui_v9::history_view_available_v9(form);
    }
    if let FormPurpose::ScopedActionsWorkflowV6(state) = &form.purpose {
        return state.acknowledgement_ready()
            && form.preview
            && !form.preview_display_v1.geometry_rejected
            && form.actions_publication.is_some();
    }
    matches!(&form.purpose,FormPurpose::ScopedActionsV3(state) if state.input_visible&&form.preview&&!form.preview_display_v1.geometry_rejected&&form.actions_publication.is_some()&&state.history.as_ref().is_some_and(|history|history.acknowledge_available()))
}
pub(crate) fn acceptance_ready_v5(form: &Form) -> bool {
    if matches!(form.purpose, FormPurpose::ScopedWorkflowEditV9(_)) {
        return crate::scoped_actions_workflow_edit_ui_v9::history_ack_ready_v9(form);
    }
    if let FormPurpose::ScopedActionsDispatchV7(state) = &form.purpose {
        return state.acceptance_ready()
            && form.preview
            && !form.preview_display_v1.geometry_rejected
            && form.actions_publication.is_some();
    }
    matches!(&form.purpose,FormPurpose::ScopedActionsV3(state) if state.input_visible&&form.preview
        &&!form.preview_display_v1.geometry_rejected&&form.actions_publication.is_some()
        &&state.history.as_ref().is_some_and(|history|history.acceptance_available_v5()))
}
pub(crate) fn footer(form: &Form) -> Option<Vec<(&'static str, FormAction)>> {
    if let Some(footer) = crate::scoped_actions_dispatch_ui_v7::footer(form) {
        return Some(footer);
    }
    if let Some(footer) = crate::scoped_actions_workflow_ui_v6::footer(form) {
        return Some(footer);
    }
    if !is_original(form) {
        return None;
    }
    Some(if form.actions_publication.is_some() {
        let mut controls = vec![
            ("[View original]", FormAction::Submit),
            ("[F4 Acknowledge]", FormAction::AcknowledgeActionsV3),
            (
                if form.submitting {
                    "[Esc Cancel]"
                } else {
                    "[Keep draft / Close]"
                },
                FormAction::Close,
            ),
        ];
        if matches!(&form.purpose,FormPurpose::ScopedActionsV3(state) if state.history.as_ref().is_some_and(|history|history.acceptance_available_v5()))
        {
            controls.insert(
                2,
                (
                    "[F6 ACK receipt]",
                    FormAction::AcknowledgeActionsAcceptanceV5,
                ),
            );
        }
        controls
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
    if crate::scoped_actions_dispatch_ui_v7::dispatch(form, action) {
        return true;
    }
    if crate::scoped_actions_workflow_ui_v6::dispatch(form, action) {
        return true;
    }
    let FormPurpose::ScopedActionsV3(state) = &mut form.purpose else {
        return false;
    };
    match action {
        FormAction::NewIntent => {
            form.error="Protected Actions reference is not new approval; Close and reopen the actual current run. Durable barriers remain.".into();
            true
        }
        FormAction::Editor => {
            form.error =
                "Original run commands have no editable body; use the actual debug choice. No external editor or saved text grants authority."
                    .into();
            true
        }
        FormAction::AcknowledgeActionsV3 => {
            if state.input_visible
                && form.actions_publication.is_some()
                && !form.submitting
                && form.preview
                && !form.preview_display_v1.geometry_rejected
                && state
                    .history
                    .as_ref()
                    .is_some_and(|history| history.acknowledge_available())
            {
                state.acknowledge_requested = true;
                form.submit_requested = true; // SAME existing before-poll foreign-intent fence.
            } else {
                form.error = "Inspect a current original result before acknowledgement".into();
            }
            true
        }
        FormAction::AcknowledgeActionsAcceptanceV5 => {
            if state.input_visible
                && form.actions_publication.is_some()
                && !form.submitting
                && form.preview
                && !form.preview_display_v1.geometry_rejected
                && state
                    .history
                    .as_ref()
                    .is_some_and(|history| history.acceptance_available_v5())
            {
                state.acceptance_requested_v5 = true;
                form.submit_requested = true;
            } else {
                form.error="Explicitly inspect the exact unobservable acceptance before F6; this never confirms deletion or releases an unknown write".into();
            }
            true
        }
        FormAction::Submit if form.actions_publication.is_some() => {
            if state.input_visible && !form.submitting {
                state.inspection_requested = true;
                form.submit_requested = true;
            } else {
                form.error = "Resize/wait to inspect the original protected Actions result".into();
            }
            true
        }
        FormAction::Submit
            if !state.ready() || !form.preview || form.preview_display_v1.geometry_rejected =>
        {
            form.error = "Fresh useful exact original Preview required; nothing sent".into();
            true
        }
        FormAction::Preview if !state.input_visible => {
            form.error =
                "Resize to inspect original Actions; hidden Preview grants no approval".into();
            true
        }
        FormAction::Preview if form.actions_publication.is_some() => {
            form.error = "Protected original is inspect only; no new Preview or retry".into();
            true
        }
        _ => false,
    }
}
pub fn retire_invalid_form(app: &mut App) -> bool {
    if crate::scoped_actions_dispatch_ui_v7::retire_invalid_form(app) {
        return true;
    }
    if crate::scoped_actions_workflow_ui_v6::retire_invalid_form(app) {
        return true;
    }
    if app.overlay != Some(Overlay::Form) || app.view != View::Actions {
        return false;
    }
    let original = app.form.as_ref().and_then(|form| {
        if let FormPurpose::ScopedActionsV3(state) = &form.purpose {
            Some(state.opened.intent().clone())
        } else {
            None
        }
    });
    if original
        .as_ref()
        .is_some_and(|intent| !crate::scoped_actions_native_v1::form_source_current_v3(app, intent))
    {
        // Only THIS actually active old Form, never a newer overlay/hit set.
        app.form = None;
        app.overlay = None;
        app.hits.clear();
        app.status =
            "Original Actions form source changed; no request or old approval restored".into();
        return true;
    }
    false
}
