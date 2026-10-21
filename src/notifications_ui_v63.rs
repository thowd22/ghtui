//! Topmost notification UI. Draw borrows short prepared rows only; no child,
//! cache read, wrapping, full workspace clone or source issuance occurs here.
use crate::{
    app::{Action, App, HitRegion, Overlay},
    notifications_v63::*,
    theme::SemanticToken,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::Modifier,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::sync::Arc;
pub const TITLE: &str = "Notifications • observed";
/// One list row: state, freshness, reason and resource, then the latest
/// actor's initials chip and login (or `—` when unknown), then title and
/// repository. Below `ROW_LOGIN_WIDTH` columns only the chip is drawn, so the
/// title stays readable at 80x24; Details always names the login. The row's
/// own style is applied by the caller.
pub(crate) const ROW_LOGIN_WIDTH: u16 = 100;
pub(crate) fn row_line(
    theme: crate::theme::Theme,
    row: &PreparedNotificationRowV63,
    width: u16,
) -> ratatui::text::Line<'static> {
    let wide = width >= ROW_LOGIN_WIDTH;
    use ratatui::text::{Line, Span};
    let split = if row.label.is_char_boundary(row.lead) {
        row.lead
    } else {
        0
    };
    let (lead, tail) = row.label.split_at(split);
    let mut spans = vec![Span::raw(lead.to_owned())];
    match &row.actor {
        Some(identity) => {
            let chip = if theme.mode == crate::theme::ColorMode::NoColor {
                theme.style(SemanticToken::Actor)
            } else {
                theme.identity_chip(crate::identity::InitialsChip::slot(identity))
            };
            spans.push(Span::styled(
                crate::identity::InitialsChip::text(identity, theme.mode),
                chip,
            ));
            spans.push(Span::raw(if wide {
                format!(
                    " {} • ",
                    crate::providers::runner::sanitize_terminal(&crate::ui_table::actor_text(
                        identity
                    ))
                )
            } else {
                " ".into()
            }));
        }
        None => spans.push(Span::raw(if wide { "— • " } else { "— " })),
    }
    spans.push(Span::raw(tail.to_owned()));
    Line::from(spans)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationActionV63 {
    Open,
    Details,
    FollowPr,
    History,
    MarkRead,
    Ignore,
    Unignore,
    MarkUnread,
    Confirm,
    Inspect,
    Acknowledge,
    Watch,
    Refresh,
    IncludeRead,
    Participating,
    Select(usize),
    Move(i32),
    Scroll(i32),
    Home,
    Back,
    Quit,
}
pub struct NotificationBrowserV63 {
    pub(crate) original: Arc<NotificationPresentationV63>,
    pub(crate) query: crate::providers::gh::notifications_v63::NotificationQueryV63,
    pub(crate) prepared: Option<PreparedNotificationsV63>,
    pub(crate) saved: Option<PreparedNotificationsV63>,
    pub(crate) action_preview: Option<Arc<crate::providers::gh::NotificationActionPreviewV63>>,
    pub(crate) painted_preview: Option<Arc<crate::prepared_form_text_v1::PreparedFormTextV1>>,
    pub(crate) inspection: Option<Arc<crate::providers::gh::NotificationInspectionV63>>,
    pub(crate) painted_inspection: Option<Arc<crate::prepared_form_text_v1::PreparedFormTextV1>>,
    pub(crate) watch: Option<crate::providers::gh::notifications_v63::NotificationWatchV63>,
    pub(crate) confirm_rendered: bool,
    pub(crate) acknowledge_rendered: bool,
    pub(crate) revision: u64,
    pub(crate) selected: usize,
    pub(crate) scroll: usize,
    pub(crate) detail: bool,
    pub(crate) viewport: crate::prepared_form_text_v1::FormPreviewViewportV1,
    pub(crate) message: &'static str,
    pub(crate) focus: Option<usize>,
    pub(crate) hits: Vec<(Rect, NotificationActionV63)>,
    pub(crate) follow_return_v63: Option<NotificationReturnV63>,
    /// Set by a confirmed Mark read/Ignore/Unignore: the message shown when
    /// the queue's automatic re-read completes.
    pub(crate) after_write_v63: Option<&'static str>,
    body: Rect,
}
pub(crate) struct NotificationReturnV63 {
    pub(crate) detail: bool,
    pub(crate) scroll: usize,
    pub(crate) viewport: crate::prepared_form_text_v1::FormPreviewViewportV1,
    pub(crate) message: &'static str,
    pub(crate) focus: Option<usize>,
}
impl NotificationBrowserV63 {
    pub(crate) fn pending(original: Arc<NotificationPresentationV63>) -> Self {
        Self {
            original,
            query: Default::default(),
            prepared: None,
            saved: None,
            action_preview: None,
            painted_preview: None,
            inspection: None,
            painted_inspection: None,
            watch: None,
            confirm_rendered: false,
            acknowledge_rendered: false,
            revision: 0,
            selected: 0,
            scroll: 0,
            detail: false,
            viewport: Default::default(),
            message: "Loading original notification query",
            focus: None,
            hits: vec![],
            follow_return_v63: None,
            after_write_v63: None,
            body: Rect::default(),
        }
    }
}
pub(crate) fn dispatch(app: &mut App, action: NotificationActionV63) {
    use NotificationActionV63 as A;
    if action == A::Open {
        crate::notifications_native_v63::open(app);
        return;
    }
    if crate::notifications_native_v63::retire_invalid(app) {
        return;
    }
    if action == A::Quit {
        crate::notifications_native_v63::close(app);
        app.quit = true;
        return;
    }
    if action == A::Back {
        if app.notifications_work_v63.is_none()
            && app.notifications_owner_v63.is_none()
            && app
                .notifications_v63
                .as_ref()
                .is_some_and(|view| view.detail)
        {
            if let Some(view) = &mut app.notifications_v63 {
                if view.action_preview.take().is_some() {
                    view.prepared = view.saved.take();
                }
                if view.inspection.take().is_some() {
                    view.prepared = view.saved.take();
                    view.painted_inspection = None;
                }
                view.detail = false;
                view.scroll = 0;
                view.hits.clear();
            }
        } else {
            crate::notifications_native_v63::close(app);
        }
        return;
    }
    if action == A::Watch {
        crate::notifications_native_v63::watch(app);
        return;
    }
    if app.notifications_work_v63.is_some() || app.notifications_owner_v63.is_some() {
        return;
    }
    match action {
        A::FollowPr => {
            let eligible = app.notifications_v63.as_ref().is_some_and(|view| {
                view.action_preview.is_none()
                    && view.inspection.is_none()
                    && view.prepared.as_ref().is_some_and(|p| {
                        p.source
                            .rows()
                            .get(view.selected)
                            .is_some_and(|row| row.has_pr_target_v63())
                    })
            });
            if eligible {
                crate::notifications_native_v63::request(app, NotificationReadPhaseV63::FollowPr);
            }
            return;
        }
        A::Inspect => {
            if app.notifications_v63.as_ref().is_some_and(|view| {
                view.prepared
                    .as_ref()
                    .is_some_and(|p| !p.attempts.is_empty())
            }) {
                crate::notifications_native_v63::request(app, NotificationReadPhaseV63::Inspect);
            }
            return;
        }
        A::Acknowledge => {
            if app.notifications_v63.as_ref().is_some_and(|view| {
                view.acknowledge_rendered
                    && view.inspection.as_ref().is_some_and(|proof| {
                        matches!(
                            proof.marker().state(),
                            crate::storage::NotificationWriteStateV63::Pending
                                | crate::storage::NotificationWriteStateV63::Uncertain
                        ) && view
                            .painted_inspection
                            .as_ref()
                            .is_some_and(|doc| Arc::ptr_eq(doc, proof.document()))
                    })
            }) {
                crate::notifications_native_v63::request(
                    app,
                    NotificationReadPhaseV63::Acknowledge,
                );
            }
            return;
        }
        A::MarkRead | A::Ignore | A::Unignore => {
            if app.notifications_v63.as_ref().is_some_and(|view| {
                view.action_preview.is_none()
                    && view.prepared.as_ref().is_some_and(|p| !p.rows.is_empty())
            }) {
                crate::notifications_native_v63::request(
                    app,
                    match action {
                        A::MarkRead => NotificationReadPhaseV63::PreviewMarkRead,
                        A::Ignore => NotificationReadPhaseV63::PreviewIgnore,
                        _ => NotificationReadPhaseV63::PreviewUnignore,
                    },
                );
            }
            return;
        }
        A::Confirm => {
            if app.notifications_v63.as_ref().is_some_and(|view| {
                view.confirm_rendered
                    && view.action_preview.as_ref().is_some_and(|preview| {
                        view.painted_preview
                            .as_ref()
                            .is_some_and(|doc| Arc::ptr_eq(doc, preview.document()))
                    })
            }) {
                crate::notifications_native_v63::request(app, NotificationReadPhaseV63::Publish);
            }
            return;
        }
        A::MarkUnread => {
            if let Some(view) = &mut app.notifications_v63 {
                view.message = "Mark unread unavailable: GitHub has no API to mark a thread unread";
            }
            return;
        }
        A::Refresh => {
            crate::notifications_native_v63::request(app, NotificationReadPhaseV63::Refresh);
            return;
        }
        A::Details => {
            if app
                .notifications_v63
                .as_ref()
                .is_some_and(|view| !view.detail)
            {
                crate::notifications_native_v63::request(app, NotificationReadPhaseV63::Details);
            }
            return;
        }
        A::History => {
            crate::notifications_native_v63::request(app, NotificationReadPhaseV63::History);
            return;
        }
        A::IncludeRead | A::Participating => {
            if let Some(view) = &mut app.notifications_v63 {
                if action == A::IncludeRead {
                    view.query.include_read = !view.query.include_read;
                } else {
                    view.query.participating = !view.query.participating;
                }
                // A changed query may not reuse a conditional cached body.
                view.prepared = None;
                view.selected = 0;
            }
            crate::notifications_native_v63::request(app, NotificationReadPhaseV63::Load);
            return;
        }
        _ => {}
    }
    let Some(view) = &mut app.notifications_v63 else {
        return;
    };
    let len = view.prepared.as_ref().map_or(0, |p| p.rows.len());
    match action {
        A::Select(index) if !view.detail && index < len => {
            view.selected = index;
            view.focus = None;
        }
        A::Move(delta) if !view.detail => {
            view.selected = view
                .selected
                .saturating_add_signed(delta as isize)
                .min(len.saturating_sub(1));
            view.scroll = view.selected;
            view.focus = None;
        }
        A::Move(delta) | A::Scroll(delta) if view.detail => {
            view.scroll = view.scroll.saturating_add_signed(delta as isize).min(65536);
        }
        A::Scroll(delta) => {
            view.selected = view
                .selected
                .saturating_add_signed(delta as isize)
                .min(len.saturating_sub(1));
            view.scroll = view.selected;
        }
        A::Home => {
            view.scroll = 0;
            if !view.detail {
                view.selected = 0;
            }
        }
        _ => {}
    }
    view.hits.clear();
}
pub(crate) fn event(app: &mut App, event: &Event) -> bool {
    if crate::notifications_native_v63::retire_invalid(app) {
        return true;
    }
    if app.overlay != Some(Overlay::NotificationsV63) {
        return false;
    }
    use NotificationActionV63 as A;
    let action = match event {
        Event::Resize(..) => {
            if let Some(view) = &mut app.notifications_v63 {
                view.confirm_rendered = false;
                view.acknowledge_rendered = false;
                view.hits.clear();
                view.body = Rect::default();
                view.painted_preview = None;
                view.painted_inspection = None;
            }
            None
        }
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                Some(A::Quit)
            } else {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('b') => Some(A::Back),
                    KeyCode::Char('q') => Some(A::Quit),
                    KeyCode::Enter => app
                        .notifications_v63
                        .as_ref()
                        .and_then(|view| view.focus.and_then(|i| view.hits.get(i).map(|(_, a)| *a)))
                        .or(Some(A::Details)),
                    KeyCode::Char('r') => Some(A::Refresh),
                    KeyCode::Char('h') => Some(A::History),
                    KeyCode::Char('m') => Some(A::MarkRead),
                    KeyCode::Char('i') => Some(A::Ignore),
                    KeyCode::Char('u') => Some(A::Unignore),
                    KeyCode::Char('U') => Some(A::MarkUnread),
                    KeyCode::Char('C') => Some(A::Confirm),
                    KeyCode::Char('I') => Some(A::Inspect),
                    KeyCode::Char('L') => Some(A::Acknowledge),
                    KeyCode::Char('W') => Some(A::Watch),
                    KeyCode::Char('a') => Some(A::IncludeRead),
                    KeyCode::Char('p') => Some(A::Participating),
                    KeyCode::Char('f') => Some(A::FollowPr),
                    KeyCode::Up | KeyCode::Char('k') => Some(A::Move(-1)),
                    KeyCode::Down | KeyCode::Char('j') => Some(A::Move(1)),
                    KeyCode::PageUp => Some(A::Scroll(-10)),
                    KeyCode::PageDown => Some(A::Scroll(10)),
                    KeyCode::Home => Some(A::Home),
                    KeyCode::Tab | KeyCode::BackTab => {
                        if let Some(view) = &mut app.notifications_v63 {
                            let count = view
                                .hits
                                .iter()
                                .take_while(|(_, a)| !matches!(a, A::Select(_)))
                                .count();
                            if count > 0 {
                                view.focus =
                                    Some(match (view.focus, key.code == KeyCode::BackTab) {
                                        (Some(i), true) => (i + count - 1) % count,
                                        (Some(i), false) => (i + 1) % count,
                                        (None, true) => count - 1,
                                        (None, false) => 0,
                                    });
                            }
                        }
                        None
                    }
                    _ => None,
                }
            }
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                app.notifications_v63.as_ref().and_then(|view| {
                    view.hits
                        .iter()
                        .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
                        .map(|(_, a)| *a)
                })
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => app
                .notifications_v63
                .as_ref()
                .filter(|view| view.body.contains((mouse.column, mouse.row).into()))
                .map(|_| {
                    A::Scroll(if mouse.kind == MouseEventKind::ScrollDown {
                        3
                    } else {
                        -3
                    })
                }),
            _ => None,
        },
        _ => None,
    };
    if let Some(action) = action {
        dispatch(app, action);
    }
    true
}
pub(crate) fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    crate::notifications_native_v63::retire_invalid(app);
    app.hits.clear();
    frame.render_widget(Clear, area);
    let theme = app.theme();
    let valid = app
        .notifications_v63
        .as_ref()
        .is_some_and(|view| view.original.owns(app));
    let busy = app.notifications_work_v63.is_some() || app.notifications_owner_v63.is_some();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(TITLE)
        .style(theme.style(SemanticToken::Normal));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(view) = &mut app.notifications_v63 else {
        return;
    };
    view.hits.clear();
    view.body = Rect::default();
    view.confirm_rendered = false;
    view.acknowledge_rendered = false;
    if !valid || inner.width < 20 || inner.height < 6 {
        return;
    }
    let mut x = inner.x;
    let mut y = inner.y;
    let mut controls = vec![
        ("[b Back / Cancel]", NotificationActionV63::Back),
        ("[q Quit]", NotificationActionV63::Quit),
        ("[r Refresh]", NotificationActionV63::Refresh),
        ("[h History]", NotificationActionV63::History),
        ("[Enter Details]", NotificationActionV63::Details),
        ("[f Follow PR]", NotificationActionV63::FollowPr),
        ("[a Include read]", NotificationActionV63::IncludeRead),
        ("[p Participating]", NotificationActionV63::Participating),
    ];
    controls.push(("[W Watch / Stop]", NotificationActionV63::Watch));
    if view.inspection.is_some() {
        controls.insert(2, ("[L Local ACK]", NotificationActionV63::Acknowledge));
    } else if view
        .prepared
        .as_ref()
        .is_some_and(|p| !p.attempts.is_empty())
    {
        controls.insert(2, ("[I Inspect latest]", NotificationActionV63::Inspect));
    } else if view.action_preview.is_some() {
        controls.insert(2, ("[C Confirm]", NotificationActionV63::Confirm));
    } else if view.prepared.as_ref().is_some_and(|p| !p.rows.is_empty()) {
        controls.extend([
            ("[m Mark read]", NotificationActionV63::MarkRead),
            ("[i Ignore]", NotificationActionV63::Ignore),
            ("[u Unignore]", NotificationActionV63::Unignore),
            (
                "[U Mark unread unavailable]",
                NotificationActionV63::MarkUnread,
            ),
        ]);
    }
    for (label, action) in controls {
        let width = label.len() as u16;
        if x > inner.x && x.saturating_add(width) > inner.right() {
            x = inner.x;
            y += 1;
        }
        if y >= inner.bottom().saturating_sub(3) {
            break;
        }
        let rect = Rect::new(x, y, width.min(inner.width), 1);
        let enabled = matches!(
            action,
            NotificationActionV63::Back | NotificationActionV63::Quit
        ) || action == NotificationActionV63::Watch && view.watch.is_some()
            || !busy
                && action != NotificationActionV63::MarkUnread
                && (action != NotificationActionV63::FollowPr
                    || view.action_preview.is_none()
                        && view.inspection.is_none()
                        && view.prepared.as_ref().is_some_and(|p| {
                            p.source
                                .rows()
                                .get(view.selected)
                                .is_some_and(|row| row.has_pr_target_v63())
                        }))
                && (action != NotificationActionV63::Watch
                    || !view.detail && view.prepared.is_some())
                && (action != NotificationActionV63::Acknowledge
                    || view.inspection.as_ref().is_some_and(|proof| {
                        matches!(
                            proof.marker().state(),
                            crate::storage::NotificationWriteStateV63::Pending
                                | crate::storage::NotificationWriteStateV63::Uncertain
                        ) && view
                            .painted_inspection
                            .as_ref()
                            .is_some_and(|doc| Arc::ptr_eq(doc, proof.document()))
                    }))
                && (action != NotificationActionV63::Confirm
                    || view.action_preview.as_ref().is_some_and(|preview| {
                        view.painted_preview
                            .as_ref()
                            .is_some_and(|doc| Arc::ptr_eq(doc, preview.document()))
                    }))
                && (action != NotificationActionV63::Details
                    || !view.detail && view.prepared.as_ref().is_some_and(|p| !p.rows.is_empty()));
        let mut style = theme.style(if enabled {
            SemanticToken::Focus
        } else {
            SemanticToken::Muted
        });
        if action == NotificationActionV63::Confirm {
            view.confirm_rendered = enabled;
        }
        if action == NotificationActionV63::Acknowledge {
            view.acknowledge_rendered = enabled;
        }
        if view.focus == Some(view.hits.len()) {
            style = style.add_modifier(Modifier::REVERSED);
        }
        frame.render_widget(Paragraph::new(label).style(style), rect);
        // Disabled controls retain SAME action; dispatch independently refuses.
        view.hits.push((rect, action));
        x = x.saturating_add(width + 1);
    }
    y += 1;
    frame.render_widget(
        Paragraph::new(view.message).style(theme.style(if busy {
            SemanticToken::Pending
        } else {
            SemanticToken::Normal
        })),
        Rect::new(inner.x, y, inner.width, 1),
    );
    y += 1;
    let footer = inner.bottom().saturating_sub(1);
    view.body = Rect::new(inner.x, y, inner.width, footer.saturating_sub(y));
    if let Some(inspection) = &view.inspection {
        match inspection.document().window(
            &mut view.viewport,
            view.body.width,
            view.body.height.min(28),
            view.scroll,
        ) {
            Ok(window) => {
                view.painted_inspection = Some(inspection.document().clone());
                view.scroll = window.top();
                for (i, row) in window.rows().enumerate() {
                    frame.render_widget(
                        Paragraph::new(row.text()),
                        Rect::new(view.body.x, view.body.y + i as u16, view.body.width, 1),
                    );
                }
            }
            Err(_) => frame.render_widget(
                Paragraph::new("Widen to66 columns to inspect current state • Back cancels"),
                view.body,
            ),
        }
    } else if let Some(preview) = &view.action_preview {
        match preview.document().window(
            &mut view.viewport,
            view.body.width,
            view.body.height.min(28),
            view.scroll,
        ) {
            Ok(window) => {
                view.painted_preview = Some(preview.document().clone());
                view.scroll = window.top();
                for (i, row) in window.rows().enumerate() {
                    frame.render_widget(
                        Paragraph::new(row.text()),
                        Rect::new(view.body.x, view.body.y + i as u16, view.body.width, 1),
                    );
                }
            }
            Err(_) => frame.render_widget(
                Paragraph::new("Widen to66 columns to inspect Preview • Back cancels"),
                view.body,
            ),
        }
    } else if let Some(prepared) = &view.prepared {
        if view.detail {
            if let Some((_, doc)) = &prepared.detail {
                match doc.window(&mut view.viewport,view.body.width,view.body.height.min(28),view.scroll) {
                    Ok(window)=>{view.scroll=window.top();for (i,row) in window.rows().enumerate(){frame.render_widget(Paragraph::new(row.text()),Rect::new(view.body.x,view.body.y+i as u16,view.body.width,1));}}
                    Err(_)=>frame.render_widget(Paragraph::new("Widen to at least66 terminal columns to inspect full details • Back remains available"),view.body),
                }
            }
        } else {
            if prepared.rows.is_empty() {
                frame.render_widget(
                    Paragraph::new("No notifications observed in this exact query."),
                    view.body,
                );
            }
            view.scroll = view.scroll.min(
                prepared
                    .rows
                    .len()
                    .saturating_sub(usize::from(view.body.height)),
            );
            if view.selected < view.scroll {
                view.scroll = view.selected;
            }
            if view.selected >= view.scroll + usize::from(view.body.height) {
                view.scroll = view
                    .selected
                    .saturating_sub(usize::from(view.body.height).saturating_sub(1));
            }
            for (offset, row) in prepared
                .rows
                .iter()
                .enumerate()
                .skip(view.scroll)
                .take(usize::from(view.body.height))
            {
                let rect = Rect::new(
                    view.body.x,
                    view.body.y + (offset - view.scroll) as u16,
                    view.body.width,
                    1,
                );
                let style = theme.style(if offset == view.selected {
                    SemanticToken::Selection
                } else if row.unread {
                    SemanticToken::Pending
                } else {
                    SemanticToken::Normal
                });
                frame.render_widget(
                    Paragraph::new(row_line(theme, row, rect.width)).style(style),
                    rect,
                );
                view.hits
                    .push((rect, NotificationActionV63::Select(offset)));
            }
        }
        frame.render_widget(
            Paragraph::new(prepared.coverage.as_ref()).style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, footer, inner.width, 1),
        );
    }
    app.hits
        .extend(view.hits.iter().map(|(rect, action)| HitRegion {
            rect: *rect,
            action: Action::NotificationsV63(*action),
        }));
}
