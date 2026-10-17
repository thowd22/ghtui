//! One closed purpose in the existing Form. Editable recovery text and local
//! receipts never manufacture an Actions source or remote-write approval.
use crate::{
    forms::{Field, Form, FormAction, FormPurpose},
    prepared_form_text_v1::PreparedFormTextV1,
    scoped_actions_workflow_edit_v9::{
        draft_v9::WorkflowDraftTextV9, OpenedWorkflowLocalEditV9, WorkflowCommittedLocalV9,
        WorkflowEditIntentV9, WorkflowEditValuesV9, WorkflowLocalPlanPreviewV9,
        WorkflowSavePreviewV9, WorkflowSavedLocalV9, WorkflowStagedLocalV9,
        WorkflowWorktreePreviewV9,
    },
};
use std::sync::Arc;
mod history_v9;
mod native_presentation_v9;
pub(crate) use native_presentation_v9::WorkflowFormIdentityV9;
mod remote_fields_v9;
pub(crate) use history_v9::{
    capture_history_v9, history_ack_key_v9, history_ack_ready_v9, history_presentation_bound_v9,
    history_previous_v9, history_refused_v9, history_requested_v9, history_view_available_v9,
    install_history_v9, WorkflowHistoryFormV9,
};
pub(crate) use remote_fields_v9::preparation_bound as remote_fields_preparation_bound_v9;

const KEYS: [&str; 6] = [
    "location", "yaml", "branch", "message", "pr_title", "pr_body",
];
const LABELS: [&str; 6] = [
    "Local choice: isolated worktree or explicit current directory",
    "Exact edited workflow YAML (private; not a remote write)",
    "Exact NEW branch (no replacement)",
    "Exact commit message (hooks/signing remain enabled)",
    "Proposed PR title (not yet published)",
    "Proposed PR body (not yet published)",
];

#[derive(Clone, Debug)]
pub(crate) enum WorkflowLocalPreviewV9 {
    Worktree(Arc<WorkflowWorktreePreviewV9>),
    Save(Arc<WorkflowSavePreviewV9>),
    Stage(Arc<WorkflowLocalPlanPreviewV9>),
    Commit(Arc<WorkflowLocalPlanPreviewV9>),
    Remote(Arc<crate::scoped_actions_workflow_edit_v9::WorkflowRemotePreviewV9>),
}
impl WorkflowLocalPreviewV9 {
    pub(crate) fn display(&self) -> &Arc<PreparedFormTextV1> {
        match self {
            Self::Worktree(p) => p.display(),
            Self::Save(p) => p.display(),
            Self::Stage(p) | Self::Commit(p) => p.display(),
            Self::Remote(p) => p.display(),
        }
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        match self {
            Self::Worktree(p) => p.retained_bytes(),
            Self::Save(p) => p.retained_bytes(),
            Self::Stage(p) | Self::Commit(p) => p.retained_bytes(),
            Self::Remote(p) => p.retained_bytes(),
        }
    }
}

#[derive(Clone)]
pub struct WorkflowEditComposerStateV9 {
    // None exists ONLY inside a private, unpublished pre-UID seed. Attaching
    // the actual closed opener is the sole transition to a drawable Form.
    opened: Option<Arc<OpenedWorkflowLocalEditV9>>,
    initial: Option<Arc<OpenedWorkflowLocalEditV9>>,
    intent: Arc<WorkflowEditIntentV9>,
    ticket: Arc<()>,
    revision: u64,
    context: Arc<str>,
    summary: Arc<PreparedFormTextV1>,
    preview: Option<WorkflowLocalPreviewV9>,
    values: Option<Arc<WorkflowEditValuesV9>>,
    saved: Option<Arc<WorkflowSavedLocalV9>>,
    staged: Option<Arc<WorkflowStagedLocalV9>>,
    committed: Option<Arc<WorkflowCommittedLocalV9>>,
    published_branch:
        Option<Arc<crate::scoped_actions_workflow_edit_v9::WorkflowPublishedBranchV9>>,
    remote_history: Option<Arc<crate::actions_write_journal::ActionsOwnedWorkflowRemoteHistoryV9>>,
    remote_reference: Option<crate::actions_write_journal::ActionsPublicationReference>,
    history_view_v9: Option<Arc<crate::scoped_actions_native_v1::journal_v5::ClosedJournalViewV5>>,
    inspect_requested_v9: bool,
    ack_requested_v9: bool,
    history_visible_v9: bool,
    pending_remote_fields: Option<remote_fields_v9::RemoteFieldsSeedV9>,
    input_visible: bool,
    confirmation_visible: bool,
    inspection_required: bool,
}
impl std::fmt::Debug for WorkflowEditComposerStateV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "WorkflowEditComposerStateV9(<private local edit/receipts; no implicit approval>)",
        )
    }
}
impl WorkflowEditComposerStateV9 {
    pub(crate) fn opened(&self) -> &Arc<OpenedWorkflowLocalEditV9> {
        self.opened
            .as_ref()
            .expect("only sealed actual opener may publish W9 Form")
    }
    pub(crate) fn intent(&self) -> &Arc<WorkflowEditIntentV9> {
        &self.intent
    }
    pub(crate) fn context(&self) -> &str {
        &self.context
    }
    pub(crate) fn identity(&self) -> &crate::providers::gh::HostIdentity {
        self.opened().identity()
    }
    pub(crate) fn editable(&self) -> bool {
        !self.inspection_required
            && (self.committed.is_none() || self.pending_remote_fields.is_none())
    }
    pub(crate) fn remote_history_only_v9(&self) -> bool {
        self.remote_reference.is_some()
            && (self.history_view_v9.is_some() || self.inspection_required)
    }
    pub(crate) fn ready(&self) -> bool {
        self.input_visible
            && self.confirmation_visible
            && self.preview.is_some()
            && !self.inspection_required
    }
    pub(crate) fn plan_lines(&self) -> &[String] {
        match &self.preview {
            Some(WorkflowLocalPreviewV9::Worktree(p)) => &p.plan().preview,
            Some(WorkflowLocalPreviewV9::Stage(p) | WorkflowLocalPreviewV9::Commit(p)) => {
                &p.plan().preview
            }
            Some(WorkflowLocalPreviewV9::Remote(p)) => p.lines(),
            _ => self.opened().lines(),
        }
    }
    pub(crate) fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.preview = None;
        self.confirmation_visible = false;
        self.inspect_requested_v9 = false;
        self.ack_requested_v9 = false;
        self.history_visible_v9 = false;
        // Receipts remain observations, but no changed input may reuse them.
        // Fresh capture requires exact original values before the next step.
    }
    pub(crate) fn resize(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.input_visible = false;
        self.confirmation_visible = false;
    }
}

