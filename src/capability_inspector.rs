//! Read-only presentation of the recorded transport inventory. Deliberately
//! undeclared until the shared application integration and PTY gates run.
//!
//! Neither static inventory flags nor a previously observed caller are current
//! permission evidence. This component cannot produce a publication action.
use crate::{
    editor::{display_line, display_width, TextBuffer},
    identity::ActorIdentity,
    providers::{
        gh::{capability_inventory, Actor, Capability, GhCliVersion, HostIdentity},
        runner::sanitize_terminal,
    },
    theme::{SemanticToken, Theme},
};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use unicode_segmentation::UnicodeSegmentation;
#[path = "capability_cli_syntax.rs"]
mod cli_syntax;

const MAX_QUERY_BYTES: usize = 512;
const MAX_QUERY_CHARACTERS: usize = 128;
const MAX_FIELD_BYTES: usize = 8192;
const MAX_ROWS: usize = 128;
const MAX_PREPARED_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InspectorAction {
    FilterClick(usize),
    Insert(String),
    Backspace,
    Delete,
    SelectAll,
    ClearFilter,
    Cursor { right: bool, select: bool },
    Edge { end: bool, select: bool },
    FocusNext(bool),
    FocusDetails,
    Row(usize),
    Move(i32),
    Page(i32),
    DetailScroll(i32),
    Close,
}
#[derive(Clone, Debug)]
pub struct InspectorHit {
    pub rect: Rect,
    pub action: InspectorAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Filter,
    List,
    Details,
    Close,
}
impl Focus {
    fn next(self, backwards: bool) -> Self {
        let all = [Self::Filter, Self::List, Self::Details, Self::Close];
        let i = all
            .iter()
            .position(|f| *f == self)
            .expect("closed focus set");
        all[(i + if backwards { 3 } else { 1 }) % all.len()]
    }
}
#[derive(Clone, Debug)]
struct Row {
    id: String,
    label: String,
    searchable: String,
    details: Vec<String>,
}
/// Public, bounded inventory text only; no repository content, cache, token,
/// callable operation, permission grant or serializable approval is retained.
#[derive(Clone, Debug)]
pub struct CapabilityInspector {
    rows: Vec<Row>,
    visible: Vec<usize>,
    selected: usize,
    list_top: usize,
    detail_top: usize,
    query: TextBuffer,
    focus: Focus,
    header: String,
    message: String,
    list_area: Rect,
    detail_area: Rect,
    query_area: Rect,
    list_size: usize,
    detail_size: usize,
}
impl CapabilityInspector {
    /// `identity` describes the last observed principal, not a fresh authority
    /// boundary. `setup_reason` is an optional already prepared diagnostic.
    /// This constructor does not spawn processes, probe hosts or read storage.
    pub fn new(
        identity: Option<&HostIdentity>,
        setup_reason: Option<&str>,
    ) -> Result<Self, String> {
        Self::prepare(capability_inventory(), identity, setup_reason)
    }
    fn prepare(
        inventory: Vec<Capability>,
        identity: Option<&HostIdentity>,
        setup_reason: Option<&str>,
    ) -> Result<Self, String> {
        if inventory.len() > MAX_ROWS {
            return Err("Capability inventory row budget exceeded".into());
        }
        let header = if let Some(identity) =
            identity.filter(|i| i.account_id.is_some_and(|id| id > 0))
        {
            let actor = ActorIdentity::github(
                &bounded(&identity.host)?,
                &Actor {
                    login: bounded(&identity.login)?,
                    ..Actor::default()
                },
            );
            format!(
                "Last observed: {} / [{}] {} / account #{} / {} (not permission authority)",
                bounded(&identity.host)?,
                actor.initials(),
                actor.label(),
                identity.account_id.expect("positive ID checked"),
                if identity.gh_version.is_empty() {
                    "gh version unverified".into()
                } else {
                    bounded(&identity.gh_version)?
                }
            )
        } else {
            "No verified numeric GitHub identity. Install gh and use gh auth login; local Git still works.".into()
        };
        let reason = setup_reason.map(bounded).transpose()?;
        let cli_version = identity
            .filter(|i| i.account_id.is_some_and(|id| id > 0))
            .and_then(|i| GhCliVersion::parse_line(&i.gh_version).ok());
        let availability = reason.map_or_else(
            || "Availability: unverified; current host, permission and exact target must be checked by the actual workflow.".into(),
            |reason| format!("Setup/availability diagnostic: {reason}. Local Git remains independent of GitHub authentication.")
        );
        let mut ids = std::collections::BTreeSet::new();
        let mut rows = Vec::with_capacity(inventory.len());
        let mut prepared_bytes = header.len();
        for capability in inventory {
            if capability.id.len() > 128
                || capability.id.is_empty()
                || !capability
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                return Err(
                    "Capability inventory has a malformed canonical action identity".into(),
                );
            }
            if !ids.insert(capability.id.clone()) {
                return Err(
                    "Capability inventory has an empty or duplicate action identity".into(),
                );
            }
            let id = bounded(&capability.id)?;
            let label = bounded(&capability.label)?;
            let task = bounded(&capability.task)?;
            let transport = bounded(&capability.transport)?;
            let permission = bounded(&capability.permission)?;
            let guard = bounded(&capability.concurrency_guard)?;
            let reason = bounded(&capability.reason)?;
            let contract = capability.requirements_v1();
            let cli_details = cli_syntax::details(&id, cli_version.as_ref());
            let cli_search = cli_details.join(" ");
            if capability.source_urls.len() > 8 {
                return Err("Capability source budget exceeded".into());
            }
            let mut details = vec![
                format!("Action: {label} ({id})"),
                availability.clone(),
                format!("Implementation: {task}; static inventory records transport {}. This is not native parity or a live support claim.", if capability.implemented { "present" } else { "pending" }),
                format!("Transport: {transport}"),
                format!("Required permission class: {permission}; not observed/granted by this inspector."),
                format!("Target/concurrency requirement: {guard}"),
                "Minimum operation version: unverified. Observing a gh version does not verify every operation or Enterprise host.".into(),
                "Preview/confirmation: handled by the exact native workflow; this inspector performs no GitHub or Git operation.".into(),
                "Paging/fixture/live evidence: task-specific verification is required; a static inventory flag or browser handoff cannot close coverage.".into(),
                format!("Inventory note (may precede newer task evidence): {reason}"),
            ];
            let mut contract_search = String::new();
            if let Some(contract) = contract {
                let rows = contract
                    .rows
                    .iter()
                    .map(|row| format!("{}: {} ({})", row.document(), row.label(), row.key()))
                    .collect::<Vec<_>>()
                    .join("; ");
                let tasks = contract
                    .tasks
                    .iter()
                    .map(|task| format!("TASK-{task}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                contract_search = format!("{} {}", contract.operations, rows);
                details.extend([
                    format!("Contract scope: {rows}"),
                    format!("Required native operations: {}", contract.operations),
                    format!("Effect class: {}", contract.effect().label()),
                    format!("Required confirmation: {}", contract.preview_requirement()),
                    format!("Required target guard: {}", contract.target_requirement()),
                    format!("Paging contract: {}", contract.paging_requirement()),
                    format!("Verification tasks: {tasks}; no task is marked complete by this inventory."),
                    format!("Host limit: {}", contract.host_limit()),
                    format!("Evidence limit: {}", contract.evidence_limit()),
                ]);
            } else {
                details.push("Contract scope: unmapped category; no operation, host floor or permission requirement is guessed.".into());
            }
            details.extend(cli_details);
            for url in capability.source_urls {
                details.push(format!("Reference: {}", bounded(&url)?));
            }
            prepared_bytes = details
                .iter()
                .fold(prepared_bytes.saturating_add(label.len()), |bytes, text| {
                    bytes.saturating_add(text.len())
                });
            if prepared_bytes > MAX_PREPARED_BYTES {
                return Err("Capability prepared text budget exceeded".into());
            }
            // Prepare bounded display rows once. Drawing never parses a DTO or
            // asks a provider, and scrolling does not rewrap unlimited content.
            let details = details
                .iter()
                .flat_map(|line| prepared_lines(line, 64))
                .collect();
            let searchable = format!(
                "{id} {label} {task} {transport} {permission} {cli_search} {contract_search}"
            )
            .to_lowercase();
            rows.push(Row {
                id,
                label,
                searchable,
                details,
            });
        }
        let visible = (0..rows.len()).collect();
        Ok(Self {
            rows,
            visible,
            selected: 0,
            list_top: 0,
            detail_top: 0,
            query: TextBuffer::default(),
            focus: Focus::Filter,
            header,
            message:
                "Read-only inventory; choosing a row shows requirements, never runs an action."
                    .into(),
            list_area: Rect::default(),
            detail_area: Rect::default(),
            query_area: Rect::default(),
            list_size: 1,
            detail_size: 1,
        })
    }
    pub fn selected_id(&self) -> Option<&str> {
        self.visible
            .get(self.selected)
            .and_then(|i| self.rows.get(*i))
            .map(|r| r.id.as_str())
    }
    pub fn query(&self) -> &str {
        &self.query.text
    }
    pub fn details(&self) -> &[String] {
        self.visible
            .get(self.selected)
            .and_then(|i| self.rows.get(*i))
            .map_or(&[], |r| r.details.as_slice())
    }
    fn keep_visible(&mut self) {
        self.selected = self.selected.min(self.visible.len().saturating_sub(1));
        self.list_top = self.list_top.min(self.selected);
        if self.selected >= self.list_top.saturating_add(self.list_size) {
            self.list_top = self.selected + 1 - self.list_size;
        }
        self.list_top = self
            .list_top
            .min(self.visible.len().saturating_sub(self.list_size));
        self.detail_top = self
            .detail_top
            .min(self.details().len().saturating_sub(self.detail_size));
    }
    fn filter(&mut self) {
        let terms = self.query.text.to_lowercase();
        self.visible = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                terms
                    .split_whitespace()
                    .all(|term| row.searchable.contains(term))
            })
            .map(|(i, _)| i)
            .collect();
        self.selected = 0;
        self.list_top = 0;
        self.detail_top = 0;
    }
    /// True means close this read-only overlay. There is deliberately no
    /// publication/launch outcome, even for a transport-present inventory row.
    pub fn dispatch(&mut self, action: InspectorAction) -> bool {
        use InspectorAction::*;
        match action {
            Close => return true,
            FocusNext(backwards) => self.focus = self.focus.next(backwards),
            FocusDetails => self.focus = Focus::Details,
            FilterClick(column) => {
                self.focus = Focus::Filter;
                self.query.click(0, column, false);
            }
            Insert(text) => {
                self.focus = Focus::Filter;
                let replaced = self.query.selection().map_or(0, |range| range.len());
                if text.len() > MAX_QUERY_BYTES
                    || self
                        .query
                        .text
                        .len()
                        .saturating_sub(replaced)
                        .saturating_add(text.len())
                        > MAX_QUERY_BYTES
                    || text.chars().any(char::is_control)
                    || sanitize_terminal(&text) != text
                {
                    self.message = "Filter refused: one safe line, at most 512 UTF-8 bytes and 128 characters.".into();
                } else {
                    let mut next = self.query.clone();
                    if next.insert(&text).is_ok()
                        && next.text.chars().count() <= MAX_QUERY_CHARACTERS
                    {
                        self.query = next;
                        self.filter();
                        self.message = "Filter changed; inventory remains read only.".into();
                    } else {
                        self.message = "Filter refused: at most 128 characters.".into();
                    }
                }
            }
            Backspace | Delete | ClearFilter => {
                self.focus = Focus::Filter;
                match action {
                    Backspace => self.query.delete_backward(),
                    Delete => self.query.delete_forward(),
                    _ => self.query.clear(),
                }
                self.filter();
            }
            Cursor { right, select } => {
                self.focus = Focus::Filter;
                self.query.horizontal(right, select);
            }
            Edge { end, select } => {
                self.focus = Focus::Filter;
                self.query.line_edge(end, true, select);
            }
            SelectAll => {
                self.focus = Focus::Filter;
                self.query.select_all();
            }
            Row(index) => {
                if let Some(position) = self.visible.iter().position(|i| *i == index) {
                    self.selected = position;
                    self.detail_top = 0;
                    self.focus = Focus::List;
                }
            }
            Move(delta) => {
                self.selected = self
                    .selected
                    .saturating_add_signed(delta as isize)
                    .min(self.visible.len().saturating_sub(1));
                self.detail_top = 0;
                self.focus = Focus::List;
            }
            Page(delta) => {
                self.selected = self
                    .selected
                    .saturating_add_signed(delta.saturating_mul(self.list_size as i32) as isize)
                    .min(self.visible.len().saturating_sub(1));
                self.detail_top = 0;
                self.focus = Focus::List;
            }
            DetailScroll(delta) => {
                self.focus = Focus::Details;
                self.detail_top = self
                    .detail_top
                    .saturating_add_signed(delta as isize)
                    .min(self.details().len().saturating_sub(self.detail_size));
            }
        }
        self.keep_visible();
        false
    }
    pub fn key(&self, code: KeyCode, modifiers: KeyModifiers) -> Option<InspectorAction> {
        use InspectorAction::*;
        let shift = modifiers.contains(KeyModifiers::SHIFT);
        let control = modifiers.contains(KeyModifiers::CONTROL);
        if modifiers.contains(KeyModifiers::ALT) {
            return None;
        }
        match code {
            KeyCode::Esc => Some(Close),
            KeyCode::Tab => Some(FocusNext(false)),
            KeyCode::BackTab => Some(FocusNext(true)),
            KeyCode::Enter if self.focus == Focus::Close => Some(Close),
            KeyCode::Enter if self.focus == Focus::Filter => Some(FocusNext(false)),
            KeyCode::Enter if self.focus == Focus::List => Some(FocusDetails),
            KeyCode::Char('a') if control && self.focus == Focus::Filter => Some(SelectAll),
            KeyCode::Char('u') if control && self.focus == Focus::Filter => Some(ClearFilter),
            KeyCode::Char(c) if !control && self.focus == Focus::Filter => {
                Some(Insert(c.to_string()))
            }
            KeyCode::Left if self.focus == Focus::Filter => Some(Cursor {
                right: false,
                select: shift,
            }),
            KeyCode::Right if self.focus == Focus::Filter => Some(Cursor {
                right: true,
                select: shift,
            }),
            KeyCode::Home if self.focus == Focus::Filter => Some(Edge {
                end: false,
                select: shift,
            }),
            KeyCode::End if self.focus == Focus::Filter => Some(Edge {
                end: true,
                select: shift,
            }),
            KeyCode::Backspace if self.focus == Focus::Filter => Some(Backspace),
            KeyCode::Delete if self.focus == Focus::Filter => Some(Delete),
            KeyCode::Down | KeyCode::Char('j') if self.focus == Focus::Details => {
                Some(DetailScroll(1))
            }
            KeyCode::Up | KeyCode::Char('k') if self.focus == Focus::Details => {
                Some(DetailScroll(-1))
            }
            KeyCode::PageDown if self.focus == Focus::Details => {
                Some(DetailScroll(self.detail_size as i32))
            }
            KeyCode::PageUp if self.focus == Focus::Details => {
                Some(DetailScroll(-(self.detail_size as i32)))
            }
            KeyCode::Down | KeyCode::Char('j') => Some(Move(1)),
            KeyCode::Up | KeyCode::Char('k') => Some(Move(-1)),
            KeyCode::PageDown => Some(Page(1)),
            KeyCode::PageUp => Some(Page(-1)),
            _ => None,
        }
    }
    pub fn mouse(
        &self,
        kind: MouseEventKind,
        column: u16,
        row: u16,
        hits: &[InspectorHit],
    ) -> Option<InspectorAction> {
        let position = Position::new(column, row);
        let delta = match kind {
            MouseEventKind::ScrollDown => Some(1),
            MouseEventKind::ScrollUp => Some(-1),
            _ => None,
        };
        if let Some(delta) = delta {
            return if self.list_area.contains(position) {
                Some(InspectorAction::Move(delta))
            } else if self.detail_area.contains(position) {
                Some(InspectorAction::DetailScroll(delta))
            } else {
                None
            };
        }
        if kind == MouseEventKind::Down(MouseButton::Left) {
            if self.query_area.contains(position) {
                Some(InspectorAction::FilterClick(usize::from(
                    column - self.query_area.x,
                )))
            } else {
                hits.iter()
                    .rev()
                    .find(|hit| hit.rect.contains(position))
                    .map(|hit| hit.action.clone())
            }
        } else {
            None
        }
    }
    /// Clear obsolete geometry immediately on resize, before another mouse
    /// report can address a rectangle from the previous terminal dimensions.
    pub fn clear_layout(&mut self) {
        self.list_area = Rect::default();
        self.detail_area = Rect::default();
        self.query_area = Rect::default();
    }
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) -> Vec<InspectorHit> {
        self.clear_layout();
        let mut hits = vec![];
        if area.is_empty() {
            return hits;
        }
        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Capabilities • read-only inventory")
            .style(theme.base());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.height < 7 || inner.width < 24 {
            if !inner.is_empty() {
                frame.render_widget(Paragraph::new("Resize to inspect. Escape closes."), inner);
                let rect = Rect::new(inner.x, inner.bottom() - 1, inner.width.min(11), 1);
                frame.render_widget(
                    Paragraph::new("[Esc Close]").style(theme.style(SemanticToken::Focus)),
                    rect,
                );
                hits.push(InspectorHit {
                    rect,
                    action: InspectorAction::Close,
                });
            }
            return hits;
        }
        let clipped = |s: &str| display_line(s, 0, usize::from(inner.width));
        frame.render_widget(
            Paragraph::new(clipped(&self.header)).style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        let query_y = inner.y + 1;
        let label = if self.focus == Focus::Filter {
            "Filter focused: "
        } else {
            "Filter: "
        };
        let label_width = label.len() as u16;
        frame.render_widget(
            Paragraph::new(label),
            Rect::new(inner.x, query_y, label_width, 1),
        );
        self.query_area = Rect::new(inner.x + label_width, query_y, inner.width - label_width, 1);
        self.query
            .ensure_visible(usize::from(self.query_area.width), 1);
        frame.render_widget(
            Paragraph::new(display_line(
                &self.query.text,
                self.query.left_column,
                usize::from(self.query_area.width),
            ))
            .style(theme.style(SemanticToken::Focus)),
            self.query_area,
        );
        if self.focus == Focus::Filter {
            frame.set_cursor_position((
                self.query_area.x
                    + self
                        .query
                        .position()
                        .1
                        .saturating_sub(self.query.left_column)
                        .min(usize::from(self.query_area.width.saturating_sub(1)))
                        as u16,
                query_y,
            ));
        }
        hits.push(InspectorHit {
            rect: self.query_area,
            action: InspectorAction::FilterClick(0),
        });
        let space = inner.height.saturating_sub(5);
        let list_height = (space / 2).clamp(1, 8);
        self.list_area = Rect::new(inner.x, inner.y + 2, inner.width, list_height);
        self.detail_area = Rect::new(
            inner.x,
            self.list_area.bottom() + 1,
            inner.width,
            space.saturating_sub(list_height),
        );
        self.list_size = usize::from(list_height).max(1);
        self.detail_size = usize::from(self.detail_area.height).max(1);
        self.keep_visible();
        if self.visible.is_empty() {
            frame.render_widget(
                Paragraph::new("No matching inventory rows; clear or edit Filter."),
                self.list_area,
            );
        }
        for (position, index) in self
            .visible
            .iter()
            .enumerate()
            .skip(self.list_top)
            .take(self.list_size)
        {
            let rect = Rect::new(
                self.list_area.x,
                self.list_area.y + (position - self.list_top) as u16,
                self.list_area.width,
                1,
            );
            frame.render_widget(
                Paragraph::new(clipped(&format!(
                    "{} {}",
                    if position == self.selected { ">" } else { " " },
                    self.rows[*index].label
                )))
                .style(if position == self.selected {
                    theme.selected(SemanticToken::Muted)
                } else {
                    theme.style(SemanticToken::Normal)
                }),
                rect,
            );
            hits.push(InspectorHit {
                rect,
                action: InspectorAction::Row(*index),
            });
        }
        frame.render_widget(
            Paragraph::new(format!(
                "Requirements • {} matching • {} focused",
                self.visible.len(),
                if self.focus == Focus::Details {
                    "details"
                } else {
                    "rows/filter"
                }
            ))
            .style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, self.list_area.bottom(), inner.width, 1),
        );
        let detail = self
            .details()
            .iter()
            .skip(self.detail_top)
            .take(self.detail_size)
            .map(|row| clipped(row))
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(detail.join("\n")).style(theme.style(SemanticToken::Normal)),
            self.detail_area,
        );
        if !self.detail_area.is_empty() {
            hits.push(InspectorHit {
                rect: self.detail_area,
                action: InspectorAction::FocusDetails,
            });
        }
        frame.render_widget(
            Paragraph::new(clipped(&self.message)).style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
        );
        let footer = [
            ("[PgUp Rows]", InspectorAction::Page(-1)),
            ("[PgDn Rows]", InspectorAction::Page(1)),
            ("[− Detail]", InspectorAction::DetailScroll(-1)),
            ("[+ Detail]", InspectorAction::DetailScroll(1)),
        ];
        let close = Rect::new(inner.right() - 11, inner.bottom() - 1, 11, 1);
        let mut x = inner.x;
        for (label, action) in footer {
            let width = crate::editor::display_width(label) as u16;
            if x + width >= close.x {
                break;
            }
            let rect = Rect::new(x, inner.bottom() - 1, width, 1);
            frame.render_widget(
                Paragraph::new(label).style(theme.style(SemanticToken::Focus)),
                rect,
            );
            hits.push(InspectorHit { rect, action });
            x += width + 1;
        }
        let style = if self.focus == Focus::Close {
            theme.selected(SemanticToken::Focus)
        } else {
            theme.style(SemanticToken::Focus)
        };
        frame.render_widget(Paragraph::new("[Esc Close]").style(style), close);
        hits.push(InspectorHit {
            rect: close,
            action: InspectorAction::Close,
        });
        hits
    }
}
fn bounded(value: &str) -> Result<String, String> {
    if value.len() > MAX_FIELD_BYTES {
        return Err("Capability inventory text budget exceeded".into());
    }
    Ok(sanitize_terminal(value).replace(['\n', '\r', '\t'], " "))
}

/// Keep ordinary words intact; only identifiers longer than one prepared row
/// split at a grapheme boundary. Concatenating the prepared rows restores the
/// exact safe display text, including spaces, rather than inventing characters.
fn prepared_lines(value: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut current = String::new();
    let mut cells = 0;
    for word in value.split_inclusive(char::is_whitespace) {
        let count = display_width(word);
        if count <= width {
            if cells + count > width && !current.is_empty() {
                rows.push(std::mem::take(&mut current));
                cells = 0;
            }
            current.push_str(word);
            cells += count;
        } else {
            for grapheme in word.graphemes(true) {
                let count = crate::editor::grapheme_width(grapheme);
                if cells + count > width && !current.is_empty() {
                    rows.push(std::mem::take(&mut current));
                    cells = 0;
                }
                current.push_str(grapheme);
                cells += count;
            }
        }
    }
    if !current.is_empty() {
        rows.push(current);
    }
    rows
}

#[cfg(test)]
#[path = "capability_inspector_tests.rs"]
mod tests;
