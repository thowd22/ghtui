//! SAME ScopedReviews overlay, distinct real Summary display state. Borrowed
//! prepared text only: no body scan/wrap/parse, provider call or write control.
use super::*;
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Margin, Rect},
    style::Modifier,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
pub const TITLE: &str = "Review operation history • inspect only";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryActionV3 {
    Select(usize),
    Move(i32),
    Scroll(i32),
    Open,
    Refresh,
    InspectCurrentV11,
    AcknowledgeV11,
    Back,
    Quit,
}
pub struct ReviewOperationHistoryViewV3 {
    pub(crate) listed: Arc<PreparedReviewOperationHistoryV3>,
    pub(crate) opened: Option<Arc<PreparedReviewOperationHistoryV3>>,
    pub(crate) recovery_v11: Option<Arc<recovery_v11::PreparedReviewRecoveryV11>>,
    pub(crate) selected: usize,
    pub(crate) scroll: usize,
    pub(crate) ticket: Arc<()>,
    pub(crate) message: &'static str,
    viewport: crate::prepared_form_text_v1::FormPreviewViewportV1,
    hits: Vec<(Rect, HistoryActionV3)>,
    body: Rect,
    interactive: bool,
}
impl ReviewOperationHistoryViewV3 {
    pub fn new(listed: Arc<PreparedReviewOperationHistoryV3>) -> Self {
        Self {
            listed,
            opened: None,
            recovery_v11: None,
            selected: 0,
            scroll: 0,
            ticket: Arc::new(()),
            message: "INSPECT ONLY history • current outcome inspection is a separate action",
            viewport: Default::default(),
            hits: vec![],
            body: Rect::default(),
            interactive: false,
        }
    }
    pub fn intent(&self) -> &Arc<ReviewOperationHistoryIntentV3> {
        self.listed.intent()
    }
    pub fn owns(&self, app: &App) -> bool {
        self.intent().owns(app)
    }
    pub fn interactive(&self) -> bool {
        self.interactive
    }
    pub fn clear_layout(&mut self) {
        self.hits.clear();
        self.body = Rect::default();
        self.interactive = false;
    }
    pub fn install_opened(&mut self, opened: Arc<PreparedReviewOperationHistoryV3>) -> bool {
        if !Arc::ptr_eq(self.intent(), opened.intent()) || opened.opened != Some(self.selected) {
            return false;
        }
        self.opened = Some(opened);
        self.recovery_v11 = None;
        self.scroll = 0;
        self.viewport = Default::default();
        self.clear_layout();
        true
    }
    pub(crate) fn install_recovery_v11(
        &mut self,
        prepared: Arc<recovery_v11::PreparedReviewRecoveryV11>,
    ) -> bool {
        if self.selected != prepared.request().row()
            || self
                .opened
                .as_ref()
                .is_none_or(|opened| !Arc::ptr_eq(opened, prepared.request().history()))
            || !Arc::ptr_eq(self.intent(), prepared.request().intent())
        {
            return false;
        }
        self.recovery_v11 = Some(prepared);
        self.scroll = 0;
        self.viewport = Default::default();
        self.message = "FRESH CURRENT OUTCOME • accepts uncertainty locally, never resends";
        self.ticket = Arc::new(());
        self.clear_layout();
        true
    }
    /// Only local display selection changes. Open/Refresh are returned to the
    /// SAME original Review queue and must freshly prove principal/source/SQL.
    pub fn apply(&mut self, action: HistoryActionV3) -> Option<HistoryActionV3> {
        if !self.interactive && !matches!(action, HistoryActionV3::Back | HistoryActionV3::Quit) {
            return None;
        }
        match action {
            HistoryActionV3::Select(index)
                if self.opened.is_none() && index < self.listed.rows() =>
            {
                if index != self.selected {
                    self.ticket = Arc::new(());
                }
                self.selected = index;
                self.scroll = index;
            }
            HistoryActionV3::Move(delta) | HistoryActionV3::Scroll(delta) => {
                if self.opened.is_some() {
                    self.scroll = self.scroll.saturating_add_signed(delta as isize);
                } else {
                    let next = self
                        .selected
                        .saturating_add_signed(delta as isize)
                        .min(self.listed.rows().saturating_sub(1));
                    if next != self.selected {
                        self.ticket = Arc::new(());
                    }
                    self.selected = next;
                    self.scroll = next;
                }
            }
            HistoryActionV3::Back if self.opened.is_some() => {
                if self.recovery_v11.take().is_some() {
                    self.scroll = 0;
                } else {
                    self.opened = None;
                    self.scroll = self.selected;
                }
                self.viewport = Default::default();
                self.ticket = Arc::new(());
            }
            HistoryActionV3::InspectCurrentV11 if self.opened.is_some() => return Some(action),
            HistoryActionV3::AcknowledgeV11
                if self
                    .recovery_v11
                    .as_ref()
                    .is_some_and(|prepared| prepared.available()) =>
            {
                return Some(action)
            }
            HistoryActionV3::AcknowledgeV11 => {
                self.message =
                    "Fresh explicit Inspect current required; history never grants acknowledgement";
            }
            HistoryActionV3::Open if self.listed.rows() == 0 => {}
            HistoryActionV3::Open
            | HistoryActionV3::Refresh
            | HistoryActionV3::Back
            | HistoryActionV3::Quit => return Some(action),
            _ => {}
        }
        self.hits.clear();
        None
    }
}
pub fn input(view: &mut ReviewOperationHistoryViewV3, event: Event) -> Option<HistoryActionV3> {
    match event {
        Event::Resize(_, _) => {
            view.clear_layout();
            None
        }
        Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
            KeyCode::Esc | KeyCode::Char('b') => Some(HistoryActionV3::Back),
            KeyCode::Char('q') => Some(HistoryActionV3::Quit),
            KeyCode::Enter => Some(HistoryActionV3::Open),
            KeyCode::Char('r') => Some(HistoryActionV3::Refresh),
            KeyCode::Char('i') => Some(HistoryActionV3::InspectCurrentV11),
            KeyCode::Char('a') => Some(HistoryActionV3::AcknowledgeV11),
            KeyCode::Up | KeyCode::Char('k') => Some(HistoryActionV3::Move(-1)),
            KeyCode::Down | KeyCode::Char('j') => Some(HistoryActionV3::Move(1)),
            KeyCode::PageUp => Some(HistoryActionV3::Scroll(-10)),
            KeyCode::PageDown => Some(HistoryActionV3::Scroll(10)),
            _ => None,
        },
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => view
                .hits
                .iter()
                .find(|(rect, _)| rect.contains((mouse.column, mouse.row).into()))
                .map(|(_, action)| *action),
            MouseEventKind::ScrollUp if view.body.contains((mouse.column, mouse.row).into()) => {
                Some(HistoryActionV3::Scroll(-3))
            }
            MouseEventKind::ScrollDown if view.body.contains((mouse.column, mouse.row).into()) => {
                Some(HistoryActionV3::Scroll(3))
            }
            _ => None,
        },
        _ => None,
    }
}
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    view: &mut ReviewOperationHistoryViewV3,
    theme: crate::theme::Theme,
    avatars: &crate::actor_avatar_states_v1::ActorAvatarStatesV1,
    busy: bool,
) {
    use crate::theme::SemanticToken;
    view.clear_layout();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(format!(" {TITLE} "))
            .style(theme.base()),
        area,
    );
    let inner = area.inner(Margin::new(1, 1));
    if inner.height == 0 {
        return;
    }
    let back = Rect::new(inner.x, inner.bottom() - 1, 12.min(inner.width), 1);
    frame.render_widget(Paragraph::new("[Esc Back]"), back);
    view.hits.push((back, HistoryActionV3::Back));
    if inner.width < 68 || inner.height < 10 {
        frame.render_widget(
            Paragraph::new("Resize to inspect protected review history; no actions or hidden body"),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        return;
    }
    view.interactive = !busy;
    let data = view.opened.as_ref().unwrap_or(&view.listed);
    let actor = view
        .recovery_v11
        .as_ref()
        .map_or(&data.actor, |prepared| prepared.actor());
    frame.render_widget(
        crate::identity::AvatarWidget {
            identity: actor,
            image: avatars.ready(actor),
            theme,
        },
        Rect::new(inner.x, inner.y, 4, 1),
    );
    let inspector = view.recovery_v11.as_ref().map(|prepared| {
        format!(
            "{} • actual inspector UID {:?} • local recovery ONLY",
            actor.label(),
            prepared.identity().account_id
        )
    });
    frame.render_widget(
        Paragraph::new(
            inspector
                .as_deref()
                .unwrap_or(data.inspector_label.as_ref()),
        )
        .style(theme.style(SemanticToken::Actor)),
        Rect::new(inner.x + 4, inner.y, inner.width - 4, 1),
    );
    frame.render_widget(
        Paragraph::new(if busy {
            "Original history read pending; Esc cancels, no writes"
        } else {
            view.message
        })
        .style(theme.style(SemanticToken::Pending)),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
    let body = Rect::new(
        inner.x,
        inner.y + 3,
        inner.width,
        inner.height.saturating_sub(6).min(28),
    );
    view.body = body;
    if view.opened.is_some() {
        let display = view
            .recovery_v11
            .as_ref()
            .map_or(&data.display, |prepared| prepared.display());
        match display.window(
            &mut view.viewport,
            body.width.saturating_sub(4),
            body.height,
            view.scroll,
        ) {
            Ok(window) => {
                view.scroll = window.top();
                for (offset, row) in window.rows().enumerate() {
                    frame.render_widget(
                        Paragraph::new(row.text()).style(theme.base()),
                        Rect::new(body.x + 4, body.y + offset as u16, window.column(), 1),
                    );
                }
            }
            Err(error) => {
                view.interactive = false;
                frame.render_widget(Paragraph::new(error.message()), body);
            }
        }
    } else {
        view.scroll = view
            .scroll
            .min(view.listed.rows().saturating_sub(usize::from(body.height)));
        for (index, label) in view
            .listed
            .labels
            .iter()
            .enumerate()
            .skip(view.scroll)
            .take(usize::from(body.height))
        {
            let rect = Rect::new(body.x, body.y + (index - view.scroll) as u16, body.width, 1);
            frame.render_widget(
                Paragraph::new(label.as_str()).style(if index == view.selected {
                    theme
                        .style(SemanticToken::Focus)
                        .add_modifier(Modifier::REVERSED)
                } else {
                    theme.base()
                }),
                rect,
            );
            if !busy {
                view.hits.push((rect, HistoryActionV3::Select(index)));
            }
        }
    }
    if view.opened.is_some() {
        let mut x = inner.x;
        for (label, action, enabled) in [
            (
                "[i Inspect current]",
                HistoryActionV3::InspectCurrentV11,
                true,
            ),
            (
                "[a Acknowledge local]",
                HistoryActionV3::AcknowledgeV11,
                view.recovery_v11
                    .as_ref()
                    .is_some_and(|prepared| prepared.available()),
            ),
        ] {
            let width = label.len() as u16;
            let rect = Rect::new(x, inner.bottom() - 2, width, 1);
            frame.render_widget(
                Paragraph::new(label).style(if enabled && !busy {
                    theme.base()
                } else {
                    theme.style(SemanticToken::Muted)
                }),
                rect,
            );
            if !busy {
                view.hits.push((rect, action));
            }
            x += width + 1;
        }
    }
    let mut x = inner.x + 12;
    for (label, action) in [
        ("[Enter Inspect]", HistoryActionV3::Open),
        ("[r Refresh]", HistoryActionV3::Refresh),
        ("[q Quit]", HistoryActionV3::Quit),
    ] {
        let width = label.len() as u16;
        if x + width > inner.right() {
            break;
        }
        let rect = Rect::new(x, inner.bottom() - 1, width, 1);
        frame.render_widget(Paragraph::new(label), rect);
        if !busy || action == HistoryActionV3::Quit {
            view.hits.push((rect, action));
        }
        x += width + 1;
    }
}
