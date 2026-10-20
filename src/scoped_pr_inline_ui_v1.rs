//! Pure presentation for a genuine scoped inline draft. No transport, storage,
//! permission constructor, retry or restored approval lives in this module.
use super::{ScopedInlineDraftV1, ScopedInlineIntentV1, ScopedInlinePreviewV1};
use crate::{
    forms::{Form, FormPurpose, PrFormKind, PrIntent},
    providers::gh::HostIdentity,
};
use std::sync::Arc;

pub const INLINE_BODY_BYTES_V1: usize = 65_536;
pub fn retire_invalid_form(app: &mut crate::app::App) -> bool {
    if !app.form.as_ref().is_some_and(|form|matches!(&form.purpose,FormPurpose::PrInline(state) if app.view!=crate::app::View::Inbox || !state.intent().owns(app))) {return false}
    app.form = None;
    app.form_dismissed = true;
    app.hits.clear();
    if app.overlay == Some(crate::app::Overlay::Form) {
        app.overlay = None;
    }
    true
}
struct PresentationTicketV1;

#[derive(Clone)]
pub struct ScopedInlineComposerStateV1 {
    pub(crate) native_origin_v1: Option<Arc<super::native_entry_v1::NativeInlineOriginV1>>,
    intent: Arc<ScopedInlineIntentV1>,
    identity: HostIdentity,
    ticket: Arc<PresentationTicketV1>,
    revision: Option<u64>,
    preview: Option<Arc<ScopedInlinePreviewV1>>,
    preview_lines: Arc<[String]>,
    draft_key: Arc<str>,
    anchor_lines: Arc<[String]>,
    anchor_text: Arc<str>,
    pub(crate) anchor_open: bool,
    pub(crate) anchor_scroll: usize,
}
impl std::fmt::Debug for ScopedInlineComposerStateV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedInlineComposerStateV1(<original presentation; no approval>)")
    }
}
impl ScopedInlineComposerStateV1 {
    pub fn intent(&self) -> &Arc<ScopedInlineIntentV1> {
        &self.intent
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn preview(&self) -> Option<&Arc<ScopedInlinePreviewV1>> {
        self.preview.as_ref()
    }
    pub fn draft_key(&self) -> &str {
        &self.draft_key
    }
    pub fn anchor_lines(&self) -> &[String] {
        &self.anchor_lines
    }
    pub fn plan_lines(&self) -> &[String] {
        if self.preview.is_some() {
            &self.preview_lines
        } else {
            &self.anchor_lines
        }
    }
    pub fn ready(&self) -> bool {
        self.revision.is_some() && self.preview.is_some()
    }
    pub fn invalidate(&mut self) {
        self.preview = None;
        self.preview_lines = Arc::from([]);
        self.revision = self.revision.and_then(|revision| revision.checked_add(1));
    }
    pub fn context(&self) -> String {
        format!(
            "@{} • account {} • {} lines {}–{} • F2 Anchor details",
            crate::app::safe_text(&self.identity.login),
            self.identity.account_id.unwrap_or(0),
            self.side(),
            self.intent.start(),
            self.intent.end()
        )
    }
    fn side(&self) -> &'static str {
        match self.intent.side() {
            crate::providers::gh::DiffSide::Left => "LEFT",
            crate::providers::gh::DiffSide::Right => "RIGHT",
        }
    }
}

