//! SOURCE-ONLY read-only details of an exact, already admitted retained leaf.
//! Workers prepare this model after authentic source/SQL/journal validation.
//! Drawing and local navigation perform no IO and confer no current authority.
use crate::{
    identity::{ActorIdentity, ActorWidget, AvatarState},
    metrics::{DashboardSnapshot, EventKind, MetricTarget, SubmittedReviewState},
    metrics_workspace::utc_label,
    providers::runner::sanitize_terminal,
    storage::RetainedMetricFact,
    theme::{SemanticToken, Theme},
};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    widgets::{Clear, Paragraph},
    Frame,
};
use std::{collections::HashMap, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;

const WRAP_CELLS: usize = 68;
const MAX_FIELD_BYTES: usize = 4096;
const MAX_ROW_BYTES: usize = 1024;
const MAX_ROWS: usize = 1024;
const MAX_BYTES: usize = 128 * 1024;
const ORIGINAL_NOTICE: &str = "Original retained/as-of metadata; not a current permission check";

#[derive(Clone, Debug)]
pub struct RetainedDetailRow {
    pub text: String,
    pub role: SemanticToken,
    pub actor: Option<ActorIdentity>,
}
/// Immutable prepared data only; there is no public arbitrary row constructor,
/// Deserialize implementation or restored approval.
pub struct PreparedRetainedMetricDetail {
    target: MetricTarget,
    title: String,
    as_of: String,
    rows: Arc<[RetainedDetailRow]>,
}
impl std::fmt::Debug for PreparedRetainedMetricDetail {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedRetainedMetricDetail")
            .field("prepared_rows", &self.rows.len())
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
struct Rows {
    raw_bytes: usize,
    prepared_bytes: usize,
    rows: Vec<RetainedDetailRow>,
}
impl Rows {
    fn field(&mut self, text: &str) -> Result<(), String> {
        self.raw_bytes = self.raw_bytes.saturating_add(text.len());
        if text.len() > MAX_FIELD_BYTES || self.raw_bytes > MAX_BYTES {
            return Err(
                "Retained detail raw metadata exceeds its bounded preparation budget".into(),
            );
        }
        Ok(())
    }
    fn push(&mut self, text: String, role: SemanticToken) -> Result<(), String> {
        self.prepared_bytes = self.prepared_bytes.saturating_add(text.len());
        if text.len() > MAX_ROW_BYTES
            || self.prepared_bytes > MAX_BYTES
            || self.rows.len() >= MAX_ROWS
        {
            return Err("Retained detail prepared row/byte budget exceeded".into());
        }
        self.rows.push(RetainedDetailRow {
            text,
            role,
            actor: None,
        });
        Ok(())
    }
    fn text(&mut self, text: &str, role: SemanticToken) -> Result<(), String> {
        self.field(text)?;
        let safe = sanitize_terminal(text);
        self.field(&safe)?;
        for line in safe.split('\n') {
            let mut chunk = String::new();
            let mut cells = 0usize;
            let mut last_break = None;
            for grapheme in line.graphemes(true) {
                if grapheme.len() > MAX_ROW_BYTES {
                    return Err("Retained detail grapheme exceeds its bounded row budget".into());
                }
                let width = crate::editor::grapheme_width(grapheme);
                while !chunk.is_empty()
                    && (cells.saturating_add(width) > WRAP_CELLS
                        || chunk.len().saturating_add(grapheme.len()) > MAX_ROW_BYTES)
                {
                    if let Some((byte, break_cells)) = last_break.take() {
                        let tail = chunk.split_off(byte);
                        self.push(chunk.trim_end().to_owned(), role)?;
                        chunk = tail;
                        cells = cells.saturating_sub(break_cells);
                    } else {
                        self.push(std::mem::take(&mut chunk), role)?;
                        cells = 0;
                    }
                }
                chunk.push_str(grapheme);
                cells = cells.saturating_add(width);
                if grapheme.chars().all(char::is_whitespace) {
                    last_break = Some((chunk.len(), cells));
                }
            }
            self.push(chunk, role)?;
        }
        Ok(())
    }
}

fn leaf_matches(left: &MetricTarget, right: &MetricTarget) -> bool {
    let (left_pr, right_pr, same_id) = match (left, right) {
        (
            MetricTarget::Review {
                pr: a,
                review_id: x,
            },
            MetricTarget::Review {
                pr: b,
                review_id: y,
            },
        )
        | (
            MetricTarget::Event { pr: a, event_id: x },
            MetricTarget::Event { pr: b, event_id: y },
        ) => (a, b, x == y),
        _ => return false,
    };
    same_id
        && left_pr.repository == right_pr.repository
        && left_pr.repository_id == right_pr.repository_id
        && left_pr.database_id == right_pr.database_id
        && left_pr.node_id == right_pr.node_id
        && left_pr.number == right_pr.number
}
fn kind_text(kind: &EventKind) -> (&'static str, SemanticToken) {
    match kind {
        EventKind::ReviewSubmitted { state, .. } => match state {
            SubmittedReviewState::Approved => {
                ("Submitted review: APPROVED", SemanticToken::Success)
            }
            SubmittedReviewState::ChangesRequested => (
                "Submitted review: CHANGES_REQUESTED",
                SemanticToken::Failure,
            ),
            SubmittedReviewState::Commented => {
                ("Submitted review: COMMENTED", SemanticToken::Reference)
            }
            SubmittedReviewState::Dismissed => (
                "Submitted review: DISMISSED (original submission time retained)",
                SemanticToken::Muted,
            ),
        },
        EventKind::ReviewRequested { .. } => (
            "Review requested: direct numeric user",
            SemanticToken::Pending,
        ),
        EventKind::ReviewRequestRemoved { .. } => (
            "Review request removed: direct numeric user",
            SemanticToken::Changed,
        ),
        EventKind::ReviewRequestedTeam { .. } => (
            "Review requested: team (not direct user membership)",
            SemanticToken::Pending,
        ),
        EventKind::ReviewRequestRemovedTeam { .. } => {
            ("Review request removed: team", SemanticToken::Changed)
        }
        EventKind::ReviewRequestedUnknownTarget => {
            ("Review requested: target unavailable", SemanticToken::Muted)
        }
        EventKind::ReviewRequestRemovedUnknownTarget => (
            "Review request removed: target unavailable",
            SemanticToken::Muted,
        ),
        EventKind::Merged => ("Merged", SemanticToken::Success),
        EventKind::Closed => ("Closed", SemanticToken::Removed),
        EventKind::Reopened => ("Reopened", SemanticToken::Changed),
        EventKind::ReadyForReview => ("Ready for review", SemanticToken::Reference),
        EventKind::ConvertedToDraft => ("Converted to draft", SemanticToken::Muted),
    }
}

impl PreparedRetainedMetricDetail {
    /// Metadata preparation is pure, not fresh access. Root must run it inside
    /// the authenticated publication boundary and recapture sources afterward.
    pub fn prepare(
        fact: &RetainedMetricFact<'_>,
        snapshot: &DashboardSnapshot,
    ) -> Result<Self, String> {
        let stable_scope =
            crate::storage::Scope::github(snapshot.scope.host(), Some(snapshot.scope.account_id()))
                .map_err(|error| error.to_string())?;
        if fact.scope() != &stable_scope
            || fact.as_of() != snapshot.as_of
            || snapshot.window.start >= snapshot.window.end
            || snapshot.window.end > snapshot.as_of
            || snapshot.cards.len() > 8
        {
            return Err(
                "Retained detail is outside the original admitted Dashboard scope/as-of".into(),
            );
        }
        let mut rows = Rows::default();
        // Bound raw metadata before formatting/cloning/sanitizing it.
        let (pr, id, category) = match fact.target() {
            MetricTarget::Review { pr, review_id } => (pr, *review_id, "Review"),
            MetricTarget::Event { pr, event_id } => (pr, *event_id, "Event"),
            _ => return Err("Retained detail requires its genuine Review/Event leaf".into()),
        };
        for text in [
            &pr.repository,
            pr.node_id.as_ref().ok_or("Canonical parent node absent")?,
            &fact.event().actor.actor.login,
            &fact.event().actor.actor.actor_type,
        ] {
            rows.field(text)?;
        }
        for text in [
            fact.provider_node(),
            fact.source(),
            fact.scope().host.as_str(),
            fact.scope().login.as_str(),
        ] {
            rows.field(text)?;
        }
        for text in [
            &fact.event().actor.actor.display_name,
            &fact.event().actor.actor.avatar_url,
        ]
        .into_iter()
        .flatten()
        {
            rows.field(text)?;
        }
        if fact.coverage().reasons.len() > 128 {
            return Err("Retained detail reason count exceeds the metadata budget".into());
        }
        for reason in &fact.coverage().reasons {
            rows.field(reason)?;
        }
        let mut contexts = Vec::new();
        let mut contributors = 0usize;
        let mut targets = 0usize;
        for card in &snapshot.cards {
            for contributor in &card.contributors {
                contributors += 1;
                targets = targets.saturating_add(1 + contributor.evidence.len());
                if contributors > 50_000 || targets > 250_000 {
                    return Err(
                        "Dashboard contributor target budget exceeded before leaf admission".into(),
                    );
                }
                if leaf_matches(&contributor.target, fact.target())
                    || contributor
                        .evidence
                        .iter()
                        .any(|target| leaf_matches(target, fact.target()))
                {
                    if !contexts.contains(&card.kind) {
                        rows.field(&card.label)?;
                        rows.field(&card.cohort)?;
                        rows.text(
                            &format!("Contributes to: {}", card.label),
                            SemanticToken::Head,
                        )?;
                        rows.text(&format!("Cohort: {}", card.cohort), SemanticToken::Muted)?;
                        rows.text(
                            &format!(
                                "Card Unknown: {} • Censored: {}",
                                card.unknown, card.censored
                            ),
                            SemanticToken::Muted,
                        )?;
                        contexts.push(card.kind);
                    }
                }
            }
        }
        if contexts.is_empty() {
            return Err(
                "Exact retained leaf is not a contributor in this admitted Dashboard model".into(),
            );
        }
        let title = format!(
            "Retained {category} metadata • {}#{} • ID {id}",
            pr.repository, pr.number
        );
        rows.text(ORIGINAL_NOTICE, SemanticToken::Muted)?;
        rows.text(
            &format!(
                "Original cache scope: {} / {}",
                fact.scope().host,
                fact.scope().login
            ),
            SemanticToken::Reference,
        )?;
        rows.text(
            &format!(
                "Window: [{}, {})",
                utc_label(snapshot.window.start),
                utc_label(snapshot.window.end)
            ),
            SemanticToken::Normal,
        )?;
        rows.text(
            &format!(
                "Repository ID: {} • PR database ID: {} • Number: {}",
                pr.repository_id.ok_or("Repository ID absent")?,
                pr.database_id.ok_or("PR database ID absent")?,
                pr.number
            ),
            SemanticToken::Reference,
        )?;
        rows.text(
            &format!(
                "Genuine PR node: {}",
                pr.node_id.as_deref().ok_or("PR node absent")?
            ),
            SemanticToken::Reference,
        )?;
        rows.text(
            &format!("Original {category} database ID: {id}"),
            SemanticToken::Reference,
        )?;
        rows.text(
            &format!("Genuine {category} node: {}", fact.provider_node()),
            SemanticToken::Reference,
        )?;
        rows.text(
            &format!("Stored source: {}", fact.source()),
            SemanticToken::Reference,
        )?;
        rows.text(
            &format!("Actual stored generation: {}", fact.generation()),
            SemanticToken::Reference,
        )?;
        let event = fact.event();
        let (text, role) = kind_text(&event.kind);
        rows.text(text, role)?;
        rows.text(
            &format!(
                "Original event time: {} • Unix {}",
                utc_label(event.at),
                event.at
            ),
            SemanticToken::Normal,
        )?;
        match event.kind {
            EventKind::ReviewRequested { reviewer_id }
            | EventKind::ReviewRequestRemoved { reviewer_id } => rows.text(
                &format!("Actual direct requested user ID: {reviewer_id}"),
                SemanticToken::Actor,
            )?,
            EventKind::ReviewRequestedTeam { team_id }
            | EventKind::ReviewRequestRemovedTeam { team_id } => rows.text(
                &format!("Actual team ID: {team_id} • no user membership inferred"),
                SemanticToken::Actor,
            )?,
            EventKind::ReviewRequestedUnknownTarget
            | EventKind::ReviewRequestRemovedUnknownTarget => rows.text(
                "Requested user/team identity: unavailable; cannot infer current viewer",
                SemanticToken::Muted,
            )?,
            _ => {}
        }
        let actor = event.actor.identity(&snapshot.scope);
        rows.text(&format!("Actor: {}", actor.label()), SemanticToken::Actor)?;
        let actor_index = rows.rows.len();
        rows.push(
            "Shared original actor / avatar fallback".into(),
            SemanticToken::Actor,
        )?;
        rows.rows[actor_index].actor = Some(actor);
        rows.text(
            &format!(
                "Actual actor account ID: {}",
                event.actor.account_id.map_or_else(
                    || "Unavailable (not inferred from username)".into(),
                    |id| id.to_string()
                )
            ),
            SemanticToken::Muted,
        )?;
        let (observed_from, observed_through) = fact.observed_interval();
        rows.text(
            &format!(
                "Original read observed: {} through {}",
                utc_label(observed_from),
                utc_label(observed_through)
            ),
            SemanticToken::Muted,
        )?;
        let coverage = fact.coverage();
        rows.text(if coverage.complete && coverage.basis.available_api_visible() {
            "Source coverage: exhausted API-visible retained backfill (not erased history)"
        } else { "Source coverage: partial/unavailable retained history; observed fact is not earliest/full history" }, SemanticToken::Pending)?;
        if let (Some(from), Some(through)) = (coverage.from, coverage.through) {
            rows.text(
                &format!(
                    "Admitted category interval: [{}, {}]",
                    utc_label(from),
                    utc_label(through)
                ),
                SemanticToken::Reference,
            )?;
        }
        for reason in &coverage.reasons {
            rows.text(
                &format!("Source limitation: {reason}"),
                SemanticToken::Muted,
            )?;
        }
        rows.text(
            "Body/text: not retained in this original metric schema",
            SemanticToken::Muted,
        )?;
        rows.text(
            "Commit/path/line: not retained; no anchor can be invented",
            SemanticToken::Muted,
        )?;
        rows.text(
            "Review/event URL: not retained; no browser or similar-PR substitution",
            SemanticToken::Muted,
        )?;
        rows.text(
            "Read-only metadata • no automatic hydration or GitHub requests",
            SemanticToken::Muted,
        )?;
        Ok(Self {
            target: fact.target().clone(),
            title: sanitize_terminal(&title),
            as_of: format!("As of: {}", utc_label(fact.as_of())),
            rows: Arc::from(rows.rows),
        })
    }
    pub fn target(&self) -> &MetricTarget {
        &self.target
    }
    pub fn rows(&self) -> &[RetainedDetailRow] {
        &self.rows
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedDetailAction {
    Select(usize),
    Move(i32),
    Page(i32),
    Home,
    End,
    FocusNext,
    FocusPrevious,
    Back,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedDetailOutcome {
    Changed,
    Back,
}
#[derive(Clone, Debug)]
pub struct RetainedDetailHit {
    pub rect: Rect,
    pub action: RetainedDetailAction,
}
#[derive(Clone, Debug, Default)]
pub struct RetainedDetailLayout {
    pub hits: Vec<RetainedDetailHit>,
    pub content: Rect,
    pub visible_rows: usize,
}
/// Local navigation of an immutable original model only. Root drops/invalidates
/// this state immediately on observed principal/access/source/journal loss.
#[derive(Clone)]
pub struct RetainedMetricDetail {
    prepared: Option<Arc<PreparedRetainedMetricDetail>>,
    selected: usize,
    top: usize,
    focused_control: Option<usize>,
    layout: RetainedDetailLayout,
}
const CONTROLS: [(&str, RetainedDetailAction); 5] = [
    ("[↑ Up]", RetainedDetailAction::Move(-1)),
    ("[↓ Down]", RetainedDetailAction::Move(1)),
    ("[PgUp]", RetainedDetailAction::Page(-1)),
    ("[PgDn]", RetainedDetailAction::Page(1)),
    ("[b Back]", RetainedDetailAction::Back),
];
impl RetainedMetricDetail {
    pub fn new(prepared: Arc<PreparedRetainedMetricDetail>) -> Self {
        Self {
            prepared: Some(prepared),
            selected: 0,
            top: 0,
            focused_control: None,
            layout: RetainedDetailLayout::default(),
        }
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn top(&self) -> usize {
        self.top
    }
    pub fn layout(&self) -> &RetainedDetailLayout {
        &self.layout
    }
    pub fn clear_layout(&mut self) {
        self.layout = RetainedDetailLayout::default();
    }
    pub fn invalidate(&mut self) {
        self.prepared = None;
        self.selected = 0;
        self.top = 0;
        self.focused_control = None;
        self.clear_layout();
    }
    pub fn key(&self, code: KeyCode, modifiers: KeyModifiers) -> Option<RetainedDetailAction> {
        if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
            return None;
        }
        Some(match code {
            KeyCode::Esc | KeyCode::Char('b') => RetainedDetailAction::Back,
            KeyCode::Up | KeyCode::Char('k') => RetainedDetailAction::Move(-1),
            KeyCode::Down | KeyCode::Char('j') => RetainedDetailAction::Move(1),
            KeyCode::PageUp => RetainedDetailAction::Page(-1),
            KeyCode::PageDown => RetainedDetailAction::Page(1),
            KeyCode::Home => RetainedDetailAction::Home,
            KeyCode::End => RetainedDetailAction::End,
            KeyCode::BackTab => RetainedDetailAction::FocusPrevious,
            KeyCode::Tab => RetainedDetailAction::FocusNext,
            KeyCode::Enter => self.focused_control.map(|index| CONTROLS[index].1)?,
            _ => return None,
        })
    }
    pub fn mouse(&self, event: MouseEvent) -> Option<RetainedDetailAction> {
        let point = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => self
                .layout
                .hits
                .iter()
                .find(|hit| hit.rect.contains(point))
                .map(|hit| hit.action),
            MouseEventKind::ScrollUp if self.layout.content.contains(point) => {
                Some(RetainedDetailAction::Move(-1))
            }
            MouseEventKind::ScrollDown if self.layout.content.contains(point) => {
                Some(RetainedDetailAction::Move(1))
            }
            _ => None,
        }
    }
    pub fn dispatch(&mut self, action: RetainedDetailAction) -> RetainedDetailOutcome {
        if action == RetainedDetailAction::Back {
            return RetainedDetailOutcome::Back;
        }
        let Some(prepared) = &self.prepared else {
            return RetainedDetailOutcome::Changed;
        };
        let last = prepared.rows.len().saturating_sub(1);
        match action {
            RetainedDetailAction::Select(index) => {
                self.selected = index.min(last);
                self.focused_control = None;
            }
            RetainedDetailAction::Move(delta) => {
                self.selected = self
                    .selected
                    .saturating_add_signed(delta as isize)
                    .min(last);
                self.focused_control = None;
            }
            RetainedDetailAction::Page(delta) => {
                let page = self.layout.visible_rows.saturating_sub(1).max(1);
                self.selected = self
                    .selected
                    .saturating_add_signed((delta as isize).saturating_mul(page as isize))
                    .min(last);
                self.focused_control = None;
            }
            RetainedDetailAction::Home => {
                self.selected = 0;
                self.focused_control = None;
            }
            RetainedDetailAction::End => {
                self.selected = last;
                self.focused_control = None;
            }
            RetainedDetailAction::FocusNext => {
                self.focused_control = match self.focused_control {
                    None => Some(0),
                    Some(index) if index + 1 < CONTROLS.len() => Some(index + 1),
                    _ => None,
                };
            }
            RetainedDetailAction::FocusPrevious => {
                self.focused_control = match self.focused_control {
                    None => Some(CONTROLS.len() - 1),
                    Some(0) => None,
                    Some(index) => Some(index - 1),
                };
            }
            RetainedDetailAction::Back => unreachable!(),
        }
        RetainedDetailOutcome::Changed
    }
    pub fn draw(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        theme: Theme,
        avatars: &HashMap<String, AvatarState>,
    ) {
        self.clear_layout();
        if area.is_empty() {
            return;
        }
        frame.render_widget(Clear, area);
        let footer_y = area.bottom() - 1;
        let back_width = 8.min(area.width);
        let back = Rect::new(area.right() - back_width, footer_y, back_width, 1);
        frame.render_widget(
            Paragraph::new(CONTROLS[4].0).style(if self.focused_control == Some(4) {
                theme.selected(SemanticToken::Focus)
            } else {
                theme.style(SemanticToken::Focus)
            }),
            back,
        );
        self.layout.hits.push(RetainedDetailHit {
            rect: back,
            action: RetainedDetailAction::Back,
        });
        if self.prepared.is_none() {
            if area.height > 1 {
                frame.render_widget(
                    Paragraph::new("Retained detail hidden; refresh after source/principal loss")
                        .style(theme.style(SemanticToken::Muted)),
                    Rect::new(area.x, area.y, area.width, area.height - 1),
                );
            }
            return;
        }
        let mut x = area.x;
        for (index, (label, action)) in CONTROLS[..4].iter().enumerate() {
            let width = crate::editor::display_width(label) as u16;
            if x.saturating_add(width) >= back.x {
                break;
            }
            let rect = Rect::new(x, footer_y, width, 1);
            frame.render_widget(
                Paragraph::new(*label).style(if self.focused_control == Some(index) {
                    theme.selected(SemanticToken::Focus)
                } else {
                    theme.style(SemanticToken::Focus)
                }),
                rect,
            );
            self.layout.hits.push(RetainedDetailHit {
                rect,
                action: *action,
            });
            x += width + 1;
        }
        let prepared = self
            .prepared
            .as_ref()
            .expect("Prepared detail checked before controls");
        let mut y = area.y;
        for (text, role) in [
            (prepared.title.as_str(), SemanticToken::Head),
            (prepared.as_of.as_str(), SemanticToken::Reference),
            (ORIGINAL_NOTICE, SemanticToken::Muted),
        ] {
            if y >= footer_y {
                break;
            }
            frame.render_widget(
                Paragraph::new(text).style(theme.style(role)),
                Rect::new(area.x, y, area.width, 1),
            );
            y += 1;
        }
        self.layout.content = Rect::new(area.x, y, area.width, footer_y.saturating_sub(y));
        self.layout.visible_rows = self.layout.content.height as usize;
        if self.layout.visible_rows == 0 {
            return;
        }
        if self.selected < self.top {
            self.top = self.selected;
        }
        if self.selected >= self.top.saturating_add(self.layout.visible_rows) {
            self.top = self.selected + 1 - self.layout.visible_rows;
        }
        for (offset, (index, row)) in prepared
            .rows
            .iter()
            .enumerate()
            .skip(self.top)
            .take(self.layout.visible_rows)
            .enumerate()
        {
            let rect = Rect::new(area.x, y + offset as u16, area.width, 1);
            let style = if self.selected == index {
                theme.selected(row.role)
            } else {
                theme.style(row.role)
            };
            frame.render_widget(
                Paragraph::new(if self.selected == index { "> " } else { "  " }).style(style),
                Rect::new(rect.x, rect.y, 2.min(rect.width), 1),
            );
            let content = Rect::new(
                rect.x + 2.min(rect.width),
                rect.y,
                rect.width.saturating_sub(2),
                1,
            );
            if let Some(actor) = &row.actor {
                // Shared actor rendering retains the selected-row overlay too.
                frame.render_widget(Paragraph::new("").style(style), content);
                let image = actor
                    .login
                    .as_ref()
                    .and_then(|login| avatars.get(login))
                    .and_then(|state| match state {
                        AvatarState::Ready(image) => Some(image.as_ref()),
                        _ => None,
                    });
                frame.render_widget(
                    ActorWidget {
                        identity: actor,
                        image,
                        theme,
                    },
                    content,
                );
            } else {
                frame.render_widget(Paragraph::new(row.text.as_str()).style(style), content);
            }
            self.layout.hits.push(RetainedDetailHit {
                rect,
                action: RetainedDetailAction::Select(index),
            });
        }
    }
}

#[cfg(test)]
#[path = "metrics_retained_detail_tests.rs"]
mod tests;
