//! Dispatch is a closed purpose in the ONE shared Form renderer/input path.
use crate::{
    app::{App, Overlay, View},
    forms::{Field, Form, FormAction, FormPurpose},
    providers::gh::*,
    scoped_actions_dispatch_publication_v7::{
        DispatchEditableSummaryV152, DispatchPreviewV7, OpenedDispatchV7, PreparedDispatchSchemaV7,
    },
};
use std::sync::Arc;
#[derive(Clone)]
pub struct DispatchComposerStateV7 {
    opened: Arc<OpenedDispatchV7>,
    ticket: Arc<()>,
    revision: u64,
    schema: Option<Arc<ScopedDispatchSchemaV7>>,
    summary_v152: Option<Arc<DispatchEditableSummaryV152>>,
    preview: Option<Arc<DispatchPreviewV7>>,
    lines: Arc<[String]>,
    input_visible: bool,
    confirmation_visible: bool,
    history: Option<Arc<crate::scoped_actions_dispatch_history_v7::DispatchHistoryV7>>,
    owned_history: Option<Arc<crate::actions_write_journal::ActionsOwnedDispatchHistoryV7>>,
    pub(crate) inspect_requested: bool,
    pub(crate) ack_requested: bool,
    pub(crate) open_run_requested: bool,
    created_run: Option<Arc<ScopedActionsRunTargetV1>>,
    // Display only. Never serialized or consulted for permission/ACK/source.
    outcome_feedback_v33: Option<DispatchOutcomeFeedbackV33>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DispatchOutcomeFeedbackV33 {
    AcceptedWithoutRun,
    AcceptedWithRun,
}
impl DispatchOutcomeFeedbackV33 {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::AcceptedWithoutRun => "HTTP204 accepted • no actual run ID • NOT completion • inspect; no resend",
            Self::AcceptedWithRun => "Dispatch accepted • actual returned run observed • NOT workflow completion or causation",
        }
    }
    pub(crate) fn label(self) -> &'static str {
        "Accepted; completion unknown"
    }
    pub(crate) fn token(self) -> crate::theme::SemanticToken {
        crate::theme::SemanticToken::Pending
    }
}
/// The native typed result is the sole issuer. Exact text detects a later
/// unrelated error; HTTP-looking text can never issue an annotation.
pub(crate) fn feedback_v33(form: &Form) -> Option<DispatchOutcomeFeedbackV33> {
    let FormPurpose::ScopedActionsDispatchV7(state) = &form.purpose else {
        return None;
    };
    let feedback = state.outcome_feedback_v33?;
    if form.submitting || form.error != feedback.message() {
        return None;
    }
    let history = state.owned_history.as_ref()?;
    if form.actions_publication.as_ref() != Some(&history.reference()) {
        return None;
    }
    match (feedback, &state.created_run) {
        (DispatchOutcomeFeedbackV33::AcceptedWithoutRun, None) => Some(feedback),
        (DispatchOutcomeFeedbackV33::AcceptedWithRun, Some(run))
            if history.matches_returned_run_v7(run) =>
        {
            Some(feedback)
        }
        _ => None,
    }
}
/// Display only: the same admitted result annotation is not a new permission.
pub(crate) fn result_heading_v42(form: &Form) -> &'static str {
    if feedback_v33(form).is_some() {
        "Accepted; completion unknown • inspect original; never resend"
    } else {
        "Original request result • inspect only; never resend"
    }
}
fn returned_run_display_proof_v42(form: &Form) -> bool {
    let FormPurpose::ScopedActionsDispatchV7(state) = &form.purpose else {
        return false;
    };
    match (
        &state.created_run,
        &state.owned_history,
        &form.actions_publication,
    ) {
        (Some(run), Some(history), Some(reference)) => {
            history.reference() == *reference && history.matches_returned_run_v7(run)
        }
        _ => false,
    }
}
/// Styling only. Actual input/capture/worker guards remain independently required.
pub(crate) fn run_display_ready_v42(form: &Form) -> bool {
    matches!(&form.purpose, FormPurpose::ScopedActionsDispatchV7(state) if state.input_visible)
        && form.preview
        && !form.submitting
        && !form.preview_display_v1.geometry_rejected
        && returned_run_display_proof_v42(form)
}
pub(crate) fn run_display_label_v42(form: &Form) -> &'static str {
    if returned_run_display_proof_v42(form) {
        "[F11 Open run]"
    } else {
        "[F11 Run unavailable]"
    }
}
impl std::fmt::Debug for DispatchComposerStateV7 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DispatchComposerStateV7(<schema/values redacted>)")
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DispatchPhaseV7 {
    OpenRun,
    Schema,
    Preview,
    Confirm,
    Inspect,
    Acknowledge,
}
pub(crate) struct DispatchRequestV7 {
    opened: Arc<OpenedDispatchV7>,
    ticket: Arc<()>,
    revision: u64,
    schema: Option<Arc<ScopedDispatchSchemaV7>>,
    summary_v152: Option<Arc<DispatchEditableSummaryV152>>,
    form_bytes_v152: usize,
    preview: Option<Arc<DispatchPreviewV7>>,
    inputs: Option<Arc<CapturedDispatchInputsV7>>,
    choice: DispatchRefChoiceV7,
    protocol: DispatchProtocolV7,
    phase: DispatchPhaseV7,
    reference: Option<crate::actions_write_journal::ActionsPublicationReference>,
    history: Option<Arc<crate::scoped_actions_dispatch_history_v7::DispatchHistoryV7>>,
    owned_history: Option<Arc<crate::actions_write_journal::ActionsOwnedDispatchHistoryV7>>,
    created_run: Option<Arc<ScopedActionsRunTargetV1>>,
}
fn text<'a>(fields: &'a [Field], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|field| field.key == key)
        .map(|field| field.editor.text.as_str())
}
fn choice(fields: &[Field]) -> Result<(DispatchRefChoiceV7, DispatchProtocolV7), String> {
    let kind = match text(fields, "namespace") {
        Some("branch") => RemoteReferenceKindV1::Branch,
        Some("tag") => RemoteReferenceKindV1::Tag,
        _ => return Err("Choose exact branch/tag namespace".into()),
    };
    let reference =
        DispatchRefChoiceV7::new(kind, text(fields, "ref").ok_or("Literal ref missing")?)
            .map_err(|_| "Choose a supported bounded literal branch/tag name")?;
    let protocol = match text(fields, "protocol") {
        Some("2022-11-28") => DispatchProtocolV7::Compatible2022Acceptance204,
        Some("2026-03-10") => DispatchProtocolV7::Detailed2026Run200,
        _ => return Err("Choose an explicit supported API protocol".into()),
    };
    Ok((reference, protocol))
}
fn input_rows<'a>(
    fields: &'a [Field],
    schema: &'a ScopedDispatchSchemaV7,
) -> impl Iterator<Item = (&'a str, &'a str, &'a str)> {
    schema
        .definition()
        .dispatch_inputs
        .iter()
        .map(move |input| {
            let mode = fields
                .iter()
                .find(|field| field.key.strip_prefix("mode:") == Some(input.name.as_str()))
                .map_or("", |field| field.editor.text.as_str());
            let value = if mode == "set" {
                fields
                    .iter()
                    .find(|field| field.key.strip_prefix("input:") == Some(input.name.as_str()))
                    .map_or("", |field| field.editor.text.as_str())
            } else {
                ""
            };
            (input.name.as_str(), mode, value)
        })
}
fn base_matches(
    fields: &[Field],
    reference: &DispatchRefChoiceV7,
    protocol: DispatchProtocolV7,
) -> bool {
    text(fields, "namespace") == Some(reference.label())
        && text(fields, "ref") == Some(reference.name())
        && text(fields, "protocol") == Some(protocol.version())
}
/// Bound incoming raw editor bytes before the shared editor clones/inserts.
/// This is input presentation only, never schema/source or write authority.
pub(crate) fn validate_input(form: &Form, incoming: &str) -> Result<(), String> {
    if !matches!(form.purpose, FormPurpose::ScopedActionsDispatchV7(_)) {
        return Ok(());
    }
    if form.submitting || form.preview || form.actions_publication.is_some() {
        return Err("Original Dispatch is read-only/busy; values are not copied".into());
    }
    let Some(field) = form.fields.get(form.focus) else {
        return Ok(());
    };
    let limit = if field.secret {
        16 * 1024
    } else if field.key == "ref" {
        1024
    } else {
        256
    };
    let removed = field.editor.selection().map_or(0, |range| range.len());
    if field
        .editor
        .text
        .len()
        .saturating_sub(removed)
        .checked_add(incoming.len())
        .is_none_or(|bytes| bytes > limit)
    {
        return Err("Dispatch input exceeds its byte limit; value hidden and not copied".into());
    }
    Ok(())
}
impl DispatchComposerStateV7 {
    pub(crate) fn opened(&self) -> &Arc<OpenedDispatchV7> {
        &self.opened
    }
    #[cfg(test)]
    pub(crate) fn schema(&self) -> Option<&Arc<ScopedDispatchSchemaV7>> {
        self.schema.as_ref()
    }
    pub fn identity(&self) -> &HostIdentity {
        self.opened.identity()
    }
    pub fn context(&self) -> String {
        format!(
            "@{} • {} • account {} • workflow #{}",
            self.identity().login,
            self.identity().host,
            crate::scoped_actions_workflow_ui_v6::account_caption_v25(self.identity().account_id),
            self.opened.intent().original().model().id
        )
    }
    pub fn ready(&self) -> bool {
        self.input_visible
            && self.confirmation_visible
            && self
                .preview
                .as_ref()
                .is_some_and(|preview| preview.available())
    }
    pub(crate) fn acceptance_ready(&self) -> bool {
        self.input_visible
            && self
                .history
                .as_ref()
                .is_some_and(|history| history.acknowledge_available())
    }
    pub fn plan_lines(&self) -> &[String] {
        if let Some(history) = &self.history {
            history.lines()
        } else if let Some(preview) = &self.preview {
            &preview.plan().preview
        } else if !self.lines.is_empty() {
            &self.lines
        } else {
            self.opened.lines()
        }
    }
    pub fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.preview = None;
        self.confirmation_visible = false;
        self.inspect_requested = false;
        self.ack_requested = false;
        self.open_run_requested = false;
        self.outcome_feedback_v33 = None;
    }
    pub(crate) fn resize(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.input_visible = false;
        self.confirmation_visible = false;
        self.inspect_requested = false;
        self.ack_requested = false;
        self.open_run_requested = false;
    }
    pub(crate) fn next_preview_phase(&self, fields: &[Field]) -> DispatchPhaseV7 {
        if self
            .schema
            .as_ref()
            .is_some_and(|schema| base_matches(fields, schema.choice(), schema.protocol()))
        {
            DispatchPhaseV7::Preview
        } else {
            DispatchPhaseV7::Schema
        }
    }
}
impl DispatchRequestV7 {
    pub(crate) fn summary_v152(&self) -> Option<&Arc<DispatchEditableSummaryV152>> {
        self.summary_v152.as_ref()
    }
    pub(crate) fn form_bytes_v152(&self) -> usize {
        self.form_bytes_v152
    }
    pub(crate) fn created_run(&self) -> Option<&Arc<ScopedActionsRunTargetV1>> {
        self.created_run.as_ref()
    }
    pub(crate) fn opened(&self) -> &Arc<OpenedDispatchV7> {
        &self.opened
    }
    pub(crate) fn schema(&self) -> Option<&Arc<ScopedDispatchSchemaV7>> {
        self.schema.as_ref()
    }
    pub(crate) fn preview(&self) -> Option<&Arc<DispatchPreviewV7>> {
        self.preview.as_ref()
    }
    pub(crate) fn inputs(&self) -> Option<&Arc<CapturedDispatchInputsV7>> {
        self.inputs.as_ref()
    }
    pub(crate) fn choice(&self) -> &DispatchRefChoiceV7 {
        &self.choice
    }
    pub(crate) fn protocol(&self) -> DispatchProtocolV7 {
        self.protocol
    }
    pub(crate) fn phase(&self) -> DispatchPhaseV7 {
        self.phase
    }
    pub(crate) fn reference(
        &self,
    ) -> Option<&crate::actions_write_journal::ActionsPublicationReference> {
        self.reference.as_ref()
    }
    pub(crate) fn history(
        &self,
    ) -> Option<&Arc<crate::scoped_actions_dispatch_history_v7::DispatchHistoryV7>> {
        self.history.as_ref()
    }
    pub(crate) fn owned_history(
        &self,
    ) -> Option<&Arc<crate::actions_write_journal::ActionsOwnedDispatchHistoryV7>> {
        self.owned_history.as_ref()
    }
    pub(crate) fn owns_presentation(&self, form: &Form) -> bool {
        matches!(&form.purpose,FormPurpose::ScopedActionsDispatchV7(state)if Arc::ptr_eq(&state.ticket,&self.ticket)&&Arc::ptr_eq(&state.opened,&self.opened))
    }
    pub(crate) fn matches_form(&self, form: &Form) -> bool {
        let FormPurpose::ScopedActionsDispatchV7(state) = &form.purpose else {
            return false;
        };
        self.owns_presentation(form)
            && state.revision == self.revision
            && base_matches(&form.fields, &self.choice, self.protocol)
            && form.actions_publication == self.reference
            && match (&state.summary_v152, &self.summary_v152) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && match (&state.created_run, &self.created_run) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && match (&state.schema, &self.schema) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && match (&state.preview, &self.preview) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && match (&state.history, &self.history) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && match (&state.owned_history, &self.owned_history) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && self.inputs.as_ref().is_none_or(|inputs| {
                self.schema.as_ref().is_some_and(|schema| {
                    form.fields.len() == 3 + 2 * schema.definition().dispatch_inputs.len()
                        && inputs.matches_borrowed_v7(input_rows(&form.fields, schema))
                })
            })
    }
    pub(crate) fn retire_busy(&self, form: &mut Form) {
        if self.owns_presentation(form) {
            form.submitting = false;
            form.invalidate_plan();
        }
    }
}
pub(crate) fn form(opened: Arc<OpenedDispatchV7>) -> Result<Form, String> {
    let mut form = opened.take_seed()?;
    form.title = "Dispatch original workflow • inspect schema first".into();
    form.purpose = FormPurpose::ScopedActionsDispatchV7(Box::new(DispatchComposerStateV7 {
        opened: opened.clone(),
        ticket: Arc::new(()),
        revision: 0,
        schema: None,
        summary_v152: None,
        preview: None,
        lines: Arc::from([]),
        input_visible: false,
        confirmation_visible: false,
        history: None,
        owned_history: None,
        inspect_requested: false,
        ack_requested: false,
        open_run_requested: false,
        created_run: None,
        outcome_feedback_v33: None,
    }));
    form.install_prepared_preview_display_v1(opened.display().clone());
    Ok(form)
}
pub(crate) fn capture(
    form: &Form,
    phase: DispatchPhaseV7,
) -> Result<Arc<DispatchRequestV7>, String> {
    let FormPurpose::ScopedActionsDispatchV7(state) = &form.purpose else {
        return Err("Not an original Dispatch Form".into());
    };
    if form.submitting || !state.input_visible {
        return Err("Inspect useful actual Dispatch Form before work".into());
    }
    if matches!(phase, DispatchPhaseV7::Schema | DispatchPhaseV7::Preview)
        && (form.actions_publication.is_some()
            || state.history.is_some()
            || state.owned_history.is_some())
    {
        return Err(
            "Protected Dispatch history is inspect-only; no editable schema or Preview".into(),
        );
    }
    let (choice, protocol) = choice(&form.fields)?;
    if phase == DispatchPhaseV7::Confirm
        && (!form.preview
            || !state.ready()
            || form.actions_publication.is_some()
            || form.preview_display_v1.geometry_rejected)
    {
        return Err("Fresh useful exact Dispatch Preview required; no POST".into());
    }
    if matches!(
        phase,
        DispatchPhaseV7::Inspect | DispatchPhaseV7::Acknowledge | DispatchPhaseV7::OpenRun
    ) && form.actions_publication.is_none()
    {
        return Err("Exact protected original Dispatch result required".into());
    }
    if phase == DispatchPhaseV7::OpenRun
        && (!form.preview
            || form.preview_display_v1.geometry_rejected
            || state.created_run.is_none()
            || state.owned_history.is_none())
    {
        return Err(
            "Actual POST200 returned Run and exact own-result proof required; no guessed204 target"
                .into(),
        );
    }
    let form_bytes_v152 = crate::scoped_actions_dispatch_publication_v7::field_bytes(&form.fields)
        .checked_add(std::mem::size_of::<Form>() + form.title.capacity() + form.error.capacity())
        .and_then(|bytes| {
            bytes.checked_add(
                form.field_rects.capacity() * std::mem::size_of::<ratatui::layout::Rect>(),
            )
        })
        .ok_or("Dispatch actual presentation capacity overflow")?;
    let inputs = if matches!(phase, DispatchPhaseV7::Preview | DispatchPhaseV7::Confirm) {
        let schema = state.schema.as_ref().ok_or("Inspect exact schema first")?;
        let summary = state
            .summary_v152
            .as_ref()
            .filter(|summary| Arc::ptr_eq(summary.schema(), schema))
            .ok_or("Actual same-schema prepared summary missing; inspect schema again")?;
        if !base_matches(&form.fields, schema.choice(), schema.protocol())
            || form.fields.len() != 3 + 2 * schema.definition().dispatch_inputs.len()
        {
            return Err("Ref/protocol/schema changed; inspect schema again".into());
        }
        let old_preview = state
            .preview
            .as_ref()
            .map_or(Ok(0), |preview| preview.new_material_bytes_v152())
            .map_err(|_| "Original Preview allocation accounting unavailable")?;
        let original_retained = state
            .opened
            .retained()
            .checked_add(summary.retained_bytes())
            .and_then(|bytes| bytes.checked_add(form_bytes_v152))
            .and_then(|bytes| bytes.checked_add(old_preview))
            .ok_or("Dispatch original presentation capacity overflow")?;
        // The existing input capture is <=65,535 encoded bytes + <=25 rows;
        // 128KiB is a conservative BEFORE-clone allocation envelope, not a
        // larger payload/source limit. Actual input capacities are checked too.
        crate::scoped_actions_dispatch_publication_v7::checked_capture_capacity_v152(
            original_retained,
            128 * 1024,
        )
        .map_err(|_| "Dispatch original presentation exceeds unchanged capacity")?;
        let inputs = CapturedDispatchInputsV7::capture(input_rows(&form.fields, schema))
            .map_err(|_| "Inputs exceed bounds or schema; values hidden")?;
        crate::scoped_actions_dispatch_publication_v7::checked_capture_capacity_v152(
            original_retained,
            inputs.retained_bytes()
                + std::mem::size_of::<DispatchRequestV7>()
                + 4 * std::mem::size_of::<usize>(),
        )
        .map_err(|_| "Dispatch actual captured presentation exceeds unchanged capacity")?;
        Some(Arc::new(inputs))
    } else {
        None
    };
    if phase == DispatchPhaseV7::Confirm
        && state.preview.as_ref().is_none_or(|preview| {
            inputs
                .as_ref()
                .is_none_or(|inputs| !preview.inputs().same(inputs))
        })
    {
        return Err("Exact Preview inputs changed; no POST".into());
    }
    Ok(Arc::new(DispatchRequestV7 {
        opened: state.opened.clone(),
        ticket: state.ticket.clone(),
        revision: state.revision,
        schema: state.schema.clone(),
        summary_v152: state.summary_v152.clone(),
        form_bytes_v152,
        preview: state.preview.clone(),
        inputs,
        choice,
        protocol,
        phase,
        reference: form.actions_publication.clone(),
        history: state.history.clone(),
        owned_history: state.owned_history.clone(),
        created_run: state.created_run.clone(),
    }))
}
/// Called only AFTER shared invalidation has retired approval and bumped the
/// request revision. Restores the actual worker-issued editable SchemaDoc,
/// never a full Preview, source lease, useful-read marker or confirmation.
pub(crate) fn restore_editable_summary_v152(form: &mut Form) {
    if form.submitting || form.preview || form.actions_publication.is_some() {
        return;
    }
    let FormPurpose::ScopedActionsDispatchV7(state) = &mut form.purpose else {
        return;
    };
    if state.preview.is_some() || state.history.is_some() || state.owned_history.is_some() {
        return;
    }
    let same = match (&state.schema, &state.summary_v152) {
        (Some(schema), Some(summary)) => {
            Arc::ptr_eq(schema, summary.schema())
                && base_matches(&form.fields, schema.choice(), schema.protocol())
                && form.fields.len() == 3 + 2 * schema.definition().dispatch_inputs.len()
        }
        _ => false,
    };
    if !same {
        state.schema = None;
        state.summary_v152 = None;
        state.lines = Arc::from([]);
        return;
    }
    let display = state
        .summary_v152
        .as_ref()
        .expect("Same actual schema pair checked")
        .display()
        .clone();
    form.install_prepared_preview_display_v1(display);
}
pub(crate) fn begin_draw(form: &mut Form, usable: bool) {
    if let FormPurpose::ScopedActionsDispatchV7(state) = &mut form.purpose {
        state.input_visible = usable;
        state.confirmation_visible = false;
    }
}
pub(crate) fn painted_preview(form: &mut Form, useful: bool) {
    if let FormPurpose::ScopedActionsDispatchV7(state) = &mut form.purpose {
        state.confirmation_visible =
            useful && state.input_visible && form.preview && state.preview.is_some();
    }
}
pub(crate) fn install_schema(
    form: &mut Form,
    request: &DispatchRequestV7,
    prepared: PreparedDispatchSchemaV7,
) -> bool {
    if !request.matches_form(form)
        || request.phase != DispatchPhaseV7::Schema
        || !Arc::ptr_eq(request.opened(), prepared.opened())
    {
        return false;
    }
    let schema = prepared.schema().clone();
    let summary = prepared.summary().clone();
    let display = prepared.display().clone();
    let lines = prepared.lines().clone();
    let fields = prepared.take_fields();
    let FormPurpose::ScopedActionsDispatchV7(state) = &mut form.purpose else {
        return false;
    };
    state.schema = Some(schema);
    state.summary_v152 = Some(summary);
    state.outcome_feedback_v33 = None;
    state.preview = None;
    state.lines = lines;
    state.revision = state.revision.wrapping_add(1);
    state.confirmation_visible = false;
    form.fields = fields;
    form.preview = false;
    form.submitting = false;
    form.preview_requested = false;
    form.submit_requested = false;
    form.focus = 0;
    form.active_field = 0;
    form.field_top = 0;
    form.error.clear();
    form.install_prepared_preview_display_v1(display);
    true
}
pub(crate) fn install_preview(
    form: &mut Form,
    request: &DispatchRequestV7,
    preview: Arc<DispatchPreviewV7>,
) -> bool {
    if !request.matches_form(form)
        || request.phase != DispatchPhaseV7::Preview
        || !Arc::ptr_eq(request.opened(), preview.opened())
        || request
            .inputs()
            .is_none_or(|inputs| !inputs.same(preview.inputs()))
    {
        return false;
    }
    if request
        .summary_v152()
        .is_none_or(|summary| !Arc::ptr_eq(summary, preview.summary()))
    {
        return false;
    }
    let display = preview.display().clone();
    let FormPurpose::ScopedActionsDispatchV7(state) = &mut form.purpose else {
        return false;
    };
    // Fresh schema remains private in Preview; fields are exactly equivalent.
    state.preview = Some(preview);
    state.outcome_feedback_v33 = None;
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
    request: &DispatchRequestV7,
    reference: crate::actions_write_journal::ActionsPublicationReference,
    message: String,
    history: Option<Arc<crate::actions_write_journal::ActionsOwnedDispatchHistoryV7>>,
    run: Option<Arc<ScopedActionsRunTargetV1>>,
    feedback: Option<DispatchOutcomeFeedbackV33>,
) -> bool {
    if !request.matches_form(form)
        || request.phase != DispatchPhaseV7::Confirm
        || history
            .as_ref()
            .is_none_or(|proof| proof.reference() != reference)
    {
        return false;
    }
    let FormPurpose::ScopedActionsDispatchV7(state) = &mut form.purpose else {
        return false;
    };
    state.owned_history = history;
    state.created_run = run;
    state.outcome_feedback_v33 = feedback;
    state.confirmation_visible = false;
    state.preview = None;
    state.schema = None;
    for field in &mut form.fields {
        if field.secret {
            field.editor.clear();
        }
    }
    form.actions_publication = Some(reference);
    form.submitting = false;
    form.preview = true;
    form.preview_requested = false;
    form.submit_requested = false;
    form.title = "Original Dispatch publication • inspect only; no resend".into();
    form.error = message;
    true
}
pub(crate) fn install_history(
    form: &mut Form,
    request: &DispatchRequestV7,
    history: Arc<crate::scoped_actions_dispatch_history_v7::DispatchHistoryV7>,
) -> bool {
    if !request.matches_form(form)
        || !matches!(
            request.phase,
            DispatchPhaseV7::Inspect | DispatchPhaseV7::Acknowledge
        )
        || request.reference() != Some(history.reference())
    {
        return false;
    }
    let display = history.display().clone();
    let FormPurpose::ScopedActionsDispatchV7(state) = &mut form.purpose else {
        return false;
    };
    state.owned_history = history.owned_history().cloned();
    state.outcome_feedback_v33 = None;
    state.history = Some(history);
    state.preview = None;
    state.confirmation_visible = false;
    form.preview = true;
    form.submitting = false;
    form.error.clear();
    form.preview_scroll = 0;
    form.install_prepared_preview_display_v1(display);
    true
}
pub(crate) fn footer(form: &Form) -> Option<Vec<(&'static str, FormAction)>> {
    let FormPurpose::ScopedActionsDispatchV7(state) = &form.purpose else {
        return None;
    };
    Some(if form.actions_publication.is_some() {
        vec![
            ("[View original]", FormAction::Submit),
            (
                "[F6 Acknowledge locally]",
                FormAction::AcknowledgeActionsAcceptanceV5,
            ),
            (run_display_label_v42(form), FormAction::OpenDispatchRunV7),
            ("[Close]", FormAction::Close),
        ]
    } else {
        vec![
            (
                if state.schema.as_ref().is_some_and(|schema| {
                    base_matches(&form.fields, schema.choice(), schema.protocol())
                }) {
                    "[Preview]"
                } else {
                    "[Inspect schema]"
                },
                FormAction::Preview,
            ),
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
    let FormPurpose::ScopedActionsDispatchV7(state) = &mut form.purpose else {
        return false;
    };
    match action {
        FormAction::Preview
            if form.preview && form.actions_publication.is_none() && !form.submitting =>
        {
            // Explicit local Back retires full Preview/approval and restores
            // only the exact worker-issued editable schema summary.
            form.invalidate_plan();
            true
        }
        FormAction::OpenDispatchRunV7 => {
            if state.input_visible
                && !form.submitting
                && form.actions_publication.is_some()
                && state.created_run.is_some()
                && state.owned_history.is_some()
            {
                state.open_run_requested = true;
                form.submit_requested = true;
            } else {
                form.error="No actual current POST200 returned-run proof; HTTP204/unknown outcomes cannot open a guessed matching run".into();
            }
            true
        }
        FormAction::Editor | FormAction::NewIntent => {
            form.error="Dispatch parameters are transient; no external editor, saved values, automatic retry or new intent from protected history".into();
            true
        }
        FormAction::Submit if form.actions_publication.is_some() => {
            if state.input_visible && !form.submitting {
                state.inspect_requested = true;
                form.submit_requested = true;
            } else {
                form.error = "Resize/wait to inspect original Dispatch".into();
            }
            true
        }
        FormAction::AcknowledgeActionsAcceptanceV5 => {
            if state.input_visible
                && !form.submitting
                && state
                    .history
                    .as_ref()
                    .is_some_and(|history| history.acknowledge_available())
                && !form.preview_display_v1.geometry_rejected
            {
                state.ack_requested = true;
                form.submit_requested = true;
            } else {
                form.error = "Inspect exact accepted Dispatch before local acceptance ACK".into();
            }
            true
        }
        FormAction::AcknowledgeActionsV3 => {
            form.error="Dispatch acceptance is not workflow completion; inspect and use explicit acceptance-only ACK".into();
            true
        }
        FormAction::Submit
            if !state.ready() || !form.preview || form.preview_display_v1.geometry_rejected =>
        {
            form.error = "Fresh useful exact Dispatch Preview required; nothing sent".into();
            true
        }
        FormAction::Preview if !state.input_visible || form.actions_publication.is_some() => {
            form.error = "Resize/use actual Dispatch; protected original is inspect only".into();
            true
        }
        _ => false,
    }
}
pub(crate) fn retire_invalid_form(app: &mut App) -> bool {
    if app.overlay != Some(Overlay::Form) || app.view != View::Actions {
        return false;
    }
    let invalid=app.form.as_ref().is_some_and(|form|matches!(&form.purpose,FormPurpose::ScopedActionsDispatchV7(state)if !crate::scoped_actions_native_v1::publication_v3::dispatch_v7::form_source_current(app,state.opened.intent())));
    if invalid {
        app.form = None;
        app.overlay = None;
        app.hits.clear();
        app.status = "Original Dispatch source changed • no old approval or values restored".into();
    }
    invalid
}
