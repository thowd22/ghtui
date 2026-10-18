//! Exact-purpose metadata on the existing original Tree source and worker.
//! Width and fields are presentation only; never a fresh Git/permission proof.
use crate::{
    app::{Action, App, HitRegion, View},
    repository_tree_jump_v1::OriginalTreeFocusV1,
    repository_tree_queue_notice_v2::OriginalTreeQueueFailureV2,
    repository_tree_viewport_v2::{
        PreparedTreeFolderDetailV2, TreeDetailCacheV2, TreeDetailFieldV2, TreeFolderIntentV2,
        TreeViewportActionV2,
    },
    repository_tree_worker_v1::OriginalTreeWorkV1,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::sync::Arc;

pub struct TreeViewportNativeV2 {
    pub width: u16,
    pub field: TreeDetailFieldV2,
    pub cache: TreeDetailCacheV2,
    pub failure: Option<Arc<OriginalTreeQueueFailureV2>>,
    pub pending: Option<Arc<OriginalTreeWorkV1>>,
    pub cancel_metadata: Option<Arc<OriginalTreeWorkV1>>,
    pub folder_intent: Option<Arc<TreeFolderIntentV2>>,
    pub folder: Option<Arc<PreparedTreeFolderDetailV2>>,
    pub folder_scroll: usize,
    pub error: String,
    pub detail_area: Rect,
    pub query_area: Rect,
    pub list_area: Rect,
    pub focus_restore: Option<Action>,
}
impl Default for TreeViewportNativeV2 {
    fn default() -> Self {
        Self {
            width: 120,
            field: Default::default(),
            cache: Default::default(),
            failure: None,
            pending: None,
            cancel_metadata: None,
            folder_intent: None,
            folder: None,
            folder_scroll: 0,
            error: String::new(),
            detail_area: Rect::default(),
            query_area: Rect::default(),
            list_area: Rect::default(),
            focus_restore: None,
        }
    }
}
pub fn inner_width(width: u16) -> u16 {
    width.saturating_sub(2).min(400)
}
pub fn member_width(width: u16) -> u16 {
    let inner = inner_width(width);
    if inner >= 100 {
        inner - inner.saturating_sub(78).min(40)
    } else {
        inner
    }
}

pub fn retire(app: &mut App) {
    let state = &mut app.original_tree_viewport_v2;
    state.cancel_metadata = state.pending.take().or(state.cancel_metadata.take());
    state.cache.clear();
    state.failure = None;
    state.folder_intent = None;
    state.folder = None;
    state.folder_scroll = 0;
    state.error.clear();
    state.focus_restore = None;
    state.detail_area = Rect::default();
    app.hits.clear();
}
pub fn synchronize(app: &mut App) {
    if app
        .original_tree_viewport_v2
        .folder_intent
        .as_ref()
        .is_some_and(|intent| {
            app.view != View::Repository
                || app.remote_repository.is_some()
                || app
                    .workspace
                    .as_ref()
                    .is_none_or(|workspace| !intent.is_current(workspace))
        })
    {
        retire(app);
    }
}
pub fn queue_member(app: &mut App) {
    let Some(jump) = app.original_tree_jump_v1.as_ref() else {
        return;
    };
    if jump.pending().is_some()
        || app
            .original_tree_work_v1
            .as_ref()
            .is_some_and(|work| !work.metadata_v2())
        || (app.loading[View::Repository.index()]
            && app.original_tree_viewport_v2.pending.is_none())
    {
        return;
    }
    let state = &mut app.original_tree_viewport_v2;
    let old = state.pending.clone();
    match state
        .cache
        .request(jump, member_width(state.width), state.field)
    {
        Ok(Some(request)) => {
            let work = Arc::new(OriginalTreeWorkV1::MemberDetail(request));
            state.pending = Some(work.clone());
            state.error.clear();
            app.original_tree_work_v1 = Some(work);
        }
        Ok(None) if state.cache.pending().is_none() => {
            state.pending = None;
            state.cancel_metadata = old;
        }
        Ok(None) => {}
        Err(reason) => {
            state.cancel_metadata = state.pending.take();
            state.cache.clear();
            state.error = reason;
        }
    }
    app.hits.clear();
}
fn queue_folder(app: &mut App) {
    let state = &mut app.original_tree_viewport_v2;
    let Some(intent) = state.folder_intent.as_ref() else {
        return;
    };
    let work = Arc::new(OriginalTreeWorkV1::FolderDetail {
        intent: intent.clone(),
        width: inner_width(state.width),
        field: state.field,
    });
    state.pending = Some(work.clone());
    state.folder = None;
    state.error.clear();
    app.original_tree_work_v1 = Some(work);
    app.hits.clear();
}
pub fn resize(app: &mut App, width: u16) {
    app.original_tree_viewport_v2.focus_restore =
        app.original_tree_jump_v1.as_ref().and_then(|jump| {
            if jump.focus == OriginalTreeFocusV1::List {
                jump.frame()
                    .and_then(|frame| frame.ticket(jump.selected))
                    .map(|ticket| {
                        Action::OriginalTreeV1(
                            crate::repository_tree_jump_v1::OriginalTreeJumpActionV1::Activate(
                                ticket,
                            ),
                        )
                    })
            } else {
                app.hits.get(app.focus).map(|hit| hit.action.clone())
            }
        });
    app.original_tree_viewport_v2.width = width;
    app.original_tree_layout_v1.clear();
    app.hits.clear();
    if app.original_tree_viewport_v2.folder_intent.is_some() {
        queue_folder(app);
    } else {
        queue_member(app);
    }
}
pub fn work_current(app: &App, work: &Arc<OriginalTreeWorkV1>) -> bool {
    if !work.metadata_v2() {
        return true;
    }
    let state = &app.original_tree_viewport_v2;
    if state
        .pending
        .as_ref()
        .is_none_or(|pending| !Arc::ptr_eq(pending, work))
    {
        return false;
    }
    match work.as_ref() {
        OriginalTreeWorkV1::MemberDetail(request) => {
            state.folder_intent.is_none()
                && state
                    .cache
                    .pending()
                    .is_some_and(|pending| Arc::ptr_eq(pending, request))
                && request.width() == member_width(state.width)
                && request.field() == state.field
                && app
                    .original_tree_jump_v1
                    .as_ref()
                    .is_some_and(|jump| request.is_current(jump))
        }
        OriginalTreeWorkV1::FolderDetail {
            intent,
            width,
            field,
        } => {
            state
                .folder_intent
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, intent))
                && *width == inner_width(state.width)
                && *field == state.field
                && app
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| intent.is_current(workspace))
        }
        _ => false,
    }
}
pub fn dispatch(app: &mut App, action: TreeViewportActionV2) {
    synchronize(app);
    match action {
        TreeViewportActionV2::Commands => crate::navigation_ui::open_palette(app),
        TreeViewportActionV2::DetailField(field) => {
            app.original_tree_viewport_v2.focus_restore = Some(Action::OriginalTreeViewportV2(
                TreeViewportActionV2::DetailField(field),
            ));
            app.original_tree_viewport_v2.field = field;
            app.original_tree_viewport_v2.folder_scroll = 0;
            if let Some(jump) = &mut app.original_tree_jump_v1 {
                jump.detail_scroll = 0;
                jump.focus = OriginalTreeFocusV1::Detail;
            }
            if app.original_tree_viewport_v2.folder_intent.is_some() {
                queue_folder(app);
            } else {
                queue_member(app);
            }
        }
        TreeViewportActionV2::DetailScroll(delta) => {
            let state = &mut app.original_tree_viewport_v2;
            if let Some(folder) = &state.folder {
                state.folder_scroll = state
                    .folder_scroll
                    .saturating_add_signed(delta as isize)
                    .min(folder.rows().len().saturating_sub(1));
            } else if let Some(jump) = &mut app.original_tree_jump_v1 {
                let max = state
                    .cache
                    .ready(jump, member_width(state.width), state.field)
                    .map_or(0, |detail| detail.rows().len().saturating_sub(1));
                jump.detail_scroll = jump
                    .detail_scroll
                    .saturating_add_signed(delta as isize)
                    .min(max);
                jump.focus = OriginalTreeFocusV1::Detail;
            }
            app.hits.clear();
        }
        TreeViewportActionV2::Retry(failure) => {
            let result = app
                .workspace
                .as_ref()
                .zip(app.original_tree_jump_v1.as_mut())
                .ok_or_else(|| "Original failed picker closed".to_owned())
                .and_then(|(workspace, jump)| failure.retry(workspace, jump));
            match result {
                Ok(work) => {
                    retire(app);
                    app.original_tree_work_v1 = Some(work);
                }
                Err(reason) => app.status = reason,
            }
        }
        TreeViewportActionV2::InspectFolder(intent) => {
            if app
                .workspace
                .as_ref()
                .is_none_or(|workspace| !intent.is_current(workspace))
                || app.loading[View::Repository.index()]
            {
                app.dispatch(Action::Explain(
                    "Original folder changed or read pending; no old detail reopened".into(),
                ));
                return;
            }
            retire(app);
            app.original_tree_viewport_v2.folder_intent = Some(intent);
            app.focus = 0;
            queue_folder(app);
        }
        TreeViewportActionV2::CloseFolder => {
            retire(app);
            app.status = "Original folder detail closed • no repository changes".into();
        }
    }
}
pub fn folder_event(app: &mut App, event: Event) -> bool {
    synchronize(app);
    if app.overlay.is_some() || app.original_tree_viewport_v2.folder_intent.is_none() {
        return false;
    }
    let action = match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                app.dispatch(Action::Quit);
                return true;
            }
            match key.code {
                KeyCode::Esc => Some(Action::OriginalTreeViewportV2(
                    TreeViewportActionV2::CloseFolder,
                )),
                KeyCode::Up | KeyCode::Char('k') => Some(Action::OriginalTreeViewportV2(
                    TreeViewportActionV2::DetailScroll(-1),
                )),
                KeyCode::Down | KeyCode::Char('j') => Some(Action::OriginalTreeViewportV2(
                    TreeViewportActionV2::DetailScroll(1),
                )),
                KeyCode::PageUp => Some(Action::OriginalTreeViewportV2(
                    TreeViewportActionV2::DetailScroll(-8),
                )),
                KeyCode::PageDown => Some(Action::OriginalTreeViewportV2(
                    TreeViewportActionV2::DetailScroll(8),
                )),
                KeyCode::Home => Some(Action::OriginalTreeViewportV2(
                    TreeViewportActionV2::DetailScroll(i32::MIN),
                )),
                KeyCode::End => Some(Action::OriginalTreeViewportV2(
                    TreeViewportActionV2::DetailScroll(i32::MAX),
                )),
                KeyCode::Left | KeyCode::Right => Some(Action::OriginalTreeViewportV2(
                    TreeViewportActionV2::DetailField(
                        app.original_tree_viewport_v2
                            .field
                            .next(key.code == KeyCode::Left),
                    ),
                )),
                KeyCode::Tab => {
                    app.focus = (app.focus + 1) % app.hits.len().max(1);
                    None
                }
                KeyCode::BackTab => {
                    app.focus =
                        (app.focus + app.hits.len().saturating_sub(1)) % app.hits.len().max(1);
                    None
                }
                KeyCode::Enter => app.hits.get(app.focus).map(|hit| hit.action.clone()),
                _ => None,
            }
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => app
                .hits
                .iter()
                .enumerate()
                .rev()
                .find(|(_, hit)| hit.rect.contains((mouse.column, mouse.row).into()))
                .map(|(index, hit)| {
                    app.focus = index;
                    hit.action.clone()
                }),
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => Some(
                Action::OriginalTreeViewportV2(TreeViewportActionV2::DetailScroll(
                    if mouse.kind == MouseEventKind::ScrollDown {
                        1
                    } else {
                        -1
                    },
                )),
            ),
            _ => None,
        },
        _ => None,
    };
    if let Some(action) = action {
        app.dispatch(action);
    }
    true
}
pub fn folder_draw(frame: &mut Frame, app: &mut App, area: Rect) -> bool {
    synchronize(app);
    if app.overlay.is_some() || app.original_tree_viewport_v2.folder_intent.is_none() {
        return false;
    }
    let theme = app.theme();
    app.hits.clear();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title("Saved folder • original detail")
            .style(theme.base()),
        area,
    );
    let inner = area.inner(ratatui::layout::Margin::new(1, 1));
    if inner.height < 4 || inner.width < 20 {
        if !inner.is_empty() {
            let rect = Rect::new(
                inner.right().saturating_sub(11).max(inner.x),
                inner.bottom() - 1,
                inner.width.min(11),
                1,
            );
            frame.render_widget(
                Paragraph::new("[Esc Close]")
                    .style(theme.style(crate::theme::SemanticToken::Reference)),
                rect,
            );
            app.hits.push(HitRegion {
                rect,
                action: Action::OriginalTreeViewportV2(TreeViewportActionV2::CloseFolder),
            });
        }
        return true;
    }
    let mut x = inner.x;
    for field in TreeDetailFieldV2::ALL {
        let label = format!("[{}]", field.label());
        let rect = Rect::new(x, inner.y, label.len() as u16, 1);
        if rect.right() > inner.right() {
            break;
        }
        frame.render_widget(
            Paragraph::new(label).style(crate::repository_tree_viewport_v2::focused_style_v2(
                theme,
                crate::theme::SemanticToken::Reference,
                app.original_tree_viewport_v2.field == field,
                app.focus == app.hits.len(),
            )),
            rect,
        );
        app.hits.push(HitRegion {
            rect,
            action: Action::OriginalTreeViewportV2(TreeViewportActionV2::DetailField(field)),
        });
        x = rect.right() + 1;
    }
    frame.render_widget(
        Paragraph::new(format!(
            "Detail: {}",
            app.original_tree_viewport_v2.field.label()
        )),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
    let body = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(3),
    );
    if let Some(folder) = &app.original_tree_viewport_v2.folder {
        for (offset, row) in folder
            .rows()
            .iter()
            .skip(app.original_tree_viewport_v2.folder_scroll)
            .take(usize::from(body.height))
            .enumerate()
        {
            frame.render_widget(
                Paragraph::new(row.as_str())
                    .style(theme.style(crate::theme::SemanticToken::Reference)),
                Rect::new(body.x, body.y + offset as u16, body.width, 1),
            );
        }
    } else {
        let text = if app.original_tree_viewport_v2.error.is_empty() {
            "Original folder metadata pending • no old detail revived"
        } else {
            app.original_tree_viewport_v2.error.as_str()
        };
        frame.render_widget(
            Paragraph::new(text).wrap(ratatui::widgets::Wrap { trim: false }),
            body,
        );
    }
    let mut x = inner.x;
    for (label, action) in [
        ("[− Detail]", TreeViewportActionV2::DetailScroll(-1)),
        ("[+ Detail]", TreeViewportActionV2::DetailScroll(1)),
        ("[Esc Close]", TreeViewportActionV2::CloseFolder),
    ] {
        let width = crate::editor::display_width(label) as u16;
        let rect = Rect::new(x, inner.bottom() - 1, width, 1);
        if rect.right() > inner.right() {
            break;
        }
        frame.render_widget(
            Paragraph::new(label).style(crate::repository_tree_viewport_v2::focused_style_v2(
                theme,
                crate::theme::SemanticToken::Reference,
                false,
                app.focus == app.hits.len(),
            )),
            rect,
        );
        app.hits.push(HitRegion {
            rect,
            action: Action::OriginalTreeViewportV2(action),
        });
        x = rect.right() + 1;
    }
    true
}

