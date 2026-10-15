//! Bounded pure browser of actual closed pending/submitted review observations.
//! Keyboard and mouse issue identical typed actions; geometry is not authority.
use super::*;
use crate::theme::SemanticToken;
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Margin, Rect},
    style::Modifier,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewBrowserActionV2 {
    Move(i32),
    Scroll(i32),
    Horizontal(i32),
    Focus(i32),
    Activate,
    Select(usize),
    Resume,
    NewBatch,
    NextPendingCommentV3,
    EditPendingCommentV3,
    DeletePendingCommentV3,
    AddPendingRangeV3,
    RequestReviewerV3,
    TeamsV3,
    RequestTeamV3,
    Effect(ReviewRequestedActionV2),
    Close,
    Quit,
}
#[derive(Clone)]
struct Hit {
    rect: Rect,
    action: ReviewBrowserActionV2,
}
pub struct ReviewBrowserV2 {
    pub(super) draft: Arc<ScopedReviewDraftV2>,
    pub(super) selection: Option<Arc<ScopedReviewSelectionV2>>,
    pub(super) pending_anchor_v3: Option<Arc<ScopedReviewAnchorIntentV2>>,
    pub(super) pending_comment_v3: usize,
    pub(super) teams_v3: Option<Arc<crate::providers::gh::ReviewRequestTeamsV3>>,
    pub(super) selected_team_v3: usize,
    teams_labels_v3: Arc<[String]>,
    teams_actors_v3: Arc<[avatars_v7::PreparedActorV7]>,
    rows: Arc<[String]>,
    details: Arc<[String]>,
    pub(super) selected: usize,
    pub(super) scroll: usize,
    pub(super) horizontal: usize,
    focus: usize,
    hits: Vec<Hit>,
    catalogue_actors_v7: Arc<[avatars_v7::PreparedActorV7]>,
    details_actors_v7: Arc<[Option<avatars_v7::PreparedActorV7>]>,
    pub(super) requester_v7: avatars_v7::PreparedActorV7,
    pub(super) avatar_view_v7: Option<avatars_v7::BrowserViewportV7>,
    pub(super) ticket: Arc<()>,
    notice: String,
    interactive: bool,
    new_batch_refusal_v12: Option<&'static str>,
}
impl std::fmt::Debug for ReviewBrowserV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReviewBrowserV2(<actual closed observations; no permission>)")
    }
}
impl ReviewBrowserV2 {
    pub fn intent(&self) -> &Arc<ScopedReviewIntentV2> {
        self.draft.intent()
    }
    #[cfg(test)]
    pub(crate) fn draft_v7_test(&self) -> &Arc<ScopedReviewDraftV2> {
        &self.draft
    }
    #[cfg(test)]
    pub(crate) fn ticket_v7_test(&self) -> &Arc<()> {
        &self.ticket
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn interactive(&self) -> bool {
        self.interactive
    }
    pub fn new_batch_disabled_v12(&self) -> Option<&'static str> {
        self.new_batch_refusal_v12
    }
    pub fn effect_disabled(&self, action: ReviewRequestedActionV2) -> Option<&'static str> {
        if self.teams_v3.is_some() {
            return Some(
                "Team selection is not a selected review; reopen/resume an actual review first",
            );
        }
        use crate::providers::gh::ObservedReviewStateV2;
        let Some(selected) = &self.selection else {
            return Some("Resume the actual selected review first; no raw ID or cached row is publication authority");
        };
        let snapshot = selected.snapshot();
        let review = snapshot.review();
        let uid = self.draft.identity().account_id;
        if action == ReviewRequestedActionV2::Dismiss {
            return (!matches!(review.state(),ObservedReviewStateV2::Approved|ObservedReviewStateV2::ChangesRequested) || !review.has_observed_submission_v2())
                .then_some("Dismiss requires an actual submitted approval/change request; server privilege remains UNKNOWN");
        }
        if !snapshot.complete()
            || !review.is_observed_own_pending()
            || review.author_account_id() != uid
            || snapshot
                .comments()
                .iter()
                .any(|comment| comment.author_account_id() != uid)
        {
            return Some("This is not a complete pending review owned by the actual numeric account; login matching is insufficient");
        }
        if matches!(
            action,
            ReviewRequestedActionV2::Approve | ReviewRequestedActionV2::RequestChanges
        ) && self
            .intent()
            .parent()
            .author_account_id()
            .is_none_or(|author| Some(author) == uid)
        {
            return Some("Actual numeric PR author is unknown or this reviewer; approval/change request is disabled");
        }
        if matches!(
            action,
            ReviewRequestedActionV2::Approve
                | ReviewRequestedActionV2::RequestChanges
                | ReviewRequestedActionV2::SubmitComment
        ) && (review.commit() != self.intent().parent().head_sha()
            || snapshot
                .comments()
                .iter()
                .any(|comment| comment.commit() != self.intent().parent().head_sha()))
        {
            return Some("Actual pending comments refer to another H; preserve text and explicitly re-anchor before submission");
        }
        None
    }
    pub fn owns(&self, app: &App) -> bool {
        self.intent().owns(app)
    }
    pub fn clear_layout(&mut self) {
        self.hits.clear();
        self.focus = 0;
        self.interactive = false;
        self.avatar_view_v7 = None;
    }
    pub(super) fn actor_v7(&self, row: usize) -> Option<&avatars_v7::PreparedActorV7> {
        if self.teams_v3.is_some() {
            self.teams_actors_v3.get(row)
        } else if self.selection.is_some() {
            self.details_actors_v7.get(row).and_then(Option::as_ref)
        } else {
            self.catalogue_actors_v7.get(row)
        }
    }
    pub fn notice(&mut self, message: &str) {
        self.notice = crate::git::display_bytes(message.as_bytes());
    }
    /// Worker only: raw body/metadata bounds precede escaped row allocation.
    pub fn prepare(
        draft: Arc<ScopedReviewDraftV2>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Self, String> {
        let actor_refs = draft
            .catalogue()
            .rows()
            .iter()
            .map(|review| (review.author(), review.author_account_id()))
            .collect::<Vec<_>>();
        let actors = avatars_v7::batch(&draft.identity().host, &actor_refs, cancel, deadline)?;
        let requester_v7 =
            avatars_v7::PreparedActorV7::requester(draft.identity(), cancel, deadline)?;
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        let mut own_pending = false;
        let mut unknown_pending_owner = false;
        for (review, display) in draft.catalogue().rows().iter().zip(&actors) {
            if cancel.is_cancelled() || Instant::now() >= deadline {
                return Err("Original review browser phase ended".into());
            }
            let actor = review.author();
            own_pending |= review.is_observed_own_pending();
            unknown_pending_owner |= matches!(
                review.state(),
                crate::providers::gh::ObservedReviewStateV2::Pending
            ) && review.author_account_id().is_none();
            if actor.login.len() > 4096 || review.node().len() > 1024 || review.commit().len() > 64
            {
                return Err("Observed review row exceeds metadata bound".into());
            }
            bytes = bytes
                .checked_add(actor.login.len().saturating_mul(6).saturating_add(1536))
                .ok_or("Review row bound overflow")?;
            if rows.len() >= 3000 || bytes > 8 * 1024 * 1024 {
                return Err(
                    "Review catalogue display exceeds bounded rows; no partial ownership".into(),
                );
            }
            rows.push(format!(
                "#{} • {} • {} • UID {:?} • commit {}",
                review.id(),
                review.state().label(),
                display.label(),
                display.uid(),
                review.commit()
            ));
        }
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err("Original review browser phase ended before result".into());
        }
        let new_batch_refusal_v12 = if !draft.catalogue().complete() {
            Some("Review list partial; own-PENDING absence UNKNOWN. Refresh before new batch.")
        } else if own_pending {
            Some("Own PENDING exists; Resume or G Add range. New batch unavailable.")
        } else if unknown_pending_owner {
            Some("Pending author UID unknown; no own-PENDING absence proof. Refresh reviews.")
        } else {
            None
        };
        Ok(Self {
            draft,
            selection: None,
            pending_anchor_v3: None,
            pending_comment_v3: 0,
            teams_v3: None,
            selected_team_v3: 0,
            teams_labels_v3: Arc::from([]),
            teams_actors_v3: Arc::from([]),
            rows: rows.into(),
            details: Arc::from([]),
            catalogue_actors_v7: actors.into(),
            details_actors_v7: Arc::from([]),
            requester_v7,
            avatar_view_v7: None,
            selected: 0,
            scroll: 0,
            horizontal: 0,
            focus: 0,
            hits: vec![],
            ticket: Arc::new(()),
            notice: String::new(),
            interactive: false,
            new_batch_refusal_v12,
        })
    }
    pub fn selected_rows(
        selection: &Arc<ScopedReviewSelectionV2>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<PreparedReviewDetailsV7, String> {
        let snapshot = selection.snapshot();
        let mut raw = snapshot.review().body().len();
        let mut escaped = snapshot
            .review()
            .body()
            .len()
            .checked_add(snapshot.review().author().login.len())
            .and_then(|bytes| bytes.checked_mul(6))
            .and_then(|bytes| bytes.checked_add(2048))
            .ok_or("Review detail display bound overflow")?;
        for comment in snapshot.comments() {
            if cancel.is_cancelled() || Instant::now() >= deadline {
                return Err("Original review details phase ended".into());
            }
            raw = raw
                .checked_add(comment.body().len())
                .and_then(|bytes| bytes.checked_add(comment.path().len()))
                .ok_or("Review detail bound overflow")?;
            escaped = comment
                .body()
                .len()
                .checked_add(comment.path().len())
                .and_then(|bytes| bytes.checked_add(comment.author().login.len()))
                .and_then(|bytes| bytes.checked_mul(6))
                .and_then(|bytes| bytes.checked_add(2048))
                .and_then(|bytes| escaped.checked_add(bytes))
                .ok_or("Review escaped detail bound overflow")?;
            if escaped > 8 * 1024 * 1024 {
                return Err("Escaped complete review details exceed bound before row/body allocation; no partial display".into());
            }
        }
        if raw > 2 * 1024 * 1024 || snapshot.comments().len() > 3000 {
            return Err("Complete review details exceed bounded display; source retained without invented empty comments".into());
        }
        let refs = std::iter::once((
            snapshot.review().author(),
            snapshot.review().author_account_id(),
        ))
        .chain(
            snapshot
                .comments()
                .iter()
                .map(|comment| (comment.author(), comment.author_account_id())),
        )
        .collect::<Vec<_>>();
        let prepared = avatars_v7::batch(&selection.identity().host, &refs, cancel, deadline)?;
        let mut actors = vec![Some(prepared[0].clone()), None];
        let mut rows = vec![
            format!(
                "Actual review #{} • {} • {} UID {:?}",
                snapshot.review().id(),
                snapshot.review().state().label(),
                prepared[0].label(),
                prepared[0].uid()
            ),
            crate::git::display_bytes(snapshot.review().body().as_bytes()),
        ];
        for (comment, display) in snapshot.comments().iter().zip(prepared.iter().skip(1)) {
            if cancel.is_cancelled() || Instant::now() >= deadline {
                return Err("Original review details phase ended".into());
            }
            rows.push(format!(
                "Comment #{} • {} UID {:?} • {} {:?} {:?} • commit {}",
                comment.id(),
                display.label(),
                display.uid(),
                crate::git::display_bytes(comment.path().as_bytes()),
                comment.side(),
                comment.line(),
                comment.commit()
            ));
            rows.push(crate::git::display_bytes(comment.body().as_bytes()));
            actors.extend([Some(display.clone()), None]);
        }
        if rows.iter().map(String::len).sum::<usize>() > 8 * 1024 * 1024 {
            return Err("Escaped review details exceed bounded display".into());
        }
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err("Original review details phase ended before result".into());
        }
        Ok(PreparedReviewDetailsV7 {
            selection: Arc::downgrade(selection),
            rows: rows.into(),
            actors: actors.into(),
        })
    }
    pub fn install_selection(
        &mut self,
        selection: Arc<ScopedReviewSelectionV2>,
        details: PreparedReviewDetailsV7,
    ) -> bool {
        if !std::sync::Weak::ptr_eq(&details.selection, &Arc::downgrade(&selection))
            || !Arc::ptr_eq(selection.intent(), self.intent())
            || self
                .draft
                .catalogue()
                .rows()
                .get(self.selected)
                .is_none_or(|row| {
                    row.id() != selection.snapshot().review().id()
                        || row.node() != selection.snapshot().review().node()
                })
        {
            return false;
        }
        self.selection = Some(selection);
        self.pending_comment_v3 = 0;
        self.teams_v3 = None;
        self.teams_labels_v3 = Arc::from([]);
        self.teams_actors_v3 = Arc::from([]);
        self.details = details.rows;
        self.details_actors_v7 = details.actors;
        self.scroll = 0;
        self.horizontal = 0;
        self.notice.clear();
        self.clear_layout();
        true
    }
    pub fn apply(&mut self, action: ReviewBrowserActionV2) -> Option<ReviewBrowserActionV2> {
        // Focused Enter/Space must take the same local transition as the
        // corresponding click or shortcut, not leak it to the effect dispatcher.
        let action = match action {
            ReviewBrowserActionV2::Activate => self.hits.get(self.focus)?.action,
            other => other,
        };
        if self.teams_v3.is_some() {
            let count = self.teams_labels_v3.len();
            match action {
                ReviewBrowserActionV2::Move(delta) => {
                    let next = self
                        .selected_team_v3
                        .saturating_add_signed(delta as isize)
                        .min(count.saturating_sub(1));
                    if next != self.selected_team_v3 {
                        self.ticket = Arc::new(());
                    }
                    self.selected_team_v3 = next;
                    self.scroll = next;
                    self.clear_layout();
                    return None;
                }
                ReviewBrowserActionV2::Select(row) => {
                    if row < count {
                        if row != self.selected_team_v3 {
                            self.ticket = Arc::new(());
                        }
                        self.selected_team_v3 = row;
                        self.scroll = row;
                        self.clear_layout();
                    }
                    return None;
                }
                _ => {}
            }
        }
        match action {
            ReviewBrowserActionV2::Move(delta) => {
                self.selected = self
                    .selected
                    .saturating_add_signed(delta as isize)
                    .min(self.rows.len().saturating_sub(1));
                self.selection = None;
                self.details = Arc::from([]);
                self.details_actors_v7 = Arc::from([]);
                self.ticket = Arc::new(());
                self.scroll = self.selected;
                self.clear_layout();
                None
            }
            ReviewBrowserActionV2::Select(row) => {
                if row < self.rows.len() {
                    self.selected = row;
                    self.selection = None;
                    self.details = Arc::from([]);
                    self.details_actors_v7 = Arc::from([]);
                    self.ticket = Arc::new(());
                    self.clear_layout();
                }
                None
            }
            ReviewBrowserActionV2::Scroll(delta) => {
                self.scroll = self.scroll.saturating_add_signed(delta as isize);
                self.avatar_view_v7 = None;
                None
            }
            ReviewBrowserActionV2::Horizontal(delta) => {
                self.horizontal = self
                    .horizontal
                    .saturating_add_signed(delta as isize)
                    .min(8 * 1024 * 1024);
                self.avatar_view_v7 = None;
                None
            }
            ReviewBrowserActionV2::Focus(delta) => {
                if !self.hits.is_empty() {
                    self.focus = (self.focus as i64 + i64::from(delta))
                        .rem_euclid(self.hits.len() as i64)
                        as usize;
                }
                None
            }
            other => Some(other),
        }
    }
}
impl ReviewBrowserV2 {
    pub(crate) fn install_teams_v3(
        &mut self,
        prepared: operations_v3::teams_v3::PreparedReviewRequestTeamsV3,
    ) -> bool {
        if !Arc::ptr_eq(self.intent(), &prepared.original) || !prepared.roster.complete() {
            return false;
        }
        self.teams_v3 = Some(prepared.roster);
        self.teams_labels_v3 = prepared.labels;
        self.teams_actors_v3 = prepared.actors;
        self.selected_team_v3 = 0;
        self.scroll = 0;
        self.horizontal = 0;
        self.ticket = Arc::new(());
        self.notice="Actual repository teams • select exact numeric/opaque team • re-request eligibility UNKNOWN".into();
        self.clear_layout();
        true
    }
}
/// Closed worker-prepared rows and exact actor-row membership, no constructor
/// from serialized data and no image/publication authority.
pub struct PreparedReviewDetailsV7 {
    selection: std::sync::Weak<ScopedReviewSelectionV2>,
    rows: Arc<[String]>,
    actors: Arc<[Option<avatars_v7::PreparedActorV7>]>,
}
const CONTROLS: [(ReviewBrowserActionV2, &str); 19] = [
    (ReviewBrowserActionV2::NewBatch, "[B New batch]"),
    (ReviewBrowserActionV2::Resume, "[R Resume selected]"),
    (
        ReviewBrowserActionV2::Effect(ReviewRequestedActionV2::UpdateSummary),
        "[U Summary]",
    ),
    (
        ReviewBrowserActionV2::Effect(ReviewRequestedActionV2::SubmitComment),
        "[C Submit comment]",
    ),
    (
        ReviewBrowserActionV2::Effect(ReviewRequestedActionV2::Approve),
        "[A Approve]",
    ),
    (
        ReviewBrowserActionV2::Effect(ReviewRequestedActionV2::RequestChanges),
        "[X Request changes]",
    ),
    (
        ReviewBrowserActionV2::Effect(ReviewRequestedActionV2::Discard),
        "[D Discard pending]",
    ),
    (
        ReviewBrowserActionV2::Effect(ReviewRequestedActionV2::Dismiss),
        "[I Dismiss: eligibility UNKNOWN]",
    ),
    (ReviewBrowserActionV2::Horizontal(-16), "[< Left]"),
    (ReviewBrowserActionV2::Horizontal(16), "[> Right]"),
    (ReviewBrowserActionV2::Close, "[Esc Close]"),
    (ReviewBrowserActionV2::Quit, "[Q Quit]"),
    (
        ReviewBrowserActionV2::NextPendingCommentV3,
        "[F2 Next comment]",
    ),
    (
        ReviewBrowserActionV2::EditPendingCommentV3,
        "[F3 Edit pending]",
    ),
    (
        ReviewBrowserActionV2::DeletePendingCommentV3,
        "[F4 Delete pending]",
    ),
    (
        ReviewBrowserActionV2::AddPendingRangeV3,
        "[G Add selected range]",
    ),
    (
        ReviewBrowserActionV2::RequestReviewerV3,
        "[F5 Re-request reviewer]",
    ),
    (ReviewBrowserActionV2::TeamsV3, "[F6 Teams]"),
    (ReviewBrowserActionV2::RequestTeamV3, "[F7 Re-request team]"),
];
pub fn key(key: KeyEvent) -> Option<ReviewBrowserActionV2> {
    use ReviewBrowserActionV2::*;
    Some(match key.code {
        KeyCode::Esc => Close,
        KeyCode::Up | KeyCode::Char('k') => Move(-1),
        KeyCode::Down | KeyCode::Char('j') => Move(1),
        KeyCode::PageUp => Scroll(-10),
        KeyCode::PageDown => Scroll(10),
        KeyCode::Tab => Focus(1),
        KeyCode::BackTab => Focus(-1),
        KeyCode::Left | KeyCode::Char('<') => Horizontal(-16),
        KeyCode::Right | KeyCode::Char('>') => Horizontal(16),
        KeyCode::Enter | KeyCode::Char(' ') => Activate,
        KeyCode::Char('B' | 'b') => NewBatch,
        KeyCode::Char('R' | 'r') => Resume,
        KeyCode::Char('U' | 'u') => Effect(ReviewRequestedActionV2::UpdateSummary),
        KeyCode::Char('C' | 'c') => Effect(ReviewRequestedActionV2::SubmitComment),
        KeyCode::Char('A' | 'a') => Effect(ReviewRequestedActionV2::Approve),
        KeyCode::Char('X' | 'x') => Effect(ReviewRequestedActionV2::RequestChanges),
        KeyCode::Char('D' | 'd') => Effect(ReviewRequestedActionV2::Discard),
        KeyCode::Char('I' | 'i') => Effect(ReviewRequestedActionV2::Dismiss),
        KeyCode::Char('Q' | 'q') => Quit,
        KeyCode::F(2) => NextPendingCommentV3,
        KeyCode::F(3) => EditPendingCommentV3,
        KeyCode::F(4) => DeletePendingCommentV3,
        KeyCode::Char('G' | 'g') => AddPendingRangeV3,
        KeyCode::F(5) => RequestReviewerV3,
        KeyCode::F(6) => TeamsV3,
        KeyCode::F(7) => RequestTeamV3,
        _ => return None,
    })
}
pub fn mouse(view: &ReviewBrowserV2, event: MouseEvent) -> Option<ReviewBrowserActionV2> {
    match event.kind {
        MouseEventKind::ScrollUp => Some(ReviewBrowserActionV2::Scroll(-3)),
        MouseEventKind::ScrollDown => Some(ReviewBrowserActionV2::Scroll(3)),
        MouseEventKind::ScrollLeft => Some(ReviewBrowserActionV2::Horizontal(-16)),
        MouseEventKind::ScrollRight => Some(ReviewBrowserActionV2::Horizontal(16)),
        MouseEventKind::Down(MouseButton::Left) => view
            .hits
            .iter()
            .find(|hit| {
                event.column >= hit.rect.x
                    && event.column < hit.rect.right()
                    && event.row >= hit.rect.y
                    && event.row < hit.rect.bottom()
            })
            .map(|hit| hit.action),
        _ => None,
    }
}
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    view: &mut ReviewBrowserV2,
    theme: crate::theme::Theme,
    busy: bool,
    avatars: &crate::actor_avatar_states_v1::ActorAvatarStatesV1,
) {
    view.hits.clear();
    view.interactive = false;
    view.avatar_view_v7 = None;
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(" Scoped reviews • observed numeric ownership • no auto submission ")
            .style(theme.base()),
        area,
    );
    let inner = area.inner(Margin::new(1, 1));
    let mut controls_height = 1u16;
    let mut used = 0u16;
    for (_, label) in CONTROLS {
        let width = label.len() as u16;
        if used > 0 && used.saturating_add(width) > inner.width {
            controls_height += 1;
            used = 0;
        }
        used = used.saturating_add(width + 1);
    }
    if inner.width < 38 || inner.height < controls_height + 5 {
        frame.render_widget(
            Paragraph::new("Resize to inspect reviews • disabled")
                .style(theme.style(SemanticToken::Pending)),
            inner,
        );
        if inner.height > 1 {
            let rect = Rect::new(inner.x, inner.bottom() - 1, inner.width.min(11), 1);
            frame.render_widget(Paragraph::new("[Esc Close]"), rect);
            view.hits.push(Hit {
                rect,
                action: ReviewBrowserActionV2::Close,
            });
        }
        return;
    }
    view.interactive = true;
    let body = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(controls_height + 3),
    );
    view.requester_v7
        .paint(frame, Rect::new(inner.x, inner.y, 4, 1), avatars, theme);
    frame.render_widget(
        Paragraph::new(format!(
            "{} • requester UID {:?} • {} reviews • coverage {}",
            view.requester_v7.label(),
            view.draft.identity().account_id,
            view.rows.len(),
            if view.draft.catalogue().complete() {
                "complete"
            } else {
                "partial: absence UNKNOWN"
            }
        ))
        .style(theme.style(SemanticToken::Muted)),
        Rect::new(inner.x + 4, inner.y, inner.width - 4, 1),
    );
    frame.render_widget(Paragraph::new(if busy{"Original review operation pending • Esc cancels; no body published until explicit Confirm"}else{&view.notice}).style(theme.style(SemanticToken::Pending)),Rect::new(inner.x,inner.y+1,inner.width,1));
    let rows = if view.teams_v3.is_some() {
        &view.teams_labels_v3
    } else if view.selection.is_some() {
        &view.details
    } else {
        &view.rows
    };
    view.scroll = view
        .scroll
        .min(rows.len().saturating_sub(usize::from(body.height)));
    for (offset, row) in rows
        .iter()
        .enumerate()
        .skip(view.scroll)
        .take(usize::from(body.height))
    {
        let rect = Rect::new(
            body.x,
            body.y + (offset - view.scroll) as u16,
            body.width,
            1,
        );
        let selected = if view.teams_v3.is_some() {
            offset == view.selected_team_v3
        } else {
            view.selection.is_none() && offset == view.selected
        };
        let start = row
            .char_indices()
            .nth(view.horizontal)
            .map_or(row.len(), |(index, _)| index);
        frame.render_widget(
            Paragraph::new(&row[start..]).style(if selected {
                theme
                    .style(SemanticToken::Focus)
                    .add_modifier(Modifier::REVERSED)
            } else {
                theme.base()
            }),
            Rect::new(rect.x + 4, rect.y, rect.width - 4, 1),
        );
        if let Some(actor) = view.actor_v7(offset) {
            actor.paint(frame, Rect::new(rect.x, rect.y, 4, 1), avatars, theme);
        }
        if (view.selection.is_none() || view.teams_v3.is_some()) && !busy {
            view.hits.push(Hit {
                rect,
                action: ReviewBrowserActionV2::Select(offset),
            });
        }
    }
    if !busy {
        view.avatar_view_v7 = Some(avatars_v7::BrowserViewportV7::new(
            view,
            view.scroll
                ..view
                    .scroll
                    .saturating_add(usize::from(body.height))
                    .min(rows.len()),
        ));
    }
    let mut x = inner.x;
    let mut y = inner.bottom() - controls_height;
    for (action, label) in CONTROLS {
        if x + label.len() as u16 > inner.right() {
            x = inner.x;
            y += 1;
        }
        if y >= inner.bottom() {
            break;
        }
        let rect = Rect::new(x, y, label.len() as u16, 1);
        let clickable = !busy
            || matches!(
                action,
                ReviewBrowserActionV2::Close | ReviewBrowserActionV2::Quit
            );
        let disabled = match action {
            ReviewBrowserActionV2::NewBatch => view.new_batch_disabled_v12().is_some(),
            ReviewBrowserActionV2::Effect(action) => view.effect_disabled(action).is_some(),
            _ => false,
        };
        let style = theme.style(if clickable && !disabled {
            SemanticToken::Focus
        } else {
            SemanticToken::Muted
        });
        frame.render_widget(
            Paragraph::new(label).style(if view.focus == view.hits.len() {
                style.add_modifier(Modifier::REVERSED)
            } else {
                style
            }),
            rect,
        );
        if clickable {
            view.hits.push(Hit { rect, action });
        }
        x = rect.right() + 1;
    }
    view.focus = view.focus.min(view.hits.len().saturating_sub(1));
}
