//! NEW undeclared native V3 action/preview. Pure bounded event/draw state;
//! existing original-local Background lane owns all actual Git work.
use crate::{
    app::{Action, App, HitRegion, Overlay, View},
    navigation_local_owner_v1::OriginalLocalNavigationOwnerV1,
    navigation_local_source_v1::OriginalSavedVisitV1,
    repository_graph_reread_v3::GraphRereadPreviewV3,
    runtime::PreparedView,
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

pub const TITLE: &str = "Re-read original Graph • first page";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphRereadActionV3 {
    Open,
    Confirm,
    Back,
    Focus(bool),
    Scroll(i32),
}

pub struct GraphRereadDialogV3 {
    preview: Arc<GraphRereadPreviewV3>,
    return_focus: (usize, bool),
    focus: usize,
    scroll: usize,
    body: Rect,
    hits: Vec<HitRegion>,
}
impl GraphRereadDialogV3 {
    pub fn hits(&self) -> &[HitRegion] {
        &self.hits
    }
}

/// Opaque prepared facet. It can only be emitted by the actual capsule factory,
/// and never authorizes another prepared kind or creates a registry/budget.
pub struct GraphRereadRetentionV3 {
    visit: Arc<OriginalSavedVisitV1>,
    operation: Arc<crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1>,
}
impl GraphRereadRetentionV3 {
    pub(crate) fn matches_operation_v3(
        &self,
        operation: &Arc<crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1>,
    ) -> bool {
        Arc::ptr_eq(&self.operation, operation)
    }
    pub(crate) fn matches_workspace(&self, workspace: &crate::workspace::Workspace) -> bool {
        workspace.graph_retention_visible_v2()
            && self.visit.graph_lease_v2().is_some_and(|lease| {
                workspace
                    .graph_state_v1
                    .as_ref()
                    .is_some_and(|state| lease.snapshot().same_source(&state.snapshot_v2()))
            })
    }
    pub(crate) fn accept(
        self,
        app: &mut App,
        workspace: &crate::workspace::Workspace,
    ) -> Result<(), String> {
        app.original_saved_visits
            .accept_graph_reread_v3(workspace, self.visit)
    }
}

pub fn refresh_available(app: &App) -> bool {
    app.view == View::Repository
        && app.remote_repository.is_none()
        && app
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.graph_retention_visible_v2())
}
fn owns_dialog(app: &App) -> bool {
    app.overlay == Some(Overlay::GraphRereadV3)
        && app.view == View::Repository
        && app.remote_repository.is_none()
        && app
            .graph_reread_dialog_v3
            .as_ref()
            .zip(app.workspace.as_ref())
            .is_some_and(|(dialog, workspace)| dialog.preview.matches_workspace(workspace))
}
pub fn retire(app: &mut App) {
    if let Some(dialog) = app.graph_reread_dialog_v3.take() {
        app.focus = dialog.return_focus.0;
        app.repository_content_focused = dialog.return_focus.1;
    }
    if app.overlay == Some(Overlay::GraphRereadV3) {
        app.overlay = None;
    }
    app.hits.clear();
}
pub fn dispatch(app: &mut App, input: GraphRereadActionV3) {
    if input == GraphRereadActionV3::Open {
        if !refresh_available(app)
            || app.overlay.is_some()
            || app.loading[View::Repository.index()]
            || app.navigation_restore_inflight
        {
            app.dispatch(Action::Explain(
                "Re-read belongs to the current idle original Graph; cancel its read first".into(),
            ));
            return;
        }
        app.observe_navigation();
        let prepared = app
            .workspace
            .as_ref()
            .ok_or_else(|| "Original Graph unavailable".to_owned())
            .and_then(|workspace| {
                GraphRereadPreviewV3::capture(workspace, &mut app.original_saved_visits)
            });
        match prepared {
            Ok(preview) => {
                app.local_commit_actors.retire_private();
                app.local_actor_layout.clear();
                app.avatar_states.clear();
                app.graph_reread_dialog_v3 = Some(GraphRereadDialogV3 {
                    preview,
                    return_focus: (app.focus, app.repository_content_focused),
                    focus: 0,
                    scroll: 0,
                    body: Rect::default(),
                    hits: Vec::with_capacity(4),
                });
                app.overlay = Some(Overlay::GraphRereadV3);
                app.hits.clear();
            }
            Err(reason) => app.dispatch(Action::Explain(reason)),
        }
        return;
    }
    if !owns_dialog(app) {
        retire(app);
        return;
    }
    match input {
        GraphRereadActionV3::Back => retire(app),
        GraphRereadActionV3::Confirm => {
            if app.loading[View::Repository.index()] {
                return;
            }
            let request = app.graph_reread_dialog_v3.as_ref().unwrap().preview.clone();
            if let Err(reason) = app.original_saved_visits.can_remember_new_graph_v3() {
                retire(app);
                app.dispatch(Action::Explain(reason));
                return;
            }
            retire(app);
            app.graph_reread_request_v3 = Some(request);
            app.user_navigated = true;
        }
        GraphRereadActionV3::Focus(back) => {
            let dialog = app.graph_reread_dialog_v3.as_mut().unwrap();
            dialog.focus = (dialog.focus + if back { 3 } else { 1 }) % 4;
        }
        GraphRereadActionV3::Scroll(delta) => {
            let dialog = app.graph_reread_dialog_v3.as_mut().unwrap();
            dialog.scroll = dialog.scroll.saturating_add_signed(delta as isize).min(128);
        }
        GraphRereadActionV3::Open => unreachable!(),
    }
}
/// Capture all modal input, including stale outside points and unknown keys.
pub fn event(app: &mut App, event: Event) -> bool {
    if app.overlay != Some(Overlay::GraphRereadV3) {
        if app.overlay.is_none()
            && refresh_available(app)
            && matches!(&event, Event::Key(key) if key.kind != KeyEventKind::Release
                && key.code == KeyCode::Char('R') && !key.modifiers.intersects(KeyModifiers::CONTROL|KeyModifiers::ALT))
        {
            dispatch(app, GraphRereadActionV3::Open);
            return true;
        }
        return false;
    }
    if !owns_dialog(app) {
        retire(app);
        return true;
    }
    let action = match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                retire(app);
                app.dispatch(Action::Quit);
                return true;
            }
            KeyCode::Esc => Some(GraphRereadActionV3::Back),
            KeyCode::Tab => Some(GraphRereadActionV3::Focus(false)),
            KeyCode::BackTab => Some(GraphRereadActionV3::Focus(true)),
            KeyCode::Enter => Some(match app.graph_reread_dialog_v3.as_ref().unwrap().focus {
                0 => GraphRereadActionV3::Confirm,
                1 => GraphRereadActionV3::Back,
                2 => GraphRereadActionV3::Scroll(-8),
                _ => GraphRereadActionV3::Scroll(8),
            }),
            KeyCode::PageUp | KeyCode::Up => Some(GraphRereadActionV3::Scroll(-8)),
            KeyCode::PageDown | KeyCode::Down => Some(GraphRereadActionV3::Scroll(8)),
            _ => None,
        },
        Event::Mouse(mouse) => {
            let dialog = app.graph_reread_dialog_v3.as_mut().unwrap();
            let point = (mouse.column, mouse.row).into();
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => dialog
                    .hits
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, hit)| hit.rect.contains(point))
                    .map(|(index, hit)| {
                        dialog.focus = index;
                        match &hit.action {
                            Action::GraphRereadV3(input) => *input,
                            _ => unreachable!(),
                        }
                    }),
                MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                    if dialog.body.contains(point) =>
                {
                    Some(GraphRereadActionV3::Scroll(
                        if mouse.kind == MouseEventKind::ScrollDown {
                            3
                        } else {
                            -3
                        },
                    ))
                }
                _ => None,
            }
        }
        Event::Resize(_, _) => {
            app.graph_reread_dialog_v3.as_mut().unwrap().hits.clear();
            app.hits.clear();
            None
        }
        _ => None,
    };
    if let Some(action) = action {
        dispatch(app, action);
    }
    true
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) -> bool {
    if app.overlay != Some(Overlay::GraphRereadV3) {
        return false;
    }
    if !owns_dialog(app) {
        retire(app);
        return true;
    }
    let theme = app.theme();
    let dialog = app.graph_reread_dialog_v3.as_mut().unwrap();
    dialog.hits.clear();
    app.hits.clear();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(TITLE)
            .style(theme.base()),
        area,
    );
    let inner = area.inner(ratatui::layout::Margin::new(1, 1));
    if inner.width < 24 || inner.height < 4 {
        return true;
    }
    // Two actual footer rows, both controls fit at40 columns. Hit geometry is
    // renewed per render, bottom-scroll pair provides the shared resize oracle.
    dialog.body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(3),
    );
    let text = dialog.preview.lines().join("\n\n");
    let lines = crate::ui::notice_lines(&text, dialog.body.width);
    dialog.scroll = dialog
        .scroll
        .min(lines.len().saturating_sub(dialog.body.height as usize));
    frame.render_widget(
        Paragraph::new(lines).scroll((dialog.scroll as u16, 0)),
        dialog.body,
    );
    for (index, (label, input, row)) in [
        (
            "[Enter Re-read]",
            GraphRereadActionV3::Confirm,
            inner.bottom() - 2,
        ),
        ("[Esc Back]", GraphRereadActionV3::Back, inner.bottom() - 2),
        (
            "[PgUp]",
            GraphRereadActionV3::Scroll(-8),
            inner.bottom() - 1,
        ),
        ("[PgDn]", GraphRereadActionV3::Scroll(8), inner.bottom() - 1),
    ]
    .into_iter()
    .enumerate()
    {
        let width = label.len() as u16;
        let x = if index % 2 == 0 {
            inner.x
        } else {
            inner.right() - width
        };
        let rect = Rect::new(x, row, width, 1);
        let style = theme
            .style(SemanticToken::Reference)
            .add_modifier(if dialog.focus == index {
                Modifier::UNDERLINED
            } else {
                Modifier::empty()
            });
        frame.render_widget(Paragraph::new(label).style(style), rect);
        dialog.hits.push(HitRegion {
            rect,
            action: Action::GraphRereadV3(input),
        });
    }
    app.hits.extend(dialog.hits.iter().cloned());
    true
}
pub fn take_request(app: &mut App) -> Option<Arc<GraphRereadPreviewV3>> {
    let request = app.graph_reread_request_v3.take()?;
    if !refresh_available(app)
        || app
            .workspace
            .as_ref()
            .is_none_or(|workspace| !request.matches_workspace(workspace))
        || app
            .original_saved_visits
            .can_remember_new_graph_v3()
            .is_err()
    {
        app.status = "Original Graph re-read owner/retention changed • old view retained".into();
        return None;
    }
    Some(request)
}
pub fn work(
    request: &GraphRereadPreviewV3,
    owner: &OriginalLocalNavigationOwnerV1,
) -> Result<PreparedView, String> {
    owner.operation().check()?;
    request.validate_owner_v3(owner)?;
    let outcome = request.execute(owner.operation())?;
    let loaded = outcome.workspace.graph_state_v1.as_ref().unwrap().loaded();
    let context = format!(
        "Local {} / original walk / {}",
        outcome.workspace.directory.display(),
        outcome.workspace.revision
    );
    owner.operation().check()?;
    Ok(PreparedView {
        workspace: Some(outcome.workspace),
        graph_reread_retention_v3: Some(GraphRereadRetentionV3 {
            visit: outcome.visit,
            operation: owner.operation().clone(),
        }),
        context: Some(context),
        status: format!(
            "Original Graph re-read • first{loaded} loaded • earlier snapshot retained in Back"
        ),
        ..PreparedView::default()
    })
}

#[cfg(test)]
#[path = "repository_graph_reread_native_v3_tests.rs"]
mod tests;