/// Constructed by the genuine worker before its final UID, never by an event
/// handler or a serialized draft. It cannot be drawn until attach succeeds.
pub(crate) struct WorkflowEditFormSeedV9 {
    form: Box<Form>,
    retained: usize,
}
impl WorkflowEditFormSeedV9 {
    pub(crate) fn prepare(
        intent: Arc<WorkflowEditIntentV9>,
        identity: &crate::providers::gh::HostIdentity,
        document: Arc<PreparedFormTextV1>,
        raw_draft: Option<&str>,
    ) -> Result<Self, String> {
        let defaults = [
            "isolated",
            intent.definition().yaml.as_str(),
            "workflow/edit",
            "Update workflow",
            "Update workflow",
            "",
        ];
        let text = match raw_draft {
            Some(body) => WorkflowDraftTextV9::decode(body)?,
            None => WorkflowDraftTextV9::from_fields(defaults)?,
        };
        let rows = text.rows();
        // Six fixed fields and bounded borrowed text; no body/row clone has
        // happened when these conservative allocation preflights run.
        let raw = rows
            .iter()
            .try_fold(0usize, |n, row| n.checked_add(row.len()))
            .ok_or_else(refused)?;
        let header = std::mem::size_of::<Form>()
            + std::mem::size_of::<WorkflowEditComposerStateV9>()
            + 8 * std::mem::size_of::<usize>()
            + 9 * std::mem::size_of::<Field>()
            + 9 * std::mem::size_of::<ratatui::layout::Rect>()
            + KEYS
                .iter()
                .chain(LABELS.iter())
                .map(|v| v.len())
                .sum::<usize>()
            + 1024;
        let preflight = raw
            .checked_mul(2)
            .and_then(|n| n.checked_add(header))
            .and_then(|n| n.checked_add(remote_fields_v9::preparation_bound(&intent).ok()?))
            .ok_or_else(refused)?;
        if preflight > 8 * 1024 * 1024 {
            return Err(refused());
        }
        let pending_remote_fields = Some(remote_fields_v9::prepare(&intent)?);
        let mut fields = Vec::with_capacity(9);
        for index in 0..6 {
            let mut field = Field::text(
                KEYS[index],
                LABELS[index],
                rows[index],
                matches!(index, 1 | 3 | 5),
            )?;
            field.debug_redacted = matches!(index, 1 | 3 | 4 | 5);
            if index == 0 {
                field.choices = vec!["isolated".into(), "direct".into()];
                field.choice_labels = vec![
                    "New isolated worktree".into(),
                    "Current directory (explicit)".into(),
                ];
            }
            fields.push(field);
        }
        let context: Arc<str> = format!(
            "{} • account {} • repository ID {} • workflow {}",
            identity.host,
            identity.account_id.ok_or_else(refused)?,
            intent.source().reference().id,
            intent.original().model().id
        )
        .into();
        let state = WorkflowEditComposerStateV9 {
            opened: None,
            initial: None,
            intent,
            ticket: Arc::new(()),
            revision: 0,
            context,
            summary: document.clone(),
            preview: None,
            values: None,
            saved: None,
            staged: None,
            committed: None,
            published_branch: None,
            remote_history: None,
            remote_reference: None,
            history_view_v9: None,
            inspect_requested_v9: false,
            ack_requested_v9: false,
            history_visible_v9: false,
            pending_remote_fields,
            input_visible: false,
            confirmation_visible: false,
            inspection_required: false,
        };
        let mut form = Box::new(Form::workflow_edit_seed_v9(Box::new(state), fields));
        // The borrowed preflight above already includes the complete nine
        // Rect payloads; Form's size includes this Vec's header. Reserve while
        // still worker-private, BEFORE final UID and later Commit admission.
        form.field_rects.reserve_exact(9);
        form.restored_draft = raw_draft.is_some();
        form.install_prepared_preview_display_v1(document);
        let retained = owned_form_bytes_v9(&form)?;
        if retained > preflight || retained > 8 * 1024 * 1024 {
            return Err(refused());
        }
        Ok(Self { form, retained })
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        self.retained
    }
    pub(crate) fn attach(
        mut self,
        opened: Arc<OpenedWorkflowLocalEditV9>,
    ) -> Result<Box<Form>, String> {
        let FormPurpose::ScopedWorkflowEditV9(state) = &mut self.form.purpose else {
            return Err(refused());
        };
        if state.opened.is_some()
            || !Arc::ptr_eq(&state.intent, opened.intent())
            || !Arc::ptr_eq(&state.summary, opened.display())
        {
            return Err(refused());
        }
        state.initial = Some(opened.clone());
        state.opened = Some(opened);
        Ok(self.form) // No body clone/parse/layout/SQL or new allocation.
    }
}

