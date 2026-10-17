//! Pure bounded thread presentation. Every visible control maps to the SAME
//! action for keys/clicks. No transport, file read, sort or workspace clone.
use super::*;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadUiActionV1 {
    CurrentCode,
    Comments,
    Publication(publication_v1::ThreadPublicationActionV1),
    Select(usize),
    Move(i32),
    Scroll(i32),
    Horizontal(i32),
    Home,
    Toggle,
    Filter,
    Source,
    Back,
    Quit,
}
pub struct ThreadBrowserV1 {
    pub(crate) code_notice_v4: Option<String>,
    pub(crate) presentation_epoch_v4: Option<u64>,
    pub comments_v2: Option<comments_v2::ui::ThreadCommentsBrowserV2>,
    pub intent: Arc<ThreadBrowseIntentV1>,
    pub prepared: Option<Arc<PreparedThreadsV1>>,
    pub selected: usize,
    pub scroll: usize,
    pub column: usize,
    pub file_only: bool,
    pub expanded: bool,
    pub source: bool,
    pub message: &'static str,
    pub hits: Vec<(Rect, ThreadUiActionV1)>,
}
impl ThreadBrowserV1 {
    pub fn pending(intent: Arc<ThreadBrowseIntentV1>) -> Self {
        let file_only = intent.selected_path_v4().is_some();
        Self {
            code_notice_v4: None,
            presentation_epoch_v4: Some(0),
            comments_v2: None,
            intent,
            prepared: None,
            selected: 0,
            scroll: 0,
            column: 0,
            file_only,
            expanded: true,
            source: false,
            message: "Loading original threads • Esc Back cancels • no writes",
            hits: vec![],
        }
    }
    fn indices(&self) -> &[usize] {
        self.prepared.as_ref().map_or(&[], |prepared| {
            if self.file_only {
                &prepared.selected_file
            } else {
                &prepared.all
            }
        })
    }
}
fn publication_available(
    view: &ThreadBrowserV1,
    action: publication_v1::ThreadPublicationActionV1,
) -> bool {
    view.prepared
        .as_ref()
        .and_then(|prepared| {
            let indices = if view.file_only {
                &prepared.selected_file
            } else {
                &prepared.all
            };
            indices
                .get(view.selected)
                .and_then(|index| prepared.observation.threads.get(*index))
        })
        .is_some_and(|thread| match action {
            publication_v1::ThreadPublicationActionV1::Reply => {
                thread.viewer_can_reply == Some(true)
            }
            publication_v1::ThreadPublicationActionV1::Resolve => {
                thread.resolved == Some(false) && thread.viewer_can_resolve == Some(true)
            }
            publication_v1::ThreadPublicationActionV1::Reopen => {
                thread.resolved == Some(true) && thread.viewer_can_unresolve == Some(true)
            }
        })
}
pub fn retire_invalid(app: &mut App) -> bool {
    let Some(intent) = app
        .scoped_threads_v1
        .as_ref()
        .filter(|view| !view.intent.owns(app))
        .map(|view| view.intent.clone())
    else {
        return false;
    };
    if crate::scoped_pr_observed_tabs_v1::resources::threads_v1::retire_original_account_v75(
        app,
        &intent.source,
        &intent.active,
        &intent.navigation,
    ) {
        return true;
    }
    if app.scoped_threads_work_v1.as_ref().is_some_and(|owner| {
        owner
            .browse_intent()
            .is_none_or(|files| !Arc::ptr_eq(files, &intent))
    }) || app.scoped_threads_owner_v1.as_ref().is_some_and(|owner| {
        owner
            .browse_intent()
            .is_none_or(|files| !Arc::ptr_eq(files, &intent))
    }) {
        if let Some(view) = &mut app.scoped_threads_v1 {
            view.hits.clear();
        }
        return false;
    }
    native_v1::close(app);
    true
}
pub fn event(app: &mut App, event: &Event) -> bool {
    if retire_invalid(app) {
        return true;
    }
    if app.overlay != Some(Overlay::PrThreadsV1) {
        return false;
    }
    if matches!(event, Event::Key(key) if key.kind!=KeyEventKind::Release)
        || matches!(event,Event::Mouse(mouse) if matches!(mouse.kind,MouseEventKind::Down(_) | MouseEventKind::ScrollDown | MouseEventKind::ScrollUp))
        || matches!(event, Event::Resize(..))
    {
        if let Some(view) = &mut app.scoped_threads_v1 {
            view.presentation_epoch_v4 = view
                .presentation_epoch_v4
                .and_then(|old| old.checked_add(1));
        }
    }
    if app
        .scoped_threads_v1
        .as_ref()
        .is_some_and(|view| view.comments_v2.is_some())
    {
        return comments_v2::ui::event(app, event);
    }
    let action = match event {
        Event::Resize(_, _) => {
            if let Some(view) = &mut app.scoped_threads_v1 {
                view.hits.clear();
            }
            None
        }
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                Some(ThreadUiActionV1::Quit)
            } else {
                match key.code {
                    KeyCode::Char('q') => Some(ThreadUiActionV1::Quit),
                    KeyCode::Esc | KeyCode::Char('b') => Some(ThreadUiActionV1::Back),
                    KeyCode::Up | KeyCode::Char('k') => Some(ThreadUiActionV1::Move(-1)),
                    KeyCode::Down | KeyCode::Char('j') => Some(ThreadUiActionV1::Move(1)),
                    KeyCode::PageUp => Some(ThreadUiActionV1::Scroll(-10)),
                    KeyCode::PageDown => Some(ThreadUiActionV1::Scroll(10)),
                    KeyCode::Left => Some(ThreadUiActionV1::Horizontal(-32)),
                    KeyCode::Right => Some(ThreadUiActionV1::Horizontal(32)),
                    KeyCode::Home => Some(ThreadUiActionV1::Home),
                    KeyCode::Enter | KeyCode::Char(' ') => Some(ThreadUiActionV1::Toggle),
                    KeyCode::Char('a') => Some(ThreadUiActionV1::Filter),
                    KeyCode::Char('i') => Some(ThreadUiActionV1::Source),
                    KeyCode::Char('c') => Some(ThreadUiActionV1::Comments),
                    KeyCode::Char('g') => Some(ThreadUiActionV1::CurrentCode),
                    KeyCode::Char('p') => Some(ThreadUiActionV1::Publication(
                        publication_v1::ThreadPublicationActionV1::Reply,
                    )),
                    KeyCode::Char('r') => Some(ThreadUiActionV1::Publication(
                        publication_v1::ThreadPublicationActionV1::Resolve,
                    )),
                    KeyCode::Char('u') => Some(ThreadUiActionV1::Publication(
                        publication_v1::ThreadPublicationActionV1::Reopen,
                    )),
                    _ => None,
                }
            }
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                app.scoped_threads_v1.as_ref().and_then(|view| {
                    view.hits
                        .iter()
                        .find(|(rect, _)| rect.contains((mouse.column, mouse.row).into()))
                        .map(|(_, action)| *action)
                })
            }
            MouseEventKind::ScrollDown => Some(ThreadUiActionV1::Scroll(3)),
            MouseEventKind::ScrollUp => Some(ThreadUiActionV1::Scroll(-3)),
            _ => None,
        },
        _ => None,
    };
    if let Some(action) = action {
        dispatch(app, action);
    }
    true
}
pub fn dispatch(app: &mut App, action: ThreadUiActionV1) {
    if action == ThreadUiActionV1::Back || action == ThreadUiActionV1::Quit {
        native_v1::close(app);
        if action == ThreadUiActionV1::Quit {
            app.quit = true;
        }
        return;
    }
    if app.loading[View::Inbox.index()] || app.scoped_threads_work_v1.is_some() {
        return;
    }
    if action == ThreadUiActionV1::Comments {
        native_v1::open_comments_v2(app);
        return;
    }
    if action == ThreadUiActionV1::CurrentCode {
        native_v1::open_current_code_v4(app);
        return;
    }
    if let ThreadUiActionV1::Publication(action) = action {
        let admitted = app
            .scoped_threads_v1
            .as_ref()
            .is_some_and(|view| publication_available(view, action));
        if admitted {
            native_v1::open_publication(app, action);
        } else {
            app.status = "Observed thread action unavailable • current permission/state not inferred • refresh explicitly".into();
        }
        return;
    }
    let Some(view) = &mut app.scoped_threads_v1 else {
        return;
    };
    if view.prepared.is_none() {
        return;
    }
    match action {
        ThreadUiActionV1::Move(delta) => {
            view.selected = view
                .selected
                .saturating_add_signed(delta as isize)
                .min(view.indices().len().saturating_sub(1));
            view.scroll = 0;
        }
        ThreadUiActionV1::Select(index) => {
            if index < view.indices().len() {
                view.selected = index;
                view.scroll = 0;
            }
        }
        ThreadUiActionV1::Scroll(delta) => {
            view.scroll = view.scroll.saturating_add_signed(delta as isize)
        }
        ThreadUiActionV1::Horizontal(delta) => {
            view.column = view
                .column
                .saturating_add_signed(delta as isize)
                .min(FIELD_CAP)
        }
        ThreadUiActionV1::Home => {
            view.scroll = 0;
            view.column = 0;
        }
        ThreadUiActionV1::Toggle => {
            view.expanded = !view.expanded;
            view.scroll = 0;
        }
        ThreadUiActionV1::Filter => {
            view.file_only = view.intent.selected_path_v4().is_some() && !view.file_only;
            view.selected = 0;
            view.scroll = 0;
        }
        ThreadUiActionV1::Source => {
            view.source = !view.source;
            view.scroll = 0;
            view.column = 0;
        }
        ThreadUiActionV1::Back | ThreadUiActionV1::Quit => unreachable!("handled above"),
        ThreadUiActionV1::Publication(_) => unreachable!("handled above"),
        ThreadUiActionV1::Comments => unreachable!("handled above"),
        ThreadUiActionV1::CurrentCode => unreachable!("handled above"),
    }
    view.hits.clear();
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.overlay != Some(Overlay::PrThreadsV1) {
        return;
    }
    if app
        .scoped_threads_v1
        .as_ref()
        .is_some_and(|view| view.comments_v2.is_some())
    {
        comments_v2::ui::draw(frame, app, area);
        return;
    }
    let theme = app.theme();
    let valid = app
        .scoped_threads_v1
        .as_ref()
        .is_some_and(|view| view.intent.owns(app));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(" PR review threads • read only ")
            .border_style(theme.style(SemanticToken::Focus)),
        area,
    );
    if !valid {
        return;
    }
    let busy = app.loading[View::Inbox.index()]
        || app.scoped_threads_work_v1.is_some()
        || app.scoped_threads_owner_v1.is_some();
    let avatars = &app.avatar_states;
    let view = app.scoped_threads_v1.as_mut().unwrap();
    view.hits.clear();
    let inside = area.inner(ratatui::layout::Margin::new(1, 1));
    if inside.width == 0 || inside.height == 0 {
        return;
    }
    let mut y = inside.y;
    // Bounded labels, wrapping and current-layout hit geometry, including80cols.
    let controls = [
        (
            if view.intent.selected_path_v4().is_some() {
                "[a All/file]"
            } else {
                "[a All PR only]"
            },
            ThreadUiActionV1::Filter,
        ),
        ("[Enter Expand]", ThreadUiActionV1::Toggle),
        (
            "[p Reply]",
            ThreadUiActionV1::Publication(publication_v1::ThreadPublicationActionV1::Reply),
        ),
        (
            "[r Resolve]",
            ThreadUiActionV1::Publication(publication_v1::ThreadPublicationActionV1::Resolve),
        ),
        (
            "[u Reopen]",
            ThreadUiActionV1::Publication(publication_v1::ThreadPublicationActionV1::Reopen),
        ),
        ("[i Source]", ThreadUiActionV1::Source),
        ("[c Comments]", ThreadUiActionV1::Comments),
        ("[g Current code]", ThreadUiActionV1::CurrentCode),
        ("[PgUp]", ThreadUiActionV1::Scroll(-10)),
        ("[PgDn]", ThreadUiActionV1::Scroll(10)),
        ("[Left]", ThreadUiActionV1::Horizontal(-32)),
        ("[Right]", ThreadUiActionV1::Horizontal(32)),
        ("[Home]", ThreadUiActionV1::Home),
        ("[Esc Back]", ThreadUiActionV1::Back),
        ("[q Quit]", ThreadUiActionV1::Quit),
    ];
    let mut x = inside.x;
    for (label, action) in controls {
        let width = label.len() as u16;
        if x.saturating_add(width) > inside.right() {
            x = inside.x;
            y += 1;
        }
        if y >= inside.bottom() {
            break;
        }
        let rect = Rect::new(x, y, width.min(inside.width), 1);
        let enabled = matches!(action, ThreadUiActionV1::Back | ThreadUiActionV1::Quit)
            || (!busy
                && match action {
                    ThreadUiActionV1::Publication(action) => publication_available(view, action),
                    _ => true,
                });
        frame.render_widget(
            Paragraph::new(label).style(theme.style(if enabled {
                SemanticToken::Reference
            } else {
                SemanticToken::Muted
            })),
            rect,
        );
        view.hits.push((rect, action));
        x += width + 1;
    }
    y += 1;
    if y >= inside.bottom() {
        return;
    }
    if let Some(notice) = &view.code_notice_v4 {
        frame.render_widget(
            Paragraph::new(notice.as_str()).style(theme.style(SemanticToken::Pending)),
            Rect::new(inside.x, y, inside.width, 1),
        );
        y += 1;
    }
    let Some(prepared) = &view.prepared else {
        frame.render_widget(
            Paragraph::new(view.message).style(theme.style(SemanticToken::Pending)),
            Rect::new(inside.x, y, inside.width, inside.bottom() - y),
        );
        return;
    };
    for text in prepared.header.iter().take(2) {
        if y >= inside.bottom() {
            return;
        }
        frame.render_widget(
            Paragraph::new(text.as_str()).style(theme.style(SemanticToken::Pending)),
            Rect::new(inside.x, y, inside.width, 1),
        );
        y += 1;
    }
    let indices = if view.file_only {
        &prepared.selected_file
    } else {
        &prepared.all
    };
    if y < inside.bottom() {
        frame.render_widget(
            Paragraph::new(format!(
                "Filter: {} • {} observed thread(s)",
                if view.file_only {
                    "SELECTED FILE"
                } else {
                    "ALL PR"
                },
                indices.len()
            ))
            .style(theme.style(SemanticToken::Muted)),
            Rect::new(inside.x, y, inside.width, 1),
        );
        y += 1;
    }
    view.selected = view.selected.min(indices.len().saturating_sub(1));
    if !view.source {
        let start = view.selected.saturating_sub(1);
        for (offset, index) in indices.iter().enumerate().skip(start).take(3) {
            if y >= inside.bottom() {
                return;
            }
            let rect = Rect::new(inside.x, y, inside.width, 1);
            let prefix = if offset == view.selected { "> " } else { "  " };
            frame.render_widget(
                Paragraph::new(ratatui::text::Line::from(vec![
                    ratatui::text::Span::raw(prefix),
                    ratatui::text::Span::raw(prepared.groups[*index].summary.as_str()),
                ]))
                .style(if offset == view.selected {
                    theme.selected(SemanticToken::Reference)
                } else {
                    theme.style(SemanticToken::Reference)
                }),
                rect,
            );
            view.hits.push((rect, ThreadUiActionV1::Select(offset)));
            y += 1;
        }
    }
    let lines: &[ThreadLineV1] = if view.source {
        &prepared.source_lines
    } else if view.expanded {
        indices
            .get(view.selected)
            .map_or(&[], |i| prepared.groups[*i].lines.as_slice())
    } else {
        &[]
    };
    let height = inside.bottom().saturating_sub(y) as usize;
    view.scroll = view.scroll.min(lines.len().saturating_sub(height));
    if lines.is_empty() && y < inside.bottom() {
        let message = if indices.is_empty() {
            "No matching observed threads; partial coverage does not establish absence. Choose a All/file."
        } else {
            "Collapsed • Enter Expand; no comment body rendered"
        };
        frame.render_widget(
            Paragraph::new(message).style(theme.style(SemanticToken::Muted)),
            Rect::new(inside.x, y, inside.width, 1),
        );
    }
    for line in lines.iter().skip(view.scroll).take(height) {
        let mut rect = Rect::new(inside.x, y, inside.width, 1);
        if let Some(identity) = &line.actor {
            let width = rect.width.min(4);
            let image = match avatars.get(identity) {
                Some(crate::identity::AvatarState::Ready(image)) => Some(image.as_ref()),
                _ => None,
            };
            frame.render_widget(
                crate::identity::AvatarWidget {
                    identity,
                    image,
                    theme,
                },
                Rect::new(rect.x, rect.y, width, 1),
            );
            rect.x += width;
            rect.width -= width;
        }
        frame.render_widget(
            Paragraph::new(line.text.as_str())
                .scroll((0, view.column as u16))
                .style(theme.style(line.role)),
            rect,
        );
        y += 1;
    }
}
