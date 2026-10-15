//! Same closed actions for keyboard/mouse; all projection changes use B's worker.
use crate::{
    app::{Action, App, View},
    git::{MutableDiffSourceV2, OriginalComparisonSourceV1},
    repository_diff_sides_worker_v1::OriginalDiffWorkV1,
    repository_native_diff_content_v2::NativeDiffPurposeV2,
    repository_native_diff_ui_v1::NativeDiffActionV1 as A,
    repository_native_diff_ui_v1::NativeDiffLayoutV1,
    repository_native_diff_v1::{NativeDiffOptionChangeV1, NativeDiffStateV1},
};
use crossterm::event::{Event, KeyEventKind};
use std::sync::Arc;

// Private concrete App adapters. The controller algorithm is shared; its typed
// state/tickets and closed result variants are not interchangeable.
trait NativeAppV2: NativeDiffPurposeV2 {
    fn preview(app: &mut App);
    fn state(app: &App) -> Option<&NativeDiffStateV1<Self>>;
    fn parts(
        app: &mut App,
    ) -> (
        &mut Option<NativeDiffStateV1<Self>>,
        &mut NativeDiffLayoutV1<Self>,
    );
    fn work(
        request: Arc<crate::repository_native_diff_v1::NativeDiffRequestV1<Self>>,
    ) -> OriginalDiffWorkV1;
}
impl NativeAppV2 for OriginalComparisonSourceV1 {
    fn preview(app: &mut App) {
        if let Some(state) = app.native_diff_v1.as_mut() {
            state.status = Self::stage_unavailable().into();
        }
    }
    fn state(app: &App) -> Option<&NativeDiffStateV1<Self>> {
        app.native_diff_v1.as_ref()
    }
    fn parts(
        app: &mut App,
    ) -> (
        &mut Option<NativeDiffStateV1<Self>>,
        &mut NativeDiffLayoutV1<Self>,
    ) {
        (&mut app.native_diff_v1, &mut app.native_diff_layout_v1)
    }
    fn work(
        request: Arc<crate::repository_native_diff_v1::NativeDiffRequestV1<Self>>,
    ) -> OriginalDiffWorkV1 {
        OriginalDiffWorkV1::Native(request)
    }
}
impl NativeAppV2 for MutableDiffSourceV2 {
    fn preview(app: &mut App) {
        crate::repository_native_mutable_plan_native_v2::begin(app);
    }
    fn state(app: &App) -> Option<&NativeDiffStateV1<Self>> {
        app.native_mutable_diff_v2.as_ref()
    }
    fn parts(
        app: &mut App,
    ) -> (
        &mut Option<NativeDiffStateV1<Self>>,
        &mut NativeDiffLayoutV1<Self>,
    ) {
        (
            &mut app.native_mutable_diff_v2,
            &mut app.native_mutable_diff_layout_v2,
        )
    }
    fn work(
        request: Arc<crate::repository_native_diff_v1::NativeDiffRequestV1<Self>>,
    ) -> OriginalDiffWorkV1 {
        OriginalDiffWorkV1::MutableNative(request)
    }
}
pub fn open(app: &mut App) {
    if app
        .workspace
        .as_ref()
        .and_then(|w| w.comparison.as_ref())
        .is_some_and(|c| {
            matches!(
                c.comparison,
                crate::git::Comparison::Working
                    | crate::git::Comparison::Staged
                    | crate::git::Comparison::Uncommitted
            )
        })
    {
        dispatch_mutable(app, A::Open);
    } else {
        dispatch(app, A::Open);
    }
}
pub fn dispatch(app: &mut App, action: A) {
    dispatch_for::<OriginalComparisonSourceV1>(app, action);
}
pub fn dispatch_mutable(app: &mut App, action: A<MutableDiffSourceV2>) {
    dispatch_for::<MutableDiffSourceV2>(app, action);
}
pub fn retire(app: &mut App) {
    crate::repository_native_mutable_plan_native_v2::retire(app);
    retire_for::<OriginalComparisonSourceV1>(app);
    retire_for::<MutableDiffSourceV2>(app);
}
pub fn synchronize(app: &mut App) {
    crate::repository_native_mutable_plan_native_v2::synchronize(app);
    synchronize_for::<OriginalComparisonSourceV1>(app);
    synchronize_for::<MutableDiffSourceV2>(app);
}
pub fn capture(app: &mut App, event: &Event) -> bool {
    synchronize(app);
    if crate::repository_native_mutable_plan_native_v2::capture(app, event) {
        return true;
    }
    if app.native_mutable_diff_v2.is_some() {
        capture_for::<MutableDiffSourceV2>(app, event)
    } else {
        capture_for::<OriginalComparisonSourceV1>(app, event)
    }
}
pub fn draw(frame: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) -> bool {
    synchronize(app);
    if crate::repository_native_mutable_plan_native_v2::draw(frame, app, area) {
        return true;
    }
    if app.native_mutable_diff_v2.is_some() {
        draw_for::<MutableDiffSourceV2>(frame, app, area)
    } else {
        draw_for::<OriginalComparisonSourceV1>(frame, app, area)
    }
}