pub(crate) fn owned_form_bytes_v9(form: &Form) -> Result<usize, String> {
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return Err(refused());
    };
    // These are never W9 producers. Refuse unexpected attached graphs instead
    // of silently treating an unrelated picker/publication as an empty one.
    if form.picker.is_some()
        || form.publication.is_some()
        || form.actions_publication.is_some()
        || form.metadata_preview.is_some()
        || form.metadata_picker.is_some()
        || form.base_selection.is_some()
    {
        return Err(refused());
    }
    let mut n = std::mem::size_of::<Form>()
        + std::mem::size_of::<WorkflowEditComposerStateV9>()
        + 8 * std::mem::size_of::<usize>()
        + state.context.len()
        + form.title.capacity()
        + form.fields.capacity() * std::mem::size_of::<Field>()
        + form.field_rects.capacity() * std::mem::size_of::<ratatui::layout::Rect>()
        + form.error.capacity()
        + (form.actions_original_lines.capacity() + form.metadata_original_lines.capacity())
            * std::mem::size_of::<String>()
        + form.metadata_picker_hits.capacity()
            * std::mem::size_of::<crate::pr_metadata_picker::PrMetadataPickerHit>();
    if let Some(pending) = &state.pending_remote_fields {
        n = n.checked_add(pending.owned_bytes()?).ok_or_else(refused)?;
    }
    for row in form
        .actions_original_lines
        .iter()
        .chain(&form.metadata_original_lines)
    {
        n = n.checked_add(row.capacity()).ok_or_else(refused)?;
    }
    for field in &form.fields {
        n = n
            .checked_add(
                field.key.capacity()
                    + field.label.capacity()
                    + field.editor.text.capacity()
                    + (field.choices.capacity() + field.choice_labels.capacity())
                        * std::mem::size_of::<String>(),
            )
            .and_then(|n| {
                n.checked_add(
                    field
                        .choices
                        .iter()
                        .chain(field.choice_labels.iter())
                        .map(String::capacity)
                        .sum::<usize>(),
                )
            })
            .ok_or_else(refused)?;
    }
    Ok(n)
}
pub(crate) fn encoded_draft(form: &Form) -> Result<String, String> {
    if !matches!(form.purpose, FormPurpose::ScopedWorkflowEditV9(_)) {
        return Err(refused());
    }
    WorkflowDraftTextV9::from_fields(borrowed_rows(form)?)?.encode()
}
fn borrowed_rows(form: &Form) -> Result<[&str; 6], String> {
    if form.fields.len() != 6 && !remote_fields_v9::valid_rows(form) {
        return Err(refused());
    }
    let mut rows = [""; 6];
    for (index, field) in form.fields.iter().take(6).enumerate() {
        if field.key != KEYS[index] || field.secret {
            return Err(refused());
        }
        rows[index] = &field.editor.text;
    }
    Ok(rows)
}
pub(crate) fn validate_input(form: &Form, incoming: &str) -> Result<(), String> {
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return Ok(());
    };
    if form.submitting || form.preview || !state.editable() {
        return Err(refused());
    }
    let Some(field) = form.fields.get(form.focus) else {
        return Ok(());
    };
    if state.committed.is_some() && form.focus < 6 {
        return Err("The actual committed six local inputs are read only; use the three explicit remote-selection fields or open a fresh workflow".into());
    }
    let limit = match field.key.as_str() {
        "yaml" => 256 * 1024,
        "message" => 32 * 1024,
        "pr_body" => 64 * 1024,
        "pr_title" => 1024,
        "branch" => 256,
        "location" => 8,
        "configured_remote" | "base_branch" => 256,
        "head_member" => 20,
        _ => return Err(refused()),
    };
    let removed = field.editor.selection().map_or(0, |range| range.len());
    let next = field
        .editor
        .text
        .len()
        .saturating_sub(removed)
        .checked_add(incoming.len())
        .ok_or_else(refused)?;
    let total = form
        .fields
        .iter()
        .skip(1)
        .take(5)
        .try_fold(0usize, |n, f| n.checked_add(f.editor.text.len()))
        .ok_or_else(refused)?;
    if next > limit
        || (form.focus != 0
            && form.focus < 6
            && total
                .saturating_sub(field.editor.text.len())
                .checked_add(next)
                .is_none_or(|n| n > 360 * 1024))
    {
        return Err(refused());
    }
    Ok(())
}
pub(crate) fn editable_summary(form: &Form) -> bool {
    matches!(&form.purpose,FormPurpose::ScopedWorkflowEditV9(state) if state.editable())
        && !form.preview
        && form.actions_publication.is_none()
}
pub(crate) fn protect_committed_input_v9(
    form: &mut Form,
    code: crossterm::event::KeyCode,
    modifiers: crossterm::event::KeyModifiers,
) -> bool {
    use crossterm::event::{KeyCode, KeyModifiers};
    let frozen = matches!(&form.purpose,FormPurpose::ScopedWorkflowEditV9(state) if state.committed.is_some())
        && form.focus < 6
        && form.focus < form.fields.len()
        && !form.preview;
    let control = modifiers.contains(KeyModifiers::CONTROL);
    let changes = matches!(code, KeyCode::Backspace | KeyCode::Delete)
        || code == KeyCode::Char('u') && control
        || matches!(code, KeyCode::Char(_)) && !control && !modifiers.contains(KeyModifiers::ALT)
        || code == KeyCode::Enter
            && form
                .fields
                .get(form.focus)
                .is_some_and(|field| field.multiline);
    if frozen && changes {
        form.error = "Actual committed local fields are read only; edit only the three explicit remote selections, or open a fresh local workflow.".into();
        true
    } else {
        false
    }
}
pub(crate) fn restore_summary(form: &mut Form) {
    if !editable_summary(form) {
        return;
    }
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return;
    };
    let display = state.summary.clone();
    form.install_prepared_preview_display_v1(display);
}
pub(crate) fn begin_draw(form: &mut Form, usable: bool) {
    if let FormPurpose::ScopedWorkflowEditV9(state) = &mut form.purpose {
        state.input_visible = usable;
        state.confirmation_visible = false;
    }
}
pub(crate) fn painted_preview(form: &mut Form, useful: bool) {
    if let FormPurpose::ScopedWorkflowEditV9(state) = &mut form.purpose {
        state.confirmation_visible =
            useful && state.input_visible && form.preview && state.preview.is_some();
        state.history_visible_v9 = useful && state.input_visible && state.history_view_v9.is_some();
    }
}
pub(crate) fn footer(form: &Form) -> Option<Vec<(&'static str, FormAction)>> {
    if let Some(footer) = history_v9::footer(form) {
        return Some(footer);
    }
    matches!(form.purpose, FormPurpose::ScopedWorkflowEditV9(_)).then(|| {
        vec![
            ("[Preview next step]", FormAction::Preview),
            ("[Editor help]", FormAction::Editor),
            ("[Confirm step]", FormAction::Submit),
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
    if history_v9::dispatch(form, action) {
        return true;
    }
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return false;
    };
    match action {
        FormAction::Editor => {
            form.error="Native multiline YAML editing is available. External-editor handoff is not yet connected to this original bounded workflow; no process/file/write was started.".into();
            true
        }
        FormAction::Preview if state.remote_reference.is_some() && state.inspection_required => {
            form.error="Original remote result is read only; inspect current facts or explicitly ACK locally, never renew Preview or resend.".into();
            true
        }
        FormAction::Preview if form.preview => {
            form.invalidate_plan();
            form.focus = form.active_field.min(form.fields.len().saturating_sub(1));
            form.field_top = form.focus;
            form.error.clear();
            true
        }
        FormAction::Preview => {
            if !state.input_visible || !state.editable() || state.inspection_required {
                form.error="Inspect the original complete workflow input; this retained result cannot retry or recreate source/approval.".into();
            } else {
                form.preview_requested = true;
            }
            true
        }
        FormAction::Submit => {
            if !form.preview || !state.ready() || form.preview_display_v1.geometry_rejected {
                form.error="Fresh complete exact local-step Preview required; no hidden confirmation or write.".into();
            } else {
                form.submit_requested = true;
            }
            true
        }
        FormAction::Choice(index, _) if state.committed.is_some() && *index < 6 => {
            form.error="The actual committed local inputs are read only; no receipt is rebound by changing a choice.".into();
            true
        }
        FormAction::Field(_) | FormAction::Choice(..) | FormAction::FieldScroll(_)
            if !state.editable() || form.preview =>
        {
            form.error="Original step Preview/result is read only; Back preserves observed receipts and never retries.".into();
            true
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkflowEditPhaseV9 {
    Preview,
    Confirm,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkflowEditStepV9 {
    Isolate,
    Save,
    Stage,
    Commit,
    PublishBranch,
    CreatePr,
}
pub(crate) struct WorkflowEditRequestV9 {
    native_presentation_v9:Option<Arc<crate::scoped_actions_native_v1::publication_v3::workflow_edit_v9::WorkflowNativePresentationV9>>,
    initial: Arc<OpenedWorkflowLocalEditV9>,
    opened: Arc<OpenedWorkflowLocalEditV9>,
    ticket: Arc<()>,
    revision: u64,
    presentation: Arc<()>,
    input_epoch: u64,
    values: Arc<WorkflowEditValuesV9>,
    mode: &'static str,
    step: WorkflowEditStepV9,
    phase: WorkflowEditPhaseV9,
    preview: Option<WorkflowLocalPreviewV9>,
    saved: Option<Arc<WorkflowSavedLocalV9>>,
    staged: Option<Arc<WorkflowStagedLocalV9>>,
    committed: Option<Arc<WorkflowCommittedLocalV9>>,
    published_branch:
        Option<Arc<crate::scoped_actions_workflow_edit_v9::WorkflowPublishedBranchV9>>,
    owned_bytes: usize,
    remote_selection:
        Option<Arc<crate::scoped_actions_workflow_edit_v9::WorkflowRemoteSelectionV9>>,
}
impl std::fmt::Debug for WorkflowEditRequestV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "WorkflowEditRequestV9(<exact original private Form/receipt Arcs; no renewed permit>)",
        )
    }
}
fn same_values(values: &WorkflowEditValuesV9, rows: [&str; 6]) -> bool {
    [
        values.yaml(),
        values.branch(),
        values.commit_message(),
        values.pr_title(),
        values.pr_body(),
    ] == rows[1..]
}
fn same_preview(a: &Option<WorkflowLocalPreviewV9>, b: &Option<WorkflowLocalPreviewV9>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(WorkflowLocalPreviewV9::Worktree(a)), Some(WorkflowLocalPreviewV9::Worktree(b))) => {
            Arc::ptr_eq(a, b)
        }
        (Some(WorkflowLocalPreviewV9::Save(a)), Some(WorkflowLocalPreviewV9::Save(b))) => {
            Arc::ptr_eq(a, b)
        }
        (Some(WorkflowLocalPreviewV9::Stage(a)), Some(WorkflowLocalPreviewV9::Stage(b)))
        | (Some(WorkflowLocalPreviewV9::Commit(a)), Some(WorkflowLocalPreviewV9::Commit(b))) => {
            Arc::ptr_eq(a, b)
        }
        (Some(WorkflowLocalPreviewV9::Remote(a)), Some(WorkflowLocalPreviewV9::Remote(b))) => {
            Arc::ptr_eq(a, b)
        }
        _ => false,
    }
}
fn same_optional<T>(a: &Option<Arc<T>>, b: &Option<Arc<T>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}
impl WorkflowEditRequestV9 {
    pub(crate) fn matches_native_work_v9(
        &self,
        work: &crate::scoped_actions_v1::ScopedActionsWorkV1,
    ) -> bool {
        self.native_presentation_v9
            .as_ref()
            .is_some_and(|presentation| presentation.matches_work(work))
    }
    pub(crate) fn opened(&self) -> &Arc<OpenedWorkflowLocalEditV9> {
        &self.opened
    }
    pub(crate) fn intent(&self) -> &Arc<WorkflowEditIntentV9> {
        self.initial.intent()
    }
    pub(crate) fn values(&self) -> &Arc<WorkflowEditValuesV9> {
        &self.values
    }
    pub(crate) fn preview(&self) -> Option<&WorkflowLocalPreviewV9> {
        self.preview.as_ref()
    }
    pub(crate) fn staged(&self) -> Option<&Arc<WorkflowStagedLocalV9>> {
        self.staged.as_ref()
    }
    pub(crate) fn saved(&self) -> Option<&Arc<WorkflowSavedLocalV9>> {
        self.saved.as_ref()
    }
    pub(crate) fn phase(&self) -> WorkflowEditPhaseV9 {
        self.phase
    }
    pub(crate) fn step(&self) -> WorkflowEditStepV9 {
        self.step
    }
    pub(crate) fn owned_bytes(&self) -> usize {
        self.owned_bytes
    }
    pub(crate) fn remote_selection(
        &self,
    ) -> Option<&Arc<crate::scoped_actions_workflow_edit_v9::WorkflowRemoteSelectionV9>> {
        self.remote_selection.as_ref()
    }
    pub(crate) fn published_branch_v9(
        &self,
    ) -> Option<&Arc<crate::scoped_actions_workflow_edit_v9::WorkflowPublishedBranchV9>> {
        self.published_branch.as_ref()
    }
    pub(crate) fn owns_presentation(&self, form: &Form) -> bool {
        matches!(&form.purpose,FormPurpose::ScopedWorkflowEditV9(state) if
            Arc::ptr_eq(&state.ticket,&self.ticket)&&state.initial.as_ref().is_some_and(|opened|Arc::ptr_eq(opened,&self.initial)))
    }
    pub(crate) fn matches_form(&self, form: &Form) -> bool {
        let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
            return false;
        };
        let (presentation, epoch) = form.presentation_v4();
        self.owns_presentation(form)
            && self
                .native_presentation_v9
                .as_ref()
                .is_none_or(|p| p.matches_form(form))
            && Arc::ptr_eq(presentation, &self.presentation)
            && epoch == Some(self.input_epoch)
            && state.revision == self.revision
            && Arc::ptr_eq(state.opened(), &self.opened)
            && same_preview(&state.preview, &self.preview)
            && same_optional(&state.saved, &self.saved)
            && same_optional(&state.staged, &self.staged)
            && same_optional(&state.committed, &self.committed)
            && same_optional(&state.published_branch, &self.published_branch)
            && self
                .remote_selection
                .as_ref()
                .is_none_or(|selection| remote_fields_v9::matches_selection(form, selection))
            && borrowed_rows(form)
                .is_ok_and(|rows| rows[0] == self.mode && same_values(&self.values, rows))
    }
    pub(crate) fn retire_busy(&self, form: &mut Form) {
        if self.owns_presentation(form) {
            form.submitting = false;
            form.invalidate_plan();
        }
    }
    /// Only the original busy owner may retire this exact unchanged payload.
    /// Unlike strict admission, cancellation follows Esc's epoch observation.
    pub(crate) fn matches_retirement_form_v9(&self, form: &Form) -> bool {
        let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
            return false;
        };
        self.owns_presentation(form)
            && self
                .native_presentation_v9
                .as_ref()
                .is_none_or(|p| p.matches_retirement_form_v9(form))
            && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
            && state.revision == self.revision
            && Arc::ptr_eq(state.opened(), &self.opened)
            && same_preview(&state.preview, &self.preview)
            && same_optional(&state.saved, &self.saved)
            && same_optional(&state.staged, &self.staged)
            && same_optional(&state.committed, &self.committed)
            && same_optional(&state.published_branch, &self.published_branch)
            && self
                .remote_selection
                .as_ref()
                .is_none_or(|selection| remote_fields_v9::matches_selection(form, selection))
            && borrowed_rows(form)
                .is_ok_and(|rows| rows[0] == self.mode && same_values(&self.values, rows))
    }
}
pub(crate) struct ClosedWorkflowFormV9 {
    opened: Arc<OpenedWorkflowLocalEditV9>,
    ticket: Arc<()>,
    revision: u64,
    presentation: Arc<()>,
    input_epoch: u64,
    held_bytes: usize,
}
impl ClosedWorkflowFormV9 {
    pub(crate) fn capture(form: &Form, intent: &WorkflowEditIntentV9) -> Option<Arc<Self>> {
        let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
            return None;
        };
        if form.submitting
            || form.preview_requested
            || form.submit_requested
            || form.editor_requested
            || !Arc::ptr_eq(state.intent().source(), intent.source())
            || !Arc::ptr_eq(state.intent().page(), intent.page())
        {
            return None;
        }
        let (presentation, epoch) = form.presentation_v4();
        Some(Arc::new(Self {
            opened: state.opened().clone(),
            ticket: state.ticket.clone(),
            revision: state.revision,
            presentation: presentation.clone(),
            input_epoch: epoch?,
            held_bytes: presentation_bytes_v9(form).ok()?,
        }))
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
            return false;
        };
        let (presentation, epoch) = form.presentation_v4();
        !form.submitting
            && Arc::ptr_eq(state.opened(), &self.opened)
            && Arc::ptr_eq(&state.ticket, &self.ticket)
            && state.revision == self.revision
            && Arc::ptr_eq(presentation, &self.presentation)
            && epoch == Some(self.input_epoch)
    }
    /// No GH/Core renewal: SAME already-issued local/file/Clear witness from
    /// the actual opener, checked on the worker before typed SaveFailed return.
    pub(crate) fn validate_boundary(
        &self,
        database: &std::path::Path,
        cancel: &crate::providers::runner::CancellationToken,
    ) -> crate::providers::gh::GithubResult<()> {
        self.opened.validate_local(database, cancel)
    }
    pub(crate) fn held_bytes(&self) -> usize {
        self.held_bytes
    }
    pub(crate) fn matches_intent(&self, intent: &WorkflowEditIntentV9) -> bool {
        Arc::ptr_eq(self.opened.intent().source(), intent.source())
            && Arc::ptr_eq(self.opened.intent().page(), intent.page())
    }
}
pub(crate) fn retire_invalid_form(app: &mut crate::app::App) -> bool {
    let stale=app.form.as_ref().is_some_and(|form|matches!(&form.purpose,FormPurpose::ScopedWorkflowEditV9(state)
        if !crate::scoped_actions_native_v1::publication_v3::workflow_edit_v9::source_current(app,state.intent())));
    if stale {
        app.form = None;
        app.form_dismissed = true;
        if app.overlay == Some(crate::app::Overlay::Form) {
            app.overlay = None;
        }
        app.hits.clear();
        app.status="Original workflow local edit source changed; private fields hidden; saved draft/actual effects retained, no renewed permission".into();
    }
    stale
}
pub(crate) fn retire_for_cancel(
    form: &mut Form,
    request: &WorkflowEditRequestV9,
    may_have_effect: bool,
) {
    if !form.submitting || !request.matches_retirement_form_v9(form) {
        return;
    }
    request.retire_busy(form);
    if let FormPurpose::ScopedWorkflowEditV9(state) = &mut form.purpose {
        state.inspection_required |= may_have_effect;
    }
}
#[cfg(test)]
pub(crate) fn capture(
    form: &Form,
    phase: WorkflowEditPhaseV9,
) -> Result<Arc<WorkflowEditRequestV9>, String> {
    capture_inner_v9(form, phase, None)
}
pub(crate) fn capture_native_v9(
    form: &Form,
    phase: WorkflowEditPhaseV9,
    presentation:Arc<crate::scoped_actions_native_v1::publication_v3::workflow_edit_v9::WorkflowNativePresentationV9>,
) -> Result<Arc<WorkflowEditRequestV9>, String> {
    if !presentation.matches_form(form) {
        return Err(refused());
    }
    capture_inner_v9(form, phase, Some(presentation))
}
fn capture_inner_v9(
    form: &Form,
    phase: WorkflowEditPhaseV9,
    native_presentation_v9:Option<Arc<crate::scoped_actions_native_v1::publication_v3::workflow_edit_v9::WorkflowNativePresentationV9>>,
) -> Result<Arc<WorkflowEditRequestV9>, String> {
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return Err(refused());
    };
    if form.submitting
        || !state.input_visible
        || state.inspection_required
        || form.actions_publication.is_some()
    {
        return Err(refused());
    }
    if phase == WorkflowEditPhaseV9::Confirm
        && (!form.preview || !state.ready() || form.preview_display_v1.geometry_rejected)
    {
        return Err(refused());
    }
    let rows = borrowed_rows(form)?;
    let mode = match rows[0] {
        "isolated" => "isolated",
        "direct" => "direct",
        _ => return Err(refused()),
    };
    WorkflowDraftTextV9::from_fields(rows)?;
    let initial = state.initial.as_ref().ok_or_else(refused)?;
    let isolated = !Arc::ptr_eq(initial, state.opened());
    if isolated && mode != "isolated" {
        return Err("The actual isolated receipt cannot be rebound to the original directory by an input edit; reopen explicitly".into());
    }
    let same = state
        .values
        .as_ref()
        .is_some_and(|values| same_values(values, rows));
    if state.committed.is_some() && !same {
        return Err("An observed commit is inspect/publish only; edited text requires an independently fresh local workflow".into());
    }
    let step = if !isolated && mode == "isolated" {
        WorkflowEditStepV9::Isolate
    } else if !same || state.saved.is_none() {
        WorkflowEditStepV9::Save
    } else if state.staged.is_none() {
        WorkflowEditStepV9::Stage
    } else if state.committed.is_none() {
        WorkflowEditStepV9::Commit
    } else if state.published_branch.is_some() {
        WorkflowEditStepV9::CreatePr
    } else {
        WorkflowEditStepV9::PublishBranch
    };
    let step = if phase == WorkflowEditPhaseV9::Confirm {
        match state.preview.as_ref().ok_or_else(refused)? {
            WorkflowLocalPreviewV9::Worktree(_) => WorkflowEditStepV9::Isolate,
            WorkflowLocalPreviewV9::Save(_) => WorkflowEditStepV9::Save,
            WorkflowLocalPreviewV9::Stage(_) => WorkflowEditStepV9::Stage,
            WorkflowLocalPreviewV9::Commit(_) => WorkflowEditStepV9::Commit,
            WorkflowLocalPreviewV9::Remote(p) => match p.phase() {
                crate::providers::gh::WorkflowEditPhaseV9::PublishBranch => {
                    WorkflowEditStepV9::PublishBranch
                }
                crate::providers::gh::WorkflowEditPhaseV9::CreatePr => WorkflowEditStepV9::CreatePr,
                _ => return Err(refused()),
            },
        }
    } else {
        step
    };
    let owned = presentation_bytes_v9(form)?
        .checked_add(
            native_presentation_v9
                .as_ref()
                .map_or(0, |p| p.held_bytes()),
        )
        .ok_or_else(refused)?;
    let confirm_header = [
        std::mem::size_of::<crate::scoped_actions_workflow_edit_v9::WorkflowWorktreeConfirmV9>(),
        std::mem::size_of::<crate::scoped_actions_workflow_edit_v9::WorkflowSaveConfirmV9>(),
        std::mem::size_of::<crate::scoped_actions_workflow_edit_v9::WorkflowStageConfirmV9>(),
        std::mem::size_of::<crate::scoped_actions_workflow_edit_v9::WorkflowCommitConfirmV9>(),
        std::mem::size_of::<crate::scoped_actions_workflow_edit_v9::WorkflowRemoteConfirmV9>(),
    ]
    .into_iter()
    .max()
    .ok_or_else(refused)?;
    let headers = std::mem::size_of::<WorkflowEditRequestV9>()
        + confirm_header
        + std::mem::size_of::<crate::scoped_actions_v1::ScopedActionsWorkV1>()
        + std::mem::size_of::<crate::scoped_actions_native_v1::ScopedActionsOwnerV1>()
        + 12 * std::mem::size_of::<usize>();
    let preflight = owned
        .checked_add(2 * 360 * 1024 + headers)
        .filter(|n| *n <= 8 * 1024 * 1024)
        .ok_or_else(refused)?;
    // Borrowed raw/capacity and exact original identities were checked BEFORE
    // cloning private values or any source/Preview Arc below.
    let values = if same {
        state.values.as_ref().ok_or_else(refused)?.clone()
    } else {
        WorkflowEditValuesV9::capture(rows[1], rows[2], rows[3], rows[4], rows[5])?
    };
    let remote_selection = if matches!(
        step,
        WorkflowEditStepV9::PublishBranch | WorkflowEditStepV9::CreatePr
    ) {
        Some(remote_fields_v9::selection(form)?)
    } else {
        None
    };
    let actual = owned
        .checked_add(values.retained_bytes())
        .and_then(|n| n.checked_add(headers))
        .and_then(|n| {
            n.checked_add(
                remote_selection
                    .as_ref()
                    .map_or(0, |selection| selection.owned_bytes_v9()),
            )
        })
        .filter(|n| *n <= preflight)
        .ok_or_else(refused)?;
    let (presentation, epoch) = form.presentation_v4();
    Ok(Arc::new(WorkflowEditRequestV9 {
        native_presentation_v9,
        initial: initial.clone(),
        opened: state.opened().clone(),
        ticket: state.ticket.clone(),
        revision: state.revision,
        presentation: presentation.clone(),
        input_epoch: epoch.ok_or_else(refused)?,
        values,
        mode,
        step,
        phase,
        preview: state.preview.clone(),
        saved: state.saved.clone(),
        staged: state.staged.clone(),
        committed: state.committed.clone(),
        published_branch: state.published_branch.clone(),
        owned_bytes: actual,
        remote_selection,
    }))
}
/// Actual capacities plus known allocation headers. Prior Preview/its Doc is
/// charged exactly once here; exact source/receipt graphs may conservatively
/// overlap, but are never discounted through guessed Arc equality.
fn presentation_bytes_v9(form: &Form) -> Result<usize, String> {
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return Err(refused());
    };
    let expected = state
        .preview
        .as_ref()
        .map_or(&state.summary, WorkflowLocalPreviewV9::display);
    if form
        .preview_display_v1
        .document
        .as_ref()
        .is_none_or(|doc| !Arc::ptr_eq(doc, expected))
    {
        return Err(refused());
    }
    let mut bytes = owned_form_bytes_v9(form)?
        .checked_add(state.summary.retained_bytes())
        .ok_or_else(refused)?;
    if let Some(initial) = &state.initial {
        bytes = bytes
            .checked_add(initial.retained_bytes())
            .ok_or_else(refused)?;
    }
    if let Some(opened) = &state.opened {
        bytes = bytes
            .checked_add(opened.retained_bytes())
            .ok_or_else(refused)?;
    }
    if let Some(values) = &state.values {
        bytes = bytes
            .checked_add(values.retained_bytes())
            .ok_or_else(refused)?;
    }
    if let Some(preview) = &state.preview {
        bytes = bytes
            .checked_add(preview.retained_bytes())
            .ok_or_else(refused)?;
    }
    if let Some(saved) = &state.saved {
        bytes = bytes
            .checked_add(saved.retained_bytes_v9().ok_or_else(refused)?)
            .ok_or_else(refused)?;
    }
    if let Some(staged) = &state.staged {
        bytes = bytes
            .checked_add(staged.retained_bytes_v9().ok_or_else(refused)?)
            .ok_or_else(refused)?;
    }
    if let Some(committed) = &state.committed {
        bytes = bytes
            .checked_add(committed.retained_bytes_v9().ok_or_else(refused)?)
            .ok_or_else(refused)?;
    }
    if let Some(branch) = &state.published_branch {
        bytes = bytes
            .checked_add(branch.retained_bytes_v9())
            .ok_or_else(refused)?;
    }
    if let Some(history) = &state.remote_history {
        bytes = bytes
            .checked_add(history.retained_bytes())
            .ok_or_else(refused)?;
    }
    if let Some(reference) = &state.remote_reference {
        bytes = bytes
            .checked_add(reference.fingerprint_sha256.capacity())
            .ok_or_else(refused)?;
    }
    if let Some(history) = &state.history_view_v9 {
        bytes = bytes
            .checked_add(history.retained_bytes_v9())
            .ok_or_else(refused)?;
    }
    (bytes <= 8 * 1024 * 1024)
        .then_some(bytes)
        .ok_or_else(refused)
}
fn refused() -> String {
    "Original workflow editor/input unavailable or exceeds its fixed private bound; no source/approval/default or write".into()
}

