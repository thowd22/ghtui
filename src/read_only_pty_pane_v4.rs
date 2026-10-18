//! Generic host-owned read-only PTY presentation. No canonical Git coordinates,
//! source factory, transport, staging or review publication exist in this pane.
use crate::{
    editor::{display_line, TextBuffer},
    providers::owned_pty_viewer_v4::{
        PtyViewerHandleV4, ViewerCommandV4, ViewerGeometryV4, ViewerSnapshotV4,
    },
    theme::{ColorMode, SemanticToken, Theme},
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::{sync::Arc, time::Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PtyPaneActionV4 {
    Return,
    Cancel,
    Command(ViewerCommandV4),
    FocusQuery,
    Find,
}
#[derive(Clone)]
struct Hit {
    rect: Rect,
    action: PtyPaneActionV4,
}
struct Layout {
    body: Rect,
    query: Rect,
    hits: Vec<Hit>,
    rows: u16,
    usable: bool,
}
const NAV: [(&str, ViewerCommandV4); 8] = [
    ("[k Up]", ViewerCommandV4::Line(-1)),
    ("[j Down]", ViewerCommandV4::Line(1)),
    ("[b Page↑]", ViewerCommandV4::Page(-1)),
    ("[Space Page↓]", ViewerCommandV4::Page(1)),
    ("[g First]", ViewerCommandV4::First),
    ("[G Last]", ViewerCommandV4::Last),
    ("[n Next]", ViewerCommandV4::Match(1)),
    ("[N Prev]", ViewerCommandV4::Match(-1)),
];

pub(crate) struct ReadOnlyPtyPaneV4 {
    handle: PtyViewerHandleV4,
    owner: Arc<()>,
    geometry: ViewerGeometryV4,
    frame: Option<Arc<ViewerSnapshotV4>>,
    layout: Layout,
    area: Rect,
    epoch: u64,
    source: Arc<str>,
    scope: Arc<str>,
    query: TextBuffer,
    query_focused: bool,
    focus: Option<usize>,
    pub(crate) status: String,
    stopping: bool,
    pub(crate) returned: bool,
    pub(crate) cleanup_proven: Option<bool>,
}
impl ReadOnlyPtyPaneV4 {
    pub(crate) fn initialize_area_v4(&mut self, area: Rect) {
        self.area = area;
        self.layout = layout(area);
    }
    pub(crate) fn new(
        handle: PtyViewerHandleV4,
        geometry: ViewerGeometryV4,
        source: Arc<str>,
        scope: Arc<str>,
    ) -> Self {
        let owner = handle.owner().clone();
        Self {
            handle,
            owner,
            geometry,
            frame: None,
            layout: layout(Rect::default()),
            area: Rect::default(),
            epoch: geometry.epoch(),
            source: safe_header(source),
            scope: safe_header(scope),
            query: TextBuffer::default(),
            query_focused: false,
            focus: None,
            status: "Opening optional read-only pager on a separate owned PTY".into(),
            stopping: false,
            returned: false,
            cleanup_proven: None,
        }
    }
    pub(crate) fn geometry(area: Rect, epoch: u64) -> Option<ViewerGeometryV4> {
        let layout = layout(area);
        if !layout.usable {
            return None;
        }
        ViewerGeometryV4::new(
            layout.body.width.min(240),
            layout.body.height.min(60),
            epoch,
        )
        .ok()
    }
    /// Must be called at resize INPUT before polling/admitting any old packet.
    pub(crate) fn resized(&mut self, area: Rect) {
        if self.area == area {
            return;
        }
        self.area = area;
        self.epoch = self.epoch.saturating_add(1);
        self.layout = layout(area);
        self.frame = None;
        self.focus = None;
        if let Some(geometry) = Self::geometry(area, self.epoch) {
            self.geometry = geometry;
            self.handle.resize(geometry);
        }
        self.status = "External child resize requested; waiting for genuine pager redraw".into();
    }
    pub(crate) fn retire(&mut self) {
        self.frame = None;
        self.layout.hits.clear();
        self.stopping = true;
        self.handle.cancel();
    }
    pub(crate) fn poll(&mut self) {
        if !self.stopping {
            if let Some(frame) = self.handle.snapshot() {
                if frame.belongs(&self.owner, self.geometry)
                    && Self::geometry(self.area, self.epoch).is_some()
                {
                    self.status = format!(
                        "External read-only PTY • {} received PTY bytes • {} charged cell bytes",
                        frame.output_bytes, frame.retained
                    );
                    self.frame = Some(frame);
                }
            }
        }
        if let Some(exit) = self.handle.exit() {
            self.frame = None;
            self.returned = true;
            self.cleanup_proven = Some(exit.cleanup_proven);
            self.status = if exit.cleanup_proven {
                format!(
                    "{} • exact owned child{} cleaned • native retained",
                    exit.reason, exit.leader
                )
            } else {
                "External cleanup NOT proven; relaunch disabled • native Return remains available"
                    .into()
            };
        }
    }
    pub(crate) fn dispatch(&mut self, action: PtyPaneActionV4) {
        match action {
            PtyPaneActionV4::Return | PtyPaneActionV4::Cancel => {
                self.retire();
                self.returned = true;
                self.status = "Returning to native; exact owned pager cleanup in progress".into();
            }
            PtyPaneActionV4::FocusQuery => {
                self.query_focused = true;
                self.focus = None;
            }
            PtyPaneActionV4::Find => {
                self.query_focused = false;
                self.send(ViewerCommandV4::Search(self.query.text.clone()));
            }
            PtyPaneActionV4::Command(command) => self.send(command),
        }
    }
    fn send(&mut self, command: ViewerCommandV4) {
        if self.stopping || Self::geometry(self.area, self.epoch).is_none() {
            self.status =
                "Resize to view/forward pager actions; Return and Cancel remain available".into();
            return;
        }
        self.query_focused = false;
        if let Err(error) = self.handle.command(command) {
            self.status = error.to_string();
        }
    }
    fn insert(&mut self, text: &str) {
        if text.len()>254||text.chars().any(|c|c.is_control()||matches!(c,'\u{061c}'|'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')){
            self.status="External search refuses control/newline/bidi or oversized text".into();return;
        }
        let replaced = self.query.selection().map_or(0, |r| r.len());
        if self.query.text.len() - replaced + text.len() > 254 {
            self.status = "External search limit254 UTF-8 bytes".into();
            return;
        }
        if let Err(error) = self.query.insert(text) {
            self.status = error;
        }
    }
    pub(crate) fn event(&mut self, event: &Event) -> bool {
        match event {
            Event::Resize(width, height) => {
                self.resized(Rect::new(self.area.x, self.area.y, *width, *height));
                true
            }
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                if key.code == KeyCode::Char('c') && ctrl {
                    self.dispatch(PtyPaneActionV4::Cancel);
                    return true;
                }
                if key.code == KeyCode::Esc {
                    self.dispatch(PtyPaneActionV4::Return);
                    return true;
                }
                if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
                    self.query_focused = false;
                    let count = self.layout.hits.len();
                    if count != 0 {
                        self.focus = Some(match self.focus {
                            None => {
                                if key.code == KeyCode::BackTab {
                                    count - 1
                                } else {
                                    0
                                }
                            }
                            Some(index) => {
                                if key.code == KeyCode::BackTab {
                                    (index + count - 1) % count
                                } else {
                                    (index + 1) % count
                                }
                            }
                        });
                    }
                    return true;
                }
                if self.query_focused {
                    match key.code {
                        KeyCode::Char('a') if ctrl => self.query.select_all(),
                        KeyCode::Char('u') if ctrl => self.query.clear(),
                        KeyCode::Char(ch)
                            if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) =>
                        {
                            self.insert(&ch.to_string())
                        }
                        KeyCode::Backspace => self.query.delete_backward(),
                        KeyCode::Delete => self.query.delete_forward(),
                        KeyCode::Left | KeyCode::Right => {
                            self.query.horizontal(key.code == KeyCode::Right, shift)
                        }
                        KeyCode::Home | KeyCode::End => {
                            self.query.line_edge(key.code == KeyCode::End, true, shift)
                        }
                        KeyCode::Enter => self.dispatch(PtyPaneActionV4::Find),
                        _ => {}
                    }
                    return true;
                }
                if key.code == KeyCode::Enter {
                    if let Some(action) = self
                        .focus
                        .and_then(|index| self.layout.hits.get(index))
                        .map(|hit| hit.action.clone())
                    {
                        self.dispatch(action);
                    }
                    return true;
                }
                let action = match key.code {
                    KeyCode::Char('q') => Some(PtyPaneActionV4::Return),
                    KeyCode::Char('/') => Some(PtyPaneActionV4::FocusQuery),
                    KeyCode::Up | KeyCode::Char('k') => {
                        Some(PtyPaneActionV4::Command(ViewerCommandV4::Line(-1)))
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        Some(PtyPaneActionV4::Command(ViewerCommandV4::Line(1)))
                    }
                    KeyCode::PageUp | KeyCode::Char('b') => {
                        Some(PtyPaneActionV4::Command(ViewerCommandV4::Page(-1)))
                    }
                    KeyCode::PageDown | KeyCode::Char(' ') => {
                        Some(PtyPaneActionV4::Command(ViewerCommandV4::Page(1)))
                    }
                    KeyCode::Home | KeyCode::Char('g') => {
                        Some(PtyPaneActionV4::Command(ViewerCommandV4::First))
                    }
                    KeyCode::End | KeyCode::Char('G') => {
                        Some(PtyPaneActionV4::Command(ViewerCommandV4::Last))
                    }
                    KeyCode::Char('n') => Some(PtyPaneActionV4::Command(ViewerCommandV4::Match(1))),
                    KeyCode::Char('N') => {
                        Some(PtyPaneActionV4::Command(ViewerCommandV4::Match(-1)))
                    }
                    _ => None,
                };
                if let Some(action) = action {
                    self.dispatch(action);
                }
                true
            }
            Event::Mouse(mouse) => {
                if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    if self.layout.query.contains((mouse.column, mouse.row).into()) {
                        self.query_focused = true;
                        self.focus = None;
                        self.query.click(
                            0,
                            usize::from(mouse.column - self.layout.query.x),
                            mouse.modifiers.contains(KeyModifiers::SHIFT),
                        );
                    } else if let Some(action) = self
                        .layout
                        .hits
                        .iter()
                        .find(|hit| hit.rect.contains((mouse.column, mouse.row).into()))
                        .map(|hit| hit.action.clone())
                    {
                        self.dispatch(action);
                    }
                } else if self.layout.body.contains((mouse.column, mouse.row).into()) {
                    match mouse.kind {
                        MouseEventKind::ScrollUp => {
                            self.dispatch(PtyPaneActionV4::Command(ViewerCommandV4::Line(-1)))
                        }
                        MouseEventKind::ScrollDown => {
                            self.dispatch(PtyPaneActionV4::Command(ViewerCommandV4::Line(1)))
                        }
                        _ => {}
                    }
                }
                true
            }
            Event::Paste(text) => {
                if self.query_focused {
                    self.insert(text);
                }
                true
            }
            _ => true, // Behind-pane actions never leak to native writes/Source.
        }
    }
    pub(crate) fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) {
        if area != self.area {
            self.resized(area);
        }
        self.poll();
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .title("Optional external pager • host-owned read-only PTY"),
            area,
        );
        let inner = area.inner(ratatui::layout::Margin::new(1, 1));
        if inner.is_empty() {
            return;
        }
        let rows = [self.scope.as_ref(), self.source.as_ref()];
        for (index, text) in rows.iter().enumerate() {
            if index < usize::from(inner.height) {
                frame.render_widget(
                    Paragraph::new(*text).style(theme.style(SemanticToken::Muted)),
                    Rect::new(inner.x, inner.y + index as u16, inner.width, 1),
                );
            }
        }
        for (index, hit) in self.layout.hits.iter().enumerate() {
            let label = label(&hit.action);
            let style = if self.focus == Some(index) {
                theme.style(SemanticToken::Focus)
            } else {
                theme.style(SemanticToken::Muted)
            };
            frame.render_widget(Paragraph::new(label).style(style), hit.rect);
        }
        let query = self.layout.query;
        if query.width > 0 {
            frame.render_widget(
                Paragraph::new("Pager search: ").style(theme.style(SemanticToken::Muted)),
                Rect::new(inner.x, query.y, query.x.saturating_sub(inner.x), 1),
            );
            self.query.ensure_visible(query.width as usize, 1);
            frame.render_widget(
                Paragraph::new(display_line(
                    &self.query.text,
                    self.query.left_column,
                    query.width as usize,
                ))
                .style(theme.style(SemanticToken::Normal)),
                query,
            );
            if self.query_focused {
                let (_, column) = self.query.position();
                frame.set_cursor_position((
                    query.x
                        + (column.saturating_sub(self.query.left_column) as u16)
                            .min(query.width.saturating_sub(1)),
                    query.y,
                ));
            }
        }
        let body = self.layout.body;
        if Self::geometry(area, self.epoch).is_none() {
            frame.render_widget(Paragraph::new("Resize to view the external pane. Return/Cancel stay host-owned; no child input is forwarded.").style(theme.style(SemanticToken::Conflict)),body);
            return;
        }
        if let Some(snapshot) = self
            .frame
            .as_ref()
            .filter(|snapshot| snapshot.belongs(&self.owner, self.geometry))
        {
            for row in 0..snapshot.geometry.rows().min(body.height) {
                for col in 0..snapshot.geometry.cols().min(body.width) {
                    let cell = &snapshot.cells[usize::from(row)
                        * usize::from(snapshot.geometry.cols())
                        + usize::from(col)];
                    if !cell.continuation {
                        let mut style = cell_style(cell.style, theme.mode);
                        if (row, col) == snapshot.cursor {
                            style = style.add_modifier(Modifier::UNDERLINED);
                        }
                        frame.buffer_mut().set_stringn(
                            body.x + col,
                            body.y + row,
                            &cell.text,
                            usize::from(body.width - col),
                            style,
                        );
                    }
                }
            }
        } else {
            frame.render_widget(
                Paragraph::new(if self.stopping {
                    "Stopping exact owned external session…"
                } else {
                    "Waiting for genuine external PTY cells…"
                }),
                body,
            );
        }
        let seconds = self
            .handle
            .deadline()
            .saturating_duration_since(Instant::now())
            .as_secs();
        if self.layout.rows < inner.height {
            frame.render_widget(
                Paragraph::new(format!("{seconds}s display-only • {}", self.status))
                    .style(theme.style(SemanticToken::Muted)),
                Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
            );
        }
    }
}

