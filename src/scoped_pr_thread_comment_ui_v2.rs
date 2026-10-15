//! Shared Form presentation, never a source/capability or mutation backend.
use super::{CommentDraftV2, CommentPreviewV2, ThreadCommentIntentV2};
use crate::{
    forms::{Form, FormPurpose, PrFormKind, PrIntent},
    pr_write_journal::thread_comment_history_v2::CommentActionV2,
    providers::gh::HostIdentity,
};
use std::sync::Arc;
struct TicketV2;
#[derive(Clone)]
pub struct CommentComposerStateV2 {
    intent: Arc<ThreadCommentIntentV2>,
    identity: HostIdentity,
    ticket: Arc<TicketV2>,
    revision: Option<u64>,
    preview: Option<Arc<CommentPreviewV2>>,
    lines: Arc<[String]>,
    original_lines: Arc<[String]>,
    key: Arc<str>,
}
impl std::fmt::Debug for CommentComposerStateV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CommentComposerStateV2(<presentation only>)")
    }
}
impl CommentComposerStateV2 {
    pub fn intent(&self) -> &Arc<ThreadCommentIntentV2> {
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
        format!("{} • numeric action account {:?} • exact selected comment • nothing sent before Confirm",crate::git::display_bytes(self.identity.login.as_bytes()),self.identity.account_id)
    }
}
pub struct CommentComposerRequestV2 {
    ticket: Arc<TicketV2>,
    revision: u64,
    intent: Arc<ThreadCommentIntentV2>,
    body: Arc<str>,
    preview: Option<Arc<CommentPreviewV2>>,
    key: Arc<str>,
}
impl std::fmt::Debug for CommentComposerRequestV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CommentComposerRequestV2(<exact ticket/revision/body; no permission>)")
    }
}
impl CommentComposerRequestV2 {
    pub fn intent(&self) -> &Arc<ThreadCommentIntentV2> {
        &self.intent
    }
    pub fn body(&self) -> &Arc<str> {
        &self.body
    }
    pub fn preview(&self) -> Option<&Arc<CommentPreviewV2>> {
        self.preview.as_ref()
    }
    pub fn draft_key(&self) -> &str {
        &self.key
    }
    pub fn matches_form(&self, form: &Form) -> bool {
        let FormPurpose::PrThreadCommentV2(state) = &form.purpose else {
            return false;
        };
        Arc::ptr_eq(&self.ticket, &state.ticket)
            && state.revision == Some(self.revision)
            && Arc::ptr_eq(&self.intent, &state.intent)
            && Arc::ptr_eq(&self.key, &state.key)
            && match (&self.preview, &state.preview) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && body(form).is_ok_and(|value| value == self.body.as_ref())
    }
    pub(crate) fn owns_presentation(&self, form: &Form) -> bool {
        matches!(&form.purpose,FormPurpose::PrThreadCommentV2(state)if Arc::ptr_eq(&self.ticket,&state.ticket)&&Arc::ptr_eq(&self.intent,&state.intent))
    }
    pub(crate) fn retire_busy_presentation(&self, form: &mut Form) -> bool {
        let FormPurpose::PrThreadCommentV2(state) = &form.purpose else {
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
pub fn retire_invalid_form(app: &mut crate::app::App) -> bool {
    let Some(intent) = app.form.as_ref().and_then(|form| match &form.purpose {
        FormPurpose::PrThreadCommentV2(state)
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
fn body(form: &Form) -> Result<&str, String> {
    let FormPurpose::PrThreadCommentV2(state) = &form.purpose else {
        return Err("Not an original selected-comment composer".into());
    };
    if state.intent.action == CommentActionV2::Delete {
        return if form.fields.is_empty() {
            Ok("")
        } else {
            Err("Delete does not send or edit comment text".into())
        };
    }
    if form.fields.len() != 1 || form.fields[0].key != "body" {
        return Err("Exact selected-comment body field required".into());
    }
    let value = form.fields[0].editor.text.as_str();
    if value.len() > super::BODY_CAP {
        return Err("Comment body exceeds64KiB before materialization".into());
    }
    Ok(value)
}
/// WORKER ONLY after original source/current principal/SQL closure.
pub fn form_from_draft(draft: CommentDraftV2) -> Result<Form, String> {
    let identity = draft.identity();
    let intent = draft.intent().clone();
    if draft.body().len() > super::BODY_CAP
        || identity.login.len() > 256
        || identity.host.len() > 253
        || crate::storage::Scope::github(&identity.host, identity.account_id)
            .ok()
            .as_ref()
            != Some(intent.scope())
    {
        return Err("Actual closed selected-comment actor/body exceeds bounds".into());
    }
    let parent = intent.listed.actual.parent();
    let mut form = Form::pr(PrIntent {
        kind: PrFormKind::Comment,
        target: parent.summary().clone(),
        identity: identity.clone(),
    })?;
    let actor = intent
        .comment()
        .author
        .as_ref()
        .map(|actor| crate::identity::ActorIdentity::github(&identity.host, &actor.actor))
        .unwrap_or_else(|| {
            crate::identity::ActorIdentity::github(
                &identity.host,
                &crate::providers::gh::Actor::default(),
            )
        });
    let original_lines=vec![format!("{} • original observation, NOT permission",intent.action.label()),
        format!("Exact selected comment #{} • {} • actual recorded author {}",intent.comment().database_id.unwrap_or(0),intent.comment().node_id,actor.label()),
        format!("{}#{} • repo #{} / PR #{} • current H {} • base {}",parent.repository(),parent.number(),parent.repository_id(),parent.database_id(),parent.head_sha(),parent.base_tip_sha()),
        format!("Independent observed parent-comment C {} • raw path {}",intent.comment().commit_sha.as_deref().unwrap_or("unavailable"),intent.comment().observed_path.as_deref().unwrap_or("unavailable")),
        "A fresh matching Preview is required. Protected history never restores this Form, approval, retry or permission.".into()]
        .into_iter().map(|line|crate::git::display_bytes(line.as_bytes())).collect::<Vec<_>>();
    if original_lines.iter().map(String::len).sum::<usize>() > 64 * 1024 {
        return Err("Selected-comment original labels exceed bounds".into());
    }
    let state = CommentComposerStateV2 {
        intent: intent.clone(),
        identity: identity.clone(),
        ticket: Arc::new(TicketV2),
        revision: Some(0),
        preview: None,
        lines: Arc::from([]),
        original_lines: original_lines.into(),
        key: Arc::from(draft.draft_key()),
    };
    form.title = format!(
        "{} • {}#{}",
        intent.action.label(),
        parent.repository(),
        parent.number()
    );
    if intent.action == CommentActionV2::Edit {
        form.fields[0].editor = crate::editor::TextBuffer::new(draft.body())?;
    } else {
        form.fields.clear();
        form.focus = 0;
        form.active_field = 0;
    }
    form.publication = None;
    form.restored_draft = draft.restored();
    form.dirty = true;
    form.purpose = FormPurpose::PrThreadCommentV2(Box::new(state));
    Ok(form)
}
pub fn request(form: &Form) -> Result<Arc<CommentComposerRequestV2>, String> {
    let FormPurpose::PrThreadCommentV2(state) = &form.purpose else {
        return Err("Not an original selected-comment Form".into());
    };
    let value = body(form)?;
    if form.publication.is_some() {
        return Err("Protected publication is inspect only • no resend".into());
    }
    if state.intent.action == CommentActionV2::Edit && value.trim().is_empty() {
        return Err("Edit needs nonempty body".into());
    }
    super::plan::preflight(value, None).map_err(|error| error.message().to_owned())?;
    Ok(Arc::new(CommentComposerRequestV2 {
        ticket: state.ticket.clone(),
        revision: state
            .revision
            .ok_or("Original comment presentation retired")?,
        intent: state.intent.clone(),
        body: Arc::from(value),
        preview: state.preview.clone(),
        key: state.key.clone(),
    }))
}
pub fn accept_preview(
    form: &mut Form,
    request: &Arc<CommentComposerRequestV2>,
    preview: Arc<CommentPreviewV2>,
) -> bool {
    if !request.matches_form(form)
        || !Arc::ptr_eq(request.intent(), preview.intent())
        || request.body().as_ref() != preview.body()
    {
        return false;
    }
    let FormPurpose::PrThreadCommentV2(state) = &mut form.purpose else {
        return false;
    };
    state.identity = preview.identity().clone();
    state.lines = preview.lines().clone();
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
pub fn confirmed_preview(form: &Form) -> Result<Arc<CommentPreviewV2>, String> {
    if !form.preview {
        return Err("Inspect a fresh comment Preview first".into());
    }
    request(form)?
        .preview()
        .cloned()
        .ok_or("Fresh exact comment approval unavailable".into())
}
pub fn validate_input(form: &Form, incoming: &str) -> Result<(), String> {
    let FormPurpose::PrThreadCommentV2(state) = &form.purpose else {
        return Err("Not selected-comment Form".into());
    };
    if state.intent.action != CommentActionV2::Edit || form.publication.is_some() {
        return Err("Selected action/history has no editable text".into());
    }
    let current = body(form)?;
    let selected = form.fields[0]
        .editor
        .selection()
        .map_or(0, |range| range.len());
    if incoming.len() > super::BODY_CAP
        || current
            .len()
            .saturating_sub(selected)
            .saturating_add(incoming.len())
            > super::BODY_CAP
    {
        return Err("Comment body exceeds64KiB • input refused before materialization".into());
    }
    Ok(())
}
pub fn replace_editor_result(form: &mut Form, value: &str) -> Result<(), String> {
    if value.len() > super::BODY_CAP {
        return Err("Comment editor result exceeds64KiB • original draft retained".into());
    }
    let FormPurpose::PrThreadCommentV2(state) = &form.purpose else {
        return Err("Not selected-comment Form".into());
    };
    if state.intent.action != CommentActionV2::Edit || form.publication.is_some() {
        return Err("Selected action/history has no editable body".into());
    }
    super::plan::preflight(value, None).map_err(|error| error.message().to_owned())?;
    form.invalidate_plan();
    form.fields[0].editor = crate::editor::TextBuffer::new(value)?;
    form.dirty = true;
    Ok(())
}
pub fn encoded_draft(form: &Form) -> Result<String, String> {
    super::encode_draft(body(form)?, form.publication.as_ref())
        .map_err(|error| error.message().to_owned())
}
