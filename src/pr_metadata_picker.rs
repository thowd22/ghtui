//! Undeclared native metadata picker. The retained catalogue is a bounded
//! observation, not publication authority. Selecting/filtering/drawing performs
//! no I/O; only explicit Choose asks the original loaded catalogue for a member.
use crate::{
    editor::{grapheme_width, TextBuffer},
    pr_metadata_catalogue::LoadedPrMetadataChoices,
    pr_metadata_choices::{PrMetadataChoices, PrMetadataValue},
    providers::{gh::MetadataChoices, runner::sanitize_terminal},
    theme::{SemanticToken, Theme},
};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use nucleo_matcher::{
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    Matcher, Utf32Str,
};
use ratatui::{
    layout::{Position, Rect},
    style::Modifier,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

const MAX_QUERY_BYTES: usize = 512;
const MAX_QUERY_CHARACTERS: usize = 128;
const MAX_OPTIONS: usize = 1000;
const MAX_FIELD_BYTES: usize = 8192;
const MAX_PREPARED_BYTES: usize = 2 * 1024 * 1024;
const MAX_DETAIL_BYTES: usize = 64 * 1024;
const MAX_DETAIL_ROWS: usize = 1024;
const PREPARED_CELLS: usize = 68;
#[path = "pr_metadata_member_picker.rs"]
mod member_picker;
use member_picker::{CurrentMemberRows, PickerChoicesView, PickerKindV87, PickerRowView};
#[path = "pr_metadata_base_picker_v32.rs"]
pub(crate) mod base_v32;
#[path = "pr_issue_link_picker_v87.rs"]
pub(crate) mod issue_link_v87;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrMetadataPickerAction {
    /// Genuine source index, not the filtered/display position.
    IndexedRow(usize),
    Move(i32),
    Page(i32),
    QueryFocus,
    FocusNext(bool),
    Choose,
    Close,
    Insert(String),
    Paste(String),
    Backspace,
    Delete,
    Clear,
    QueryCursor {
        right: bool,
        select: bool,
    },
    QueryEdge {
        end: bool,
        select: bool,
    },
    QuerySelectAll,
    QueryClick(usize),
    DetailScroll(i32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrMetadataPickerOutcome {
    Changed,
    Chosen(usize),
    Closed,
}

#[derive(Clone, Debug)]
pub struct PrMetadataPickerHit {
    pub rect: Rect,
    pub action: PrMetadataPickerAction,
}

#[derive(Clone)]
pub struct PrMetadataPicker {
    source: Arc<LoadedPrMetadataChoices>,
    state: PickerState,
    current: Option<Arc<CurrentMemberRows>>,
}
impl std::fmt::Debug for PrMetadataPicker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PrMetadataPicker(<bounded original-bound choices>)")
    }
}
impl PrMetadataPicker {
    pub fn new(source: Arc<LoadedPrMetadataChoices>) -> Self {
        let coverage = source.coverage();
        let accounting = source.accounting();
        let limits = source.limits();
        let invalid = coverage.reasons.len() > 128
            || coverage
                .reasons
                .iter()
                .any(|reason| reason.len() > MAX_FIELD_BYTES)
            || source
                .identity()
                .host
                .len()
                .saturating_add(source.identity().login.len())
                .saturating_add(source.target().pr.repository.len())
                > MAX_FIELD_BYTES;
        let diagnostics = if invalid {
            vec![]
        } else {
            coverage.reasons.clone()
        };
        let header = SourceHeader {
            scope: format!(
                "{} · {} #{} · @{} (account #{})",
                source.identity().host,
                source.target().pr.repository,
                source.target().pr.number,
                source.identity().login,
                source.identity().account_id.unwrap_or_default()
            ),
            counts: format!(
                "Pages {}/{} requested · rows kept {} / read {} / excluded {}",
                coverage.fetched_pages,
                coverage.requested_pages,
                coverage.admitted_rows,
                coverage.fetched_rows,
                coverage.excluded_rows
            ),
            accounting: format!(
                "Requests {} · charged {} bytes",
                accounting.requests(),
                accounting.charged_bytes(),
            ),
            captures: format!(
                "Captured stdout {} / diagnostics {} · failed reservation {} bytes",
                accounting.successful_stdout_bytes(),
                accounting.successful_diagnostic_bytes(),
                accounting.failed_reserved_bytes()
            ),
            limits: format!(
                "Caps: pages {} · rows {} · charged bytes {} · time {}s",
                limits.max_pages,
                limits.max_rows,
                limits.max_response_bytes,
                limits.max_duration.as_secs()
            ),
            diagnostics,
            invalid,
        };
        let state = PickerState::prepare(source.prepared(), header);
        Self {
            source,
            state,
            current: None,
        }
    }
    pub fn source(&self) -> &Arc<LoadedPrMetadataChoices> {
        &self.source
    }
    pub fn selected_index(&self) -> Option<usize> {
        self.state.selected_index()
    }
    pub fn query(&self) -> &str {
        &self.state.query.text
    }
    pub fn key(&self, code: KeyCode, modifiers: KeyModifiers) -> Option<PrMetadataPickerAction> {
        self.state.key(code, modifiers)
    }
    pub fn dispatch(&mut self, action: PrMetadataPickerAction) -> PrMetadataPickerOutcome {
        if let Some(rows) = &self.current {
            return self
                .state
                .dispatch(rows.as_ref(), action, |index| rows.choose(index));
        }
        let source = &self.source;
        self.state.dispatch(source.prepared(), action, |index| {
            source
                .choose(index)
                .map(|_| ())
                .map_err(|error| error.message)
        })
    }
    pub fn draw(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        theme: Theme,
    ) -> Vec<PrMetadataPickerHit> {
        if let Some(rows) = &self.current {
            return self.state.draw(frame, rows.as_ref(), area, theme);
        }
        self.state.draw(frame, self.source.prepared(), area, theme)
    }
    /// Root overlays capture the event even when this returns None, so there is
    /// no fall-through to the underlying PR. Hits belong to the latest draw.
    pub fn mouse(
        &self,
        kind: MouseEventKind,
        column: u16,
        row: u16,
        hits: &[PrMetadataPickerHit],
    ) -> Option<PrMetadataPickerAction> {
        self.state.mouse(kind, column, row, hits)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Query,
    List,
    Choose,
    Close,
}
impl Focus {
    fn next(self, backwards: bool) -> Self {
        let values = [Self::Query, Self::List, Self::Choose, Self::Close];
        let index = values.iter().position(|value| *value == self).unwrap_or(0);
        values[(index + if backwards { 3 } else { 1 }) % values.len()]
    }
}
struct SourceHeader {
    scope: String,
    counts: String,
    accounting: String,
    captures: String,
    limits: String,
    diagnostics: Vec<String>,
    invalid: bool,
}
#[derive(Clone)]
struct PickerState {
    query: TextBuffer,
    focus: Focus,
    visible: Vec<usize>,
    selected: usize,
    scroll: usize,
    page_size: usize,
    detail_scroll: usize,
    detail_size: usize,
    detail: Vec<String>,
    common_detail: Vec<String>,
    header: Vec<String>,
    message: String,
    blocked: bool,
    list_area: Rect,
    detail_area: Rect,
    query_area: Rect,
}
impl PickerState {
    fn prepare(choices: &impl PickerChoicesView, header: SourceHeader) -> Self {
        let mut state = Self {
            query: TextBuffer::default(),
            focus: Focus::Query,
            visible: vec![],
            selected: 0,
            scroll: 0,
            page_size: 1,
            detail_scroll: 0,
            detail_size: 1,
            detail: vec![],
            common_detail: vec![],
            header: vec![],
            message: "Read-only choices. Select a row, then Choose; publication still needs Preview/Confirm.".into(),
            blocked: false,
            list_area: Rect::default(),
            detail_area: Rect::default(),
            query_area: Rect::default(),
        };
        let prepared = state.prepare_header(choices, header).and_then(|()| {
            let mut bytes = 0usize;
            if choices.choices().len() > MAX_OPTIONS {
                return Err(
                    "Picker row cap exceeded; no choices hidden behind a complete claim".into(),
                );
            }
            for row in choices.choices() {
                for text in [Some(row.label()), row.disabled_reason()]
                    .into_iter()
                    .flatten()
                {
                    checked_field(text, &mut bytes)?;
                }
                if let Some(actor) = row.actor() {
                    for text in [
                        Some(actor.name.as_str()),
                        actor.host.as_deref(),
                        actor.login.as_deref(),
                        actor.avatar_url.as_deref(),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        checked_field(text, &mut bytes)?;
                    }
                }
            }
            state.visible = (0..choices.choices().len()).collect();
            state.prepare_detail(choices)
        });
        if let Err(reason) = prepared {
            state.blocked = true;
            state.visible.clear();
            state.detail.clear();
            state.common_detail.clear();
            state.header = vec!["Unavailable: native picker preparation limit reached".into()];
            state.message = reason;
        } else if let Some(reason) = choices.unavailable_reason() {
            state.message = bounded_message(reason);
        }
        state
    }
    fn prepare_header(
        &mut self,
        choices: &impl PickerChoicesView,
        header: SourceHeader,
    ) -> Result<(), String> {
        if header.invalid {
            return Err(
                "Source diagnostic/scope display caps exceeded; choice UI is unavailable".into(),
            );
        }
        let mut bytes = 0usize;
        for text in [
            &header.scope,
            &header.counts,
            &header.accounting,
            &header.captures,
            &header.limits,
        ] {
            checked_field(text, &mut bytes)?;
        }
        if header.diagnostics.len() > 128 {
            return Err("Picker diagnostic row cap exceeded".into());
        }
        for text in &header.diagnostics {
            checked_field(text, &mut bytes)?;
        }
        let availability = if choices.unavailable_reason().is_some() {
            "Unavailable: inspect reasons below; Choose is disabled"
        } else if choices.current_members() {
            "Current member snapshot: writer verifies complete sets before removal"
        } else if choices.complete() {
            "Complete catalogue: observed indexed choices; no automatic changes"
        } else {
            "Partial catalogue: observed choices only; unseen membership is unknown"
        };
        // Each summary is one bounded readable line. Full source text is also
        // available in the prepared, scrollable detail panel below.
        self.header = vec![
            single_line(&header.scope),
            availability.into(),
            single_line(&header.counts),
            single_line(&header.accounting),
            single_line(&header.captures),
            single_line(&header.limits),
        ];
        let mut detail = DetailRows::default();
        for text in [
            &header.scope,
            &header.counts,
            &header.accounting,
            &header.captures,
            &header.limits,
        ] {
            detail.text(text)?;
        }
        if let Some(reason) = choices.unavailable_reason() {
            checked_field(reason, &mut bytes)?;
            detail.text(reason)?;
        }
        if let Some(reason) = choices.coverage_notice() {
            checked_field(reason, &mut bytes)?;
            detail.text(reason)?;
        }
        for reason in &header.diagnostics {
            detail.text(reason)?;
        }
        self.common_detail = detail.rows;
        Ok(())
    }
    fn selected_index(&self) -> Option<usize> {
        self.visible.get(self.selected).copied()
    }
    fn prepare_detail(&mut self, choices: &impl PickerChoicesView) -> Result<(), String> {
        let mut detail = DetailRows::default();
        if let Some(row) = self
            .selected_index()
            .and_then(|index| choices.choices().get(index))
        {
            detail.text(row.label())?;
            if let Some(actor) = row.actor() {
                detail.text(&format!(
                    "[{}] {} · avatar fallback",
                    actor.initials(),
                    actor.label()
                ))?;
            }
            if let Some(value) = row.value() {
                let text = match value {
                    PrMetadataValue::BaseBranch { name, .. } => format!("Exact branch: {name}"),
                    PrMetadataValue::Label { id, name } => format!("Exact label #{id}: {name}"),
                    PrMetadataValue::Assignee { account_id, login } => {
                        format!("Exact assignee account #{account_id}: @{login}")
                    }
                    PrMetadataValue::Reviewer { account_id, login } => {
                        format!("Exact reviewer account #{account_id}: @{login}")
                    }
                    PrMetadataValue::ReviewTeam { team_id, slug } => {
                        format!("Exact team #{team_id}: {slug} (not a user account)")
                    }
                    PrMetadataValue::Milestone { number } => format!("Exact milestone #{number}"),
                };
                detail.text(&text)?;
                if let PrMetadataValue::BaseBranch { name, .. } = value {
                    if sanitize_terminal(name) != *name {
                        detail.text("Exact raw branch (escaped; safe display differs):")?;
                        detail.escaped_name(name)?;
                    }
                }
            }
            for text in row.extra_detail() {
                detail.text(text)?;
            }
            detail.text(
                choices
                    .unavailable_reason()
                    .or_else(|| row.disabled_reason())
                    .unwrap_or(if choices.current_members() {
                        "Current observed member. Choose is navigation only; Preview/Confirm and fresh complete-set guards still required."
                    } else {
                        "Eligible observed value. Choose does not publish or change local files."
                    }),
            )?;
        } else {
            detail.text("No matching observed choice. Scope was not broadened.")?;
        }
        for text in &self.common_detail {
            detail.push(text.clone())?;
        }
        self.detail = detail.rows;
        self.detail_scroll = 0;
        Ok(())
    }
    fn key(&self, code: KeyCode, modifiers: KeyModifiers) -> Option<PrMetadataPickerAction> {
        use PrMetadataPickerAction::*;
        let shift = modifiers.contains(KeyModifiers::SHIFT);
        let control = modifiers.contains(KeyModifiers::CONTROL);
        Some(match code {
            KeyCode::Esc => Close,
            KeyCode::Enter => {
                if self.focus == Focus::Close {
                    Close
                } else {
                    Choose
                }
            }
            KeyCode::Tab | KeyCode::BackTab => FocusNext(shift || code == KeyCode::BackTab),
            KeyCode::Up => Move(-1),
            KeyCode::Down => Move(1),
            KeyCode::PageUp if control => DetailScroll(-(self.detail_size as i32)),
            KeyCode::PageDown if control => DetailScroll(self.detail_size as i32),
            KeyCode::PageUp => Page(-1),
            KeyCode::PageDown => Page(1),
            KeyCode::Home if self.focus == Focus::Query => QueryEdge {
                end: false,
                select: shift,
            },
            KeyCode::End if self.focus == Focus::Query => QueryEdge {
                end: true,
                select: shift,
            },
            KeyCode::Home => Move(i32::MIN),
            KeyCode::End => Move(i32::MAX),
            KeyCode::Left if self.focus == Focus::Query => QueryCursor {
                right: false,
                select: shift,
            },
            KeyCode::Right if self.focus == Focus::Query => QueryCursor {
                right: true,
                select: shift,
            },
            KeyCode::Backspace => Backspace,
            KeyCode::Delete => Delete,
            KeyCode::Char('u') if control => Clear,
            KeyCode::Char('a') if control => QuerySelectAll,
            KeyCode::Char('j') if self.focus == Focus::List && modifiers.is_empty() => Move(1),
            KeyCode::Char('k') if self.focus == Focus::List && modifiers.is_empty() => Move(-1),
            KeyCode::Char(character)
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                Insert(character.to_string())
            }
            _ => return None,
        })
    }
    fn dispatch(
        &mut self,
        choices: &impl PickerChoicesView,
        action: PrMetadataPickerAction,
        choose: impl FnOnce(usize) -> Result<(), String>,
    ) -> PrMetadataPickerOutcome {
        use PrMetadataPickerAction::*;
        match action {
            Close => return PrMetadataPickerOutcome::Closed,
            Choose => {
                if self.blocked {
                    self.message = "Choose disabled: picker preparation is unavailable".into();
                } else if let Some(index) = self.selected_index() {
                    match choose(index) {
                        Ok(()) => return PrMetadataPickerOutcome::Chosen(index),
                        Err(reason) => self.message = bounded_message(&reason),
                    }
                } else {
                    self.message = "Choose disabled: no observed matching member".into();
                }
            }
            QueryFocus => self.focus = Focus::Query,
            FocusNext(backwards) => self.focus = self.focus.next(backwards),
            IndexedRow(index) => {
                if let Some(position) = self.visible.iter().position(|value| *value == index) {
                    self.selected = position;
                    self.focus = Focus::List;
                    self.selection_changed(choices);
                } else {
                    self.message = "That genuine row is not in the current observed filter; no selection changed".into();
                }
            }
            Move(delta) | Page(delta) => {
                let amount = if matches!(action, Page(_)) {
                    delta.saturating_mul(self.page_size as i32)
                } else {
                    delta
                };
                self.selected = self
                    .selected
                    .saturating_add_signed(amount as isize)
                    .min(self.visible.len().saturating_sub(1));
                self.focus = Focus::List;
                self.selection_changed(choices);
            }
            Insert(text) | Paste(text) => {
                self.focus = Focus::Query;
                // Refuse before sanitizing/cloning; a paste cannot allocate a
                // giant transient value or silently turn multiline into a ref.
                let replaced = self.query.selection().map_or(0, |range| range.len());
                let raw_bytes = self
                    .query
                    .text
                    .len()
                    .saturating_sub(replaced)
                    .saturating_add(text.len());
                if text.len() > MAX_QUERY_BYTES
                    || raw_bytes > MAX_QUERY_BYTES
                    || text.chars().any(char::is_control)
                    || sanitize_terminal(&text) != text
                {
                    self.message =
                        "Filter refused: at most 512 UTF8 bytes, 128 characters, and one line"
                            .into();
                } else {
                    let mut next = self.query.clone();
                    if next.insert(&text).is_err()
                        || next.text.chars().count() > MAX_QUERY_CHARACTERS
                    {
                        self.message =
                            "Filter refused: at most 512 UTF8 bytes and 128 characters".into();
                    } else {
                        self.query = next;
                        self.filter(choices);
                    }
                }
            }
            Backspace | Delete | Clear => {
                self.focus = Focus::Query;
                match action {
                    Backspace => self.query.delete_backward(),
                    Delete => self.query.delete_forward(),
                    _ => self.query.clear(),
                }
                self.filter(choices);
            }
            QueryCursor { right, select } => {
                self.focus = Focus::Query;
                self.query.horizontal(right, select);
            }
            QueryEdge { end, select } => {
                self.focus = Focus::Query;
                self.query.line_edge(end, true, select);
            }
            QuerySelectAll => {
                self.focus = Focus::Query;
                self.query.select_all();
            }
            QueryClick(column) => {
                self.focus = Focus::Query;
                self.query.click(0, column, false);
            }
            DetailScroll(delta) => {
                self.detail_scroll = self
                    .detail_scroll
                    .saturating_add_signed(delta as isize)
                    .min(self.detail.len().saturating_sub(self.detail_size))
            }
        }
        PrMetadataPickerOutcome::Changed
    }
    fn selection_changed(&mut self, choices: &impl PickerChoicesView) {
        self.keep_selection_visible();
        if let Err(reason) = self.prepare_detail(choices) {
            self.blocked = true;
            self.message = reason;
        } else {
            self.message = choices.unavailable_reason().map_or_else(
                || self.selected_index().and_then(|index| choices.choices().get(index)).and_then(|row| row.disabled_reason()).unwrap_or("Selected exact observed row; Enter/Choose validates it without publication").to_owned(),
                str::to_owned,
            );
        }
    }
    fn keep_selection_visible(&mut self) {
        self.scroll = self.scroll.min(self.selected);
        if self.selected >= self.scroll.saturating_add(self.page_size) {
            self.scroll = self.selected + 1 - self.page_size;
        }
        self.scroll = self
            .scroll
            .min(self.visible.len().saturating_sub(self.page_size));
    }
    fn filter(&mut self, choices: &impl PickerChoicesView) {
        if self.blocked {
            return;
        }
        let pattern = Pattern::new(
            &self.query.text,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
        let mut buffer = vec![];
        let mut scores = choices
            .choices()
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                pattern
                    .score(Utf32Str::new(row.label(), &mut buffer), &mut matcher)
                    .map(|score| (index, score))
            })
            .collect::<Vec<_>>();
        scores.sort_by(|(left, left_score), (right, right_score)| {
            right_score.cmp(left_score).then_with(|| left.cmp(right))
        });
        self.visible = scores.into_iter().map(|(index, _)| index).collect();
        self.selected = 0;
        self.scroll = 0;
        self.selection_changed(choices);
    }
    fn mouse(
        &self,
        kind: MouseEventKind,
        column: u16,
        row: u16,
        hits: &[PrMetadataPickerHit],
    ) -> Option<PrMetadataPickerAction> {
        let position = Position::new(column, row);
        match kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let delta = if kind == MouseEventKind::ScrollUp {
                    -3
                } else {
                    3
                };
                if self.list_area.contains(position) {
                    Some(PrMetadataPickerAction::Move(delta))
                } else if self.detail_area.contains(position) {
                    Some(PrMetadataPickerAction::DetailScroll(delta))
                } else {
                    None
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if self.query_area.contains(position) {
                    Some(PrMetadataPickerAction::QueryClick(usize::from(
                        column - self.query_area.x,
                    )))
                } else {
                    hits.iter()
                        .rev()
                        .find(|hit| hit.rect.contains(position))
                        .map(|hit| hit.action.clone())
                }
            }
            _ => None,
        }
    }
    fn draw(
        &mut self,
        frame: &mut Frame,
        choices: &impl PickerChoicesView,
        area: Rect,
        theme: Theme,
    ) -> Vec<PrMetadataPickerHit> {
        self.list_area = Rect::default();
        self.detail_area = Rect::default();
        self.query_area = Rect::default();
        let mut hits = vec![];
        if area.is_empty() {
            return hits;
        }
        frame.render_widget(Clear, area);
        let title = match choices.kind() {
            PickerKindV87::IssueLinks => "PR development links · Issue",
            PickerKindV87::Metadata(kind) if choices.current_members() => match kind {
                MetadataChoices::Labels => "Current PR metadata members · Label",
                MetadataChoices::Assignees => "Current PR metadata members · Assignee",
                MetadataChoices::Collaborators => "Current PR metadata members · Reviewer",
                MetadataChoices::Teams => "Current PR metadata members · Reviewer team",
                _ => "Current PR metadata members",
            },
            PickerKindV87::Metadata(kind) => match kind {
                MetadataChoices::Branches => "PR metadata choices · Base branch",
                MetadataChoices::Labels => "PR metadata choices · Label",
                MetadataChoices::Assignees => "PR metadata choices · Assignee",
                MetadataChoices::Collaborators => "PR metadata choices · Reviewer",
                MetadataChoices::Teams => "PR metadata choices · Reviewer team",
                MetadataChoices::Milestones => "PR metadata choices · Milestone",
            },
        };
        let focus_label = match self.focus {
            Focus::Query => "Filter focused",
            Focus::List => "Rows focused",
            Focus::Choose => "Choose focused",
            Focus::Close => "Back focused",
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!("{title} · {focus_label}"))
            .style(theme.base());
        let body = block.inner(area);
        frame.render_widget(block, area);
        if body.is_empty() {
            return hits;
        }
        let mut y = body.y;
        for (index, label) in self.header.iter().enumerate() {
            if y >= body.bottom().saturating_sub(3) {
                break;
            }
            let role = if index != 1 {
                SemanticToken::Muted
            } else if self.blocked || choices.unavailable_reason().is_some() {
                SemanticToken::Failure
            } else if choices.complete() {
                SemanticToken::Success
            } else {
                SemanticToken::Pending
            };
            frame.render_widget(
                Paragraph::new(clipped(label, usize::from(body.width))).style(theme.style(role)),
                Rect::new(body.x, y, body.width, 1),
            );
            y += 1;
        }
        if y < body.bottom().saturating_sub(2) {
            let caption = if self.focus == Focus::Query {
                "Filter focused: "
            } else {
                "Filter: "
            };
            let caption_width = caption.len().min(usize::from(body.width)) as u16;
            frame.render_widget(
                Paragraph::new(caption).style(theme.style(SemanticToken::Focus)),
                Rect::new(body.x, y, caption_width, 1),
            );
            self.query_area = Rect::new(body.x + caption_width, y, body.width - caption_width, 1);
            hits.push(PrMetadataPickerHit {
                rect: Rect::new(body.x, y, body.width, 1),
                action: PrMetadataPickerAction::QueryFocus,
            });
            self.query
                .ensure_visible(usize::from(self.query_area.width), 1);
            let text = visible_query(&self.query, usize::from(self.query_area.width));
            frame.render_widget(
                Paragraph::new(text).style(theme.style(SemanticToken::Normal)),
                self.query_area,
            );
            if self.focus == Focus::Query && self.query_area.width > 0 {
                let column = self
                    .query
                    .position()
                    .1
                    .saturating_sub(self.query.left_column)
                    .min(usize::from(self.query_area.width - 1));
                frame.set_cursor_position(Position::new(self.query_area.x + column as u16, y));
            }
            y += 1;
        }
        let buttons = [
            (
                "[Enter Choose]",
                PrMetadataPickerAction::Choose,
                Some(Focus::Choose),
            ),
            (
                "[Esc Back]",
                PrMetadataPickerAction::Close,
                Some(Focus::Close),
            ),
            ("[Ctrl-U Clear]", PrMetadataPickerAction::Clear, None),
            ("[PgUp]", PrMetadataPickerAction::Page(-1), None),
            ("[PgDn]", PrMetadataPickerAction::Page(1), None),
        ];
        let mut x = body.x;
        for (label, action, focus) in buttons {
            let width = label.len() as u16;
            if x.saturating_add(width) > body.right() || y >= body.bottom().saturating_sub(1) {
                break;
            }
            let rect = Rect::new(x, y, width, 1);
            let mut style = theme.style(
                if action == PrMetadataPickerAction::Choose
                    && (self.blocked
                        || choices.unavailable_reason().is_some()
                        || self
                            .selected_index()
                            .and_then(|index| choices.choices()[index].disabled_reason())
                            .is_some())
                {
                    SemanticToken::Muted
                } else {
                    SemanticToken::Focus
                },
            );
            if focus == Some(self.focus) {
                style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
            }
            frame.render_widget(Paragraph::new(label).style(style), rect);
            hits.push(PrMetadataPickerHit { rect, action });
            x += width + 1;
        }
        y += 1;
        let footer_y = body.bottom().saturating_sub(1);
        if y < footer_y {
            let remaining = usize::from(footer_y - y);
            let detail_height = (remaining / 2).clamp(1, 7);
            let list_height = remaining.saturating_sub(detail_height);
            self.page_size = list_height.max(1);
            self.keep_selection_visible();
            self.list_area = Rect::new(body.x, y, body.width, list_height as u16);
            for (offset, index) in self
                .visible
                .iter()
                .skip(self.scroll)
                .take(list_height)
                .enumerate()
            {
                let row = &choices.choices()[*index];
                let disabled = self.blocked
                    || choices.unavailable_reason().is_some()
                    || row.disabled_reason().is_some();
                let prefix = if disabled {
                    "! disabled · "
                } else if choices.current_members() {
                    "◆ current · "
                } else {
                    "◆ eligible · "
                };
                let label = format!("{prefix}{}", single_line(row.label()));
                let rect = Rect::new(body.x, y + offset as u16, body.width, 1);
                let token = if disabled {
                    SemanticToken::Pending
                } else {
                    row.role()
                };
                let style = if self.selected_index() == Some(*index) {
                    theme.selected(token)
                } else {
                    theme.style(token)
                };
                frame.render_widget(
                    Paragraph::new(clipped(&label, usize::from(rect.width))).style(style),
                    rect,
                );
                hits.push(PrMetadataPickerHit {
                    rect,
                    action: PrMetadataPickerAction::IndexedRow(*index),
                });
            }
            if self.visible.is_empty() && list_height > 0 {
                frame.render_widget(
                    Paragraph::new("No observed matching choices; scope unchanged")
                        .style(theme.style(SemanticToken::Muted)),
                    self.list_area,
                );
            }
            y += list_height as u16;
            let has_tip = self
                .selected_index()
                .and_then(|index| choices.choices()[index].value())
                .is_some_and(|value| matches!(value, PrMetadataValue::BaseBranch { .. }));
            self.detail_size =
                usize::from(footer_y.saturating_sub(y.saturating_add(1 + u16::from(has_tip))))
                    .max(1);
            if y < footer_y {
                let labels = [
                    ("[Ctrl-PgUp Detail]", -(self.detail_size as i32)),
                    ("[Ctrl-PgDn Detail]", self.detail_size as i32),
                ];
                let mut x = body.x;
                for (label, delta) in labels {
                    let width = label.len() as u16;
                    if x + width > body.right() {
                        break;
                    }
                    let rect = Rect::new(x, y, width, 1);
                    frame.render_widget(
                        Paragraph::new(label).style(theme.style(SemanticToken::Focus)),
                        rect,
                    );
                    hits.push(PrMetadataPickerHit {
                        rect,
                        action: PrMetadataPickerAction::DetailScroll(delta),
                    });
                    x += width + 1;
                }
                y += 1;
            }
            if let Some(PrMetadataValue::BaseBranch { commit_sha, .. }) = self
                .selected_index()
                .and_then(|index| choices.choices()[index].value())
            {
                if y < footer_y {
                    let label = if usize::from(body.width) >= commit_sha.len() + 5 {
                        format!("Tip: {commit_sha}")
                    } else {
                        "Full tip requires a wider terminal; it is not abbreviated".into()
                    };
                    frame.render_widget(
                        Paragraph::new(label).style(theme.style(SemanticToken::Reference)),
                        Rect::new(body.x, y, body.width, 1),
                    );
                    y += 1;
                }
            }
            self.detail_size = usize::from(footer_y.saturating_sub(y)).max(1);
            self.detail_scroll = self
                .detail_scroll
                .min(self.detail.len().saturating_sub(self.detail_size));
            self.detail_area = Rect::new(body.x, y, body.width, footer_y.saturating_sub(y));
            for (offset, label) in self
                .detail
                .iter()
                .skip(self.detail_scroll)
                .take(usize::from(self.detail_area.height))
                .enumerate()
            {
                frame.render_widget(
                    Paragraph::new(clipped(label, usize::from(body.width)))
                        .style(theme.style(SemanticToken::Normal)),
                    Rect::new(body.x, y + offset as u16, body.width, 1),
                );
            }
        }
        frame.render_widget(
            Paragraph::new(clipped(&self.message, usize::from(body.width)))
                .style(theme.style(SemanticToken::Pending)),
            Rect::new(body.x, footer_y, body.width, 1),
        );
        hits
    }
}

