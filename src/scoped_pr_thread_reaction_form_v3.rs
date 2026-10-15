//! Shared no-body Form presentation for a fresh selected-comment reaction.
//! Exact ticket/intent/Preview ownership, no restored approval or authority.
use super::{ReactionDraftV3, ReactionPreviewV3, ThreadReactionIntentV3};
use crate::{
    forms::{Form, FormPurpose, PrFormKind, PrIntent},
    providers::gh::HostIdentity,
};
use std::sync::Arc;
struct TicketV3;
#[derive(Clone)]
pub struct ReactionComposerStateV3 {
    intent: Arc<ThreadReactionIntentV3>,
    identity: HostIdentity,
    ticket: Arc<TicketV3>,
    revision: Option<u64>,
    preview: Option<Arc<ReactionPreviewV3>>,
    lines: Arc<[String]>,
    original_lines: Arc<[String]>,
    key: Arc<str>,
}
impl std::fmt::Debug for ReactionComposerStateV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReactionComposerStateV3(<no body presentation; no permit>)")
    }
}
impl ReactionComposerStateV3 {
    pub fn intent(&self) -> &Arc<ThreadReactionIntentV3> {
        &self.intent
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn draft_key(&self) -> &str {
        &self.key
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
        self.revision = self.revision.and_then(|value| value.checked_add(1));
    }
    pub fn context(&self) -> String {
        format!(
            "{} • numeric action account {:?} • no text body • exact original reaction target",
            crate::git::display_bytes(self.identity.login.as_bytes()),
            self.identity.account_id
        )
    }
}
pub struct ReactionComposerRequestV3 {
    ticket: Arc<TicketV3>,
    revision: u64,
    intent: Arc<ThreadReactionIntentV3>,
    preview: Option<Arc<ReactionPreviewV3>>,
    key: Arc<str>,
}
impl std::fmt::Debug for ReactionComposerRequestV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReactionComposerRequestV3(<exact no-body ticket; no permit>)")
    }
}
impl ReactionComposerRequestV3 {
    pub fn intent(&self) -> &Arc<ThreadReactionIntentV3> {
        &self.intent
    }
    pub fn preview(&self) -> Option<&Arc<ReactionPreviewV3>> {
        self.preview.as_ref()
    }
    pub fn draft_key(&self) -> &str {
        &self.key
    }
    pub fn matches_form(&self, form: &Form) -> bool {
        let FormPurpose::PrThreadReactionV3(state) = &form.purpose else {
            return false;
        };
        form.fields.is_empty()
            && Arc::ptr_eq(&self.ticket, &state.ticket)
            && state.revision == Some(self.revision)
            && Arc::ptr_eq(&self.intent, &state.intent)
            && Arc::ptr_eq(&self.key, &state.key)
            && match (&self.preview, &state.preview) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
    pub(crate) fn owns_presentation(&self, form: &Form) -> bool {
        matches!(&form.purpose,FormPurpose::PrThreadReactionV3(state)if Arc::ptr_eq(&self.ticket,&state.ticket)&&Arc::ptr_eq(&self.intent,&state.intent))
    }
    pub(crate) fn retire_busy_presentation(&self, form: &mut Form) -> bool {
        if !self.owns_presentation(form) {
            return false;
        }
        form.submitting = false;
        form.invalidate_plan();
        true
    }
}
pub fn retire_invalid_form(app: &mut crate::app::App) -> bool {
    let Some(intent) = app.form.as_ref().and_then(|form| match &form.purpose {
        FormPurpose::PrThreadReactionV3(state)
            if app.view != crate::app::View::Inbox || !state.intent.owns(app) =>
        {
            Some(state.intent.clone())
        }
        _ => None,
    }) else {
        return false;
    };
    let original = intent.original();
    if crate::remote_scope_gate_v1::retire_original_account_mismatch(
        app,
        &original.source,
        &original.active,
    ) {
        return true;
    }
    app.form = None;
    app.form_dismissed = true;
    if app.overlay == Some(crate::app::Overlay::Form) {
        app.overlay = None;
    }
    app.hits.clear();
    true
}
/// Worker only, after original private source/numeric-principal SQL closure.
pub fn form_from_draft(draft: ReactionDraftV3) -> Result<Form, String> {
    let identity = draft.identity();
    let intent = draft.intent().clone();
    if !draft.body().is_empty()
        || identity.login.len() > 256
        || identity.host.len() > 253
        || crate::storage::Scope::github(&identity.host, identity.account_id)
            .ok()
            .as_ref()
            != Some(intent.scope())
    {
        return Err("Actual closed reaction actor/empty-body presentation unavailable".into());
    }
    let parent = intent.listed.actual.parent();
    let mut form = Form::pr(PrIntent {
        kind: PrFormKind::Comment,
        target: parent.summary().clone(),
        identity: identity.clone(),
    })?;
    let state = ReactionComposerStateV3 {
        intent: intent.clone(),
        identity: identity.clone(),
        ticket: Arc::new(TicketV3),
        revision: Some(0),
        preview: None,
        lines: Arc::from([]),
        original_lines: draft.lines().clone(),
        key: Arc::from(draft.draft_key()),
    };
    form.title = format!(
        "{} • {}#{}",
        intent.action.label(),
        parent.repository(),
        parent.number()
    );
    form.fields.clear();
    form.focus = 0;
    form.active_field = 0;
    form.publication = None;
    form.restored_draft = draft.restored();
    // Genuine ordinary autosave establishes a real revision even for an
    // untouched no-body form on an already-used shared draft producer.
    form.dirty = true;
    form.purpose = FormPurpose::PrThreadReactionV3(Box::new(state));
    Ok(form)
}
pub fn request(form: &Form) -> Result<Arc<ReactionComposerRequestV3>, String> {
    let FormPurpose::PrThreadReactionV3(state) = &form.purpose else {
        return Err("Not an original reaction Form".into());
    };
    if !form.fields.is_empty() || form.publication.is_some() {
        return Err(
            "Reaction has no text body; protected history is inspect only, never resend".into(),
        );
    }
    Ok(Arc::new(ReactionComposerRequestV3 {
        ticket: state.ticket.clone(),
        revision: state
            .revision
            .ok_or("Original reaction presentation retired")?,
        intent: state.intent.clone(),
        preview: state.preview.clone(),
        key: state.key.clone(),
    }))
}
pub fn accept_preview(
    form: &mut Form,
    request: &Arc<ReactionComposerRequestV3>,
    preview: Arc<ReactionPreviewV3>,
) -> bool {
    if !request.matches_form(form)
        || !Arc::ptr_eq(request.intent(), preview.intent())
        || !preview.body().is_empty()
    {
        return false;
    }
    let display_v1 = preview.display_v1().clone();
    let FormPurpose::PrThreadReactionV3(state) = &mut form.purpose else {
        return false;
    };
    state.identity = preview.identity().clone();
    state.lines = preview.lines().clone();
    state.preview = Some(preview);
    form.install_prepared_preview_display_v1(display_v1);
    form.preview = true;
    form.preview_requested = false;
    form.submit_requested = false;
    form.preview_scroll = 0;
    form.error.clear();
    true
}
pub fn confirmed_preview(form: &Form) -> Result<Arc<ReactionPreviewV3>, String> {
    if !form.preview {
        return Err("Inspect a fresh reaction Preview first".into());
    }
    request(form)?
        .preview()
        .cloned()
        .ok_or("Fresh exact reaction approval unavailable".into())
}
pub fn encoded_draft(form: &Form) -> Result<String, String> {
    if !matches!(form.purpose, FormPurpose::PrThreadReactionV3(_)) || !form.fields.is_empty() {
        return Err("Not an exact no-body reaction intent".into());
    }
    super::encode_draft("", form.publication.as_ref())
        .map_err(|failure| failure.message().to_owned())
}
