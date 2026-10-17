//! Shared TextBuffer/Form; presentation tickets are not source or approval.
use super::*;
use crate::{
    forms::{Form, FormPurpose, PrFormKind, PrIntent},
    providers::gh::HostIdentity,
};
#[derive(Clone)]
pub struct FileCommentComposerV75 {
    intent: Arc<FileCommentIntentV75>,
    identity: HostIdentity,
    ticket: Arc<()>,
    revision: Option<u64>,
    preview: Option<Arc<FileCommentPreviewV75>>,
    lines: Arc<[String]>,
    original: Arc<[String]>,
}
impl std::fmt::Debug for FileCommentComposerV75 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileCommentComposerV75(<observed file/body; no permit>)")
    }
}
impl FileCommentComposerV75 {
    pub(crate) fn intent(&self) -> &Arc<FileCommentIntentV75> {
        &self.intent
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn draft_key(&self) -> &str {
        self.intent.draft_key()
    }
    pub(crate) fn ready(&self) -> bool {
        self.revision.is_some() && self.preview.is_some()
    }
    pub(crate) fn invalidate(&mut self) {
        self.preview = None;
        self.lines = Arc::from([]);
        self.revision = self.revision.and_then(|n| n.checked_add(1));
    }
    pub(crate) fn plan_lines(&self) -> &[String] {
        if self.preview.is_some() {
            &self.lines
        } else {
            &self.original
        }
    }
    pub(crate) fn context(&self) -> String {
        format!(
            "{} • numeric acting account {:?} • whole FILE comment • no line/range",
            crate::git::display_bytes(self.identity.login.as_bytes()),
            self.identity.account_id
        )
    }
}
pub(crate) struct FileCommentRequestV75 {
    ticket: Arc<()>,
    revision: u64,
    intent: Arc<FileCommentIntentV75>,
    body: Arc<str>,
    preview: Option<Arc<FileCommentPreviewV75>>,
}
impl std::fmt::Debug for FileCommentRequestV75 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileCommentRequestV75(<bounded body/exact ticket; no permit>)")
    }
}
impl FileCommentRequestV75 {
    pub(crate) fn intent(&self) -> &Arc<FileCommentIntentV75> {
        &self.intent
    }
    pub(crate) fn body(&self) -> &Arc<str> {
        &self.body
    }
    pub(crate) fn preview(&self) -> Option<&Arc<FileCommentPreviewV75>> {
        self.preview.as_ref()
    }
    pub(crate) fn draft_key(&self) -> &str {
        self.intent.draft_key()
    }
    pub(crate) fn owns_presentation(&self, form: &Form) -> bool {
        matches!(&form.purpose,FormPurpose::PrFileCommentV75(state) if Arc::ptr_eq(&self.ticket,&state.ticket)&&Arc::ptr_eq(&self.intent,&state.intent))
    }
    pub(crate) fn matches_form(&self, form: &Form) -> bool {
        let FormPurpose::PrFileCommentV75(state) = &form.purpose else {
            return false;
        };
        self.owns_presentation(form)
            && state.revision == Some(self.revision)
            && match (&self.preview, &state.preview) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && body(form).is_ok_and(|s| s == self.body.as_ref())
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
fn body(form: &Form) -> Result<&str, String> {
    if !matches!(form.purpose, FormPurpose::PrFileCommentV75(_))
        || form.fields.len() != 1
        || form.fields[0].key != "body"
    {
        return Err("Exact original FILE body field required".into());
    }
    let value = form.fields[0].editor.text.as_str();
    if value.len() > super::BODY_CAP {
        return Err("FILE body exceeds64KiB; nothing truncated".into());
    }
    Ok(value)
}
/// WORKER ONLY, after bounded draft and actual original account/source close.
pub(crate) fn form_from_draft(draft: FileCommentDraftV75) -> Result<Form, String> {
    if draft.body().len() > super::BODY_CAP || draft.identity().login.len() > 4096 {
        return Err("Original FILE draft metadata exceeds bounds".into());
    }
    let intent = draft.intent().clone();
    let (view, _) = intent.observed();
    let parent = view.observed.parent();
    let identity = draft.identity().clone();
    if Scope::github(&identity.host, identity.account_id)
        .ok()
        .as_ref()
        != Some(intent.scope())
    {
        return Err("Actual closed FILE principal differs".into());
    }
    let mut form = Form::pr(PrIntent {
        kind: PrFormKind::Comment,
        target: parent.summary().clone(),
        identity: identity.clone(),
    })?;
    let original:Vec<String>=["FILE-level review comment • observed original member, not permission".into(),format!("Exact raw file {}",intent.path()),format!("Current observed PR H {} • base {}",parent.head_sha(),parent.base_tip_sha()),"No line1, side, range, existing thread or inline eligibility is fabricated. Server permission is authoritative.".into(),"Preview is read-only; explicit Confirm creates a fresh one-attempt original source/account/member/draft closure. Protected publication history never restores approval.".into()].iter().map(|s|crate::git::display_bytes(s.as_bytes())).collect();
    form.title = format!("FILE comment • {}#{}", parent.repository(), parent.number());
    form.fields[0].editor = crate::editor::TextBuffer::new(draft.body())?;
    form.purpose = FormPurpose::PrFileCommentV75(Box::new(FileCommentComposerV75 {
        intent,
        identity,
        ticket: Arc::new(()),
        revision: Some(0),
        preview: None,
        lines: Arc::from([]),
        original: original.into(),
    }));
    form.preview = false;
    form.preview_requested = false;
    form.submit_requested = false;
    form.submitting = false;
    form.error = if draft.restored() {
        "Saved unsent FILE draft restored • no approval restored".into()
    } else {
        String::new()
    };
    form.dirty = true;
    Ok(form)
}
pub(crate) fn request(form: &Form) -> Result<Arc<FileCommentRequestV75>, String> {
    let FormPurpose::PrFileCommentV75(state) = &form.purpose else {
        return Err("Not an original FILE composer".into());
    };
    if form.publication.is_some() {
        return Err("Original FILE publication is inspect-only • no retry/restore".into());
    }
    let value = body(form)?;
    super::plan::preflight_draft(value, None).map_err(|e| e.message().to_owned())?;
    Ok(Arc::new(FileCommentRequestV75 {
        ticket: state.ticket.clone(),
        revision: state
            .revision
            .ok_or("FILE presentation revision exhausted")?,
        intent: state.intent.clone(),
        body: Arc::from(value),
        preview: state.preview.clone(),
    }))
}
/// A queued click may be followed by Resize before capture. Recheck the
/// actual current prepared display here, not only in the earlier key handler.
pub(crate) fn confirmed_request_v82(form: &Form) -> Result<Arc<FileCommentRequestV75>, String> {
    if !form.preview || form.preview_display_v1.geometry_rejected {
        return Err("Inspect a fresh useful whole FILE Preview before explicit Confirm".into());
    }
    let request = request(form)?;
    if request.preview().is_none_or(|preview| {
        !Arc::ptr_eq(preview.intent(), request.intent())
            || preview.body() != request.body().as_ref()
    }) {
        return Err("Fresh matching FILE Preview required; nothing queued".into());
    }
    Ok(request)
}
pub(crate) fn accept_preview(
    form: &mut Form,
    request: &FileCommentRequestV75,
    preview: Arc<FileCommentPreviewV75>,
) -> bool {
    if !request.matches_form(form)
        || !Arc::ptr_eq(request.intent(), preview.intent())
        || request.body().as_ref() != preview.body()
        || form.publication.is_some()
    {
        return false;
    }
    let FormPurpose::PrFileCommentV75(state) = &mut form.purpose else {
        return false;
    };
    state.identity = preview.identity().clone();
    state.lines = preview.lines().clone();
    let display = preview.display_v1().clone();
    state.preview = Some(preview);
    form.preview = true;
    form.preview_requested = false;
    form.submit_requested = false;
    form.preview_scroll = 0;
    // Preview hides the body field. Keyboard focus belongs to a visible
    // footer control, not the now-unpainted editor or its old caret.
    form.focus = form.fields.len();
    form.error.clear();
    form.install_prepared_preview_display_v1(display);
    true
}
pub fn validate_input(form: &Form, incoming: &str) -> Result<(), String> {
    if form.publication.is_some() {
        return Err("Protected FILE body is inspect-only".into());
    }
    let value = body(form)?;
    let selected = form.fields[0].editor.selection().map_or(0, |s| s.len());
    if incoming.len() > super::BODY_CAP
        || value
            .len()
            .saturating_sub(selected)
            .saturating_add(incoming.len())
            > super::BODY_CAP
    {
        return Err("FILE input exceeds64KiB before materialization".into());
    }
    Ok(())
}
pub fn replace_editor_result(form: &mut Form, value: &str) -> Result<(), String> {
    if value.len() > super::BODY_CAP || form.publication.is_some() {
        return Err("FILE editor result refused before materialization; original retained".into());
    }
    body(form)?;
    super::plan::preflight_draft(value, None).map_err(|e| e.message().to_owned())?;
    form.invalidate_plan();
    form.fields[0].editor = crate::editor::TextBuffer::new(value)?;
    form.dirty = true;
    Ok(())
}
pub fn encoded_draft(form: &Form) -> Result<String, String> {
    super::plan::encode_draft(body(form)?, form.publication.as_ref())
        .map_err(|e| e.message().to_owned())
}
pub fn retire_invalid_form(app: &mut App) -> bool {
    let Some(intent) = app.form.as_ref().and_then(|f| match &f.purpose {
        FormPurpose::PrFileCommentV75(state) if !state.intent.owns(app) => {
            Some(state.intent.clone())
        }
        _ => None,
    }) else {
        return false;
    };
    if crate::scoped_pr_observed_tabs_v1::resources::threads_v1::retire_original_account_v75(
        app,
        &intent.original.source,
        &intent.original.active,
        &intent.original.navigation,
    ) {
        return true;
    }
    app.form = None;
    app.form_dismissed = true;
    if app.overlay == Some(Overlay::Form) {
        app.overlay = None;
    }
    app.hits.clear();
    true
}