fn checked_field(text: &str, bytes: &mut usize) -> Result<(), String> {
    if text.len() > MAX_FIELD_BYTES || text.len() > MAX_PREPARED_BYTES.saturating_sub(*bytes) {
        return Err("Native picker field/prepared byte cap exceeded before formatting".into());
    }
    *bytes += text.len();
    Ok(())
}
fn bounded_message(reason: &str) -> String {
    if reason.len() > MAX_FIELD_BYTES {
        "Choice refused; bounded source explanation unavailable".into()
    } else {
        sanitize_terminal(reason).replace(['\n', '\t'], " ")
    }
}
fn single_line(text: &str) -> String {
    sanitize_terminal(text).replace(['\n', '\t'], " ")
}
fn clipped(text: &str, cells: usize) -> String {
    if cells == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut width = 0usize;
    let mut ellipsis_boundary = 0usize;
    for grapheme in text.graphemes(true) {
        let next = grapheme_width(grapheme);
        if width.saturating_add(next) > cells {
            result.truncate(ellipsis_boundary);
            result.push('…');
            break;
        }
        result.push_str(grapheme);
        width += next;
        if width < cells {
            ellipsis_boundary = result.len();
        }
    }
    result
}
fn visible_query(query: &TextBuffer, cells: usize) -> String {
    let mut offset = 0usize;
    let mut width = 0usize;
    for (index, grapheme) in query.text.grapheme_indices(true) {
        if width >= query.left_column {
            offset = index;
            break;
        }
        width += grapheme_width(grapheme);
        offset = index + grapheme.len();
    }
    clipped(&query.text[offset..], cells)
}
#[derive(Default)]
struct DetailRows {
    rows: Vec<String>,
    bytes: usize,
}
impl DetailRows {
    fn escaped_name(&mut self, raw: &str) -> Result<(), String> {
        if raw.len() > MAX_FIELD_BYTES {
            return Err("Raw branch escape preparation cap exceeded".into());
        }
        let mut row = String::new();
        for character in raw.chars().flat_map(char::escape_default) {
            if row.len() == PREPARED_CELLS {
                self.push(std::mem::take(&mut row))?;
            }
            row.push(character);
        }
        self.push(row)
    }
    fn push(&mut self, text: String) -> Result<(), String> {
        if self.rows.len() >= MAX_DETAIL_ROWS
            || text.len() > MAX_FIELD_BYTES
            || text.len() > MAX_DETAIL_BYTES.saturating_sub(self.bytes)
        {
            return Err("Native picker detail row/byte cap reached; selection unavailable".into());
        }
        self.bytes += text.len();
        self.rows.push(text);
        Ok(())
    }
    fn text(&mut self, text: &str) -> Result<(), String> {
        if text.len() > MAX_FIELD_BYTES {
            return Err("Native picker detail raw field cap exceeded".into());
        }
        let safe = sanitize_terminal(text).replace('\t', "⇥");
        for line in safe.split('\n') {
            let mut current = String::new();
            let mut cells = 0usize;
            for word in line.split_inclusive(char::is_whitespace) {
                let word_cells = grapheme_width(word);
                if !current.is_empty() && cells.saturating_add(word_cells) > PREPARED_CELLS {
                    self.push(std::mem::take(&mut current))?;
                    cells = 0;
                }
                for grapheme in word.graphemes(true) {
                    let width = grapheme_width(grapheme);
                    if !current.is_empty() && cells.saturating_add(width) > PREPARED_CELLS {
                        self.push(std::mem::take(&mut current))?;
                        cells = 0;
                    }
                    if grapheme.len() > MAX_FIELD_BYTES.saturating_sub(current.len()) {
                        return Err("Native picker detail grapheme byte cap exceeded".into());
                    }
                    current.push_str(grapheme);
                    cells = cells.saturating_add(width);
                }
            }
            self.push(current)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "pr_metadata_picker_tests.rs"]
mod tests;
