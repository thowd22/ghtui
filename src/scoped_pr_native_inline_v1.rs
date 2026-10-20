//! Closed Native canonical-range entry into the EXISTING inline composer.
//! No renderer row/full-file position is a review anchor or permission.
use super::{native_v1, ScopedInlineIntentV1};
use crate::{
    app::{Action, App, View},
    providers::gh::PrImageSideV1,
    repository_native_diff_content_v2::NativeDiffContentV2,
    repository_native_diff_ui_v1::{NativeDiffActionV1 as A, NativeDiffHitV1},
    repository_native_diff_v1::NativeCurrentRangeV2,
    repository_view::DiffSide,
    scoped_pr_native_diff_source_v1::ScopedPrNativeSourceV1,
    theme::SemanticToken,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use std::sync::Arc;

pub(crate) struct NativeInlineOriginV1 {
    selected: Arc<NativeCurrentRangeV2<ScopedPrNativeSourceV1>>,
    width: u16,
    viewport: (usize, usize, bool, u16),
    anchor: (PrImageSideV1, u32, u32),
    focus: (bool, bool, Option<usize>),
}
impl std::fmt::Debug for NativeInlineOriginV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NativeInlineOriginV1(<original canonical range/viewport; no permission>)")
    }
}
impl NativeInlineOriginV1 {
    fn capture(app: &App) -> Result<(Arc<Self>, Arc<ScopedInlineIntentV1>), String> {
        if app.view != View::Inbox || app.overlay.is_some() {
            return Err("Choose a current Native PR patch range first".into());
        }
        let state = app
            .pr_native_diff_v1
            .as_ref()
            .ok_or("Current Native PR file unavailable")?;
        if state.external_v4.is_some() {
            return Err("Return from the owned external viewer before choosing an original Native comment range".into());
        }
        if state.is_full_side() {
            return Err("Full-file rows are read-only context, not canonical comment anchors. Back to patch, select explicit LEFT/RIGHT lines, then Comment.".into());
        }
        let selected = state.capture_current_range_v2()?;
        let range = selected.range();
        let source = selected.source();
        if !source.proof().owns(app) || range.file() != 0 {
            return Err("Original selected PR source is no longer current".into());
        }
        let side=match range.side(){DiffSide::Old=>PrImageSideV1::Old,DiffSide::New=>PrImageSideV1::New,DiffSide::Both=>return Err("Both-side/hunk selection has no single review side. Choose Split, explicitly select LEFT or RIGHT, then Comment; no side converted.".into())};
        let ((first_hunk, first_line), (last_hunk, last_line)) = range.endpoints();
        if first_hunk != last_hunk {
            return Err("Inline comments require one genuine canonical hunk; select again".into());
        }
        let file = source
            .canonical_file(range.file())
            .ok_or("Original canonical member unavailable")?;
        let hunk = file
            .hunks
            .get(first_hunk)
            .ok_or("Original canonical hunk unavailable")?;
        let coordinate = |index: usize| {
            hunk.lines.get(index).and_then(|line| match side {
                PrImageSideV1::Old => line.old_line,
                PrImageSideV1::New => line.new_line,
            })
        };
        let first = coordinate(first_line)
            .ok_or("First endpoint is not a line on this explicit original side")?;
        let last = coordinate(last_line)
            .ok_or("Last endpoint is not a line on this explicit original side")?;
        let ending = hunk
            .lines
            .get(last_line)
            .expect("original endpoint checked")
            .kind;
        if side == PrImageSideV1::Old && ending != crate::diff::LineKind::Removed {
            return Err("LEFT comments must end on an actual deleted line. Context needs explicit RIGHT; no side converted.".into());
        }
        let proof = source.proof();
        let server = proof
            .original_view
            .observed
            .files()
            .get(proof.file_index())
            .ok_or("Original API file member unavailable")?;
        if first == 0
            || first > last
            || last - first > 9999
            || proof.observed().image(side).path().is_empty()
            || proof.observed().image(side).path().len() > 4096
            || server.path.is_empty()
            || server.path.len() > 4096
        {
            return Err("Original inline range/path exceeds bounds; no anchor inferred".into());
        }
        // This is the SAME genuine immutable proof, not an Images/Files view.
        // Full range/endpoint semantics are rechecked by the existing worker.
        let intent = Arc::new(ScopedInlineIntentV1 {
            proof: proof.clone(),
            side,
            first,
            last,
            suggest: false,
        });
        let layout = &app.pr_native_diff_layout_v1;
        let origin = Arc::new(Self {
            selected,
            width: state.width,
            viewport: (state.selected, state.scroll, state.right, state.horizontal),
            anchor: (side, first, last),
            focus: (layout.controls, layout.query_focused, layout.focus),
        });
        Ok((origin, intent))
    }
    fn matches_native(&self, app: &App) -> bool {
        app.view == View::Inbox
            && app.pr_native_diff_v1.as_ref().is_some_and(|state| {
                self.selected.matches(state) && self.selected.source().proof().owns(app)
            })
    }
    pub(crate) fn matches_open(&self, app: &App) -> bool {
        self.matches_native(app)
            && app.overlay.is_none()
            && crate::scoped_pr_native_diff_native_v1::width(app) == self.width
            && app.pr_native_diff_v1.as_ref().is_some_and(|state| {
                (state.selected, state.scroll, state.right, state.horizontal) == self.viewport
            })
    }
    pub(crate) fn matches_intent(&self, intent: &ScopedInlineIntentV1) -> bool {
        Arc::ptr_eq(self.selected.source().proof(), &intent.proof)
            && self.anchor == (intent.side, intent.first, intent.last)
    }
}
pub fn open(app: &mut App) {
    if app.loading[View::Inbox.index()]
        || app.scoped_inline_owner_v1.is_some()
        || native_v1::newer_intent(app)
    {
        app.status = "Wait or cancel the original pending operation before Comment".into();
        return;
    }
    match NativeInlineOriginV1::capture(app) {
        Ok((origin, intent)) => native_v1::enqueue_native(app, intent, origin),
        Err(reason) => app.dispatch(Action::Explain(reason)),
    }
}
pub(crate) fn return_native(app: &mut App, origin: &Arc<NativeInlineOriginV1>) {
    if app.view != View::Inbox
        || app.loading[View::Inbox.index()]
        || !app.pr_native_diff_v1.as_ref().is_some_and(|state| {
            state.external_v4.is_none()
                && origin.selected.matches_retained_return_v1(state)
                && origin.selected.source().proof().owns(app)
        })
        || native_v1::newer_intent(app)
    {
        return;
    }
    if app.overlay.is_some() {
        return;
    }
    let width = crate::scoped_pr_native_diff_native_v1::width(app);
    let state = app
        .pr_native_diff_v1
        .as_mut()
        .expect("checked original native state");
    app.pr_native_diff_layout_v1.clear();
    (
        app.pr_native_diff_layout_v1.controls,
        app.pr_native_diff_layout_v1.query_focused,
        app.pr_native_diff_layout_v1.focus,
    ) = origin.focus;
    if state.width != width || state.current_frame().is_none() {
        state.width = width;
        match state.request() {
            Ok(request) => crate::scoped_pr_native_diff_native_v1::queue_projection(app, request),
            Err(reason) => app.status = reason,
        }
    }
}
pub(crate) fn form_closed(app: &mut App) {
    let origin = app.form.as_ref().and_then(|form| match &form.purpose {
        crate::forms::FormPurpose::PrInline(state) => state.native_origin_v1.clone(),
        _ => None,
    });
    if let Some(origin) = origin {
        return_native(app, &origin);
    }
}
fn pending(
    app: &App,
) -> Option<(
    Arc<native_v1::ScopedInlineWorkV1>,
    Arc<NativeInlineOriginV1>,
)> {
    let work = if let Some(queued) = &app.scoped_inline_work_v1 {
        queued.clone()
    } else {
        app.scoped_inline_owner_v1.as_ref()?.work().clone()
    };
    let origin = work.native_origin()?.clone();
    (origin.matches_native(app)
        && app.overlay.is_none()
        && !native_v1::newer_intent_allowing_work(app, Some(&work)))
    .then_some((work, origin))
}
pub(crate) fn event(app: &mut App, event: &Event) -> bool {
    let Some((work, origin)) = pending(app) else {
        return false;
    };
    let cancel = match event {
        Event::Resize(_, _) => {
            app.pr_native_diff_layout_v1.clear();
            if let Some(state) = &mut app.pr_native_diff_v1 {
                state.retire_presentation_for_inline_resize_v1();
            }
            true
        }
        Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
            KeyCode::Esc => true,
            KeyCode::Char('q') => {
                app.dispatch(Action::Quit);
                return true;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                app.pr_native_diff_layout_v1
                    .cycle(key.code == KeyCode::BackTab);
                return true;
            }
            KeyCode::Enter => match app.pr_native_diff_layout_v1.activated() {
                Some(A::Quit) => {
                    app.dispatch(Action::Quit);
                    return true;
                }
                _ => true,
            },
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.dispatch(Action::Quit);
                return true;
            }
            _ => false,
        },
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
            match app
                .pr_native_diff_layout_v1
                .hits()
                .iter()
                .find(|hit| hit.rect.contains((mouse.column, mouse.row).into()))
                .map(|hit| &hit.action)
            {
                Some(A::CancelInline) => true,
                Some(A::Quit) => {
                    app.dispatch(Action::Quit);
                    return true;
                }
                _ => false,
            }
        }
        _ => false,
    };
    if cancel {
        if app
            .scoped_inline_work_v1
            .as_ref()
            .is_some_and(|queued| Arc::ptr_eq(queued, &work))
        {
            app.scoped_inline_work_v1 = None;
            return_native(app, &origin);
        } else {
            app.scoped_inline_cancel_v1 = true;
        }
        app.status="Canceling original Native inline read • no renewed Inline Open • selection retained only while source remains current".into();
        app.pr_native_diff_layout_v1.clear();
    }
    true
}
pub(crate) fn draw_pending(frame: &mut Frame, app: &mut App, area: Rect) -> bool {
    let Some((_, origin)) = pending(app) else {
        return false;
    };
    if !origin.matches_open(app) {
        return false;
    }
    let theme = app.theme();
    let layout = &mut app.pr_native_diff_layout_v1;
    layout.clear();
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title("Opening inline comment • original Native range")
        .style(theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.is_empty() {
        return true;
    }
    let mut x = inner.x;
    let mut y = inner.y;
    for (label, action) in [
        ("[Esc Cancel inline]", A::CancelInline),
        ("[q Quit]", A::Quit),
    ] {
        let width = (label.len() as u16).min(inner.width);
        if x > inner.x && x + width > inner.right() {
            x = inner.x;
            y = y.saturating_add(1);
        }
        if y < inner.bottom() {
            let rect = Rect::new(x, y, width, 1);
            frame.render_widget(
                Paragraph::new(label).style(theme.style(
                    if layout.focus == Some(layout.hits.len()) {
                        SemanticToken::Focus
                    } else {
                        SemanticToken::Pending
                    },
                )),
                rect,
            );
            layout.hits.push(NativeDiffHitV1 { rect, action });
            x = x.saturating_add(width + 1);
        }
    }
    y = y.saturating_add(2);
    if y < inner.bottom() {
        frame.render_widget(Paragraph::new("Fresh original account/repository/PR/head verification pending. No comment or approval exists. All underlying diff/editor actions are captured; Resize cancels this original read, never retries it. Native source, range and viewport remain retained for Back only if still current.").wrap(Wrap{trim:false}).style(theme.style(SemanticToken::Muted)),Rect::new(inner.x,y,inner.width,inner.bottom()-y));
    }
    true
}