pub fn reject(
    app: &mut App,
    work: &Arc<OriginalTreeWorkV1>,
    owner: Option<&Arc<crate::repository_tree_worker_v1::OriginalTreeWorkerOwnerV1>>,
    reason: &str,
) {
    if let Some(workspace) = app.workspace.as_ref() {
        if let Some(jump) = app.original_tree_jump_v1.as_mut() {
            if let Some(failure) =
                OriginalTreeQueueFailureV2::reject(workspace, jump, work, owner, reason)
            {
                app.original_tree_viewport_v2.failure = Some(failure);
                return;
            }
        }
        if work.metadata_v2() && work_current(app, work) {
            if let OriginalTreeWorkV1::MemberDetail(request) = work.as_ref() {
                app.original_tree_viewport_v2
                    .cache
                    .reject_pending_v2(request);
            }
            app.original_tree_viewport_v2.pending = None;
            app.original_tree_viewport_v2.error = crate::app::safe_text(reason);
            return;
        }
        if let Some(notice) = crate::repository_tree_queue_notice_v2::reject_navigation_v2(
            workspace,
            app.original_tree_jump_v1.as_mut(),
            work,
            owner,
            reason,
        ) {
            app.status = notice.clone();
            if app.original_tree_jump_v1.is_none() {
                app.dispatch(Action::Explain(notice));
            }
        }
    }
}