pub(crate) fn install_preview(
    form: &mut Form,
    request: &WorkflowEditRequestV9,
    preview: WorkflowLocalPreviewV9,
) -> bool {
    if !request.matches_form(form) || request.phase() != WorkflowEditPhaseV9::Preview {
        return false;
    }
    let matching = match (&preview, request.step()) {
        (WorkflowLocalPreviewV9::Worktree(p), WorkflowEditStepV9::Isolate) => {
            Arc::ptr_eq(p.target().opened(), request.opened())
                && Arc::ptr_eq(p.target().values(), request.values())
        }
        (WorkflowLocalPreviewV9::Save(p), WorkflowEditStepV9::Save) => {
            Arc::ptr_eq(p.opened(), request.opened()) && Arc::ptr_eq(p.values(), request.values())
        }
        (WorkflowLocalPreviewV9::Stage(p), WorkflowEditStepV9::Stage)
        | (WorkflowLocalPreviewV9::Commit(p), WorkflowEditStepV9::Commit) => request
            .saved()
            .is_some_and(|saved| Arc::ptr_eq(saved, p.saved())),
        (
            WorkflowLocalPreviewV9::Remote(p),
            WorkflowEditStepV9::PublishBranch | WorkflowEditStepV9::CreatePr,
        ) => {
            request
                .remote_selection()
                .is_some_and(|selection| Arc::ptr_eq(selection, p.selection()))
                && matches!(
                    (request.step(), p.phase()),
                    (
                        WorkflowEditStepV9::PublishBranch,
                        crate::providers::gh::WorkflowEditPhaseV9::PublishBranch
                    ) | (
                        WorkflowEditStepV9::CreatePr,
                        crate::providers::gh::WorkflowEditPhaseV9::CreatePr
                    )
                )
        }
        _ => false,
    };
    if !matching {
        return false;
    }
    let display = preview.display().clone();
    let FormPurpose::ScopedWorkflowEditV9(state) = &mut form.purpose else {
        return false;
    };
    if state
        .values
        .as_ref()
        .is_none_or(|old| !Arc::ptr_eq(old, request.values()))
    {
        state.saved = None;
        state.staged = None;
        state.committed = None;
    }
    state.values = Some(request.values().clone());
    state.preview = Some(preview);
    state.confirmation_visible = false;
    form.preview = true;
    form.preview_scroll = 0;
    form.focus = form.fields.len();
    form.submitting = false;
    form.preview_requested = false;
    form.submit_requested = false;
    form.error.clear();
    form.install_prepared_preview_display_v1(display);
    true
}
pub(crate) enum WorkflowLocalReceiptV9 {
    Isolated(Arc<crate::scoped_actions_workflow_edit_v9::WorkflowIsolatedLocalV9>),
    Saved(Arc<WorkflowSavedLocalV9>),
    Staged(Arc<WorkflowStagedLocalV9>),
    Committed(Arc<WorkflowCommittedLocalV9>),
    InspectionRequired,
}
pub(crate) fn install_remote_result_v9(
    form: &mut Form,
    request: &WorkflowEditRequestV9,
    result: crate::actions_controller::WorkflowRemoteExecutionV9,
) -> bool {
    let Some(WorkflowLocalPreviewV9::Remote(preview)) = request.preview() else {
        return false;
    };
    if !request.matches_form(form)
        || request.phase() != WorkflowEditPhaseV9::Confirm
        || !result.matches_preview_v9(preview)
    {
        return false;
    }
    let Some(published) = result.into_published_v9() else {
        return false;
    };
    let FormPurpose::ScopedWorkflowEditV9(state) = &mut form.purpose else {
        return false;
    };
    state.summary = published.document;
    state.remote_reference = Some(published.reference);
    state.remote_history = published.history;
    state.published_branch = published.branch;
    state.inspection_required = state.published_branch.is_none();
    state.preview = None;
    state.confirmation_visible = false;
    form.submitting = false;
    // PR has no next write step. Display the actual worker-prepared receipt
    // document, never the prior approval Doc; state.preview remains None.
    form.preview = state.inspection_required;
    form.preview_requested = false;
    form.submit_requested = false;
    form.error=if state.published_branch.is_some(){"Exact newly published branch D observed; no repeat. Separate fresh Create-PR Preview/Confirm remains required."}else{"Canonical created PR observed and exact receipt durable; workflow completion/causation not claimed. Inspect original history; never resend."}.into();
    form.focus = form.fields.len();
    if form.preview {
        let display = state.summary.clone();
        form.install_prepared_preview_display_v1(display);
    } else {
        restore_summary(form);
    }
    true
}
pub(crate) fn install_receipt(
    form: &mut Form,
    request: &WorkflowEditRequestV9,
    receipt: WorkflowLocalReceiptV9,
) -> bool {
    if !request.matches_form(form) || request.phase() != WorkflowEditPhaseV9::Confirm {
        return false;
    }
    let matches = match (&receipt, request.preview()) {
        (WorkflowLocalReceiptV9::Isolated(r), Some(WorkflowLocalPreviewV9::Worktree(p))) => {
            Arc::ptr_eq(r.preview(), p)
        }
        (WorkflowLocalReceiptV9::Saved(r), Some(WorkflowLocalPreviewV9::Save(p))) => {
            Arc::ptr_eq(r.opened(), p.opened()) && Arc::ptr_eq(r.values(), p.values())
        }
        (WorkflowLocalReceiptV9::Staged(r), Some(WorkflowLocalPreviewV9::Stage(p))) => {
            Arc::ptr_eq(r.saved(), p.saved())
        }
        (WorkflowLocalReceiptV9::Committed(r), Some(WorkflowLocalPreviewV9::Commit(p))) => {
            Arc::ptr_eq(r.staged().saved(), p.saved())
                && request
                    .staged()
                    .is_some_and(|old| Arc::ptr_eq(old, r.staged()))
                && form.fields.len() == 6
                && form.fields.capacity() >= 9
                && form.field_rects.capacity() >= 9
                && matches!(&form.purpose,FormPurpose::ScopedWorkflowEditV9(state) if state.pending_remote_fields.is_some())
        }
        (WorkflowLocalReceiptV9::InspectionRequired, Some(_)) => true,
        _ => false,
    };
    if !matches {
        return false;
    }
    let FormPurpose::ScopedWorkflowEditV9(state) = &mut form.purpose else {
        return false;
    };
    let message = match receipt {
        WorkflowLocalReceiptV9::Isolated(r) => {
            state.summary = r.opened().display().clone();
            state.opened = Some(r.opened().clone());
            "Actual isolated worktree/NEW branch observed; original working tree untouched. Preview exact Save next."
        }
        WorkflowLocalReceiptV9::Saved(r) => {
            state.saved = Some(r);
            state.staged = None;
            "Actual exact file Save observed; index/HEAD unchanged. Preview exact Stage next."
        }
        WorkflowLocalReceiptV9::Staged(r) => {
            state.staged = Some(r);
            "Actual exact path staged. Preview canonical staged diff and commit message next; hooks/signing stay enabled."
        }
        WorkflowLocalReceiptV9::Committed(r) => {
            let seed = state
                .pending_remote_fields
                .take()
                .expect("exact pre-UID pending seed admission above");
            form.fields.extend(seed.fields);
            form.field_rects.resize(9, ratatui::layout::Rect::default());
            state.committed = Some(r);
            "Actual reviewed C..D commit observed. Remote branch/PR remain unchanged; separate association/Preview/Confirm required."
        }
        WorkflowLocalReceiptV9::InspectionRequired => {
            state.inspection_required = true;
            "Effect may have occurred. Inspect actual files/Git; no automatic retry/rollback/deletion or remote approval."
        }
    };
    state.preview = None;
    state.confirmation_visible = false;
    state.values = Some(request.values().clone());
    form.submitting = false;
    form.preview = false;
    form.preview_requested = false;
    form.submit_requested = false;
    form.error = message.into();
    form.focus = form.fields.len();
    restore_summary(form);
    true
}