fn retire_for<S: NativeAppV2>(app: &mut App) {
    if S::parts(app).0.take().is_some() {
        app.original_diff_cancel_v1 = true;
        app.original_diff_work_v1 = None;
    }
    *S::parts(app).1 = Default::default();
    app.hits.clear();
}
fn synchronize_for<S: NativeAppV2>(app: &mut App) {
    if S::state(app).is_some_and(|state| {
        app.view != View::Repository
            || app.remote_repository.is_some()
            || app
                .workspace
                .as_ref()
                .is_none_or(|workspace| !state.owns(workspace))
    }) {
        retire_for::<S>(app);
    }
}
fn queue<S: NativeAppV2>(app: &mut App) {
    let Some(state) = S::parts(app).0.as_mut() else {
        return;
    };
    match state.request() {
        Ok(request) => app.original_diff_work_v1 = Some(Arc::new(S::work(request))),
        Err(reason) => {
            state.status = reason;
            app.original_diff_work_v1 = None;
            app.original_diff_cancel_v1 = true;
        }
    }
    S::parts(app).1.clear();
    app.hits.clear();
}
fn visible<S: NativeAppV2>(app: &mut App) {
    let height = S::parts(app).1.body_height();
    if let Some(state) = S::parts(app).0.as_mut() {
        if state.selected < state.scroll {
            state.scroll = state.selected;
        } else if state.selected >= state.scroll.saturating_add(height) {
            state.scroll = state.selected.saturating_add(1).saturating_sub(height);
        }
    }
}
fn dispatch_for<S: NativeAppV2>(app: &mut App, action: A<S>) {
    synchronize(app);
    if action == A::Open {
        if app.view != View::Repository
            || app.remote_repository.is_some()
            || app.loading[View::Repository.index()]
            || app.repository_load_requested
            || app.repository_action.is_some()
        {
            app.dispatch(Action::Explain(
                "Choose a loaded local comparison first; wait or cancel its active read".into(),
            ));
            return;
        }
        let opened = app
            .workspace
            .as_ref()
            .ok_or_else(|| "Native diff source is unavailable".to_owned())
            .and_then(|workspace| {
                NativeDiffStateV1::<S>::open_source(
                    workspace,
                    app.original_tree_viewport_v2
                        .width
                        .saturating_sub(2)
                        .min(512),
                )
            });
        match opened {
            Ok(state) => {
                crate::repository_tree_native_v1::retire(app);
                crate::repository_diff_sides_native_v1::retire(app);
                retire(app);
                *S::parts(app).0 = Some(state);
                *S::parts(app).1 = Default::default();
                queue::<S>(app);
            }
            Err(reason) => app.dispatch(Action::Explain(reason)),
        }
        return;
    }
    if action == A::Quit {
        app.dispatch(Action::Quit);
        return;
    }
    if action == A::Close {
        retire(app);
        app.status = S::closed_status().into();
        return;
    }
    if S::state(app).is_none() {
        return;
    }
    if app.native_mutable_plan_v2.is_some() {
        return;
    }
    if action == A::Stage {
        S::preview(app);
        return;
    }
    if action == A::Source {
        S::parts(app).1.clear();
        app.dispatch(Action::RepositorySource);
        return;
    }
    if action == A::Controls {
        S::parts(app).1.controls = !S::parts(app).1.controls;
        S::parts(app).1.focus = None;
        S::parts(app).1.clear();
        return;
    }
    if matches!(action, A::FocusQuery | A::FocusBody) {
        S::parts(app).1.query_focused = action == A::FocusQuery;
        S::parts(app).1.controls = false;
        S::parts(app).1.focus = None;
        return;
    }
    if let A::FocusNext(back) = action {
        S::parts(app).1.cycle(back);
        return;
    }
    if action == A::Activate {
        if let Some(action) = S::parts(app).1.activated() {
            dispatch_for::<S>(app, action);
        }
        return;
    }
    let (state, layout) = S::parts(app);
    let state = state.as_mut().expect("capturing native state");
    let mut prepare = false;
    let mut error = None;
    match action {
        A::QueryClick(column) => {
            layout.query_focused = true;
            layout.focus = None;
            state.query.click(0, column, false);
        }
        A::Insert(value) => {
            // Reject before TextBuffer normalization/allocation or cloning a
            // pasted payload. The query is a selected-file patch scope only.
            let selection = state.query.selection().map_or(0, |range| range.len());
            if value.len() > 256
                || state.query.text.len() - selection + value.len() > 256
                || value.chars().any(char::is_control)
            {
                error = Some(
                    "Native patch query is limited to256 bytes; control bytes are refused".into(),
                );
            } else {
                match state.query.insert(&value) {
                    Ok(()) => prepare = true,
                    Err(reason) => error = Some(reason),
                }
            }
        }
        A::Backspace => {
            state.query.delete_backward();
            prepare = true;
        }
        A::Delete => {
            state.query.delete_forward();
            prepare = true;
        }
        A::SelectAll => state.query.select_all(),
        A::ClearQuery => {
            state.query.clear();
            prepare = true;
        }
        A::Cursor { right, select } => state.query.horizontal(right, select),
        A::Edge { end, select } => state.query.line_edge(end, true, select),
        A::Regex => {
            state.regex = !state.regex;
            prepare = true;
        }
        A::Option(change) => match state.option(change) {
            Ok(()) => prepare = true,
            Err(reason) => error = Some(reason),
        },
        A::File(delta) => {
            if let Some(source) = state.source() {
                if source.member_count() == 0 {
                    error=Some("No admitted changed files in this original comparison; no file can be opened".into());
                } else {
                    let next = state
                        .file
                        .saturating_add_signed(delta as isize)
                        .min(source.member_count().saturating_sub(1));
                    if next != state.file {
                        state.file = next;
                        state.selected = 0;
                        state.scroll = 0;
                        prepare = true;
                    }
                }
            } else {
                error = Some("Wait for the original changed-file catalogue".into());
            }
        }
        A::Point { ticket, row, right } => {
            if !state.current_frame().is_some_and(|frame| {
                frame.accepts(&ticket) && frame.ticket(row, right).as_ref() == Some(&ticket)
            }) {
                return;
            }
            state.selected = row;
            state.right = right;
            layout.query_focused = false;
            layout.focus = None;
        }
        A::Drag { first, last } => match state.select_between(first, last) {
            Ok(()) => prepare = true,
            Err(reason) => error = Some(reason),
        },
        A::Move { delta, extend } => {
            if let Some(frame) = state.current_frame() {
                let next = state
                    .selected
                    .saturating_add_signed(delta as isize)
                    .min(frame.rows().len().saturating_sub(1));
                let ticket = frame.ticket(next, state.right);
                state.selected = next;
                if extend {
                    if let Some(ticket) = ticket {
                        match state.select(ticket, true) {
                            Ok(()) => prepare = true,
                            Err(reason) => error = Some(reason),
                        }
                    }
                }
            }
        }
        A::First => state.selected = 0,
        A::Last => {
            if let Some(frame) = state.current_frame() {
                state.selected = frame.rows().len().saturating_sub(1);
            }
        }
        A::Horizontal(delta) => {
            state.horizontal = (i32::from(state.horizontal) + delta).clamp(0, 4096) as u16
        }
        A::Scroll(delta) => {
            if let Some(frame) = state.frame() {
                state.scroll = state
                    .scroll
                    .saturating_add_signed(delta as isize)
                    .min(frame.rows().len().saturating_sub(1));
            }
        }
        A::Hunk(delta) => {
            if let Some(frame) = state.current_frame() {
                let current = frame
                    .ticket(state.selected, state.right)
                    .map(|ticket| ticket.key().hunk());
                let index =
                    current.map_or(0, |current| current.saturating_add_signed(delta as isize));
                if let Some(rows) = frame.hunks().get(index).and_then(|hunk| hunk.rows()) {
                    state.selected = rows.start;
                } else {
                    error =
                        Some("Requested hunk is hidden or outside this selected-file patch".into());
                }
            }
        }
        A::Match(delta) => {
            if let Some(frame) = state.current_frame() {
                let matches = frame.matches();
                let coordinate = frame
                    .ticket(state.selected, state.right)
                    .map(|ticket| (ticket.key().hunk(), ticket.key().line()));
                let index = coordinate.map_or_else(
                    || {
                        if delta > 0 {
                            0
                        } else {
                            matches.len().saturating_sub(1)
                        }
                    },
                    |coordinate| {
                        if delta > 0 {
                            matches
                                .partition_point(|m| (m.key().hunk(), m.key().line()) <= coordinate)
                        } else {
                            matches
                                .partition_point(|m| (m.key().hunk(), m.key().line()) < coordinate)
                                .saturating_sub(1)
                        }
                    },
                );
                if let Some(found) = matches.get(index).or_else(|| matches.first()) {
                    if let Some(row) = frame.first_matching_line_row(found.key()) {
                        state.selected = row;
                    } else {
                        error=Some("Matching original line is hidden by context/generated/focused-side options; no neighbouring row was selected".into());
                    }
                } else {
                    error = Some(
                        "No matches in the selected-file patch; full-side content was not searched"
                            .into(),
                    );
                }
            }
        }
        A::Select => {
            layout.controls = false;
            layout.focus = None;
            if let Some(ticket) = state
                .current_frame()
                .and_then(|frame| frame.ticket(state.selected, state.right))
            {
                match state.select(ticket, false) {
                    Ok(()) => prepare = true,
                    Err(reason) => error = Some(reason),
                }
            } else {
                error = Some("Choose an original changed line, not a header or blank cell".into());
            }
        }
        A::SelectHunk => {
            let hunk = state
                .current_frame()
                .and_then(|frame| frame.ticket(state.selected, state.right))
                .map(|ticket| ticket.key().hunk());
            if let Some(hunk) = hunk {
                match state.select_hunk(hunk) {
                    Ok(()) => prepare = true,
                    Err(reason) => error = Some(reason),
                }
            } else {
                error = Some("Choose a genuine changed-line row before selecting its hunk".into());
            }
        }
        A::ClearRange => {
            state.clear_selection();
            prepare = true;
        }
        _ => {}
    }
    if let Some(reason) = error {
        state.status = reason;
    }
    if prepare {
        queue::<S>(app);
    } else {
        visible::<S>(app);
    }
}
fn capture_for<S: NativeAppV2>(app: &mut App, event: &Event) -> bool {
    synchronize(app);
    if S::state(app).is_none() || app.overlay.is_some() {
        return false;
    }
    match event {
        Event::Resize(width, _) => {
            let width = width.saturating_sub(2).min(512);
            if S::state(app).is_some_and(|state| state.width != width) {
                S::parts(app).0.as_mut().expect("native resize").width = width;
                queue::<S>(app);
            } else {
                S::parts(app).1.clear();
            }
            return true;
        }
        Event::Key(key) if key.kind == KeyEventKind::Release => return true,
        Event::Key(key) => {
            if key.code == crossterm::event::KeyCode::Char('g') && !S::parts(app).1.query_focused {
                let file = S::state(app).expect("native control").file;
                dispatch_for::<S>(
                    app,
                    A::Option(NativeDiffOptionChangeV1::MarkGenerated(file)),
                );
                return true;
            }
            if let Some(action) = S::parts(app).1.key(key.code, key.modifiers) {
                dispatch_for::<S>(app, action);
            }
        }
        Event::Mouse(mouse) => {
            if let Some(action) = S::parts(app).1.mouse(mouse.kind, mouse.column, mouse.row) {
                dispatch_for::<S>(app, action);
            }
        }
        Event::Paste(value) => {
            if S::parts(app).1.query_focused {
                if value.len() <= 256 {
                    dispatch_for::<S>(app, A::Insert(value.clone()));
                } else {
                    S::parts(app).0.as_mut().expect("native query").status =
                        "Native patch query is limited to256 bytes; paste refused without cloning"
                            .into();
                }
            }
        }
        _ => {}
    }
    true
}
fn draw_for<S: NativeAppV2>(
    frame: &mut ratatui::Frame,
    app: &mut App,
    area: ratatui::layout::Rect,
) -> bool {
    synchronize(app);
    let theme = app.theme();
    let (state, layout) = S::parts(app);
    let Some(state) = state.as_mut() else {
        return false;
    };
    layout.draw(frame, state, area, theme);
    true
}
