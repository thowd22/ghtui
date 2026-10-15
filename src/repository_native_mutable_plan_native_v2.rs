//! Separate read-only preview and explicit single-attempt Confirm on B's queue.
use crate::{
    app::{Action, App, View},
    git::MutableDiffSourceV2,
    repository_diff_sides_worker_v1::OriginalDiffWorkV1,
    repository_native_mutable_plan_v2::{
        NativeMutableConfirmRequestV2, NativeMutablePreviewRequestV2, NativePlanNoteV2,
        PreparedNativeMutablePlanV2, PreparedNativeSelectedOutcomeV2,
    },
    theme::SemanticToken,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMutablePlanActionV2 {
    Close,
    Confirm,
    ExplainResize,
    Reload,
    Quit,
    Scroll(i32),
    Horizontal(i32),
    Focus(bool),
    Activate,
}
enum Phase {
    Preparing(Arc<NativeMutablePreviewRequestV2>),
    Preview(Arc<PreparedNativeMutablePlanV2>),
    Confirming(Arc<NativeMutableConfirmRequestV2>),
    Outcome(Arc<NativePlanNoteV2>),
}
pub struct NativeMutablePlanUiV2 {
    source: Arc<MutableDiffSourceV2>,
    phase: Phase,
    scroll: usize,
    horizontal: u16,
    focus: Option<usize>,
    hits: Vec<(Rect, NativeMutablePlanActionV2)>,
    body: Rect,
    width: u16,
    confirmation_visible: bool,
    resize_explained: bool,
}
impl NativeMutablePlanUiV2 {
    pub fn accepts_work(&self, work: &OriginalDiffWorkV1, app: &App) -> bool {
        let Some(workspace) = app.workspace.as_ref() else {
            return false;
        };
        let Some(state) = app.native_mutable_diff_v2.as_ref() else {
            return false;
        };
        if app.native_diff_v1.is_some() || !self.source.intent().owns(workspace) {
            return false;
        }
        match (&self.phase, work) {
            (Phase::Preparing(current), OriginalDiffWorkV1::MutablePreview(request)) => {
                Arc::ptr_eq(current, request) && request.accepts(state) && request.owns(workspace)
            }
            (Phase::Confirming(current), OriginalDiffWorkV1::MutableConfirm(request)) => {
                Arc::ptr_eq(current, request) && request.accepts(state) && request.owns(workspace)
            }
            _ => false,
        }
    }
    pub fn accept_preview(&mut self, prepared: Arc<PreparedNativeMutablePlanV2>) -> bool {
        if !matches!(&self.phase,Phase::Preparing(request) if Arc::ptr_eq(request,prepared.request()))
        {
            return false;
        }
        self.phase = Phase::Preview(prepared);
        self.confirmation_visible = false;
        self.scroll = 0;
        self.hits.clear();
        self.focus = None;
        true
    }
    pub fn accept_outcome(&mut self, prepared: PreparedNativeSelectedOutcomeV2) -> bool {
        if !matches!(&self.phase,Phase::Confirming(request) if prepared.matches(request)) {
            return false;
        }
        self.phase = Phase::Outcome(prepared.into_note());
        self.confirmation_visible = false;
        self.scroll = 0;
        self.hits.clear();
        self.focus = None;
        true
    }
    pub fn fail(&mut self, reason: &str) {
        self.confirmation_visible = false;
        let reason = if reason.len() <= 4096 {
            crate::providers::runner::sanitize_terminal(reason)
        } else {
            "Diagnostic exceeded bounds; inspect repository state".into()
        };
        self.phase=Phase::Outcome(NativePlanNoteV2::prepare(Arc::from(format!("{reason}\nNo automatic retry. Reload and inspect current state before another write."))).expect("bounded4096 diagnostic"));
        self.scroll = 0;
        self.hits.clear();
        self.focus = None;
    }
    fn confirming(&self) -> bool {
        matches!(self.phase, Phase::Confirming(_))
    }
    fn outcome(&self) -> bool {
        matches!(self.phase, Phase::Outcome(_))
    }
}
pub fn begin(app: &mut App) {
    if app.native_mutable_plan_v2.is_some() {
        return;
    }
    let request = app
        .native_mutable_diff_v2
        .as_ref()
        .ok_or("Choose a current local native range first")
        .and_then(|state| {
            app.workspace
                .as_ref()
                .ok_or("Original local workspace unavailable")
                .and_then(|workspace| {
                    if state.owns(workspace)
                        && state.source().is_some_and(|source| {
                            source.intent().kind()
                                == crate::git::MutableDiffKindV2::Uncommitted
                        })
                    {
                        return Err("Preview disabled: all tracked edits has no inferred index direction; choose Working or Staged first");
                    }
                    NativeMutablePreviewRequestV2::capture(state, workspace).map_err(|_| {
                        "Choose a current nonempty Working/Staged changed-line range before preview"
                    })
                })
        });
    match request {
        Ok(request) => {
            app.native_mutable_plan_v2 = Some(NativeMutablePlanUiV2 {
                source: request.source().clone(),
                phase: Phase::Preparing(request.clone()),
                scroll: 0,
                horizontal: 0,
                focus: None,
                hits: vec![],
                body: Rect::default(),
                width: 120,
                confirmation_visible: false,
                resize_explained: false,
            });
            app.original_diff_work_v1 = Some(Arc::new(OriginalDiffWorkV1::MutablePreview(request)));
            app.native_mutable_diff_layout_v2.clear();
            app.native_mutable_diff_layout_v2.controls = false;
            app.native_mutable_diff_layout_v2.focus = None;
            app.hits.clear();
        }
        Err(reason) => {
            if let Some(state) = app.native_mutable_diff_v2.as_mut() {
                state.status = reason.into();
            }
        }
    }
}
pub fn retire(app: &mut App) {
    if let Some(plan) = app.native_mutable_plan_v2.take() {
        if let Phase::Confirming(request) = plan.phase {
            request.retire();
        }
        app.original_diff_work_v1 = None;
        app.original_diff_cancel_v1 = true;
        app.hits.clear();
    }
}
/// A rejected queue admission has no worker. Retire only the exact selected
/// purpose that is still capturing this source/range; never a newer modal.
pub(crate) fn submission_failed(app: &mut App, work: &OriginalDiffWorkV1, reason: &str) -> bool {
    if app
        .native_mutable_plan_v2
        .as_ref()
        .is_none_or(|plan| !plan.accepts_work(work, app))
    {
        return false;
    }
    if let OriginalDiffWorkV1::MutableConfirm(request) = work {
        request.retire();
    }
    app.native_mutable_plan_v2.as_mut().expect("exact pending selected purpose checked").fail(&format!("Selected request was not submitted: {reason}\nNo Git write was started by this request."));
    app.native_mutable_diff_v2
        .as_mut()
        .expect("current selected parent checked")
        .invalidate_after_selected_attempt_v2();
    app.native_mutable_diff_layout_v2.clear();
    app.hits.clear();
    true
}
pub fn synchronize(app: &mut App) {
    if app.native_mutable_plan_v2.as_ref().is_some_and(|plan| {
        app.view != View::Repository
            || app.remote_repository.is_some()
            || app
                .workspace
                .as_ref()
                .is_none_or(|workspace| !plan.source.intent().owns(workspace))
            || app.native_mutable_diff_v2.is_none()
    }) {
        retire(app);
    }
}
pub fn dispatch(app: &mut App, action: NativeMutablePlanActionV2) {
    synchronize(app);
    if app.native_mutable_plan_v2.is_none() {
        return;
    }
    if action == NativeMutablePlanActionV2::Quit {
        app.dispatch(Action::Quit);
        return;
    }
    if action == NativeMutablePlanActionV2::Reload {
        app.dispatch(Action::Refresh);
        return;
    }
    if action == NativeMutablePlanActionV2::ExplainResize {
        app.native_mutable_plan_v2
            .as_mut()
            .unwrap()
            .resize_explained = true;
        return;
    }
    if action == NativeMutablePlanActionV2::Close {
        let plan = app.native_mutable_plan_v2.as_mut().unwrap();
        if plan.confirming() {
            if let Phase::Confirming(request) = &plan.phase {
                request.retire();
            }
            plan.fail("Confirmation canceled; the index may already have changed. Inspect before another write.");
            app.original_diff_work_v1 = None;
            app.original_diff_cancel_v1 = true;
            if let Some(state) = app.native_mutable_diff_v2.as_mut() {
                state.invalidate_after_selected_attempt_v2();
            }
        } else if plan.outcome() {
            crate::repository_native_diff_native_v1::retire(app);
            app.status =
                "Selected operation closed; reload repository state before another write".into();
        } else {
            let width = plan.width;
            retire(app);
            if let Some(state) = app.native_mutable_diff_v2.as_mut() {
                if state.width != width {
                    state.width = width;
                    match state.request() {
                        Ok(request) => {
                            app.original_diff_work_v1 =
                                Some(Arc::new(OriginalDiffWorkV1::MutableNative(request)))
                        }
                        Err(reason) => state.status = reason,
                    }
                }
            }
            app.native_mutable_diff_layout_v2.clear();
        }
        return;
    }
    if action == NativeMutablePlanActionV2::Confirm {
        if !app
            .native_mutable_plan_v2
            .as_ref()
            .is_some_and(|plan| plan.confirmation_visible)
        {
            return;
        }
        let Some(Phase::Preview(prepared)) = app.native_mutable_plan_v2.as_ref().map(|p| &p.phase)
        else {
            return;
        };
        let Some(state) = app.native_mutable_diff_v2.as_ref() else {
            return;
        };
        let Some(workspace) = app.workspace.as_ref() else {
            return;
        };
        match NativeMutableConfirmRequestV2::capture(prepared, state, workspace) {
            Ok(request) => {
                let plan = app.native_mutable_plan_v2.as_mut().unwrap();
                plan.phase = Phase::Confirming(request.clone());
                plan.confirmation_visible = false;
                plan.hits.clear();
                plan.focus = None;
                app.original_diff_work_v1 =
                    Some(Arc::new(OriginalDiffWorkV1::MutableConfirm(request)));
            }
            Err(reason) => {
                app.native_mutable_plan_v2.as_mut().unwrap().fail(&reason);
                if let Some(state) = app.native_mutable_diff_v2.as_mut() {
                    state.invalidate_after_selected_attempt_v2();
                }
            }
        }
        return;
    }
    let plan = app.native_mutable_plan_v2.as_mut().unwrap();
    match action {
        NativeMutablePlanActionV2::Scroll(delta) => {
            plan.scroll = plan
                .scroll
                .saturating_add_signed(delta as isize)
                .min(50_000)
        }
        NativeMutablePlanActionV2::Horizontal(delta) => {
            plan.horizontal = (i32::from(plan.horizontal) + delta).clamp(0, 4096) as u16
        }
        NativeMutablePlanActionV2::Focus(back) => {
            let len = plan.hits.len();
            if len > 0 {
                plan.focus = Some(plan.focus.map_or(if back { len - 1 } else { 0 }, |i| {
                    if back {
                        (i + len - 1) % len
                    } else {
                        (i + 1) % len
                    }
                }));
            }
        }
        NativeMutablePlanActionV2::Activate => {
            if let Some(action) = plan
                .focus
                .and_then(|i| plan.hits.get(i))
                .map(|(_, a)| a.clone())
            {
                dispatch(app, action);
            }
        }
        _ => {}
    }
}
pub fn capture(app: &mut App, event: &Event) -> bool {
    synchronize(app);
    let Some(plan) = app.native_mutable_plan_v2.as_mut() else {
        return false;
    };
    if app.overlay.is_some() {
        return false;
    }
    let action = match event {
        Event::Resize(width, _) => {
            plan.width = width.saturating_sub(2).min(512);
            plan.confirmation_visible = false;
            plan.resize_explained = false;
            plan.hits.clear();
            plan.body = Rect::default();
            None
        }
        Event::Key(key) if key.kind == KeyEventKind::Release => None,
        Event::Key(key) => match key.code {
            KeyCode::Char('q') => Some(NativeMutablePlanActionV2::Quit),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(NativeMutablePlanActionV2::Quit)
            }
            KeyCode::Esc => Some(NativeMutablePlanActionV2::Close),
            KeyCode::Enter if plan.focus.is_some() => Some(NativeMutablePlanActionV2::Activate),
            KeyCode::Enter
                if plan
                    .hits
                    .iter()
                    .any(|(_, action)| *action == NativeMutablePlanActionV2::Confirm) =>
            {
                Some(NativeMutablePlanActionV2::Confirm)
            }
            KeyCode::Enter
                if plan
                    .hits
                    .iter()
                    .any(|(_, action)| *action == NativeMutablePlanActionV2::ExplainResize) =>
            {
                Some(NativeMutablePlanActionV2::ExplainResize)
            }
            KeyCode::Char('r') if plan.outcome() => Some(NativeMutablePlanActionV2::Reload),
            KeyCode::Tab | KeyCode::BackTab => Some(NativeMutablePlanActionV2::Focus(
                key.code == KeyCode::BackTab,
            )),
            KeyCode::Up | KeyCode::Char('k') => Some(NativeMutablePlanActionV2::Scroll(-1)),
            KeyCode::Down | KeyCode::Char('j') => Some(NativeMutablePlanActionV2::Scroll(1)),
            KeyCode::PageUp => Some(NativeMutablePlanActionV2::Scroll(-i32::from(
                plan.body.height.max(1),
            ))),
            KeyCode::PageDown => Some(NativeMutablePlanActionV2::Scroll(i32::from(
                plan.body.height.max(1),
            ))),
            KeyCode::Home => {
                plan.scroll = 0;
                None
            }
            KeyCode::End => {
                plan.scroll = 50_000;
                None
            }
            KeyCode::Left => Some(NativeMutablePlanActionV2::Horizontal(-8)),
            KeyCode::Right => Some(NativeMutablePlanActionV2::Horizontal(8)),
            _ => None,
        },
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => plan
                .hits
                .iter()
                .rev()
                .find(|(rect, _)| rect.contains((mouse.column, mouse.row).into()))
                .map(|(_, a)| a.clone()),
            MouseEventKind::ScrollUp if plan.body.contains((mouse.column, mouse.row).into()) => {
                Some(NativeMutablePlanActionV2::Scroll(-3))
            }
            MouseEventKind::ScrollDown if plan.body.contains((mouse.column, mouse.row).into()) => {
                Some(NativeMutablePlanActionV2::Scroll(3))
            }
            MouseEventKind::ScrollLeft if plan.body.contains((mouse.column, mouse.row).into()) => {
                Some(NativeMutablePlanActionV2::Horizontal(-8))
            }
            MouseEventKind::ScrollRight if plan.body.contains((mouse.column, mouse.row).into()) => {
                Some(NativeMutablePlanActionV2::Horizontal(8))
            }
            _ => None,
        },
        _ => None,
    };
    if let Some(action) = action {
        dispatch(app, action);
    }
    true
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) -> bool {
    synchronize(app);
    let theme = app.theme();
    let Some(plan) = app.native_mutable_plan_v2.as_mut() else {
        return false;
    };
    frame.render_widget(Clear, area);
    let pane = Rect::new(area.x, area.y, area.width.min(514), area.height);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title("Selected patch • confirm")
            .style(theme.style(SemanticToken::Normal)),
        pane,
    );
    plan.hits.clear();
    plan.confirmation_visible = false;
    plan.body = Rect::default();
    let inner = pane.inner(ratatui::layout::Margin::new(1, 1));
    plan.width = inner.width;
    if inner.width < 32 || inner.height < 12 {
        frame.render_widget(
            Paragraph::new(if plan.resize_explained {
                "Confirm refused; resize to34x14"
            } else {
                "Resize to34x14; confirmation disabled"
            }),
            inner,
        );
        if inner.height > 1 {
            let close = Rect::new(inner.x, inner.bottom() - 1, inner.width.min(11), 1);
            frame.render_widget(Paragraph::new("[Esc Close]"), close);
            plan.hits.push((close, NativeMutablePlanActionV2::Close));
            let mut x = close.right().saturating_add(1);
            for (label, action) in [
                ("[Enter disabled]", NativeMutablePlanActionV2::ExplainResize),
                ("[q Quit]", NativeMutablePlanActionV2::Quit),
            ] {
                let width = crate::editor::display_width(label) as u16;
                if x + width <= inner.right() {
                    let rect = Rect::new(x, inner.bottom() - 1, width, 1);
                    frame.render_widget(Paragraph::new(label), rect);
                    plan.hits.push((rect, action));
                    x += width + 1;
                }
            }
        } else {
            plan.hits.push((inner, NativeMutablePlanActionV2::Close));
        }
        return true;
    }
    plan.confirmation_visible = matches!(plan.phase, Phase::Preview(_));
    let mut controls = match &plan.phase {
        Phase::Preview(_) => vec![
            ("[Esc Back]", NativeMutablePlanActionV2::Close),
            ("[Enter Confirm]", NativeMutablePlanActionV2::Confirm),
        ],
        Phase::Outcome(_) => vec![
            ("[Esc Close]", NativeMutablePlanActionV2::Close),
            ("[r Reload/inspect]", NativeMutablePlanActionV2::Reload),
        ],
        _ => vec![("[Esc Cancel]", NativeMutablePlanActionV2::Close)],
    };
    controls.push(("[q Quit]", NativeMutablePlanActionV2::Quit));
    let mut x = inner.x;
    let mut y = inner.y;
    for (label, action) in &controls {
        let width = crate::editor::display_width(label) as u16;
        if x > inner.x && x + width > inner.right() {
            x = inner.x;
            y += 1;
        }
        let rect = Rect::new(x, y, width.min(inner.width), 1);
        let index = plan.hits.len();
        let style = theme.style(if plan.focus == Some(index) {
            SemanticToken::Focus
        } else {
            SemanticToken::Reference
        });
        frame.render_widget(Paragraph::new(*label).style(style), rect);
        plan.hits.push((rect, action.clone()));
        x = x.saturating_add(width + 1);
    }
    y += 1;
    let status = match plan.phase {
        Phase::Preparing(_) => "Preparing exact selected patch • read only",
        Phase::Preview(_) => "Ready preview • no changes until explicit Confirm",
        Phase::Confirming(_) => "Confirming once • Cancel may not undo an already-started write",
        Phase::Outcome(_) => "Result • old diff rows retired • no automatic retry",
    };
    frame.render_widget(
        Paragraph::new(status).style(theme.style(SemanticToken::Changed)),
        Rect::new(inner.x, y, inner.width, 1),
    );
    y += 1;
    plan.body = Rect::new(
        inner.x,
        y,
        inner.width,
        inner.bottom().saturating_sub(y + 1),
    );
    let mut footer_x = inner.x;
    for (label, action) in controls {
        let width = (crate::editor::display_width(label) as u16).min(inner.width);
        if footer_x + width > inner.right() {
            break;
        }
        let rect = Rect::new(footer_x, inner.bottom() - 1, width, 1);
        let style = theme.style(if plan.focus == Some(plan.hits.len()) {
            SemanticToken::Focus
        } else {
            SemanticToken::Reference
        });
        frame.render_widget(Paragraph::new(label).style(style), rect);
        plan.hits.push((rect, action));
        footer_x = footer_x.saturating_add(width + 1);
    }
    let count = match &plan.phase {
        Phase::Preview(prepared) => prepared.plan().preview.len(),
        Phase::Outcome(note) => note.line_count(),
        _ => 0,
    };
    plan.scroll = plan
        .scroll
        .min(count.saturating_sub(usize::from(plan.body.height).max(1)));
    for row in 0..usize::from(plan.body.height) {
        let index = plan.scroll + row;
        let line = match &plan.phase {
            Phase::Preview(prepared) => prepared.plan().preview.get(index).map(String::as_str),
            Phase::Outcome(note) => note.line(index),
            _ => None,
        };
        let Some(line) = line else {
            break;
        };
        let style = theme.style(if line.starts_with('+') {
            SemanticToken::Added
        } else if line.starts_with('-') {
            SemanticToken::Removed
        } else {
            SemanticToken::Normal
        });
        frame.render_widget(
            Paragraph::new(line)
                .scroll((0, plan.horizontal))
                .style(style),
            Rect::new(plan.body.x, plan.body.y + row as u16, plan.body.width, 1),
        );
    }
    true
}
#[cfg(test)]
#[path = "repository_native_mutable_plan_native_v2_tests.rs"]
mod tests;
