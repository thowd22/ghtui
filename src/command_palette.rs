//! Bounded caller-prepared command discovery, not current target authority.
//! The native overlay captures input and dispatches existing guarded actions.
//!
//! The app must invalidate this catalogue on context/source/principal changes
//! and dispatch a chosen existing Action through its ordinary current guards.
//! This component never executes an action, probes a host or stores approval.
use crate::{
    app::Action,
    editor::{display_line, display_width, grapheme_width, TextBuffer},
    providers::runner::sanitize_terminal,
    theme::{SemanticToken, Theme},
};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use nucleo_matcher::{
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    Matcher, Utf32Str,
};
use ratatui::{
    layout::{Position, Rect},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};
use std::{collections::BTreeSet, fmt};
use unicode_segmentation::UnicodeSegmentation;

const MAX_ENTRIES: usize = 256;
const MAX_FIELD_BYTES: usize = 4096;
const MAX_KEYWORDS: usize = 16;
const MAX_KEYWORD_BYTES: usize = 512;
const MAX_RAW_BYTES: usize = 512 * 1024;
const MAX_PREPARED_BYTES: usize = 2 * 1024 * 1024;
const MAX_DETAIL_ROWS: usize = 256;
const MAX_QUERY_BYTES: usize = 512;
const MAX_QUERY_GRAPHEMES: usize = 128;

