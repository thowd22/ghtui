//! Owned volatile controls. No Form/draft, provider/SQL, regex or log clone in
//! input/draw. All displayed controls emit the same typed key/mouse action.
use super::*;
#[path = "scoped_pr_failed_step_navigation_v8.rs"]
mod failed_steps_v8;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrCheckContinueUiV2 {
    Mode(LoadedLogSearchModeV2),
    SubmitSearch,
    Close,
    Quit,
    Focus(usize),
    EditorClick(usize, bool),
    NextMatch,
    ClearMatches,
    Details,
    FailedStep(bool),
    Scroll(i32),
}
pub struct PrCheckLogEditorV2 {
    route: Arc<PrChecksRoute>,
    capture: Arc<PrCheckLogsObservation>,
    buffer: crate::editor::TextBuffer,
    mode: LoadedLogSearchModeV2,
    focus: usize,
    hits: Vec<(Rect, PrCheckContinueUiV2)>,
    field: Rect,
}
pub struct AnnotationDetailV2 {
    route: Arc<PrChecksRoute>,
    read: Arc<PrCheckAnnotationsReadV2>,
    index: usize,
    top: usize,
    focus: usize,
    hits: Vec<(Rect, PrCheckContinueUiV2)>,
}
impl PrCheckLogEditorV2 {
    #[cfg(test)]
    pub(crate) fn query(&self) -> &str {
        &self.buffer.text
    }
    fn owns(&self, app: &App) -> bool {
        owns_origin(app, &self.route.origin)
            && app
                .pr_checks_route
                .as_ref()
                .is_some_and(|r| Arc::ptr_eq(r, &self.route))
            && self
                .route
                .loaded_capture_v2
                .as_ref()
                .is_some_and(|c| Arc::ptr_eq(c, &self.capture))
    }
    fn insert(&mut self, value: &str) {
        // Bound BEFORE normalization/copy, then use existing grapheme editor.
        if value.len() > 1024 || value.contains(['\n', '\r', '\0']) {
            return;
        }
        let removed = self.buffer.selection().map_or(0, |r| r.len());
        if self
            .buffer
            .text
            .len()
            .saturating_sub(removed)
            .checked_add(value.len())
            .is_some_and(|n| n <= 1024)
        {
            let _ = self.buffer.insert(value);
        }
    }
}
impl AnnotationDetailV2 {
    fn owns(&self, app: &App) -> bool {
        owns_origin(app, &self.route.origin)
            && app
                .pr_checks_route
                .as_ref()
                .is_some_and(|r| Arc::ptr_eq(r, &self.route))
            && self
                .route
                .annotations_v2
                .as_ref()
                .is_some_and(|r| Arc::ptr_eq(r, &self.read))
    }
}
pub fn open(app: &mut App, route: Arc<PrChecksRoute>) {
    let Some(capture) = route.loaded_capture_v2.clone() else {
        return;
    };
    app.pr_check_log_editor_v2 = Some(PrCheckLogEditorV2 {
        route,
        capture,
        buffer: crate::editor::TextBuffer::default(),
        mode: LoadedLogSearchModeV2::ExactText,
        focus: 0,
        hits: vec![],
        field: Rect::default(),
    });
    app.pr_check_annotations_detail_v2 = None;
    app.overlay = Some(crate::app::Overlay::PrCheckContinuationV2);
    app.hits.clear();
}
pub fn dispatch(app: &mut App, action: PrCheckContinueUiV2) {
    if app.scoped_actions_v1.is_some() {
        return;
    }
    use PrCheckContinueUiV2::*;
    if action == Quit {
        retire(app);
        app.dispatch(Action::Quit);
        return;
    }
    if action == Close {
        app.pr_check_log_editor_v2 = None;
        app.pr_check_annotations_detail_v2 = None;
        app.overlay = None;
        app.hits.clear();
        return;
    }
    if action == NextMatch {
        next_match(app, true);
        return;
    }
    if let FailedStep(next) = action {
        failed_steps_v8::navigate(app, next);
        return;
    }
    if action == ClearMatches {
        corrective_v3::retire_pending_search(app);
        clear_matches(app);
        app.status = "Loaded-log matches cleared • no external effects".into();
        return;
    }
    if action == Details {
        let Some(route) = app
            .pr_checks_route
            .clone()
            .filter(|r| owns_origin(app, &r.origin))
        else {
            return;
        };
        let Some(read) = route.annotations_v2.clone() else {
            return;
        };
        let Some(index) = app
            .actions
            .as_ref()
            .and_then(|w| w.selected.checked_sub(ANNOTATION_ROW_BASE_V2))
            .filter(|i| *i < read.rows().len())
        else {
            return;
        };
        app.pr_check_annotations_detail_v2 = Some(AnnotationDetailV2 {
            route,
            read,
            index,
            top: 0,
            focus: 0,
            hits: vec![],
        });
        app.pr_check_log_editor_v2 = None;
        app.overlay = Some(crate::app::Overlay::PrCheckContinuationV2);
        app.hits.clear();
        return;
    }
    if let Some(mut editor) = app.pr_check_log_editor_v2.take() {
        if !editor.owns(app) {
            app.overlay = None;
            return;
        }
        match action {
            Mode(mode) => {
                editor.mode = mode;
                editor.focus = if mode == LoadedLogSearchModeV2::ExactText {
                    1
                } else {
                    2
                };
            }
            Focus(index) => editor.focus = index.min(5),
            EditorClick(column, extend) => {
                editor.focus = 0;
                editor.buffer.click(0, column, extend);
            }
            SubmitSearch => {
                app.overlay = None;
                queue_search(app, editor.route, editor.buffer.text, editor.mode);
                return;
            }
            _ => {}
        }
        app.pr_check_log_editor_v2 = Some(editor);
    } else if let Some(mut detail) = app.pr_check_annotations_detail_v2.take() {
        if !detail.owns(app) {
            app.overlay = None;
            return;
        }
        match action {
            Scroll(n) => detail.top = detail.top.saturating_add_signed(n as isize).min(4096),
            Focus(i) => detail.focus = i.min(1),
            _ => {}
        }
        app.pr_check_annotations_detail_v2 = Some(detail);
    }
}
pub fn clear_matches(app: &mut App) {
    if app.scoped_actions_v1.is_some() {
        return;
    }
    if let Some(matches) = app.pr_check_log_matches_v2.take() {
        if app
            .pr_checks_route
            .as_ref()
            .and_then(|r| r.loaded_capture_v2.as_ref())
            .is_some_and(|capture| matches.belongs_to(capture))
        {
            if let Some(workspace) = app.actions.as_mut().filter(|w| w.view == ActionsView::Logs) {
                for &index in matches.indices() {
                    if let Some(row) = workspace.rows.get_mut(index) {
                        row.role = crate::theme::SemanticToken::Normal;
                    }
                }
            }
        }
    }
}
pub fn next_match(app: &mut App, next: bool) {
    if app.scoped_actions_v1.is_some() {
        return;
    }
    let Some(route) = app
        .pr_checks_route
        .as_ref()
        .filter(|r| owns_origin(app, &r.origin))
    else {
        return;
    };
    let Some(matches) = app.pr_check_log_matches_v2.as_ref() else {
        return;
    };
    let Some(workspace) = app.actions.as_mut() else {
        return;
    };
    if route
        .loaded_capture_v2
        .as_ref()
        .is_none_or(|capture| !matches.belongs_to(capture))
        || workspace.view != ActionsView::Logs
    {
        return;
    }
    let indices = matches.indices();
    if indices.is_empty() {
        return;
    }
    let current = workspace.selected;
    let target = if next {
        indices
            .iter()
            .copied()
            .find(|i| *i > current)
            .unwrap_or(indices[0])
    } else {
        indices[0]
    };
    workspace.selected = target;
    workspace.top = target.saturating_sub(app.actions_visible_rows / 2);
    app.actions_content_focused = true;
}
pub fn event(app: &mut App, event: Event) -> bool {
    if app.overlay != Some(crate::app::Overlay::PrCheckContinuationV2) {
        return false;
    }
    let owned = app
        .pr_check_log_editor_v2
        .as_ref()
        .is_some_and(|e| e.owns(app))
        || app
            .pr_check_annotations_detail_v2
            .as_ref()
            .is_some_and(|e| e.owns(app));
    if !owned {
        dispatch(app, PrCheckContinueUiV2::Close);
        return true;
    }
    use PrCheckContinueUiV2::*;
    let mut action = None;
    match event {
        Event::Resize(_, _) => {
            if let Some(e) = app.pr_check_log_editor_v2.as_mut() {
                e.hits.clear();
                e.field = Rect::default();
            }
            if let Some(d) = app.pr_check_annotations_detail_v2.as_mut() {
                d.hits.clear();
            }
        }
        Event::Paste(text) => {
            if let Some(e) = app.pr_check_log_editor_v2.as_mut().filter(|e| e.focus == 0) {
                e.insert(&text);
            }
        }
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                action = Some(Quit)
            } else if key.code == KeyCode::Esc {
                action = Some(Close)
            } else if let Some(e) = app.pr_check_log_editor_v2.as_mut() {
                let extend = key.modifiers.contains(KeyModifiers::SHIFT);
                match key.code {
                    KeyCode::Tab | KeyCode::BackTab => {
                        e.focus = (e.focus + if key.code == KeyCode::Tab { 1 } else { 5 }) % 6
                    }
                    KeyCode::Enter => {
                        action = Some(match e.focus {
                            0 | 3 => SubmitSearch,
                            1 => Mode(LoadedLogSearchModeV2::ExactText),
                            2 => Mode(LoadedLogSearchModeV2::Regex),
                            4 => Close,
                            _ => Quit,
                        })
                    }
                    KeyCode::Left if e.focus == 0 => e.buffer.horizontal(false, extend),
                    KeyCode::Right if e.focus == 0 => e.buffer.horizontal(true, extend),
                    KeyCode::Home if e.focus == 0 => e.buffer.line_edge(false, false, extend),
                    KeyCode::End if e.focus == 0 => e.buffer.line_edge(true, false, extend),
                    KeyCode::Backspace if e.focus == 0 => e.buffer.delete_backward(),
                    KeyCode::Delete if e.focus == 0 => e.buffer.delete_forward(),
                    KeyCode::Char('a')
                        if e.focus == 0 && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        e.buffer.select_all()
                    }
                    KeyCode::Char(ch)
                        if e.focus == 0
                            && !key
                                .modifiers
                                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                    {
                        e.insert(&ch.to_string())
                    }
                    _ => {}
                }
            } else if let Some(d) = app.pr_check_annotations_detail_v2.as_mut() {
                action = match key.code {
                    KeyCode::Tab | KeyCode::BackTab => {
                        d.focus = 1 - d.focus;
                        None
                    }
                    KeyCode::Enter => Some(if d.focus == 0 { Close } else { Quit }),
                    KeyCode::Char('q') => Some(Quit),
                    KeyCode::Up | KeyCode::Char('k') => Some(Scroll(-1)),
                    KeyCode::Down | KeyCode::Char('j') => Some(Scroll(1)),
                    KeyCode::PageUp => Some(Scroll(-10)),
                    KeyCode::PageDown => Some(Scroll(10)),
                    _ => None,
                };
            }
        }
        Event::Mouse(mouse) => {
            if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                if let Some(e) = app.pr_check_log_editor_v2.as_ref() {
                    action = if e.field.contains((mouse.column, mouse.row).into()) {
                        Some(EditorClick(
                            usize::from(mouse.column - e.field.x) + e.buffer.left_column,
                            mouse.modifiers.contains(KeyModifiers::SHIFT),
                        ))
                    } else {
                        e.hits
                            .iter()
                            .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
                            .map(|(_, a)| *a)
                    };
                } else if let Some(d) = app.pr_check_annotations_detail_v2.as_ref() {
                    action = d
                        .hits
                        .iter()
                        .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
                        .map(|(_, a)| *a);
                }
            } else if app.pr_check_annotations_detail_v2.is_some() {
                action = match mouse.kind {
                    MouseEventKind::ScrollDown => Some(Scroll(3)),
                    MouseEventKind::ScrollUp => Some(Scroll(-3)),
                    _ => None,
                };
            }
        }
        _ => {}
    }
    if let Some(action) = action {
        dispatch(app, action);
    }
    true // paste, resize, unknown keys and stale geometry NEVER leak underneath.
}
fn button(frame: &mut Frame, rect: Rect, text: &str, focused: bool, theme: crate::theme::Theme) {
    let mut style = theme.style(crate::theme::SemanticToken::Reference);
    if focused {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    frame.render_widget(Paragraph::new(text).style(style), rect);
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.overlay != Some(crate::app::Overlay::PrCheckContinuationV2) {
        return;
    }
    if !app
        .pr_check_log_editor_v2
        .as_ref()
        .is_some_and(|e| e.owns(app))
        && !app
            .pr_check_annotations_detail_v2
            .as_ref()
            .is_some_and(|d| d.owns(app))
    {
        dispatch(app, PrCheckContinueUiV2::Close);
        return;
    }
    let theme = app.theme();
    frame.render_widget(Clear, area);
    let inner = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );
    if area.height < 14 || area.width < 24 {
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .title(" Resize for private detail "),
            area,
        );
        let close = Rect::new(inner.x, inner.y, inner.width, 1.min(inner.height));
        let quit = Rect::new(
            inner.x,
            inner.bottom().saturating_sub(1),
            inner.width,
            1.min(inner.height),
        );
        button(frame, close, "[Esc Close]", true, theme);
        button(frame, quit, "[q Quit]", false, theme);
        if let Some(e) = app.pr_check_log_editor_v2.as_mut() {
            e.hits = vec![
                (close, PrCheckContinueUiV2::Close),
                (quit, PrCheckContinueUiV2::Quit),
            ];
            e.field = Rect::default();
        }
        if let Some(d) = app.pr_check_annotations_detail_v2.as_mut() {
            d.hits = vec![
                (close, PrCheckContinueUiV2::Close),
                (quit, PrCheckContinueUiV2::Quit),
            ];
        }
        return;
    }
    if let Some(e) = app.pr_check_log_editor_v2.as_mut() {
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .title(" Loaded-log search • captured snapshot only "),
            area,
        );
        e.hits.clear();
        let target = e.capture.captured_target_v2();
        let header = format!(
            "Run {} • original attempt {} • captured job {} • read as-of {} • no reload",
            target.run_id(),
            target.attempt(),
            target.job_id(),
            e.capture.observed_through()
        );
        frame.render_widget(
            Paragraph::new(header).wrap(Wrap { trim: false }),
            Rect::new(inner.x, inner.y, inner.width, 3.min(inner.height)),
        );
        e.field = Rect::new(inner.x, inner.y + 3, inner.width, 1);
        e.buffer
            .ensure_visible(usize::from(inner.width.saturating_sub(7)), 1);
        let shown = crate::editor::display_line(
            &e.buffer.text,
            e.buffer.left_column,
            usize::from(inner.width.saturating_sub(7)),
        );
        frame.render_widget(
            Paragraph::new(format!("Query: {shown}"))
                .style(theme.style(crate::theme::SemanticToken::Normal)),
            e.field,
        );
        e.field.x = e.field.x.saturating_add(7);
        e.field.width = e.field.width.saturating_sub(7);
        if e.focus == 0 {
            let (_, col) = e.buffer.position();
            frame.set_cursor_position((
                e.field
                    .x
                    .saturating_add(col.saturating_sub(e.buffer.left_column) as u16)
                    .min(e.field.right().saturating_sub(1)),
                e.field.y,
            ));
        }
        let rows = [
            (
                1,
                "[Exact text]",
                PrCheckContinueUiV2::Mode(LoadedLogSearchModeV2::ExactText),
            ),
            (
                2,
                "[Regex]",
                PrCheckContinueUiV2::Mode(LoadedLogSearchModeV2::Regex),
            ),
            (3, "[Search]", PrCheckContinueUiV2::SubmitSearch),
            (4, "[Esc Close]", PrCheckContinueUiV2::Close),
            (5, "[q Quit]", PrCheckContinueUiV2::Quit),
        ];
        for (i, (focus, label, action)) in rows.into_iter().enumerate() {
            let rect = Rect::new(inner.x, inner.y + 5 + i as u16, inner.width, 1);
            if rect.y < inner.bottom() {
                button(frame, rect, label, e.focus == focus, theme);
                e.hits.push((rect, action));
            }
        }
        frame.render_widget(
            Paragraph::new(format!(
                "Mode: {:?} • 1024-byte volatile query • next/clear are local",
                e.mode
            )),
            Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
        );
    } else if let Some(d) = app.pr_check_annotations_detail_v2.as_mut() {
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .title(" Reported annotation • read only / not a canonical range "),
            area,
        );
        d.hits.clear();
        let Some(row) = d.read.rows().get(d.index) else {
            return;
        };
        let mut lines=vec![Line::from(format!("CheckRun {} • job {} • API-visible annotation ordinal {} (not server ID)",d.read.check_id().map_or_else(||"unknown".into(),|id|id.to_string()),d.read.target().map_or_else(||"unknown".into(),|t|t.job_id().to_string()),d.index+1)),
            Line::from(crate::providers::runner::sanitize_terminal(&format!("Reported path: {} • reported lines: {:?} • columns: {:?}",row.path().unwrap_or("unavailable"),row.lines(),row.columns()))),
            Line::from("Original tree/range capability unavailable in this slice; no file jump/inline publication"),
            Line::from("Author unavailable from annotations API; no actor inferred from workflow runner")];
        // Borrow pre-split bounded worker strings; no foreground body clone.
        lines.extend(
            row.prepared_display()
                .iter()
                .map(|line| Line::from(line.as_str())),
        );
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((d.top.min(u16::MAX as usize) as u16, 0)),
            Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.saturating_sub(2),
            ),
        );
        for (i, label, action) in [
            (0, "[Esc Close]", PrCheckContinueUiV2::Close),
            (1, "[q Quit]", PrCheckContinueUiV2::Quit),
        ] {
            let rect = Rect::new(
                inner.x + if i == 0 { 0 } else { inner.width / 2 },
                inner.bottom().saturating_sub(1),
                inner.width / 2,
                1,
            );
            button(frame, rect, label, d.focus == i, theme);
            d.hits.push((rect, action));
        }
    }
}
/// Reserve disjoint rows BEFORE shared viewer. No overlaid stale candidates
/// hit box or hidden duplicate button; return the remaining content rectangle.
pub fn draw_controls(frame: &mut Frame, app: &mut App, area: Rect) -> Rect {
    if app.scoped_actions_v1.is_some() {
        return area;
    }
    let watching = app
        .pr_check_watch_v2
        .as_ref()
        .is_some_and(|watch| watch.active());
    if let Some(workspace) = app.actions.as_mut().filter(|_| {
        app.pr_checks_route
            .as_ref()
            .is_some_and(|route| route.scoped_v1())
    }) {
        for control in &mut workspace.controls {
            if control.action == ActionsAction::Watch {
                control.label = if watching {
                    "[w Stop watch]"
                } else {
                    "[w Watch]"
                }
                .into();
            }
        }
    }
    let Some(route) = app
        .pr_checks_route
        .as_ref()
        .filter(|r| r.scoped_v1() && owns_origin(app, &r.origin))
    else {
        return area;
    };
    let Some(workspace) = app.actions.as_ref() else {
        return area;
    };
    let theme = app.theme();
    let mut controls = vec![];
    if workspace.view == ActionsView::Jobs {
        let enabled = failed_steps_v8::available(app);
        controls.extend([
            (
                "[> Next failed step]",
                PrCheckContinueUiV2::FailedStep(true),
                enabled,
            ),
            (
                "[< Previous failed step]",
                PrCheckContinueUiV2::FailedStep(false),
                enabled,
            ),
        ]);
    }
    if workspace.view == ActionsView::Annotations && route.annotations_v2().is_some() {
        controls.push(("[d Reported details]", PrCheckContinueUiV2::Details, true));
    }
    if workspace.view == ActionsView::Logs
        && app.pr_check_log_matches_v2.as_ref().is_some_and(|matches| {
            route
                .loaded_capture_v2
                .as_ref()
                .is_some_and(|capture| matches.belongs_to(capture))
        })
    {
        controls.extend([
            ("[n Next match]", PrCheckContinueUiV2::NextMatch, true),
            ("[x Clear matches]", PrCheckContinueUiV2::ClearMatches, true),
        ]);
    }
    if controls.is_empty() || area.height == 0 {
        return area;
    }
    let mut x = area.x;
    let mut y = area.y;
    let mut cleared_row = None;
    for (label, action, enabled) in controls {
        let width = (Span::raw(label).width() as u16).min(area.width);
        if x.saturating_add(width) > area.right() {
            x = area.x;
            y = y.saturating_add(1);
        }
        if y >= area.bottom() || width == 0 {
            break;
        }
        if cleared_row != Some(y) {
            frame.render_widget(Clear, Rect::new(area.x, y, area.width, 1));
            cleared_row = Some(y);
        }
        let rect = Rect::new(x, y, width, 1);
        let mut style = theme.style(if enabled {
            crate::theme::SemanticToken::Reference
        } else {
            crate::theme::SemanticToken::Muted
        });
        if !app.actions_content_focused && app.focus == app.hits.len() {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        frame.render_widget(Paragraph::new(label).style(style), rect);
        app.hits.push(crate::app::HitRegion {
            rect,
            action: Action::PrCheckContinueV2(action),
        });
        x = x.saturating_add(width + 1);
    }
    let reserved = y.saturating_sub(area.y).saturating_add(1).min(area.height);
    Rect::new(
        area.x,
        area.y.saturating_add(reserved),
        area.width,
        area.height.saturating_sub(reserved),
    )
}
