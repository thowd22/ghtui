//! Shared-Form presentation of actual original thread intent. No transport,
//! SQL, restored approval, source constructor or inferred server permission.
use super::{
    ScopedThreadDraftV1, ScopedThreadPreviewV1, ScopedThreadPublicationIntentV1,
    ThreadPublicationActionV1,
};
use crate::{
    forms::{Form, FormPurpose, PrFormKind, PrIntent},
    providers::gh::HostIdentity,
};
use std::sync::Arc;
pub fn retire_invalid_form(app: &mut crate::app::App) -> bool {
    let original = app.form.as_ref().and_then(|form| match &form.purpose {
        FormPurpose::PrThread(state) => Some(state.intent().prepared.intent.clone()),
        _ => None,
    });
    if original.is_some_and(|intent| {
        crate::scoped_pr_observed_tabs_v1::resources::threads_v1::retire_original_account_v75(
            app,
            &intent.source,
            &intent.active,
            &intent.navigation,
        )
    }) {
        return true;
    }
    if !app.form.as_ref().is_some_and(|form| matches!(&form.purpose, FormPurpose::PrThread(state) if app.view != crate::app::View::Inbox || !state.intent().owns(app))) {
        return false;
    }
    app.form = None;
    app.form_dismissed = true;
    if app.overlay == Some(crate::app::Overlay::Form) {
        app.overlay = None;
    }
    app.hits.clear();
    true
}
struct PresentationTicketV1;
#[derive(Clone)]
pub struct ThreadComposerStateV1 {
    intent: Arc<ScopedThreadPublicationIntentV1>,
    identity: HostIdentity,
    ticket: Arc<PresentationTicketV1>,
    revision: Option<u64>,
    preview: Option<Arc<ScopedThreadPreviewV1>>,
    lines: Arc<[String]>,
    original_lines: Arc<[String]>,
    draft_key: Arc<str>,
}
impl std::fmt::Debug for ThreadComposerStateV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ThreadComposerStateV1(<actual presentation; no approval>)")
    }
}
impl ThreadComposerStateV1 {
    pub fn intent(&self) -> &Arc<ScopedThreadPublicationIntentV1> {
        &self.intent
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn draft_key(&self) -> &str {
        &self.draft_key
    }
    pub fn ready(&self) -> bool {
        self.revision.is_some() && self.preview.is_some()
    }
    pub fn plan_lines(&self) -> &[String] {
        if self.preview.is_some() {
            &self.lines
        } else {
            &self.original_lines
        }
    }
    pub fn invalidate(&mut self) {
        self.preview = None;
        self.lines = Arc::from([]);
        self.revision = self.revision.and_then(|revision| revision.checked_add(1));
    }
    pub fn context(&self) -> String {
        format!(
            "{} • numeric account {:?} • exact original thread • no writes until Confirm",
            crate::git::display_bytes(self.identity.login.as_bytes()),
            self.identity.account_id
        )
    }
}
pub struct ThreadComposerRequestV1 {
    ticket: Arc<PresentationTicketV1>,
    revision: u64,
    intent: Arc<ScopedThreadPublicationIntentV1>,
    body: Arc<str>,
    preview: Option<Arc<ScopedThreadPreviewV1>>,
    draft_key: Arc<str>,
}
impl std::fmt::Debug for ThreadComposerRequestV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ThreadComposerRequestV1(<ticket/revision/body; no permission>)")
    }
}
impl ThreadComposerRequestV1 {
    pub fn intent(&self) -> &Arc<ScopedThreadPublicationIntentV1> {
        &self.intent
    }
    pub fn body(&self) -> &Arc<str> {
        &self.body
    }
    pub fn preview(&self) -> Option<&Arc<ScopedThreadPreviewV1>> {
        self.preview.as_ref()
    }
    pub fn draft_key(&self) -> &str {
        &self.draft_key
    }
    pub fn matches_form(&self, form: &Form) -> bool {
        let FormPurpose::PrThread(state) = &form.purpose else {
            return false;
        };
        Arc::ptr_eq(&self.ticket, &state.ticket)
            && state.revision == Some(self.revision)
            && Arc::ptr_eq(&self.intent, &state.intent)
            && Arc::ptr_eq(&self.draft_key, &state.draft_key)
            && match (&self.preview, &state.preview) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && body(form).is_ok_and(|body| body == self.body.as_ref())
    }
    pub(crate) fn owns_presentation(&self, form: &Form) -> bool {
        matches!(&form.purpose,FormPurpose::PrThread(state)if Arc::ptr_eq(&self.ticket,&state.ticket)&&Arc::ptr_eq(&self.intent,&state.intent))
    }
    pub(crate) fn retire_busy_presentation(&self, form: &mut Form) -> bool {
        let FormPurpose::PrThread(state) = &form.purpose else {
            return false;
        };
        if !Arc::ptr_eq(&self.ticket, &state.ticket) || !Arc::ptr_eq(&self.intent, &state.intent) {
            return false;
        }
        form.submitting = false;
        form.invalidate_plan();
        true
    }
}
fn body(form: &Form) -> Result<&str, String> {
    let FormPurpose::PrThread(state) = &form.purpose else {
        return Err("Not an original thread composer".into());
    };
    if state.intent.action() != ThreadPublicationActionV1::Reply {
        return if form.fields.is_empty() {
            Ok("")
        } else {
            Err("State transition must not contain a fabricated comment field".into())
        };
    }
    if form.fields.len() != 1 || form.fields[0].key != "body" {
        return Err("Thread reply needs its exact body field".into());
    }
    let text = form.fields[0].editor.text.as_str();
    if text.len() > super::BODY_CAP {
        return Err("Thread body exceeds64KiB; nothing truncated".into());
    }
    Ok(text)
}
/// WORKER ONLY, after actual source/UID/parent/draft closure. Summary cloning
/// supplies shared Form presentation, not account/mutation authority.
pub fn form_from_draft(draft: ScopedThreadDraftV1) -> Result<Form, String> {
    if draft.body().len() > super::BODY_CAP {
        return Err("Thread draft exceeds64KiB before materialization".into());
    }
    let identity = draft.identity();
    if identity.login.len() > 256
        || identity.host.len() > 253
        || crate::storage::Scope::github(&identity.host, identity.account_id)
            .ok()
            .as_ref()
            != Some(draft.intent().scope())
    {
        return Err("Thread draft has no matching actual closed account".into());
    }
    let intent = draft.intent().clone();
    let parent = &intent.prepared.parent_v4;
    let mut form = Form::pr(PrIntent {
        kind: PrFormKind::Comment,
        target: parent.summary().clone(),
        identity: identity.clone(),
    })?;
    let original_lines:Arc<[String]>=vec![format!("{} • original observed target, not permission",intent.action().label()),format!("Exact thread {}",intent.thread().node_id),format!("Original {} PR #{} • repository ID {} • PR ID {}",parent.repository(),parent.number(),parent.repository_id(),parent.database_id()),format!("Current PR H {} • base {}",parent.head_sha(),parent.base_tip_sha()),format!("Observed state {:?} • outdated {:?} • subject {:?}",intent.thread().resolved,intent.thread().outdated,intent.thread().subject_type),"Reply root is selected only by actual replyTo:null and REST ID; observed parent-comment commit is not PR H. Fresh Preview rechecks all of this.".into(),"Restored publication metadata is history, never approval or resend permission.".into()].into_iter().map(|line|crate::git::display_bytes(line.as_bytes())).collect::<Vec<_>>().into();
    if original_lines.iter().map(String::len).sum::<usize>() > 64 * 1024 {
        return Err("Thread target display exceeds bounds".into());
    }
    let state = ThreadComposerStateV1 {
        intent: intent.clone(),
        identity: identity.clone(),
        ticket: Arc::new(PresentationTicketV1),
        revision: Some(0),
        preview: None,
        lines: Arc::from([]),
        original_lines,
        draft_key: Arc::from(draft.draft_key()),
    };
    form.title = format!(
        "{} • {}#{}",
        intent.action().label(),
        parent.repository(),
        parent.number()
    );
    if intent.action() == ThreadPublicationActionV1::Reply {
        form.fields[0].editor = crate::editor::TextBuffer::new(draft.body())?;
    } else {
        form.fields.clear();
        form.focus = 0;
        form.active_field = 0;
    }
    form.publication = draft.publication().cloned();
    form.restored_draft = !draft.body().is_empty() || draft.publication().is_some();
    form.purpose = FormPurpose::PrThread(Box::new(state));
    // Register this actual current intent through the normal autosave producer,
    // including an untouched state-only action. Publication may then capture
    // that real revision; it must never manufacture a missing producer entry.
    form.dirty = form.publication.is_none();
    Ok(form)
}
pub fn request(form: &Form) -> Result<Arc<ThreadComposerRequestV1>, String> {
    let FormPurpose::PrThread(state) = &form.purpose else {
        return Err("Not an original thread composer".into());
    };
    let text = body(form)?;
    if state.intent.action() == ThreadPublicationActionV1::Reply && text.trim().is_empty() {
        return Err("Thread reply needs nonempty bounded text before Preview".into());
    }
    if form.publication.is_some() {
        return Err("Original thread publication is history; inspect without resending".into());
    }
    preflight_draft(form)?;
    Ok(Arc::new(ThreadComposerRequestV1 {
        ticket: state.ticket.clone(),
        revision: state
            .revision
            .ok_or("Thread presentation retired; reopen explicitly")?,
        intent: state.intent.clone(),
        body: Arc::from(text),
        preview: state.preview.clone(),
        draft_key: state.draft_key.clone(),
    }))
}
pub fn accept_preview(
    form: &mut Form,
    request: &Arc<ThreadComposerRequestV1>,
    preview: Arc<ScopedThreadPreviewV1>,
) -> bool {
    if !request.matches_form(form)
        || !Arc::ptr_eq(request.intent(), preview.intent())
        || request.body().as_ref() != preview.body()
    {
        return false;
    }
    let FormPurpose::PrThread(state) = &mut form.purpose else {
        return false;
    };
    state.identity = preview.identity().clone();
    state.lines = preview.prepared_lines().clone();
    let display_v1 = preview.display_v1().clone();
    state.preview = Some(preview);
    form.preview = true;
    form.preview_requested = false;
    form.submit_requested = false;
    form.preview_scroll = 0;
    form.error.clear();
    form.install_prepared_preview_display_v1(display_v1);
    true
}
pub fn confirmed_preview(form: &Form) -> Result<Arc<ScopedThreadPreviewV1>, String> {
    if !form.preview {
        return Err("Inspect a fresh exact thread Preview before Confirm".into());
    }
    let request = request(form)?;
    request
        .preview()
        .cloned()
        .filter(|preview| {
            Arc::ptr_eq(preview.intent(), request.intent())
                && preview.body() == request.body().as_ref()
        })
        .ok_or("Fresh matching thread preview required".into())
}
pub fn validate_input(form: &Form, incoming: &str) -> Result<(), String> {
    let FormPurpose::PrThread(state) = &form.purpose else {
        return Err("Not a thread composer".into());
    };
    if state.intent.action() != ThreadPublicationActionV1::Reply {
        return Err("Resolution changes state only; it does not send comment text".into());
    }
    if form.publication.is_some() && !incoming.is_empty() {
        return Err("Original thread publication is inspect-only; saved body retained".into());
    }
    let current = body(form)?;
    let selected = form.fields[0]
        .editor
        .selection()
        .map_or(0, |selection| selection.len());
    if incoming.len() > super::BODY_CAP
        || current
            .len()
            .saturating_sub(selected)
            .saturating_add(incoming.len())
            > super::BODY_CAP
    {
        return Err("Thread body exceeds64KiB; input refused before materialization".into());
    }
    Ok(())
}
pub fn replace_editor_result(form: &mut Form, value: &str) -> Result<(), String> {
    if form.publication.is_some() {
        return Err(
            "Original thread publication is inspect-only; editor output was not adopted".into(),
        );
    }
    if value.len() > super::BODY_CAP {
        return Err("Thread editor result exceeds64KiB; original draft retained".into());
    }
    validate_input_for_replace(form)?;
    form.invalidate_plan();
    form.fields[0].editor = crate::editor::TextBuffer::new(value)?;
    form.dirty = true;
    Ok(())
}
fn validate_input_for_replace(form: &Form) -> Result<(), String> {
    let FormPurpose::PrThread(state) = &form.purpose else {
        return Err("Not a thread composer".into());
    };
    if state.intent.action() != ThreadPublicationActionV1::Reply {
        return Err("Resolution has no comment editor".into());
    }
    body(form).map(|_| ())
}
fn preflight_draft(form: &Form) -> Result<(), String> {
    super::plan::preflight_encoded_draft_v1(body(form)?, form.publication.as_ref())
        .map(|_| ())
        .map_err(|error| error.message().into())
}
pub fn encoded_draft(form: &Form) -> Result<String, String> {
    super::plan::encode_draft(body(form)?, form.publication.as_ref())
        .map_err(|error| error.message().into())
}