/// Owned input prevents action payload clones during preparation. Labels and
/// keywords must be safe single-line text. IDs are stable catalogue identities,
/// never commands, permissions or GitHub resource identity inferred from text.
pub struct PaletteEntry {
    pub id: String,
    pub label: String,
    pub keywords: Vec<String>,
    pub action: Action,
    /// The key that runs the same action from the view (a footer key, a view number).
    /// Display only: it is never searched or dispatched by the palette.
    pub key: Option<String>,
}
impl fmt::Debug for PaletteEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PaletteEntry").finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteAction {
    /// Genuine original catalogue index; selection alone emits no app action.
    IndexedRow(usize),
    /// An explicit result click, equivalent to Enter on that selected result.
    Activate(usize),
    Move(i32),
    Page(i32),
    ListEdge(bool),
    FocusQuery,
    FocusList,
    FocusDetails,
    FocusNext(bool),
    QueryClick(usize),
    Insert(String),
    Paste(String),
    Backspace,
    Delete,
    ClearQuery,
    SelectAll,
    Cursor {
        right: bool,
        select: bool,
    },
    QueryEdge {
        end: bool,
        select: bool,
    },
    DetailScroll(i32),
    DetailPage(i32),
    DetailEdge(bool),
    Choose,
    Close,
}
#[derive(PartialEq, Eq)]
pub enum PaletteOutcome {
    Changed,
    Rejected(String),
    Chosen {
        original_index: usize,
        action: Action,
    },
    Closed,
    Ignored,
}
impl fmt::Debug for PaletteOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Changed => f.write_str("Changed"),
            Self::Rejected(_) => f.write_str("Rejected"),
            Self::Chosen { original_index, .. } => f
                .debug_struct("Chosen")
                .field("original_index", original_index)
                .finish_non_exhaustive(),
            Self::Closed => f.write_str("Closed"),
            Self::Ignored => f.write_str("Ignored"),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteHit {
    pub rect: Rect,
    pub action: PaletteAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Query,
    List,
    Details,
    Choose,
    Close,
}
impl Focus {
    fn next(self, backwards: bool) -> Self {
        let order = [
            Self::Query,
            Self::List,
            Self::Details,
            Self::Choose,
            Self::Close,
        ];
        let index = order.iter().position(|focus| *focus == self).unwrap_or(0);
        order[(index + if backwards { 4 } else { 1 }) % order.len()]
    }
    fn label(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::List => "results",
            Self::Details => "details",
            Self::Choose => "choose",
            Self::Close => "close",
        }
    }
}
struct PreparedEntry {
    id: String,
    label: String,
    search: String,
    exact_keywords: Vec<String>,
    detail: Vec<String>,
    explanation: bool,
    action: Option<Action>,
    shortcut: String,
}
/// Intentionally no Clone/serde: this is transient discovery state, not a
/// serializable/restored authorization or an approval to execute an operation.
pub struct CommandPalette {
    entries: Vec<PreparedEntry>,
    visible: Vec<usize>,
    selected: usize,
    list_top: usize,
    detail_top: usize,
    query: TextBuffer,
    focus: Focus,
    message: String,
    list_area: Rect,
    query_area: Rect,
    detail_area: Rect,
    hits: Vec<PaletteHit>,
    list_size: usize,
    detail_size: usize,
    terminal: bool,
    detail_width_v2: u16,
    detail_current_v2: Option<(usize, u16, std::sync::Arc<[String]>)>,
    detail_prior_v2: Option<(usize, u16, std::sync::Arc<[String]>)>,
}
impl fmt::Debug for CommandPalette {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandPalette")
            .field("entries", &self.entries.len())
            .field("matching", &self.visible.len())
            .field("terminal", &self.terminal)
            .finish_non_exhaustive()
    }
}
impl CommandPalette {
    pub fn new(entries: Vec<PaletteEntry>) -> Result<Self, String> {
        // Check all raw inputs/counts before preparation, case folding or any
        // clone. Existing typed actions are moved, never debugged/serialized.
        if entries.len() > MAX_ENTRIES {
            return Err("Command catalogue exceeds 256 entries".into());
        }
        let mut ids = BTreeSet::new();
        let mut raw_bytes = 0usize;
        for entry in &entries {
            if entry.id.is_empty()
                || entry.id.len() > 128
                || !entry.id.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
                })
                || !ids.insert(entry.id.as_str())
            {
                return Err("Command catalogue IDs must be stable, safe and unique".into());
            }
            if entry.label.len() > MAX_FIELD_BYTES
                || entry.label.trim().is_empty()
                || display_width(&entry.label) == 0
                || entry.keywords.len() > MAX_KEYWORDS
            {
                return Err("Command needs a label and at most 16 keywords".into());
            }
            raw_bytes = raw_bytes.saturating_add(entry.id.len());
            for (index, field) in std::iter::once(&entry.label)
                .chain(entry.keywords.iter())
                .enumerate()
            {
                let max_bytes = if index == 0 {
                    MAX_FIELD_BYTES
                } else {
                    MAX_KEYWORD_BYTES
                };
                if field.len() > max_bytes || !safe_line(field) {
                    return Err("Command labels/keywords need safe single-line bounded text".into());
                }
                raw_bytes = raw_bytes.saturating_add(field.len());
            }
            if let Action::Explain(reason) = &entry.action {
                if reason.len() > MAX_FIELD_BYTES {
                    return Err("Command explanation exceeds 4096 bytes".into());
                }
                raw_bytes = raw_bytes.saturating_add(reason.len());
            }
            if raw_bytes > MAX_RAW_BYTES {
                return Err("Command catalogue raw text budget exceeded".into());
            }
        }
        let mut prepared = Vec::with_capacity(entries.len());
        let mut prepared_bytes = 0usize;
        for entry in entries {
            let explanation = matches!(&entry.action, Action::Explain(_));
            let reason = match &entry.action {
                Action::Explain(reason) => sanitize_terminal(reason).replace('\t', "    "),
                _ => "The app rechecks the current target and permissions. Writes still require their own preview and confirmation.".into(),
            };
            let mut exact_keywords = entry
                .keywords
                .iter()
                .map(|word| word.trim().to_lowercase())
                .collect::<Vec<_>>();
            exact_keywords.push(entry.id.to_lowercase());
            exact_keywords.push(entry.label.to_lowercase());
            let search = format!(
                "{} {} {} {}",
                entry.id,
                entry.label,
                entry.keywords.join(" "),
                if explanation {
                    reason.replace(['\n', '\r', '\t'], " ")
                } else {
                    // Generic workflow disclaimers are not command keywords:
                    // they would make unrelated results match "write" etc.
                    String::new()
                }
            );
            let detail = prepare_lines(&format!(
                "{}\nID: {}\n{}\nCatalogue observation is not current mutation authority.",
                entry.label, entry.id, reason
            ))?;
            prepared_bytes = prepared_bytes
                .saturating_add(entry.id.len())
                .saturating_add(entry.label.len())
                .saturating_add(search.len())
                .saturating_add(exact_keywords.iter().map(String::len).sum::<usize>())
                .saturating_add(detail.iter().map(String::len).sum::<usize>());
            if prepared_bytes > MAX_PREPARED_BYTES {
                return Err("Command catalogue prepared text budget exceeded".into());
            }
            prepared.push(PreparedEntry {
                id: entry.id,
                label: entry.label,
                search,
                exact_keywords,
                detail,
                explanation,
                action: Some(entry.action),
                shortcut: entry
                    .key
                    .filter(|key| !key.is_empty() && key.len() <= 32 && safe_line(key))
                    .unwrap_or_default(),
            });
        }
        let mut palette = Self {
            entries: prepared,
            visible: vec![],
            selected: 0,
            list_top: 0,
            detail_top: 0,
            query: TextBuffer::default(),
            focus: Focus::Query,
            message: "Click a result or press Enter; current workflow guards still apply.".into(),
            list_area: Rect::default(),
            query_area: Rect::default(),
            detail_area: Rect::default(),
            hits: vec![],
            list_size: 1,
            detail_size: 1,
            terminal: false,
            detail_width_v2: 64,
            detail_current_v2: None,
            detail_prior_v2: None,
        };
        palette.filter();
        Ok(palette)
    }
    /// Display-only key hint shown beside a runnable command. It is never
    /// searched or dispatched; the existing key handler owns that shortcut.
    pub fn set_shortcut(&mut self, id: &str, key: &str) {
        if key.is_empty() || key.len() > 32 || !safe_line(key) {
            return;
        }
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.shortcut = key.into();
        }
    }
    pub fn shortcut(&self, id: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.id == id && !entry.shortcut.is_empty())
            .map(|entry| entry.shortcut.as_str())
    }
    pub fn visible_indices(&self) -> &[usize] {
        &self.visible
    }
    pub fn selected_index(&self) -> Option<usize> {
        self.visible.get(self.selected).copied()
    }
    pub fn selected_id(&self) -> Option<&str> {
        self.selected_index()
            .map(|index| self.entries[index].id.as_str())
    }
    pub fn query(&self) -> &str {
        &self.query.text
    }
    pub fn clear_layout(&mut self) {
        self.list_area = Rect::default();
        self.query_area = Rect::default();
        self.detail_area = Rect::default();
        self.hits.clear();
    }
    /// Drop old action payloads/private labels, not merely their hit rectangles.
    /// Root calls this or drops the instance on any contextual privacy change.
    pub fn invalidate(&mut self) {
        self.terminal = true;
        self.entries.clear();
        self.visible.clear();
        self.query.clear();
        self.message.clear();
        self.detail_current_v2 = None;
        self.detail_prior_v2 = None;
        self.clear_layout();
    }
    fn detail(&self) -> &[String] {
        if let Some((index, width, rows)) = &self.detail_current_v2 {
            if Some(*index) == self.selected_index() && *width == self.detail_width_v2 {
                return rows;
            }
        }
        self.selected_index()
            .map_or(&[], |index| self.entries[index].detail.as_slice())
    }
    /// Event/preparation boundary only, never draw: bounded current/prior
    /// geometry variants over the same admitted catalogue and original index.
    pub fn prepare_width_v2(&mut self, width: u16) -> Result<(), String> {
        self.detail_width_v2 = width;
        let Some(index) = self.selected_index() else {
            self.detail_current_v2 = None;
            return Ok(());
        };
        if !(20..=400).contains(&width) {
            return Err("Command detail width is unavailable; Close remains accessible".into());
        }
        if self
            .detail_current_v2
            .as_ref()
            .is_some_and(|(old, w, _)| *old == index && *w == width)
        {
            return Ok(());
        }
        if self
            .detail_prior_v2
            .as_ref()
            .is_some_and(|(old, w, _)| *old == index && *w == width)
        {
            std::mem::swap(&mut self.detail_current_v2, &mut self.detail_prior_v2);
            return Ok(());
        }
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        for raw in &self.entries[index].detail {
            if raw
                .graphemes(true)
                .any(|grapheme| grapheme_width(grapheme) > usize::from(width))
            {
                return Err("Command detail grapheme exceeds the actual viewport".into());
            }
            for row in wrap_words(raw, usize::from(width)) {
                bytes += row.len();
                rows.push(row);
                if rows.len() > 512 || bytes > MAX_PREPARED_BYTES {
                    return Err("Command width detail exceeds its bounded rows/bytes".into());
                }
            }
        }
        if rows.len() > 512 || bytes > MAX_PREPARED_BYTES {
            return Err("Command width detail exceeds its bounded rows/bytes".into());
        }
        self.detail_prior_v2 = self.detail_current_v2.take();
        if self.detail_prior_v2.as_ref().is_some_and(|(_, _, prior)| {
            prior
                .iter()
                .map(String::len)
                .sum::<usize>()
                .saturating_add(bytes)
                > MAX_PREPARED_BYTES
        }) {
            self.detail_prior_v2 = None;
        }
        self.detail_current_v2 = Some((index, width, rows.into()));
        self.detail_top = self.detail_top.min(self.detail().len().saturating_sub(1));
        self.clear_layout();
        Ok(())
    }
    fn keep_visible(&mut self) {
        self.list_top = self.list_top.min(self.selected);
        if self.selected >= self.list_top.saturating_add(self.list_size) {
            self.list_top = self.selected + 1 - self.list_size;
        }
        self.list_top = self
            .list_top
            .min(self.visible.len().saturating_sub(self.list_size));
        self.detail_top = self
            .detail_top
            .min(self.detail().len().saturating_sub(self.detail_size));
    }
    fn filter(&mut self) {
        let query = self.query.text.trim();
        if query.is_empty() {
            // Runnable commands first; unavailable ones keep their original
            // relative order below so "[why unavailable]" stays discoverable.
            let (available, unavailable): (Vec<usize>, Vec<usize>) =
                (0..self.entries.len()).partition(|index| !self.entries[*index].explanation);
            self.visible = available.into_iter().chain(unavailable).collect();
        } else {
            let folded = query.to_lowercase();
            let pattern = Pattern::new(
                query,
                CaseMatching::Smart,
                Normalization::Smart,
                AtomKind::Fuzzy,
            );
            let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
            let mut buffer = vec![];
            let mut scores = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| {
                    let exact = entry.exact_keywords.iter().any(|word| word == &folded);
                    let score =
                        pattern.score(Utf32Str::new(&entry.search, &mut buffer), &mut matcher);
                    // A match in the label/keywords/ID outranks one found only
                    // in an unavailable command's explanation text.
                    let primary = exact
                        || entry.exact_keywords.iter().any(|word| {
                            pattern
                                .score(Utf32Str::new(word, &mut buffer), &mut matcher)
                                .is_some()
                        });
                    (exact || score.is_some()).then_some((
                        index,
                        (exact, primary, !entry.explanation),
                        score.unwrap_or(0),
                    ))
                })
                .collect::<Vec<_>>();
            scores.sort_by(|left, right| {
                right
                    .1
                    .cmp(&left.1)
                    .then_with(|| right.2.cmp(&left.2))
                    .then_with(|| left.0.cmp(&right.0))
            });
            self.visible = scores.into_iter().map(|(index, _, _)| index).collect();
        }
        self.selected = 0;
        self.list_top = 0;
        self.detail_top = 0;
    }
    fn activate(&mut self, index: usize) -> PaletteOutcome {
        if !self.visible.contains(&index) {
            return PaletteOutcome::Rejected(
                "Command is not in the current filtered catalogue".into(),
            );
        }
        let Some(action) = self
            .entries
            .get_mut(index)
            .and_then(|entry| entry.action.take())
        else {
            return PaletteOutcome::Ignored;
        };
        self.invalidate();
        PaletteOutcome::Chosen {
            original_index: index,
            action,
        }
    }
    pub fn dispatch(&mut self, action: PaletteAction) -> PaletteOutcome {
        use PaletteAction::*;
        if self.terminal {
            return PaletteOutcome::Ignored;
        }
        match action {
            Close => {
                self.invalidate();
                return PaletteOutcome::Closed;
            }
            Activate(index) => return self.activate(index),
            Choose => {
                return self
                    .selected_index()
                    .map_or(PaletteOutcome::Ignored, |index| self.activate(index))
            }
            IndexedRow(index) => {
                let Some(position) = self
                    .visible
                    .iter()
                    .position(|candidate| *candidate == index)
                else {
                    return PaletteOutcome::Rejected(
                        "Command is not in the current filtered catalogue".into(),
                    );
                };
                self.selected = position;
                self.detail_top = 0;
                self.focus = Focus::List;
            }
            Move(delta) | Page(delta) => {
                let delta = if matches!(action, Page(_)) {
                    delta.saturating_mul(self.list_size as i32)
                } else {
                    delta
                };
                self.selected = self
                    .selected
                    .saturating_add_signed(delta as isize)
                    .min(self.visible.len().saturating_sub(1));
                self.detail_top = 0;
                self.focus = Focus::List;
            }
            ListEdge(end) => {
                self.selected = if end {
                    self.visible.len().saturating_sub(1)
                } else {
                    0
                };
                self.detail_top = 0;
                self.focus = Focus::List;
            }
            FocusQuery => self.focus = Focus::Query,
            FocusList => self.focus = Focus::List,
            FocusDetails => self.focus = Focus::Details,
            FocusNext(backwards) => self.focus = self.focus.next(backwards),
            QueryClick(column) => {
                self.focus = Focus::Query;
                self.query.click(0, column, false);
            }
            Insert(text) | Paste(text) => {
                // Refusal is atomic: do not broaden scope or change caret/focus.
                let removed = self
                    .query
                    .selection()
                    .map_or(0, |selection| selection.len());
                if text.len() > MAX_QUERY_BYTES
                    || !safe_line(&text)
                    || self
                        .query
                        .text
                        .len()
                        .saturating_sub(removed)
                        .saturating_add(text.len())
                        > MAX_QUERY_BYTES
                {
                    self.message =
                        "Query refused: safe single line, at most 512 UTF-8 bytes / 128 graphemes."
                            .into();
                    return PaletteOutcome::Rejected(self.message.clone());
                }
                let mut candidate = self.query.clone();
                if candidate.insert(&text).is_err()
                    || candidate.text.graphemes(true).count() > MAX_QUERY_GRAPHEMES
                {
                    self.message = "Query refused: at most 128 graphemes.".into();
                    return PaletteOutcome::Rejected(self.message.clone());
                }
                self.query = candidate;
                self.focus = Focus::Query;
                self.filter();
                self.message =
                    "Filtered prepared commands; current workflow guards still apply.".into();
            }
            Backspace | Delete | ClearQuery => {
                match action {
                    Backspace => self.query.delete_backward(),
                    Delete => self.query.delete_forward(),
                    _ => self.query.clear(),
                }
                self.focus = Focus::Query;
                self.filter();
            }
            SelectAll => {
                self.focus = Focus::Query;
                self.query.select_all();
            }
            Cursor { right, select } => {
                self.focus = Focus::Query;
                self.query.horizontal(right, select);
            }
            QueryEdge { end, select } => {
                self.focus = Focus::Query;
                self.query.line_edge(end, true, select);
            }
            DetailScroll(delta) | DetailPage(delta) => {
                let delta = if matches!(action, DetailPage(_)) {
                    delta.saturating_mul(self.detail_size as i32)
                } else {
                    delta
                };
                self.focus = Focus::Details;
                self.detail_top = self
                    .detail_top
                    .saturating_add_signed(delta as isize)
                    .min(self.detail().len().saturating_sub(self.detail_size));
            }
            DetailEdge(end) => {
                self.focus = Focus::Details;
                self.detail_top = if end {
                    self.detail().len().saturating_sub(self.detail_size)
                } else {
                    0
                };
            }
        }
        self.keep_visible();
        self.clear_layout();
        PaletteOutcome::Changed
    }
    pub fn key(&self, code: KeyCode, modifiers: KeyModifiers) -> Option<PaletteAction> {
        use PaletteAction::*;
        if self.terminal {
            return None;
        }
        let shift = modifiers.contains(KeyModifiers::SHIFT);
        if code == KeyCode::Esc {
            return Some(Close);
        }
        if modifiers.contains(KeyModifiers::ALT) || modifiers.contains(KeyModifiers::SUPER) {
            return None;
        }
        if modifiers.contains(KeyModifiers::CONTROL) {
            return match code {
                KeyCode::Char('a') => Some(SelectAll),
                KeyCode::Char('u') => Some(ClearQuery),
                KeyCode::Char('c') => Some(Close),
                _ => None,
            };
        }
        match code {
            KeyCode::Tab => Some(FocusNext(shift)),
            KeyCode::BackTab => Some(FocusNext(true)),
            KeyCode::Enter => Some(if self.focus == Focus::Close {
                Close
            } else {
                Choose
            }),
            KeyCode::Up | KeyCode::Down => {
                let delta = if code == KeyCode::Up { -1 } else { 1 };
                Some(if self.focus == Focus::Details {
                    DetailScroll(delta)
                } else {
                    Move(delta)
                })
            }
            KeyCode::PageUp | KeyCode::PageDown => {
                let delta = if code == KeyCode::PageUp { -1 } else { 1 };
                Some(if self.focus == Focus::Details {
                    DetailPage(delta)
                } else {
                    Page(delta)
                })
            }
            KeyCode::Home | KeyCode::End => {
                let end = code == KeyCode::End;
                Some(match self.focus {
                    Focus::Query => QueryEdge { end, select: shift },
                    Focus::Details => DetailEdge(end),
                    _ => ListEdge(end),
                })
            }
            KeyCode::Left | KeyCode::Right => Some(Cursor {
                right: code == KeyCode::Right,
                select: shift,
            }),
            KeyCode::Backspace => Some(Backspace),
            KeyCode::Delete => Some(Delete),
            KeyCode::Char('j') | KeyCode::Char('k') if self.focus == Focus::List => {
                Some(Move(if code == KeyCode::Char('j') { 1 } else { -1 }))
            }
            KeyCode::Char(character) => Some(Insert(character.to_string())),
            _ => None,
        }
    }
    pub fn mouse(&self, kind: MouseEventKind, column: u16, row: u16) -> Option<PaletteAction> {
        if self.terminal {
            return None;
        }
        let point = Position::new(column, row);
        match kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let delta = if kind == MouseEventKind::ScrollUp {
                    -1
                } else {
                    1
                };
                if self.list_area.contains(point) {
                    Some(PaletteAction::Move(delta))
                } else if self.detail_area.contains(point) {
                    Some(PaletteAction::DetailScroll(delta))
                } else {
                    None
                }
            }
            MouseEventKind::Down(MouseButton::Left) if self.query_area.contains(point) => Some(
                PaletteAction::QueryClick(usize::from(column - self.query_area.x)),
            ),
            MouseEventKind::Down(MouseButton::Left) => self
                .hits
                .iter()
                .rev()
                .find(|hit| hit.rect.contains(point))
                .map(|hit| hit.action.clone()),
            _ => None,
        }
    }
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) -> Vec<PaletteHit> {
        self.clear_layout();
        if self.terminal || area.is_empty() {
            return vec![];
        }
        frame.render_widget(Clear, area);
        let block = crate::ui_chrome::overlay_block(theme, "Commands · current context");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let mut hits = vec![];
        // Geometry is prepared at the input boundary; never rewrap in draw.
        if inner.height < 10 || inner.width < 20 {
            if !inner.is_empty() {
                frame.render_widget(
                    Paragraph::new("Resize to find commands. Escape closes."),
                    inner,
                );
                let rect = Rect::new(inner.x, inner.bottom() - 1, inner.width.min(11), 1);
                frame.render_widget(
                    Paragraph::new("[Esc Close]").style(theme.style(SemanticToken::Focus)),
                    rect,
                );
                hits.push(PaletteHit {
                    rect,
                    action: PaletteAction::Close,
                });
            }
            self.hits = hits.clone();
            return hits;
        }
        let line = |text: &str| display_line(text, 0, usize::from(inner.width));
        frame.render_widget(
            Paragraph::new(line(
                "Prepared commands · not current permission/target authority",
            ))
            .style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        let label = if self.focus == Focus::Query {
            "Filter focused: "
        } else {
            "Filter: "
        };
        frame.render_widget(
            Paragraph::new(label),
            Rect::new(inner.x, inner.y + 1, label.len() as u16, 1),
        );
        hits.push(PaletteHit {
            rect: Rect::new(inner.x, inner.y + 1, label.len() as u16, 1),
            action: PaletteAction::FocusQuery,
        });
        self.query_area = Rect::new(
            inner.x + label.len() as u16,
            inner.y + 1,
            inner.width - label.len() as u16,
            1,
        );
        self.query
            .ensure_visible(usize::from(self.query_area.width), 1);
        // A wide glyph cannot be half a visible caret cell. Align the actual
        // shared editor viewport to the first whole displayed grapheme.
        let mut cells = 0usize;
        for grapheme in self.query.text.graphemes(true) {
            if cells >= self.query.left_column {
                self.query.left_column = cells;
                break;
            }
            cells += grapheme_width(grapheme);
        }
        frame.render_widget(
            Paragraph::new(query_line(
                &self.query,
                usize::from(self.query_area.width),
                theme,
            )),
            self.query_area,
        );
        hits.push(PaletteHit {
            rect: self.query_area,
            action: PaletteAction::QueryClick(0),
        });
        if self.focus == Focus::Query {
            frame.set_cursor_position((
                self.query_area.x
                    + self
                        .query
                        .position()
                        .1
                        .saturating_sub(self.query.left_column)
                        .min(usize::from(self.query_area.width - 1)) as u16,
                self.query_area.y,
            ));
        }
        let caption = Rect::new(inner.x, inner.y + 2, inner.width, 1);
        frame.render_widget(
            Paragraph::new(format!(
                "{} matching · {} focused",
                self.visible.len(),
                self.focus.label()
            ))
            .style(theme.style(SemanticToken::Muted)),
            caption,
        );
        hits.push(PaletteHit {
            rect: caption,
            action: PaletteAction::FocusList,
        });
        let space = inner.height - 7;
        let list_height = (space.saturating_mul(2) / 3).clamp(1, 12);
        self.list_area = Rect::new(inner.x, inner.y + 3, inner.width, list_height);
        self.detail_area = Rect::new(
            inner.x,
            self.list_area.bottom() + 1,
            inner.width,
            space - list_height,
        );
        self.list_size = usize::from(list_height);
        self.detail_size = usize::from(self.detail_area.height).max(1);
        self.keep_visible();
        if self.visible.is_empty() {
            let text = if self.entries.is_empty() {
                "No commands supplied for this context."
            } else {
                "No matching commands; edit or clear Filter."
            };
            frame.render_widget(Paragraph::new(line(text)), self.list_area);
        }
        for (position, index) in self
            .visible
            .iter()
            .enumerate()
            .skip(self.list_top)
            .take(self.list_size)
        {
            let entry = &self.entries[*index];
            let rect = Rect::new(
                inner.x,
                self.list_area.y + (position - self.list_top) as u16,
                inner.width,
                1,
            );
            let suffix = if entry.explanation {
                " [why unavailable]"
            } else {
                ""
            };
            let role = if entry.explanation {
                SemanticToken::Muted
            } else {
                SemanticToken::Normal
            };
            let style = if position == self.selected {
                theme.selected(role)
            } else {
                theme.style(role)
            };
            frame.render_widget(
                Paragraph::new(line(&format!(
                    "{} {}{suffix}",
                    if position == self.selected { ">" } else { " " },
                    entry.label
                )))
                .style(style),
                rect,
            );
            let key_width = display_width(&entry.shortcut) as u16;
            if !entry.explanation
                && key_width > 0
                && usize::from(rect.width)
                    > display_width(&entry.label) + usize::from(key_width) + 6
            {
                // The key reads like a footer key: the view accent, right-aligned.
                let key_style = if position == self.selected {
                    style
                } else {
                    theme
                        .style(SemanticToken::Accent)
                        .add_modifier(ratatui::style::Modifier::BOLD)
                };
                frame.render_widget(
                    Paragraph::new(entry.shortcut.as_str()).style(key_style),
                    Rect::new(rect.right() - key_width - 1, rect.y, key_width, 1),
                );
            }
            hits.push(PaletteHit {
                rect,
                action: PaletteAction::Activate(*index),
            });
        }
        let detail_caption = Rect::new(inner.x, self.list_area.bottom(), inner.width, 1);
        frame.render_widget(
            Paragraph::new("Selected command · requirements / explanation")
                .style(theme.style(SemanticToken::Muted)),
            detail_caption,
        );
        hits.push(PaletteHit {
            rect: detail_caption,
            action: PaletteAction::FocusDetails,
        });
        for (offset, text) in self
            .detail()
            .iter()
            .skip(self.detail_top)
            .take(self.detail_size)
            .enumerate()
        {
            if inner.width < 64
                && self
                    .detail_current_v2
                    .as_ref()
                    .is_none_or(|(index, width, _)| {
                        Some(*index) != self.selected_index() || *width > inner.width
                    })
            {
                break;
            }
            frame.render_widget(
                Paragraph::new(line(text)).style(theme.style(SemanticToken::Normal)),
                Rect::new(inner.x, self.detail_area.y + offset as u16, inner.width, 1),
            );
        }
        if inner.width < 64
            && self
                .detail_current_v2
                .as_ref()
                .is_none_or(|(index, width, _)| {
                    Some(*index) != self.selected_index() || *width > inner.width
                })
        {
            frame.render_widget(
                Paragraph::new("Detail unavailable at this width; choose again or resize")
                    .wrap(ratatui::widgets::Wrap { trim: false }),
                self.detail_area,
            );
        }
        hits.push(PaletteHit {
            rect: self.detail_area,
            action: PaletteAction::FocusDetails,
        });
        frame.render_widget(
            Paragraph::new(line(&self.message)).style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, inner.bottom() - 3, inner.width, 1),
        );
        let mut x = inner.x;
        for (label, action) in [
            (
                if inner.width < 64 {
                    "[Pg↑]"
                } else {
                    "[PgUp Rows]"
                },
                PaletteAction::Page(-1),
            ),
            (
                if inner.width < 64 {
                    "[Pg↓]"
                } else {
                    "[PgDn Rows]"
                },
                PaletteAction::Page(1),
            ),
            (
                if inner.width < 64 {
                    "[−]"
                } else {
                    "[− Detail]"
                },
                PaletteAction::DetailScroll(-1),
            ),
            (
                if inner.width < 64 {
                    "[+]"
                } else {
                    "[+ Detail]"
                },
                PaletteAction::DetailScroll(1),
            ),
            (
                if inner.width < 64 {
                    "[Tab]"
                } else {
                    "[Tab Focus]"
                },
                PaletteAction::FocusNext(false),
            ),
        ] {
            let width = display_width(label) as u16;
            if width > inner.right().saturating_sub(x) {
                break;
            }
            let rect = Rect::new(x, inner.bottom() - 2, width, 1);
            frame.render_widget(
                Paragraph::new(label).style(theme.style(SemanticToken::Focus)),
                rect,
            );
            hits.push(PaletteHit { rect, action });
            x += width + 1;
        }
        for (label, rect, action, focus) in [
            (
                "[Enter Choose]",
                Rect::new(inner.x, inner.bottom() - 1, 14, 1),
                PaletteAction::Choose,
                Focus::Choose,
            ),
            (
                "[Esc Close]",
                Rect::new(inner.right() - 11, inner.bottom() - 1, 11, 1),
                PaletteAction::Close,
                Focus::Close,
            ),
        ] {
            let style = if self.focus == focus {
                theme.selected(SemanticToken::Focus)
            } else {
                theme.style(SemanticToken::Focus)
            };
            frame.render_widget(Paragraph::new(label).style(style), rect);
            hits.push(PaletteHit { rect, action });
        }
        self.hits = hits.clone();
        hits
    }
}
fn safe_line(text: &str) -> bool {
    !text.chars().any(char::is_control) && sanitize_terminal(text) == text
}
fn query_line(query: &TextBuffer, width: usize, theme: Theme) -> Line<'static> {
    let mut spans = vec![];
    let mut consumed = 0usize;
    let mut shown = 0usize;
    let selection = query.selection();
    for (offset, grapheme) in query.text.grapheme_indices(true) {
        let cells = grapheme_width(grapheme);
        if consumed >= query.left_column {
            if shown + cells > width {
                break;
            }
            let selected = selection
                .as_ref()
                .is_some_and(|range| offset < range.end && offset + grapheme.len() > range.start);
            spans.push(Span::styled(
                grapheme.to_owned(),
                if selected {
                    theme.selected(SemanticToken::Selection)
                } else {
                    theme.style(SemanticToken::Normal)
                },
            ));
            shown += cells;
        }
        consumed += cells;
    }
    Line::from(spans)
}
fn prepare_lines(text: &str) -> Result<Vec<String>, String> {
    let mut output = vec![];
    for raw_line in text.lines() {
        for row in wrap_words(raw_line, 64) {
            output.push(row);
            if output.len() > MAX_DETAIL_ROWS {
                return Err("Command detail row budget exceeded".into());
            }
        }
    }
    Ok(output)
}

/// Splits one line into rows of at most `width` cells and 256 bytes, breaking after a space
/// when the row has one so words stay whole. Spaces stay at the end of their row, so the rows
/// concatenate back to the exact original text.
fn wrap_words(line: &str, width: usize) -> Vec<String> {
    let mut rows = vec![];
    let mut current = String::new();
    let mut cells = 0usize;
    for grapheme in line.graphemes(true) {
        let next = grapheme_width(grapheme);
        if !current.is_empty() && (cells + next > width || current.len() + grapheme.len() > 256) {
            let carry = if grapheme == " " {
                String::new()
            } else {
                match current.rfind(' ') {
                    Some(at) if at + 1 < current.len() => current.split_off(at + 1),
                    _ => String::new(),
                }
            };
            rows.push(std::mem::take(&mut current));
            cells = display_width(&carry);
            current = carry;
        }
        current.push_str(grapheme);
        cells += next;
    }
    rows.push(current);
    rows
}

#[cfg(test)]
#[path = "command_palette_tests.rs"]
mod tests;