fn cell_style(style: Style, mode: ColorMode) -> Style {
    match mode {
        ColorMode::NoColor => style.fg(Color::Reset).bg(Color::Reset),
        ColorMode::TrueColor => style,
        ColorMode::Indexed256 | ColorMode::LowColor => style
            .fg(crate::theme::downsample(
                style.fg.unwrap_or(Color::Reset),
                mode,
            ))
            .bg(crate::theme::downsample(
                style.bg.unwrap_or(Color::Reset),
                mode,
            )),
    }
}

fn safe_header(text: Arc<str>) -> Arc<str> {
    if text.len() > 8192 {
        return Arc::from(
            "Source metadata exceeds bounded header; Return to native for exact details",
        );
    }
    if !text.chars().any(|c|c.is_control()||matches!(c,'\u{061c}'|'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')){return text;}
    Arc::from(text.chars().map(|c|if c.is_control()||matches!(c,'\u{061c}'|'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'){'\u{fffd}'}else{c}).collect::<String>())
}

fn label(action: &PtyPaneActionV4) -> &'static str {
    match action {
        PtyPaneActionV4::Return => "[Esc Return]",
        PtyPaneActionV4::Cancel => "[^C Cancel]",
        PtyPaneActionV4::FocusQuery => "[/ Search]",
        PtyPaneActionV4::Find => "[Find]",
        PtyPaneActionV4::Command(command) => NAV
            .iter()
            .find(|(_, c)| c == command)
            .map_or("[Action]", |(label, _)| *label),
    }
}
fn layout(area: Rect) -> Layout {
    let inner = area.inner(ratatui::layout::Margin::new(1, 1));
    let mut hits = Vec::with_capacity(12);
    let mut x = inner.x;
    let mut y = inner.y.saturating_add(2);
    let actions = std::iter::once(PtyPaneActionV4::Return)
        .chain(std::iter::once(PtyPaneActionV4::Cancel))
        .chain(
            NAV.iter()
                .map(|(_, command)| PtyPaneActionV4::Command(command.clone())),
        )
        .chain([PtyPaneActionV4::FocusQuery, PtyPaneActionV4::Find]);
    for action in actions {
        let width = ratatui::text::Line::from(label(&action)).width() as u16;
        if x > inner.x && x.saturating_add(width) > inner.right() {
            x = inner.x;
            y = y.saturating_add(1);
        }
        if y < inner.bottom() && inner.width > 0 {
            hits.push(Hit {
                rect: Rect::new(x, y, width.min(inner.width), 1),
                action,
            });
            x = x.saturating_add(width + 1);
        }
    }
    let query_y = y.saturating_add(1);
    let query = if query_y < inner.bottom() {
        Rect::new(
            inner.x + 14.min(inner.width),
            query_y,
            inner.width.saturating_sub(14),
            1,
        )
    } else {
        Rect::default()
    };
    let body_y = query_y.saturating_add(1);
    let body = Rect::new(
        inner.x,
        body_y.min(inner.bottom()),
        inner.width,
        inner.bottom().saturating_sub(body_y).saturating_sub(1),
    );
    if area.width < 40 || area.height < 12 || body.width < 12 || body.height < 4 {
        // Resize-only presentation has just host Return/Cancel. Do not spend
        // the whole tiny surface on disabled navigation and hide its reason.
        hits.clear();
        x = inner.x;
        y = inner.y;
        for action in [PtyPaneActionV4::Return, PtyPaneActionV4::Cancel] {
            let width = ratatui::text::Line::from(label(&action)).width() as u16;
            if x > inner.x && x.saturating_add(width) > inner.right() {
                x = inner.x;
                y = y.saturating_add(1);
            }
            if y < inner.bottom() && inner.width > 0 {
                hits.push(Hit {
                    rect: Rect::new(x, y, width.min(inner.width), 1),
                    action,
                });
                x = x.saturating_add(width + 1);
            }
        }
        let body_y = y.saturating_add(1).min(inner.bottom());
        return Layout {
            body: Rect::new(
                inner.x,
                body_y,
                inner.width,
                inner.bottom().saturating_sub(body_y),
            ),
            query: Rect::default(),
            hits,
            rows: 1,
            usable: false,
        };
    }
    Layout {
        body,
        query,
        hits,
        rows: body_y.saturating_sub(inner.y),
        usable: true,
    }
}

#[cfg(test)]
#[path = "read_only_pty_pane_v4_tests.rs"]
mod tests;
