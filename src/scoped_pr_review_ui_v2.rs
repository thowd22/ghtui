//! Shared Form presentation of genuine closed review data. Saved text/anchors
//! and in-memory Preview are NOT current source, approval or write permission.
use super::*;
use crate::{
    forms::{Field, Form, FormPurpose, PrFormKind, PrIntent},
    providers::gh::HostIdentity,
};
#[path = "scoped_pr_review_local_batch_v10.rs"]
pub mod local_batch_v10;

pub(super) struct PresentationTicketV2;
#[derive(Clone)]
pub struct ReviewComposerStateV2 {
    draft: Arc<ScopedReviewDraftV2>,
    selection: Option<Arc<ScopedReviewSelectionV2>>,
    action: ReviewRequestedActionV2,
    anchor: Option<Arc<ReviewAnchorMetadataV2>>,
    local_v10: local_batch_v10::LocalBatchV10,
    pub(super) ticket: Arc<PresentationTicketV2>,
    pub(super) revision: Option<u64>,
    pub(super) preview: Option<Arc<ScopedReviewPreviewV2>>,
    pub(super) header_actors_v7: Arc<[avatars_v7::PreparedActorV7]>,
    pub(super) avatar_view_v7: Option<avatars_v7::FormViewportV7>,
    identity: HostIdentity,
    lines: Arc<[String]>,
    original_lines: Arc<[String]>,
    key: Arc<str>,
    input_visible: bool,
    confirmation_visible: bool,
}
impl std::fmt::Debug for ReviewComposerStateV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReviewComposerStateV2(<actual presentation; no publication proof>)")
    }
}
impl ReviewComposerStateV2 {
    pub fn intent(&self) -> &Arc<ScopedReviewIntentV2> {
        self.draft.intent()
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
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
        if self.preview.is_some() {
            &self.lines
        } else {
            &self.original_lines
        }
    }
    pub(super) fn preview_actor_v7(&self, row: usize) -> Option<&avatars_v7::PreparedActorV7> {
        let actors = self.preview.as_ref()?.actor_rows_v7();
        actors
            .binary_search_by_key(&row, |(index, _)| *index)
            .ok()
            .map(|index| &actors[index].1)
    }
    pub fn invalidate(&mut self) {
        self.local_v10.clear_paint();
        self.preview = None;
        self.lines = Arc::from([]);
        self.confirmation_visible = false;
        self.avatar_view_v7 = None;
        self.revision = self.revision.and_then(|revision| revision.checked_add(1));
    }
    pub fn context(&self) -> String {
        format!(
            "{} • numeric UID {:?} • {} • fresh Preview + explicit Confirm required",
            crate::git::display_bytes(self.identity.login.as_bytes()),
            self.identity.account_id,
            self.action.label()
        )
    }
}
pub struct ReviewComposerRequestV2 {
    ticket: Arc<PresentationTicketV2>,
    revision: u64,
    request: Arc<ScopedReviewRequestV2>,
    summary: Arc<str>,
    comment: Arc<str>,
    preview: Option<Arc<ScopedReviewPreviewV2>>,
    key: Arc<str>,
    local_mask_v10: u32,
}
impl std::fmt::Debug for ReviewComposerRequestV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReviewComposerRequestV2(<exact ticket/revision; no approval>)")
    }
}
impl ReviewComposerRequestV2 {
    pub fn request(&self) -> &Arc<ScopedReviewRequestV2> {
        &self.request
    }
    pub fn preview(&self) -> Option<&Arc<ScopedReviewPreviewV2>> {
        self.preview.as_ref()
    }
    pub fn draft_key(&self) -> &str {
        &self.key
    }
    pub(crate) fn owns_presentation(&self, form: &Form) -> bool {
        matches!(&form.purpose,FormPurpose::PrReviewV2(state) if Arc::ptr_eq(&self.ticket,&state.ticket) && Arc::ptr_eq(self.request.intent(),state.intent()))
    }
    pub fn matches_form(&self, form: &Form) -> bool {
        let FormPurpose::PrReviewV2(state) = &form.purpose else {
            return false;
        };
        Arc::ptr_eq(&self.ticket, &state.ticket)
            && state.revision == Some(self.revision)
            && Arc::ptr_eq(self.request.intent(), state.intent())
            && Arc::ptr_eq(&self.key, &state.key)
            && self.local_mask_v10 == state.local_v10.mask()
            && form
                .fields
                .get(1.min(form.fields.len())..1.min(form.fields.len()) + state.local_v10.count())
                .is_some_and(|local| {
                    state.local_v10.matches_comments(
                        local,
                        state.draft.values(),
                        self.request.values().comments(),
                    )
                })
            && match (&self.preview, &state.preview) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && fields(form).is_ok_and(|(summary, comment)| {
                summary == self.summary.as_ref() && comment == self.comment.as_ref()
            })
    }
    pub(crate) fn retire_busy(&self, form: &mut Form) -> bool {
        let FormPurpose::PrReviewV2(state) = &form.purpose else {
            return false;
        };
        if !Arc::ptr_eq(&self.ticket, &state.ticket)
            || !Arc::ptr_eq(self.request.intent(), state.intent())
        {
            return false;
        }
        form.submitting = false;
        form.invalidate_plan();
        true
    }
}
fn fields(form: &Form) -> Result<(&str, &str), String> {
    let FormPurpose::PrReviewV2(state) = &form.purpose else {
        return Err("Not an original review composer".into());
    };
    let expected = usize::from(state.action != ReviewRequestedActionV2::Discard)
        + usize::from(state.anchor.is_some())
        + state.local_v10.count();
    if form.fields.len() != expected {
        return Err("Review fields no longer match their exact requested effect".into());
    }
    let local_start = usize::from(state.action != ReviewRequestedActionV2::Discard);
    state
        .local_v10
        .validate_fields(&form.fields[local_start..local_start + state.local_v10.count()])?;
    let summary = if state.action == ReviewRequestedActionV2::Discard {
        ""
    } else {
        let field = form
            .fields
            .first()
            .filter(|field| field.key == "summary")
            .ok_or("Original review summary field unavailable")?;
        field.editor.text.as_str()
    };
    let comment = if state.anchor.is_some() {
        form.fields
            .last()
            .filter(|field| field.key == "anchored-comment")
            .ok_or("Original anchored comment field unavailable")?
            .editor
            .text
            .as_str()
    } else {
        ""
    };
    if summary.len() > 65_536 || comment.len() > 65_536 {
        return Err("Review field exceeds64KiB; no truncation".into());
    }
    Ok((summary, comment))
}
fn requested(form: &Form, for_preview: bool) -> Result<Arc<ScopedReviewRequestV2>, String> {
    let FormPurpose::PrReviewV2(state) = &form.purpose else {
        return Err("Not an original review composer".into());
    };
    let (summary, comment) = fields(form)?;
    if state.action == ReviewRequestedActionV2::CreatePending {
        state.local_v10.preflight(
            &form.fields[1..1 + state.local_v10.count()],
            state.draft.values(),
            summary,
            state
                .anchor
                .as_ref()
                .filter(|_| !comment.trim().is_empty())
                .map(|anchor| (anchor.as_ref(), comment)),
        )?;
        let mut comments = state.local_v10.comments(
            &form.fields[1..1 + state.local_v10.count()],
            state.draft.values(),
        )?;
        if let Some(anchor) = &state.anchor {
            if comment.trim().is_empty() {
                if for_preview {
                    return Err(
                        "Write the exact selected comment or cancel; no invented empty anchor"
                            .into(),
                    );
                }
            } else {
                comments.push(ReviewDraftCommentV2 {
                    anchor: anchor.clone(),
                    body: Arc::from(comment),
                });
            }
        }
        let values = Arc::new(ReviewDraftValuesV2::replace(
            Arc::from(summary),
            comments.into(),
        )?);
        Ok(ScopedReviewRequestV2::create(&state.draft, values))
    } else {
        ScopedReviewRequestV2::selected_draft_v12(
            &state.draft,
            state
                .selection
                .clone()
                .ok_or("Select an actual fresh server review")?,
            state.action,
            Arc::from(summary),
        )
    }
}
/// WORKER ONLY: genuine close supplied draft/selected account. The optional
/// canonical range is reduced to intent metadata here, not on keypress/draw.
pub fn form_from_draft(
    draft: Arc<ScopedReviewDraftV2>,
    selection: Option<Arc<ScopedReviewSelectionV2>>,
    action: ReviewRequestedActionV2,
    anchor: Option<Arc<ScopedReviewAnchorIntentV2>>,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Form, String> {
    if draft.needs_new_v12 {
        return Err("Inspect the exact terminal action, then explicitly choose New action".into());
    }
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err("Original review open phase ended".into());
    }
    if (action == ReviewRequestedActionV2::CreatePending) != selection.is_none()
        || selection
            .as_ref()
            .is_some_and(|selected| !Arc::ptr_eq(selected.intent(), draft.intent()))
        || anchor.is_some() && action != ReviewRequestedActionV2::CreatePending
    {
        return Err("Review action/selection belongs to another actual source".into());
    }
    if selection.as_ref().is_some_and(|selected| {
        selected.action_key(action).map_or(true, |key| {
            key != draft
                .lineage_v12
                .as_ref()
                .map_or(draft.key(), |revision| revision.base_key())
        })
    }) {
        return Err("Open the exact selected-action draft under its own source bracket; do not overwrite the local batch".into());
    }
    let metadata = anchor
        .map(|anchor| {
            anchor
                .prepare_metadata(draft.intent(), cancel, deadline)
                .map(Arc::new)
        })
        .transpose()?;
    let identity = draft.identity().clone(); // actual closing account of THIS action opener
    if identity.login.len() > 256
        || identity.host.len() > 253
        || Scope::github(&identity.host, identity.account_id)
            .ok()
            .as_ref()
            != Some(draft.intent().scope())
    {
        return Err("Original composer closed account unavailable".into());
    }
    let parent = draft.intent().parent();
    let mut header_actors_v7 = vec![avatars_v7::PreparedActorV7::requester(
        &identity, cancel, deadline,
    )?];
    if let Some(selected) = &selection {
        let actual = draft
            .catalogue()
            .rows()
            .iter()
            .find(|row| {
                row.id() == selected.snapshot().review().id()
                    && row.node() == selected.snapshot().review().node()
            })
            .ok_or("Actual fresh selected review author unavailable")?;
        header_actors_v7.extend(avatars_v7::batch(
            &identity.host,
            &[(actual.author(), actual.author_account_id())],
            cancel,
            deadline,
        )?);
    }
    let mut form = Form::pr(PrIntent {
        kind: PrFormKind::Comment,
        target: parent.summary().clone(),
        identity: identity.clone(),
    })?;
    let mut lines=vec![action.label().into(),format!("Original {}#{} • repo {} • PR {}",parent.repository(),parent.number(),parent.repository_id(),parent.database_id()),
        format!("H {} • base {} • not an expected-head CAS",parent.head_sha(),parent.base_tip_sha()),
        "Actual source/UID/SQL/head and every selected range rechecked on Preview AND Confirm; saved history never approves/resends.".into()];
    for comment in draft.values().comments() {
        lines.push(format!(
            "Draft {} {:?} {}–{} • exact text retained",
            crate::git::display_bytes(comment.anchor.server_path.as_bytes()),
            comment.anchor.side,
            comment.anchor.first,
            comment.anchor.last
        ));
    }
    if let Some(anchor) = &metadata {
        lines.push(format!(
            "Add exact {} {:?} {}–{}",
            crate::git::display_bytes(anchor.server_path.as_bytes()),
            anchor.side,
            anchor.first,
            anchor.last
        ));
    }
    let initial = if action == ReviewRequestedActionV2::CreatePending || draft.restored_values() {
        draft.values().summary()
    } else if (draft.lineage_v12.is_some() && action != ReviewRequestedActionV2::UpdateSummary)
        || matches!(
            action,
            ReviewRequestedActionV2::Discard | ReviewRequestedActionV2::Dismiss
        )
    {
        ""
    } else {
        let selected = selection
            .as_ref()
            .ok_or("Original review selection unavailable")?;
        draft
            .catalogue()
            .rows()
            .iter()
            .find(|row| {
                row.id() == selected.snapshot().review().id()
                    && row.node() == selected.snapshot().review().node()
            })
            .ok_or("Actual current-source selected review absent")?
            .body()
    };
    form.fields = if action == ReviewRequestedActionV2::Discard {
        vec![]
    } else {
        vec![Field::text(
            "summary",
            if action == ReviewRequestedActionV2::Dismiss {
                "Dismissal reason (server privilege UNKNOWN)"
            } else {
                "Review summary"
            },
            initial,
            true,
        )?]
    };
    // Retained editors precede the optional newly selected anchor.
    let (local_v10, local_fields) = local_batch_v10::LocalBatchV10::prepare(
        draft.values(),
        action == ReviewRequestedActionV2::CreatePending,
        cancel,
        deadline,
    )?;
    form.fields.extend(local_fields);
    if metadata.is_some() {
        form.fields.push(Field::text(
            "anchored-comment",
            "Exact selected pending comment",
            "",
            true,
        )?);
    }
    if lines.iter().map(String::len).sum::<usize>() > 128 * 1024 {
        return Err("Review target presentation exceeds bounded metadata".into());
    }
    let state = ReviewComposerStateV2 {
        draft: draft.clone(),
        selection,
        action,
        anchor: metadata,
        local_v10,
        ticket: Arc::new(PresentationTicketV2),
        revision: Some(0),
        preview: None,
        header_actors_v7: header_actors_v7.into(),
        avatar_view_v7: None,
        identity: identity.clone(),
        lines: Arc::from([]),
        original_lines: lines
            .into_iter()
            .map(|line| crate::git::display_bytes(line.as_bytes()))
            .collect::<Vec<_>>()
            .into(),
        key: Arc::from(draft.key()),
        input_visible: false,
        confirmation_visible: false,
    };
    form.title = format!(
        "{} • {}#{}",
        action.label(),
        parent.repository(),
        parent.number()
    );
    form.publication = draft.publication().cloned();
    form.restored_draft = !draft.values().summary().is_empty()
        || !draft.values().comments().is_empty()
        || draft.publication().is_some();
    form.purpose = FormPurpose::PrReviewV2(Box::new(state));
    form.focus = 0;
    form.active_field = 0;
    form.dirty = form.publication.is_none(); // affirmative actual initial revision, including empty/no-field Discard
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err("Original review open phase ended before result".into());
    }
    Ok(form)
}
pub fn request(form: &Form) -> Result<Arc<ReviewComposerRequestV2>, String> {
    let FormPurpose::PrReviewV2(state) = &form.purpose else {
        return Err("Not an original review composer".into());
    };
    if !state.input_visible {
        return Err("Resize to inspect the current original review target/controls before Preview or Confirm".into());
    }
    if form.publication.is_some() {
        return Err("Original review publication is history, never permission to resend".into());
    }
    let (summary, comment) = fields(form)?;
    let requested = requested(form, true)?;
    requested.encoded_draft(None)?;
    Ok(Arc::new(ReviewComposerRequestV2 {
        ticket: state.ticket.clone(),
        revision: state
            .revision
            .ok_or("Review presentation retired; reopen explicitly")?,
        request: requested,
        summary: Arc::from(summary),
        comment: Arc::from(comment),
        preview: state.preview.clone(),
        key: state.key.clone(),
        local_mask_v10: state.local_v10.mask(),
    }))
}
pub fn accept_preview(
    form: &mut Form,
    request: &Arc<ReviewComposerRequestV2>,
    preview: Arc<ScopedReviewPreviewV2>,
) -> bool {
    if !request.matches_form(form) || !Arc::ptr_eq(request.request(), preview.request()) {
        return false;
    }
    let FormPurpose::PrReviewV2(state) = &mut form.purpose else {
        return false;
    };
    let mut headers = vec![preview.requester_v7().clone()];
    if let Some((_, actor)) = preview.actor_rows_v7().first() {
        headers.push(actor.clone());
    }
    state.identity = preview.identity().clone();
    state.lines = preview.lines().clone();
    state.header_actors_v7 = headers.into();
    state.avatar_view_v7 = None;
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
pub fn confirmed_preview(form: &Form) -> Result<Arc<ScopedReviewPreviewV2>, String> {
    if !form.preview {
        return Err("Inspect a fresh exact review Preview before Confirm".into());
    }
    if !matches!(&form.purpose,FormPurpose::PrReviewV2(state) if state.confirmation_visible) {
        return Err(
            "Current review Preview/Confirm controls have not been usefully painted; resize first"
                .into(),
        );
    }
    request(form)?
        .preview()
        .cloned()
        .ok_or_else(|| "Original review Preview unavailable".into())
}
pub fn encoded_draft(form: &Form) -> Result<String, String> {
    requested(form, false)?.encoded_draft(form.publication.as_ref())
}
pub fn validate_input(form: &Form, incoming: &str) -> Result<(), String> {
    fields(form)?;
    if form.publication.is_some() && !incoming.is_empty() {
        return Err(
            "Original review publication history is read-only; no fields or approval restored"
                .into(),
        );
    }
    let Some(field) = form.fields.get(form.focus) else {
        return Ok(());
    };
    let selection = field
        .editor
        .selection()
        .map_or(0, |selection| selection.len());
    if incoming.len() > 65_536
        || field
            .editor
            .text
            .len()
            .saturating_sub(selection)
            .saturating_add(incoming.len())
            > 65_536
    {
        return Err("Review input exceeds64KiB; nothing was truncated".into());
    }
    Ok(())
}
pub fn replace_editor_result(form: &mut Form, value: &str) -> Result<(), String> {
    fields(form)?;
    if form.publication.is_some() {
        return Err("Original review publication history is read-only; no editor replay".into());
    }
    if value.len() > 65_536 {
        return Err("Review external editor result exceeds64KiB; original text retained".into());
    }
    let index = form.active_field;
    if index >= form.fields.len() {
        return Err("Review has no editable selected field".into());
    }
    form.invalidate_plan();
    form.fields[index].editor = crate::editor::TextBuffer::new(value)?;
    form.dirty = true;
    Ok(())
}
pub fn retire_invalid_form(app: &mut App) -> bool {
    // Check genuine original-account loss before dropping the last retained
    // intent that the shared Main privacy helper needs to identify its owner.
    if super::native_v2::retire_invalid(app) {
        return true;
    }
    if !app.form.as_ref().is_some_and(
        |form| matches!(&form.purpose,FormPurpose::PrReviewV2(state) if !state.intent().owns(app)),
    ) {
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
/// Pure current geometry marker, NOT source/access/publication authority.
pub fn begin_draw(form: &mut Form, usable: bool) {
    if let FormPurpose::PrReviewV2(state) = &mut form.purpose {
        state.input_visible = usable;
        state.confirmation_visible = false;
        state.avatar_view_v7 = None;
        state.local_v10.clear_paint();
    }
}
pub fn painted_preview(form: &mut Form, visible: bool) {
    if let FormPurpose::PrReviewV2(state) = &mut form.purpose {
        state.confirmation_visible =
            visible && state.input_visible && state.preview.is_some() && form.preview;
    }
}
pub fn clear_layout(form: &mut Form) {
    begin_draw(form, false);
}
