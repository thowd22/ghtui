//! Separate typed purpose INSIDE the existing shared Form and Review owner
//! union. No fake legacy review action, queue, source or Preview is invented.
use super::*;
use crate::forms::{Field, Form, FormPurpose, PrFormKind, PrIntent};
use preview_v3::{ReviewOperationPreviewV3, ReviewOperationRequestV3};
use std::sync::Weak;

#[derive(Clone)]
struct AvatarViewportV3 {
    ticket: Weak<super::super::super::ui_v2::PresentationTicketV2>,
    revision: Option<u64>,
    source: Weak<ScopedReviewOperationSourceV3>,
    preview: Option<Weak<ReviewOperationPreviewV3>>,
    scroll: usize,
    actors: usize,
}
#[derive(Clone)]
pub struct ReviewOperationComposerStateV3 {
    source: Arc<ScopedReviewOperationSourceV3>,
    pub(in super::super::super) ticket: Arc<super::super::super::ui_v2::PresentationTicketV2>,
    revision: Option<u64>,
    preview: Option<Arc<ReviewOperationPreviewV3>>,
    key: Arc<str>,
    input_visible: bool,
    confirmation_visible: bool,
    avatar_view: Option<AvatarViewportV3>,
}
impl std::fmt::Debug for ReviewOperationComposerStateV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "ReviewOperationComposerStateV3(<exact presentation; no publication authority>)",
        )
    }
}
impl ReviewOperationComposerStateV3 {
    pub fn operation_intent(&self) -> &Arc<ScopedReviewOperationIntentV3> {
        self.source.intent()
    }
    pub fn intent(&self) -> &Arc<ScopedReviewIntentV2> {
        self.source.intent().original()
    }
    pub fn identity(&self) -> &HostIdentity {
        self.display_source().identity()
    }
    pub fn draft_key(&self) -> &str {
        &self.key
    }
    pub fn input_visible(&self) -> bool {
        self.input_visible
    }
    pub fn ready(&self) -> bool {
        self.revision.is_some() && self.preview.is_some() && self.confirmation_visible
    }
    pub fn plan_lines(&self) -> &[String] {
        self.display_source().lines()
    }
    pub fn invalidate(&mut self) {
        self.preview = None;
        self.confirmation_visible = false;
        self.avatar_view = None;
        self.revision = self.revision.and_then(|revision| revision.checked_add(1));
    }
    pub fn context(&self) -> String {
        format!(
            "{} • current numeric UID {:?} • {} • fresh Preview + explicit Confirm",
            crate::git::display_bytes(self.identity().login.as_bytes()),
            self.identity().account_id,
            self.source.intent().action().label()
        )
    }
    fn display_source(&self) -> &Arc<ScopedReviewOperationSourceV3> {
        self.preview
            .as_ref()
            .map_or(&self.source, |preview| preview.source())
    }
    pub(crate) fn avatar_actors(
        &self,
        scroll: usize,
        preview: bool,
    ) -> Option<Vec<&crate::providers::gh::Actor>> {
        let marker = self.avatar_view.as_ref()?;
        if !self.input_visible
            || marker.scroll != scroll
            || preview != self.preview.is_some()
            || marker.revision != self.revision
            || !Weak::ptr_eq(&marker.ticket, &Arc::downgrade(&self.ticket))
            || !Weak::ptr_eq(&marker.source, &Arc::downgrade(self.display_source()))
            || !match (&marker.preview, &self.preview) {
                (None, None) => true,
                (Some(a), Some(b)) => Weak::ptr_eq(a, &Arc::downgrade(b)),
                _ => false,
            }
        {
            return None;
        }
        Some(
            self.display_source()
                .actors()
                .iter()
                .take(marker.actors)
                .filter_map(|actor| actor.observed_actor_v3())
                .collect(),
        )
    }
}
pub struct ReviewOperationComposerRequestV3 {
    ticket: Arc<super::super::super::ui_v2::PresentationTicketV2>,
    revision: u64,
    request: Arc<ReviewOperationRequestV3>,
    preview: Option<Arc<ReviewOperationPreviewV3>>,
    key: Arc<str>,
}
impl std::fmt::Debug for ReviewOperationComposerRequestV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "ReviewOperationComposerRequestV3(<exact original purpose/ticket/revision/body>)",
        )
    }
}
impl ReviewOperationComposerRequestV3 {
    pub fn request(&self) -> &Arc<ReviewOperationRequestV3> {
        &self.request
    }
    pub fn preview(&self) -> Option<&Arc<ReviewOperationPreviewV3>> {
        self.preview.as_ref()
    }
    pub fn draft_key(&self) -> &str {
        &self.key
    }
    pub fn owns_presentation(&self, form: &Form) -> bool {
        matches!(&form.purpose,FormPurpose::PrReviewOperationV3(state)
        if Arc::ptr_eq(&self.ticket,&state.ticket)&&Arc::ptr_eq(self.request.source(),&state.source))
    }
    pub fn matches_form(&self, form: &Form) -> bool {
        let FormPurpose::PrReviewOperationV3(state) = &form.purpose else {
            return false;
        };
        self.owns_presentation(form)
            && state.revision == Some(self.revision)
            && Arc::ptr_eq(&self.key, &state.key)
            && match (&self.preview, &state.preview) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
            && body(form).is_ok_and(|body| body == self.request.body())
    }
    pub(crate) fn retire_busy(&self, form: &mut Form) -> bool {
        if !self.owns_presentation(form) {
            return false;
        }
        form.submitting = false;
        form.invalidate_plan();
        true
    }
}
fn body(form: &Form) -> Result<&str, String> {
    let FormPurpose::PrReviewOperationV3(state) = &form.purpose else {
        return Err("Not a typed pending operation composer".into());
    };
    if state.source.intent().action().needs_body() {
        let field = form
            .fields
            .first()
            .filter(|field| field.key == "pending-body" && form.fields.len() == 1)
            .ok_or("Original pending operation body field unavailable")?;
        if field.editor.text.len() > 65_536 {
            return Err("Pending operation body exceeds64KiB".into());
        }
        Ok(&field.editor.text)
    } else if form.fields.is_empty() {
        Ok("")
    } else {
        Err("Bodyless requested effect has unexpected fields".into())
    }
}
pub fn form_from_source(
    source: Arc<ScopedReviewOperationSourceV3>,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Form, String> {
    clock(cancel, deadline).map_err(|_| "Original operation opener expired")?;
    let parent = source.intent().original().parent();
    let action = source.intent().action();
    let mut form = Form::pr(PrIntent {
        kind: PrFormKind::Comment,
        target: parent.summary().clone(),
        identity: source.identity().clone(),
    })?;
    form.title = format!(
        "{} • {}#{}",
        action.label(),
        parent.repository(),
        parent.number()
    );
    form.fields = if action.needs_body() {
        vec![Field::text(
            "pending-body",
            "Exact server PENDING comment",
            source.body(),
            true,
        )?]
    } else {
        vec![]
    };
    let key = source.key.clone();
    let restored = source.restored_body();
    let display = source.display().clone();
    form.purpose = FormPurpose::PrReviewOperationV3(Box::new(ReviewOperationComposerStateV3 {
        source,
        ticket: Arc::new(super::super::super::ui_v2::PresentationTicketV2),
        revision: Some(0),
        preview: None,
        key,
        input_visible: false,
        confirmation_visible: false,
        avatar_view: None,
    }));
    form.install_prepared_preview_display_v1(display);
    form.focus = 0;
    form.active_field = 0;
    form.restored_draft = restored;
    form.publication = None;
    form.dirty = true;
    clock(cancel, deadline).map_err(|_| "Original operation opener expired before result")?;
    Ok(form)
}
pub fn request(form: &Form) -> Result<Arc<ReviewOperationComposerRequestV3>, String> {
    let FormPurpose::PrReviewOperationV3(state) = &form.purpose else {
        return Err("Not a typed pending operation composer".into());
    };
    if !state.input_visible || form.publication.is_some() {
        return Err(
            "Inspect current useful original target; history never grants publication".into(),
        );
    }
    let request = ReviewOperationRequestV3::new(state.source.clone(), Arc::from(body(form)?))?;
    Ok(Arc::new(ReviewOperationComposerRequestV3 {
        ticket: state.ticket.clone(),
        revision: state
            .revision
            .ok_or("Original operation presentation retired")?,
        request,
        preview: state.preview.clone(),
        key: state.key.clone(),
    }))
}
pub fn accept_preview(
    form: &mut Form,
    request: &Arc<ReviewOperationComposerRequestV3>,
    preview: Arc<ReviewOperationPreviewV3>,
) -> bool {
    if !request.matches_form(form) || !Arc::ptr_eq(request.request(), preview.request()) {
        return false;
    }
    let FormPurpose::PrReviewOperationV3(state) = &mut form.purpose else {
        return false;
    };
    let display = preview.source().display().clone();
    state.preview = Some(preview);
    state.confirmation_visible = false;
    state.avatar_view = None;
    form.install_prepared_preview_display_v1(display);
    form.preview = true;
    form.preview_requested = false;
    form.submit_requested = false;
    form.preview_scroll = 0;
    form.error.clear();
    true
}
pub fn confirmed_preview(form: &Form) -> Result<Arc<ReviewOperationPreviewV3>, String> {
    let FormPurpose::PrReviewOperationV3(state) = &form.purpose else {
        return Err("Not a typed pending operation composer".into());
    };
    if !form.preview || !state.confirmation_visible {
        return Err("Inspect fresh useful Preview/Confirm controls before explicit Confirm".into());
    }
    request(form)?
        .preview()
        .cloned()
        .ok_or_else(|| "Actual pending operation Preview unavailable".into())
}
pub fn encoded_draft(form: &Form) -> Result<String, String> {
    draft_v3::encode(body(form)?, form.publication.as_ref())
}
pub fn retire_invalid_form(app: &mut App) -> bool {
    if super::super::super::native_v2::retire_invalid(app) {
        return true;
    }
    if !app.form.as_ref().is_some_and(|form|matches!(&form.purpose,FormPurpose::PrReviewOperationV3(state)if !state.intent().owns(app))){return false;}
    app.form = None;
    app.form_dismissed = true;
    if app.overlay == Some(crate::app::Overlay::Form) {
        app.overlay = None;
    }
    app.hits.clear();
    true
}
pub fn validate_input(form: &Form, incoming: &str) -> Result<(), String> {
    let value = body(form)?;
    if form.publication.is_some() {
        return Err("Protected original operation history is read-only".into());
    }
    let Some(field) = form.fields.get(form.focus) else {
        return Ok(());
    };
    let selected = field
        .editor
        .selection()
        .map_or(0, |selection| selection.len());
    if incoming.len() > 65_536
        || value
            .len()
            .saturating_sub(selected)
            .saturating_add(incoming.len())
            > 65_536
    {
        return Err("Pending body exceeds64KiB; nothing truncated".into());
    }
    Ok(())
}
pub fn replace_editor_result(form: &mut Form, value: &str) -> Result<(), String> {
    body(form)?;
    if form.publication.is_some() || value.len() > 65_536 {
        return Err("Original pending editor result refused; prior body retained".into());
    }
    let index = form.active_field;
    if index >= form.fields.len() {
        return Err("Original bodyless action has no editor".into());
    }
    let replacement = crate::editor::TextBuffer::new(value)?;
    form.invalidate_plan();
    form.fields[index].editor = replacement;
    form.dirty = true;
    Ok(())
}
pub fn begin_draw(form: &mut Form, usable: bool) {
    if let FormPurpose::PrReviewOperationV3(state) = &mut form.purpose {
        state.input_visible = usable;
        state.confirmation_visible = false;
        state.avatar_view = None;
    }
}
pub fn painted_preview(form: &mut Form, visible: bool) {
    if let FormPurpose::PrReviewOperationV3(state) = &mut form.purpose {
        state.confirmation_visible =
            visible && state.input_visible && state.preview.is_some() && form.preview;
    }
}
pub fn clear_layout(form: &mut Form) {
    begin_draw(form, false);
}
pub fn draw_requester(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    form: &Form,
    avatars: &crate::actor_avatar_states_v1::ActorAvatarStatesV1,
    theme: crate::theme::Theme,
) -> bool {
    let FormPurpose::PrReviewOperationV3(state) = &form.purpose else {
        return false;
    };
    if let Ok(actor) = state.display_source().requester() {
        actor.paint(frame, area, avatars, theme);
    }
    true
}
pub fn draw_authors(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    form: &mut Form,
    avatars: &crate::actor_avatar_states_v1::ActorAvatarStatesV1,
    theme: crate::theme::Theme,
) -> u16 {
    let FormPurpose::PrReviewOperationV3(state) = &mut form.purpose else {
        return 0;
    };
    if !state.input_visible || area.width < 4 || area.height == 0 {
        return 0;
    }
    let source = state.display_source().clone();
    let count = source.actors().len().min(usize::from(area.height)).min(3);
    for (index, actor) in source.actors().iter().take(count).enumerate() {
        let row = ratatui::layout::Rect::new(area.x, area.y + index as u16, area.width, 1);
        actor.paint(
            frame,
            ratatui::layout::Rect::new(row.x, row.y, 4, 1),
            avatars,
            theme,
        );
        let role = match (source.intent().action(), index) {
            (ReviewOperationActionV3::RerequestReview, _) => "Actual selected reviewer",
            (_, 0) => "Observed PENDING review author",
            _ => "Observed PENDING comment author",
        };
        frame.render_widget(
            ratatui::widgets::Paragraph::new(ratatui::text::Line::from(vec![
                ratatui::text::Span::raw(role),
                ratatui::text::Span::raw(" "),
                ratatui::text::Span::raw(actor.label()),
                ratatui::text::Span::raw(actor.uid_label_v3()),
            ]))
            .style(theme.style(crate::theme::SemanticToken::Actor)),
            ratatui::layout::Rect::new(row.x + 4, row.y, row.width - 4, 1),
        );
    }
    state.avatar_view = Some(AvatarViewportV3 {
        ticket: Arc::downgrade(&state.ticket),
        revision: state.revision,
        source: Arc::downgrade(&source),
        preview: state.preview.as_ref().map(Arc::downgrade),
        scroll: form.preview_scroll,
        actors: count,
    });
    count as u16
}