/// Scheduling intent only. A parent worker owner additionally checks the
/// actual App/source/queue/token/deadline before any result effects.
pub struct InlineComposerRequestV1 {
    ticket: Arc<PresentationTicketV1>,
    revision: u64,
    intent: Arc<ScopedInlineIntentV1>,
    body: Arc<str>,
    preview: Option<Arc<ScopedInlinePreviewV1>>,
}
impl std::fmt::Debug for InlineComposerRequestV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InlineComposerRequestV1(<original presentation/body; no write authority>)")
    }
}
impl InlineComposerRequestV1 {
    pub fn intent(&self) -> &Arc<ScopedInlineIntentV1> {
        &self.intent
    }
    pub fn body(&self) -> &Arc<str> {
        &self.body
    }
    pub fn preview(&self) -> Option<&Arc<ScopedInlinePreviewV1>> {
        self.preview.as_ref()
    }
    /// Exact original Form allocation + input epoch; a stale owner cannot drop
    /// a newer edit/Preview or another Form merely sharing this source.
    pub(crate) fn owns_presentation_v14(&self, form: &Form) -> bool {
        let FormPurpose::PrInline(state) = &form.purpose else {
            return false;
        };
        Arc::ptr_eq(&self.ticket, &state.ticket)
            && state.revision == Some(self.revision)
            && Arc::ptr_eq(&self.intent, &state.intent)
            && same_preview(self.preview.as_ref(), state.preview.as_ref())
    }
    pub fn matches_form(&self, form: &Form) -> bool {
        let FormPurpose::PrInline(state) = &form.purpose else {
            return false;
        };
        Arc::ptr_eq(&self.ticket, &state.ticket)
            && state.revision == Some(self.revision)
            && Arc::ptr_eq(&self.intent, &state.intent)
            && same_preview(self.preview.as_ref(), state.preview.as_ref())
            && body(form).is_ok_and(|body| body == self.body.as_ref())
    }
    /// Retirement only, never approval: the original presentation can release
    /// its busy flag after an out-of-band edit, but cannot touch a new Form.
    pub(crate) fn retire_busy_presentation(&self, form: &mut Form) -> bool {
        let FormPurpose::PrInline(state) = &form.purpose else {
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
fn same_preview(
    a: Option<&Arc<ScopedInlinePreviewV1>>,
    b: Option<&Arc<ScopedInlinePreviewV1>>,
) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}
fn body(form: &Form) -> Result<&str, String> {
    if form.fields.len() != 1 || form.fields[0].key != "body" {
        return Err("Inline composer has no exact body field".into());
    }
    let value = &form.fields[0].editor.text;
    if value.len() > INLINE_BODY_BYTES_V1 {
        return Err(
            "Inline text exceeds 65536 UTF8 bytes; nothing was truncated or published".into(),
        );
    }
    Ok(value)
}

/// WORKER ONLY: prepare presentation after closing the actual draft under its
/// original source. Foreground admission keeps only this prepared Form/ticket.
/// Cloning a genuine summary below is presentation, never permission.
pub fn form_from_draft(draft: ScopedInlineDraftV1) -> Result<Form, String> {
    if draft.body().len() > INLINE_BODY_BYTES_V1 {
        return Err("Inline draft exceeds 65536 UTF8 bytes; no field was materialized".into());
    }
    let identity = draft.identity();
    if identity.login.len() > 256
        || identity.host.len() > 253
        || crate::storage::Scope::github(&identity.host, identity.account_id)
            .ok()
            .as_ref()
            != Some(draft.intent().scope())
    {
        return Err("Inline draft has no matching closed numeric account".into());
    }
    let intent = draft.intent().clone();
    let side = match intent.side() {
        crate::providers::gh::DiffSide::Left => "LEFT",
        crate::providers::gh::DiffSide::Right => "RIGHT",
    };
    let mut anchor_lines =
        vec!["ORIGINAL INLINE ANCHOR • navigation intent, not permission".into()];
    if let Some(from) = draft.carried_from() {
        // Explicit re-anchor: text only; this NEW selection is the anchor.
        anchor_lines.push(format!("RE-ANCHORED • text carried from {from}"));
        anchor_lines.push("The PR changed since that selection. Check these new lines fit the text before Preview.".into());
    }
    anchor_lines.extend::<[String; 9]>([
        format!("Canonical API filename: {}",crate::git::display_bytes(intent.server_path().as_bytes())),
        format!("Selected image path: {}",crate::git::display_bytes(intent.selected_image_path())),
        format!("{side} lines {}–{}",intent.start(),intent.end()),
        "Publication commit H:".into(),intent.review_head().into(),
        "Old content common starting point M:".into(),intent.merge_base().into(),
        "LEFT must end on a deleted line; unchanged context needs an explicitly selected RIGHT range. No automatic side conversion.".into(),
        "Edit/paste/restore never publishes. Fresh Preview and explicit Confirm independently recheck current source, account, head and storage.".into(),
    ]);
    let anchor_lines: Arc<[String]> = anchor_lines.into();
    if anchor_lines.iter().map(String::len).sum::<usize>() > 64 * 1024 {
        return Err("Inline anchor exceeds bounded display metadata".into());
    }
    let anchor_text = Arc::from(anchor_lines.join("\n"));
    let state = ScopedInlineComposerStateV1 {
        native_origin_v1: None,
        intent: intent.clone(),
        identity: identity.clone(),
        ticket: Arc::new(PresentationTicketV1),
        revision: Some(0),
        preview: None,
        preview_lines: Arc::from([]),
        draft_key: Arc::from(draft.draft_key()),
        anchor_lines,
        anchor_text,
        anchor_open: false,
        anchor_scroll: 0,
    };
    let mut form = Form::pr(PrIntent {
        kind: PrFormKind::Comment,
        target: intent.proof.observed().parent().summary().clone(),
        identity: identity.clone(),
    })?;
    form.title = format!(
        "Inline comment • {}#{} • {side} {}–{}{}",
        intent.proof.observed().parent().repository(),
        intent.proof.observed().parent().number(),
        intent.start(),
        intent.end(),
        if draft.carried_from().is_some() {
            " • re-anchored"
        } else {
            ""
        }
    );
    form.fields[0].editor = crate::editor::TextBuffer::new(draft.body())?;
    form.publication = draft.publication().cloned();
    form.restored_draft = !draft.body().is_empty() || draft.publication().is_some();
    form.purpose = FormPurpose::PrInline(Box::new(state));
    Ok(form)
}
pub(crate) fn bind_native_origin(
    form: &mut Form,
    origin: Arc<super::native_entry_v1::NativeInlineOriginV1>,
) {
    if let FormPurpose::PrInline(state) = &mut form.purpose {
        if origin.matches_intent(state.intent()) {
            state.native_origin_v1 = Some(origin);
        }
    }
}

/// Check bytes BEFORE trim, Arc allocation or any editor/worker body clone.
pub fn request(form: &Form) -> Result<Arc<InlineComposerRequestV1>, String> {
    let FormPurpose::PrInline(state) = &form.purpose else {
        return Err("Not an original inline composer".into());
    };
    let text = body(form)?;
    if text.trim().is_empty() {
        return Err("Inline comment needs nonempty text before Preview".into());
    }
    let revision = state
        .revision
        .ok_or("Inline presentation retired; explicitly reopen its range")?;
    if form.publication.is_some() {
        return Err(
            "Original inline publication metadata is history, never permission to resend".into(),
        );
    }
    preflight_encoded_draft(form)?;
    Ok(Arc::new(InlineComposerRequestV1 {
        ticket: state.ticket.clone(),
        revision,
        intent: state.intent.clone(),
        body: Arc::from(text),
        preview: state.preview.clone(),
    }))
}
pub fn accept_preview(
    form: &mut Form,
    request: &Arc<InlineComposerRequestV1>,
    preview: Arc<ScopedInlinePreviewV1>,
) -> bool {
    if !request.matches_form(form)
        || !Arc::ptr_eq(request.intent(), preview.intent())
        || request.body().as_ref() != preview.body()
    {
        return false;
    }
    if preview.body().len() > INLINE_BODY_BYTES_V1
        || preview.lines().iter().map(String::len).sum::<usize>() > 64 * 1024
    {
        return false;
    }
    let FormPurpose::PrInline(state) = &mut form.purpose else {
        return false;
    };
    state.identity = preview.identity().clone();
    state.preview_lines = preview.visible_lines_v1().clone();
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
pub fn confirmed_preview(form: &Form) -> Result<Arc<ScopedInlinePreviewV1>, String> {
    let request = request(form)?;
    request
        .preview()
        .cloned()
        .filter(|preview| {
            Arc::ptr_eq(preview.intent(), request.intent())
                && preview.body() == request.body().as_ref()
        })
        .ok_or("Choose fresh Preview and inspect the exact anchor/body before Confirm".into())
}
pub fn validate_input(form: &Form, incoming: &str) -> Result<(), String> {
    let current = body(form)?;
    let selection = form.fields[0]
        .editor
        .selection()
        .map_or(0, |selection| selection.len());
    if incoming.len() > INLINE_BODY_BYTES_V1
        || current
            .len()
            .saturating_sub(selection)
            .saturating_add(incoming.len())
            > INLINE_BODY_BYTES_V1
    {
        return Err(
            "Inline text exceeds 65536 UTF8 bytes; edit refused without truncation or publication"
                .into(),
        );
    }
    Ok(())
}
/// Exact borrowed envelope preflight without cloning body or allocating escaped
/// JSON beyond the same 128KiB recovery contract. Never silently truncates.
fn write_encoded_draft(form: &Form, writer: impl std::io::Write) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Values<'a> {
        body: &'a str,
    }
    #[derive(serde::Serialize)]
    struct Envelope<'a> {
        version: u8,
        values: Values<'a>,
        publication: &'a Option<crate::pr_write_journal::PrPublicationReference>,
    }
    if !matches!(form.purpose, FormPurpose::PrInline(_)) {
        return Err("Not an original inline composer".into());
    }
    let value = Envelope {
        version: 2,
        values: Values { body: body(form)? },
        publication: &form.publication,
    };
    serde_json::to_writer(writer, &value).map_err(|_| {
        "Encoded inline draft exceeds128KiB; original saved draft retained".to_string()
    })
}
fn preflight_encoded_draft(form: &Form) -> Result<(), String> {
    struct Bound(usize);
    impl std::io::Write for Bound {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|len| *len <= 128 * 1024)
                .ok_or_else(|| std::io::Error::other("Inline encoded draft exceeds128KiB"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    write_encoded_draft(form, Bound(0))
}
pub fn encoded_draft(form: &Form) -> Result<String, String> {
    preflight_encoded_draft(form)?;
    let mut bytes = Vec::new();
    write_encoded_draft(form, &mut bytes)?;
    String::from_utf8(bytes).map_err(|_| "Inline draft could not be encoded".into())
}
pub fn replace_editor_result(form: &mut Form, value: &str) -> Result<(), String> {
    if value.len() > INLINE_BODY_BYTES_V1 {
        return Err(
            "External editor result exceeds 65536 UTF8 bytes; original draft retained".into(),
        );
    }
    let FormPurpose::PrInline(_) = &form.purpose else {
        return Err("Not an original inline composer".into());
    };
    body(form)?;
    form.invalidate_plan(); // Even an identical restored editor body retires approval.
    form.fields[0].editor = crate::editor::TextBuffer::new(value)?;
    form.dirty = true;
    Ok(())
}

pub fn anchor_open(form: &Form) -> bool {
    matches!(&form.purpose,FormPurpose::PrInline(state) if state.anchor_open)
}
pub fn anchor_action(form: &mut Form, delta: Option<i32>) {
    let FormPurpose::PrInline(state) = &mut form.purpose else {
        return;
    };
    if let Some(delta) = delta {
        state.anchor_scroll = state.anchor_scroll.saturating_add_signed(delta as isize)
    } else {
        state.anchor_open = !state.anchor_open;
        state.anchor_scroll = 0;
    }
}
/// Read-only complete escaped original metadata, never a live-target proof.
/// Drawing reads only bounded prepared lines; no provider/cache/filesystem work.
pub fn draw_anchor(
    frame: &mut ratatui::Frame,
    form: &mut Form,
    rect: ratatui::layout::Rect,
    theme: crate::theme::Theme,
) -> Vec<crate::app::HitRegion> {
    use crate::{
        app::{Action, HitRegion},
        forms::FormAction,
        theme::SemanticToken,
    };
    use ratatui::{
        layout::{Margin, Rect},
        widgets::{Block, Borders, Clear, Paragraph},
    };
    let FormPurpose::PrInline(state) = &mut form.purpose else {
        return vec![];
    };
    form.field_rects.clear();
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(" Original inline anchor • observed intent, not permission ")
            .style(theme.base()),
        rect,
    );
    let inner = rect.inner(Margin::new(1, 1));
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(2),
    );
    let lines = crate::ui::notice_lines(state.anchor_text.as_ref(), body.width);
    let total = lines.len();
    let paragraph = Paragraph::new(lines);
    state.anchor_scroll = state
        .anchor_scroll
        .min(total.saturating_sub(body.height as usize));
    frame.render_widget(
        paragraph
            .scroll((state.anchor_scroll.min(u16::MAX as usize) as u16, 0))
            .style(theme.style(SemanticToken::Normal)),
        body,
    );
    frame.render_widget(
        Paragraph::new(format!(
            "Observed anchor only • {}/{} rows • nothing published",
            state.anchor_scroll + 1,
            total.max(1)
        ))
        .style(theme.style(SemanticToken::Muted)),
        Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 1),
    );
    let mut x = inner.x;
    let mut hits = Vec::new();
    for (label, action) in [
        ("[↑ Up]", FormAction::InlineAnchorScroll(-3)),
        ("[↓ Down]", FormAction::InlineAnchorScroll(3)),
        ("[Esc Back to draft]", FormAction::InlineAnchor),
    ] {
        let width = (label.len() as u16).min(inner.right().saturating_sub(x));
        if width == 0 {
            continue;
        }
        let rect = Rect::new(x, inner.bottom().saturating_sub(1), width, 1);
        frame.render_widget(
            Paragraph::new(label).style(theme.style(SemanticToken::Focus)),
            rect,
        );
        hits.push(HitRegion {
            rect,
            action: Action::Form(action),
        });
        x = x.saturating_add(width + 1);
    }
    hits
}
