//! Volatile, original-page-bound controls. Input/draw do no GH/SQL or
//! full-log clones. Keys and mouse hits share the same typed action semantics.
//! TASK-116: log Search is a type-to-filter finder. Each edit re-ranks the
//! retained, bounded (<=5000 lines) snapshot locally; Enter or a click on a
//! result installs the matches through the same original Search work.
use super::*;
use crate::{
    app::Overlay,
    editor::TextBuffer,
    log_finder_v1::{
        first_error_line_v1, rank_log_lines_v1, window_start_v1, FinderRankingV1, ERROR_PATTERN_V1,
        LIVE_CAP_V1,
    },
    pr_checks_native::scoped::choices::continuations_v2::LoadedLogSearchModeV2,
    providers::gh::{RunFilter, ScopedRunQueryV2},
    scoped_actions_resources_v2::ScopedActionsReadActionV2,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadUiActionV2 {
    OpenFilters,
    OpenSearch,
    Next(bool),
    Clear,
    Watch,
    Mode(LoadedLogSearchModeV2),
    Apply,
    Close,
    Quit,
    Focus(usize),
    Field(usize, usize, bool),
    /// Choose a live finder result (index into the ranked list) and jump to it.
    Pick(usize),
    /// Jump to the first error-looking line; n/N then step through errors.
    FirstError,
}
#[derive(Clone, Copy)]
enum EditorKindV2 {
    Filters,
    Search(LoadedLogSearchModeV2),
}
pub struct ReadEditorV2 {
    source: Arc<ScopedActionsSourceV1>,
    page: Arc<ScopedActionsPageV1>,
    epoch: u64,
    kind: EditorKindV2,
    fields: Vec<TextBuffer>,
    focus: usize,
    hits: Vec<(Rect, ReadUiActionV2)>,
    field_hits: Vec<(Rect, usize)>,
    ready: bool,
    finder: FinderV2,
}
/// Live finder state. Ranked against the editor's own retained page only.
#[derive(Default)]
struct FinderV2 {
    ranking: FinderRankingV1,
    error: Option<String>,
    selected: usize,
    top: usize,
    seen: Option<(String, LoadedLogSearchModeV2)>,
}
impl ReadEditorV2 {
    /// Re-rank only when the query or mode changed. Bounded and local.
    fn refresh_finder(&mut self) {
        let EditorKindV2::Search(mode) = self.kind else {
            return;
        };
        let query = &self.fields[0].text;
        if self
            .finder
            .seen
            .as_ref()
            .is_some_and(|(q, m)| q == query && *m == mode)
        {
            return;
        }
        let Some(snapshot) = self.page.loaded_log_snapshot_v2() else {
            return;
        };
        let text = snapshot.text();
        match rank_log_lines_v1(
            text,
            query,
            mode,
            LIVE_CAP_V1,
            Instant::now() + Duration::from_millis(250),
        ) {
            Ok(ranking) => {
                self.finder.ranking = ranking;
                self.finder.error = None;
            }
            Err(reason) => {
                self.finder.ranking = FinderRankingV1 {
                    lines: text.lines().count(),
                    ..FinderRankingV1::default()
                };
                self.finder.error = Some(reason);
            }
        }
        self.finder.selected = 0;
        self.finder.top = 0;
        self.finder.seen = Some((query.clone(), mode));
    }
    fn step(&mut self, delta: isize) {
        let len = self.finder.ranking.hits.len();
        if len == 0 {
            return;
        }
        let next = (self.finder.selected as isize + delta).clamp(0, len as isize - 1);
        self.finder.selected = next as usize;
    }
    pub(super) fn clear_geometry_v2(&mut self) {
        self.hits.clear();
        self.field_hits.clear();
        self.ready = false;
    }
    fn owns(&self, app: &App) -> bool {
        app.view == View::Actions
            && app.overlay == Some(Overlay::ScopedActionsReadV2)
            && !newer_foreign_intent(app)
            && app.scoped_actions_v1.as_ref().is_some_and(|state| {
                current(app, state)
                    && Arc::ptr_eq(&state.source, &self.source)
                    && Arc::ptr_eq(&state.page, &self.page)
                    && state.epoch == self.epoch
            })
    }
    fn count(&self) -> usize {
        if matches!(self.kind, EditorKindV2::Filters) {
            10
        } else {
            // Find, Fuzzy, Exact text, Regex, First error, Jump, Close, Quit.
            8
        }
    }
    fn close_focus(&self) -> usize {
        self.count() - 2
    }
    fn insert(&mut self, value: &str) {
        if value.len() > 1024 || value.chars().any(char::is_control) {
            return;
        }
        let total = self.fields.iter().map(|f| f.text.len()).sum::<usize>();
        if let Some(field) = self.fields.get_mut(self.focus) {
            let kept = field
                .text
                .len()
                .saturating_sub(field.selection().map_or(0, |r| r.len()));
            if kept.saturating_add(value.len()) <= 1024
                && total
                    .saturating_sub(field.text.len())
                    .saturating_add(kept)
                    .saturating_add(value.len())
                    <= 4096
            {
                let _ = field.insert(value);
            }
        }
    }
}
fn action(ui: ReadUiActionV2) -> ActionsAction {
    ActionsAction::ScopedReadV2(ScopedActionsReadActionV2::Ui(ui))
}
fn close(app: &mut App) {
    // Private editor hits are valid only while this overlay is actually active.
    // A retained editor is not permission to clear a newer overlay's geometry.
    if app.overlay != Some(Overlay::ScopedActionsReadV2) {
        return;
    }
    if let Some(state) = &mut app.scoped_actions_v1 {
        state.editor_v2 = None;
    }
    app.overlay = None;
    app.hits.clear();
}
fn stop(app: &mut App, message: bool) {
    let active = app
        .scoped_actions_v1
        .as_ref()
        .is_some_and(|s| s.watch_v2.is_some());
    if !active {
        return;
    }
    // Original cancellation is immediate, including a currently held child.
    // Background later retires only the exact existing logical Actions owner.
    cancel_current(app);
    if message {
        app.status="Finite Watch stopped • original token canceled • retained snapshot • no restart or mutation".into();
    }
}
fn open(app: &mut App, filters: bool) {
    // Pure admission precedes all cancellation/epoch/status/hit effects. In
    // particular an older retained page cannot cover an independently newer
    // request or stop its original token merely by receiving a shortcut.
    if app.view != View::Actions
        || app.overlay.is_some()
        || newer_foreign_intent(app)
        || app.scoped_actions_work_v1.is_some()
        || app.scoped_actions_owner_v1.is_some()
        || app.loading[View::Actions.index()]
    {
        return;
    }
    let Some(state) = app.scoped_actions_v1.as_ref().filter(|s| current(app, s)) else {
        return;
    };
    let fields = if filters {
        let Some(window) = state
            .page
            .run_window_v2()
            .filter(|_| state.source.pr_origin_v2().is_none())
        else {
            explain(
                app,
                "Open an actual repository Runs page for editable filters; PR source stays pinned",
            );
            return;
        };
        let f = window.query().filter();
        [&f.actor, &f.branch, &f.event, &f.status]
            .into_iter()
            .map(|s| s.clone().unwrap_or_default())
            .chain([
                f.workflow_id.map(|id| id.to_string()).unwrap_or_default(),
                f.head_sha.clone().unwrap_or_default(),
                f.created.clone().unwrap_or_default(),
            ])
            .collect::<Vec<_>>()
    } else {
        if state.page.loaded_log_snapshot_v2().is_none() {
            explain(app,"Open actual selected-job logs first; a workspace String or old PR capture cannot become an Actions log source");
            return;
        }
        // The error-line pattern is an internal query, never shown as typed text.
        vec![state
            .matches_v2
            .as_ref()
            .filter(|m| m.query() != ERROR_PATTERN_V1)
            .map_or_else(String::new, |m| m.query().to_owned())]
    };
    let mode = state
        .matches_v2
        .as_ref()
        .filter(|m| m.query() != ERROR_PATTERN_V1)
        .map_or(LoadedLogSearchModeV2::Fuzzy, |m| m.mode());
    let mut editor = ReadEditorV2 {
        source: state.source.clone(),
        page: state.page.clone(),
        epoch: state.epoch,
        kind: if filters {
            EditorKindV2::Filters
        } else {
            EditorKindV2::Search(mode)
        },
        fields: fields
            .into_iter()
            .map(|value| TextBuffer::new(value).expect("bounded actual filter/query"))
            .collect(),
        focus: 0,
        hits: vec![],
        field_hits: vec![],
        ready: false,
        finder: FinderV2::default(),
    };
    editor.refresh_finder();
    // Only a valid, explicitly requested current editor stops Watch. Stop's
    // normal epoch change is retained; no source/clock is renewed by editing.
    stop(app, false);
    let state = app
        .scoped_actions_v1
        .as_mut()
        .expect("same exclusive current page");
    editor.epoch = state.epoch;
    state.editor_v2 = Some(editor);
    app.overlay = Some(Overlay::ScopedActionsReadV2);
    app.hits.clear();
}
fn queue(app: &mut App, work: Result<Arc<ScopedActionsWorkV1>, String>) {
    match work {
        Ok(work) => {
            if let Some(state) = &mut app.scoped_actions_v1 {
                state.epoch = work.epoch;
            }
            app.scoped_actions_work_v1 = Some(work);
            app.scoped_actions_cancel_v1 = false;
            prepare_policy(app, true);
            app.hits.clear();
            app.status="Original Actions read queued • original30s includes queue wait • Back cancels • no mutation".into();
        }
        Err(reason) => explain(app, reason),
    }
}
fn apply(app: &mut App) {
    if !app
        .scoped_actions_v1
        .as_ref()
        .and_then(|s| s.editor_v2.as_ref())
        .is_some_and(|editor| editor.owns(app) && editor.ready)
    {
        return;
    }
    let Some(editor) = app
        .scoped_actions_v1
        .as_mut()
        .and_then(|s| s.editor_v2.take())
    else {
        return;
    };
    let epoch = editor.epoch.wrapping_add(1);
    let work = match editor.kind {
        EditorKindV2::Filters => {
            let opt = |i: usize| {
                (!editor.fields[i].text.is_empty()).then(|| editor.fields[i].text.clone())
            };
            let workflow = if editor.fields[4].text.is_empty() {
                Ok(None)
            } else {
                editor.fields[4]
                    .text
                    .parse::<u64>()
                    .ok()
                    .filter(|n| *n > 0)
                    .map(Some)
                    .ok_or("Workflow filter must be a positive numeric ID")
            };
            workflow
                .map_err(str::to_owned)
                .and_then(|workflow_id| {
                    ScopedRunQueryV2::first(RunFilter {
                        actor: opt(0),
                        branch: opt(1),
                        event: opt(2),
                        status: opt(3),
                        workflow_id,
                        head_sha: opt(5),
                        created: opt(6),
                    })
                    .map_err(|_| "Invalid bounded filter; no query sent".into())
                })
                .and_then(|query| {
                    ScopedActionsWorkV1::capture_filter_v2(editor.page.clone(), query, epoch)
                })
        }
        EditorKindV2::Search(mode) => {
            // Jump to the chosen result, centred, when there is one.
            let chosen = editor
                .finder
                .ranking
                .hits
                .get(editor.finder.selected)
                .map(|hit| hit.line);
            // The installed search page lists log lines from row 0.
            let position = match (chosen, app.actions.as_ref()) {
                (Some(line), _) => (line, line.saturating_sub(app.actions_visible_rows / 2)),
                (None, Some(w)) => (w.selected, w.top),
                _ => (0, 0),
            };
            ScopedActionsWorkV1::capture_search_v2(
                editor.page.clone(),
                editor.fields[0].text.clone(),
                mode,
                position,
                epoch,
            )
        }
    };
    if let Err(reason) = work {
        app.scoped_actions_v1.as_mut().expect("owned").editor_v2 = Some(editor);
        app.status = reason;
        return;
    }
    app.overlay = None;
    app.hits.clear();
    queue(app, work);
}
fn next(app: &mut App, forward: bool) {
    let Some(state) = app.scoped_actions_v1.as_ref().filter(|s| current(app, s)) else {
        return;
    };
    let Some(matches) = state
        .matches_v2
        .as_ref()
        .filter(|m| m.matches_page(&state.page))
    else {
        explain(
            app,
            "No current matching captured-log lines; Search loaded logs first",
        );
        return;
    };
    if matches.indices().is_empty() {
        app.status = "No matching lines in this actual captured job log".into();
        return;
    }
    let errors = matches.query() == ERROR_PATTERN_V1;
    let page = state.page.clone();
    let indices: Vec<usize> = matches.indices().to_vec();
    let Some(w) = app.actions.as_mut().filter(|w| w.view == ActionsView::Logs) else {
        return;
    };
    // A truncated log starts with one notice row before line 1.
    let offset = log_row_offset(w, &page);
    let line = w.selected.saturating_sub(offset);
    let at_line = w.selected >= offset;
    let found = if forward {
        indices
            .iter()
            .copied()
            .find(|i| !at_line || *i > line)
            .unwrap_or(indices[0])
    } else {
        indices
            .iter()
            .copied()
            .rev()
            .find(|i| at_line && *i < line)
            .unwrap_or(*indices.last().expect("nonempty"))
    };
    let position = indices.iter().position(|i| *i == found).unwrap_or(0) + 1;
    w.selected = found + offset;
    w.top = w.selected.saturating_sub(app.actions_visible_rows / 2);
    app.actions_content_focused = true;
    app.status = format!(
        "Captured log {} line {} • {position} of {} • n/N next/previous • local navigation only • no Git/review anchor",
        if errors { "error" } else { "matching" },
        found + 1,
        indices.len()
    );
}
/// Rows before log line 1 (the truncation notice of a partial log).
fn log_row_offset(
    w: &crate::actions_workspace::ActionsWorkspace,
    page: &ScopedActionsPageV1,
) -> usize {
    page.loaded_log_snapshot_v2()
        .map_or(0, |snapshot| {
            w.rows.len().saturating_sub(snapshot.text().lines().count())
        })
        .min(1)
}
/// Same original Search work as the finder, with the error-line pattern.
fn first_error(app: &mut App) {
    let Some(state) = app.scoped_actions_v1.as_ref().filter(|s| current(app, s)) else {
        return;
    };
    let Some(snapshot) = state.page.loaded_log_snapshot_v2() else {
        explain(
            app,
            "Open a job's logs first; First error searches the loaded log",
        );
        return;
    };
    let Some(line) = first_error_line_v1(snapshot.text()) else {
        if app.overlay == Some(Overlay::ScopedActionsReadV2) {
            close(app);
        }
        app.status = "No error lines (##[error], error:, FAILED, nonzero exit code) in this captured job log • nothing changed".into();
        return;
    };
    let page = state.page.clone();
    // The installed search page lists log lines from row 0.
    let position = (line, line.saturating_sub(app.actions_visible_rows / 2));
    if app.overlay == Some(Overlay::ScopedActionsReadV2) {
        close(app);
    }
    // Like opening Search, a local search stops this page's finite Watch.
    stop(app, false);
    let Some(epoch) = app.scoped_actions_v1.as_ref().map(|state| state.epoch) else {
        return;
    };
    let work = ScopedActionsWorkV1::capture_search_v2(
        page,
        ERROR_PATTERN_V1.into(),
        LoadedLogSearchModeV2::Regex,
        position,
        epoch.wrapping_add(1),
    );
    queue(app, work);
}
fn clear(app: &mut App) {
    if let Some(state) = &mut app.scoped_actions_v1 {
        if let Some(matches) = state
            .matches_v2
            .take()
            .filter(|m| m.matches_page(&state.page))
        {
            if let Some(w) = app.actions.as_mut().filter(|w| w.view == ActionsView::Logs) {
                let offset = log_row_offset(w, &state.page);
                for &i in matches.indices() {
                    if let Some(row) = w.rows.get_mut(i + offset) {
                        // Runner error markers keep their failure colour.
                        row.role = if row.label.contains("##[error]") {
                            crate::theme::SemanticToken::Failure
                        } else {
                            crate::theme::SemanticToken::Normal
                        };
                    }
                }
            }
            state.retained_bytes = state.retained_bytes.saturating_sub(matches.owned_bytes());
        }
    }
    app.status =
        "Captured-log matches cleared • original rows retained • no external effects".into();
}
pub fn intercept(app: &mut App, input: &ActionsAction) -> bool {
    let ui = match input {
        ActionsAction::ScopedReadV2(ScopedActionsReadActionV2::Ui(ui)) => Some(*ui),
        ActionsAction::Compose(crate::actions_workspace::ActionsCommand::SearchLogs) => {
            Some(ReadUiActionV2::OpenSearch)
        }
        ActionsAction::Watch => Some(ReadUiActionV2::Watch),
        ActionsAction::Back
            if app
                .scoped_actions_v1
                .as_ref()
                .is_some_and(|s| s.watch_v2.is_some()) =>
        {
            stop(app, true);
            return true;
        }
        _ => None,
    };
    let Some(ui) = ui else {
        return false;
    };
    use ReadUiActionV2::*;
    if ui == Close {
        close(app);
        return true;
    }
    if ui == Quit {
        if !app
            .scoped_actions_v1
            .as_ref()
            .and_then(|s| s.editor_v2.as_ref())
            .is_some_and(|editor| editor.owns(app))
        {
            return true;
        }
        close(app);
        retire(app);
        app.dispatch(Action::Quit);
        return true;
    }
    if app.overlay == Some(Overlay::ScopedActionsReadV2) {
        let owned = app
            .scoped_actions_v1
            .as_ref()
            .and_then(|s| s.editor_v2.as_ref())
            .is_some_and(|e| e.owns(app));
        if !owned {
            close(app);
            return true;
        }
        if ui == Apply {
            apply(app);
            return true;
        }
        if ui == FirstError {
            first_error(app);
            return true;
        }
        if let Pick(index) = ui {
            if let Some(e) = app
                .scoped_actions_v1
                .as_mut()
                .and_then(|s| s.editor_v2.as_mut())
                .filter(|e| index < e.finder.ranking.hits.len())
            {
                e.finder.selected = index;
                apply(app);
            }
            return true;
        }
        if let Some(e) = app
            .scoped_actions_v1
            .as_mut()
            .and_then(|s| s.editor_v2.as_mut())
        {
            match ui {
                Mode(mode) if matches!(e.kind, EditorKindV2::Search(_)) => {
                    e.kind = EditorKindV2::Search(mode);
                    // Typing continues in the query after a mode toggle.
                    e.focus = 0;
                    e.refresh_finder();
                }
                Focus(i) => e.focus = i.min(e.count() - 1),
                Field(i, column, extend) if i < e.fields.len() => {
                    e.focus = i;
                    e.fields[i].click(0, column, extend);
                }
                _ => {}
            }
        }
        return true;
    }
    if app.overlay.is_some() {
        return true;
    }
    if ui == Watch
        && app
            .scoped_actions_v1
            .as_ref()
            .is_some_and(|s| s.watch_v2.is_some())
    {
        stop(app, true);
        return true;
    }
    if matches!(ui, OpenFilters | OpenSearch) {
        open(app, ui == OpenFilters);
        return true;
    }
    if app.scoped_actions_work_v1.is_some()
        || app.scoped_actions_owner_v1.is_some()
        || app.loading[View::Actions.index()]
    {
        explain(
            app,
            "Current original Actions read pending; Cancel or wait before local controls",
        );
        return true;
    }
    match ui {
        Next(forward) => next(app, forward),
        Clear => clear(app),
        FirstError => first_error(app),
        Watch => {
            let Some(state) = app.scoped_actions_v1.as_ref().filter(|s| current(app, s)) else {
                return true;
            };
            if !matches!(
                state.page.view(),
                ActionsView::Run | ActionsView::Jobs | ActionsView::Logs
            ) || app
                .actions
                .as_ref()
                .is_none_or(|workspace| workspace.run.is_none())
            {
                explain(app,"Open the actual Run, Jobs or selected-job Logs view for finite Watch; this page has no Watch control or request");
                return true;
            }
            match ActionsReadWatchV2::start(&state.page, state.page.view() == ActionsView::Logs) {
                Ok(watch) => {
                    app.scoped_actions_v1.as_mut().expect("current").watch_v2 = Some(watch);
                    if let Some(w) = &mut app.actions {
                        w.watching = true;
                    }
                    app.status="Finite Watch armed • exact original run/attempt/head/job • first tick in5s • 12ticks/120s/32MiB • w Stop • no mutation".into();
                }
                Err(reason) => explain(app, reason),
            }
        }
        _ => {}
    }
    true
}
/// Same main-loop schedule and same Work/slot. A tick cannot rearm the quota,
/// change A/H, or start behind an editor/newer source. Stop shares its token.
pub fn tick(app: &mut App) {
    synchronize(app);
    let Some(state) = app.scoped_actions_v1.as_ref() else {
        return;
    };
    let Some(watch) = state.watch_v2.clone() else {
        return;
    };
    if !current(app, state) || !watch.matches(&state.page) || !watch.active() {
        stop(app, false);
        app.status="Finite Watch exhausted, expired or target changed • explicit rearm required • retained snapshot".into();
        return;
    }
    if app.view != View::Actions || app.overlay.is_some() || newer_foreign_intent(app) {
        stop(app, false);
        return;
    }
    if !watch.due()
        || app.loading[View::Actions.index()]
        || app.scoped_actions_owner_v1.is_some()
        || app.scoped_actions_work_v1.is_some()
    {
        return;
    }
    let work = ScopedActionsWorkV1::capture_watch_v2(
        state.page.clone(),
        watch,
        state.epoch.wrapping_add(1),
    );
    queue(app, work);
}
#[cfg(test)]
pub(crate) fn capture_watch_kernel_for_test_v2(app: &mut App) -> Arc<ScopedActionsWorkV1> {
    // Scheduling is NOT proved here. Real-binary journeys wait for the real
    // five-second cadence; this helper exercises the identical closed worker
    // constructor/current state and quota without sleeping code tests.
    let state = app
        .scoped_actions_v1
        .as_mut()
        .expect("actual current state");
    let work = ScopedActionsWorkV1::capture_watch_v2(
        state.page.clone(),
        state.watch_v2.clone().expect("actual explicit Watch"),
        state.epoch.wrapping_add(1),
    )
    .expect("actual original watch work");
    state.epoch = work.epoch;
    work
}
/// Borrow before other handlers clone Paste. Mouse drag uses the same genuine
/// grapheme editor/column mapping as keyboard Shift-selection.
pub fn event(app: &mut App, event: &Event) -> bool {
    let modal = app.overlay == Some(Overlay::ScopedActionsReadV2);
    if !modal {
        if app.view != View::Actions || app.overlay.is_some() || app.scoped_actions_v1.is_none() {
            return false;
        }
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                let logs = app
                    .scoped_actions_v1
                    .as_ref()
                    .is_some_and(|s| s.page.view() == ActionsView::Logs);
                // n/N step through matches only while a search is installed,
                // so Runners and Annotations keep their keys otherwise.
                let matching = logs
                    && app.scoped_actions_v1.as_ref().is_some_and(|s| {
                        s.matches_v2
                            .as_ref()
                            .is_some_and(|m| m.matches_page(&s.page) && !m.indices().is_empty())
                    });
                let ui = match key.code {
                    KeyCode::Char('e') if logs => Some(ReadUiActionV2::FirstError),
                    KeyCode::Char('n') if matching => Some(ReadUiActionV2::Next(true)),
                    KeyCode::Char('N') if matching => Some(ReadUiActionV2::Next(false)),
                    KeyCode::Char('f') => Some(ReadUiActionV2::OpenFilters),
                    KeyCode::Char('/') => Some(ReadUiActionV2::OpenSearch),
                    KeyCode::Char('>') => Some(ReadUiActionV2::Next(true)),
                    KeyCode::Char('<') => Some(ReadUiActionV2::Next(false)),
                    KeyCode::Char('x') => Some(ReadUiActionV2::Clear),
                    KeyCode::Char('w') => Some(ReadUiActionV2::Watch),
                    _ => None,
                };
                if let Some(ui) = ui {
                    app.dispatch(Action::Actions(action(ui)));
                    return true;
                }
            }
        }
        return false;
    }
    if !app
        .scoped_actions_v1
        .as_ref()
        .and_then(|s| s.editor_v2.as_ref())
        .is_some_and(|e| e.owns(app))
    {
        close(app);
        return true;
    }
    let mut chosen = None;
    if let Some(e) = app
        .scoped_actions_v1
        .as_mut()
        .and_then(|s| s.editor_v2.as_mut())
    {
        match event {
            Event::Resize(_, _) => {
                e.hits.clear();
                e.field_hits.clear();
                e.ready = false;
            }
            Event::Paste(value) => e.insert(value),
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let extend = key.modifiers.contains(KeyModifiers::SHIFT);
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    chosen = Some(ReadUiActionV2::Quit);
                } else {
                    match key.code {
                        KeyCode::Esc => chosen = Some(ReadUiActionV2::Close),
                        KeyCode::Tab | KeyCode::BackTab => {
                            e.focus = (e.focus
                                + if key.code == KeyCode::Tab {
                                    1
                                } else {
                                    e.count() - 1
                                })
                                % e.count();
                        }
                        KeyCode::Enter => {
                            let search = matches!(e.kind, EditorKindV2::Search(_));
                            chosen = Some(if e.focus == e.count() - 1 {
                                ReadUiActionV2::Quit
                            } else if e.focus == e.close_focus() {
                                ReadUiActionV2::Close
                            } else if search && matches!(e.focus, 1..=3) {
                                ReadUiActionV2::Mode(match e.focus {
                                    1 => LoadedLogSearchModeV2::Fuzzy,
                                    2 => LoadedLogSearchModeV2::ExactText,
                                    _ => LoadedLogSearchModeV2::Regex,
                                })
                            } else if search && e.focus == 4 {
                                ReadUiActionV2::FirstError
                            } else {
                                ReadUiActionV2::Apply
                            });
                        }
                        KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown
                            if matches!(e.kind, EditorKindV2::Search(_)) =>
                        {
                            e.step(match key.code {
                                KeyCode::Up => -1,
                                KeyCode::Down => 1,
                                KeyCode::PageUp => -10,
                                _ => 10,
                            });
                        }
                        KeyCode::Left if e.focus < e.fields.len() => {
                            e.fields[e.focus].horizontal(false, extend)
                        }
                        KeyCode::Right if e.focus < e.fields.len() => {
                            e.fields[e.focus].horizontal(true, extend)
                        }
                        KeyCode::Home if e.focus < e.fields.len() => {
                            e.fields[e.focus].line_edge(false, false, extend)
                        }
                        KeyCode::End if e.focus < e.fields.len() => {
                            e.fields[e.focus].line_edge(true, false, extend)
                        }
                        KeyCode::Backspace if e.focus < e.fields.len() => {
                            e.fields[e.focus].delete_backward()
                        }
                        KeyCode::Delete if e.focus < e.fields.len() => {
                            e.fields[e.focus].delete_forward()
                        }
                        KeyCode::Char('a')
                            if e.focus < e.fields.len()
                                && key.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            e.fields[e.focus].select_all()
                        }
                        KeyCode::Char(ch)
                            if !key
                                .modifiers
                                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                        {
                            e.insert(&ch.to_string())
                        }
                        _ => {}
                    }
                }
            }
            Event::Mouse(mouse)
                if matches!(e.kind, EditorKindV2::Search(_))
                    && matches!(
                        mouse.kind,
                        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                    ) =>
            {
                e.step(if mouse.kind == MouseEventKind::ScrollUp {
                    -3
                } else {
                    3
                });
            }
            Event::Mouse(mouse)
                if matches!(
                    mouse.kind,
                    MouseEventKind::Down(MouseButton::Left)
                        | MouseEventKind::Drag(MouseButton::Left)
                ) =>
            {
                let extend = mouse.kind == MouseEventKind::Drag(MouseButton::Left)
                    || mouse.modifiers.contains(KeyModifiers::SHIFT);
                chosen = e
                    .field_hits
                    .iter()
                    .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
                    .map(|(r, i)| {
                        ReadUiActionV2::Field(
                            *i,
                            usize::from(mouse.column - r.x) + e.fields[*i].left_column,
                            extend,
                        )
                    });
                if chosen.is_none() && !extend {
                    chosen = e
                        .hits
                        .iter()
                        .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
                        .map(|(_, a)| *a);
                }
            }
            _ => {}
        }
        e.refresh_finder();
    }
    if let Some(ui) = chosen {
        intercept(app, &action(ui));
    }
    true
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.overlay != Some(Overlay::ScopedActionsReadV2) {
        return;
    }
    if !app
        .scoped_actions_v1
        .as_ref()
        .and_then(|s| s.editor_v2.as_ref())
        .is_some_and(|e| e.owns(app))
    {
        close(app);
        return;
    }
    let theme = app.theme();
    let e = app
        .scoped_actions_v1
        .as_mut()
        .expect("owned")
        .editor_v2
        .as_mut()
        .expect("owned");
    let filters = matches!(e.kind, EditorKindV2::Filters);
    e.hits.clear();
    e.field_hits.clear();
    e.ready = area.width >= 64 && area.height >= if filters { 20 } else { 12 };
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(if !e.ready {
                " Resize for original Actions editor "
            } else if filters {
                " Original repository run filters "
            } else {
                " Find in job log "
            }),
        area,
    );
    let inner = area.inner(ratatui::layout::Margin::new(1, 1));
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if !e.ready {
        for (y, label, ui) in [
            (inner.y, "[Esc Close editor]", ReadUiActionV2::Close),
            (inner.bottom() - 1, "[Ctrl-C Quit]", ReadUiActionV2::Quit),
        ] {
            if ui == ReadUiActionV2::Quit && inner.height < 2 {
                continue;
            }
            let rect = Rect::new(inner.x, y, inner.width, 1);
            frame.render_widget(
                Paragraph::new(label).style(theme.style(crate::theme::SemanticToken::Reference)),
                rect,
            );
            e.hits.push((rect, ui));
        }
        return;
    }
    if !filters {
        draw_finder(frame, e, theme, inner);
        return;
    }
    frame.render_widget(Paragraph::new(if filters{"Each field is optional; Apply requests page1 only. Partial pages are not absence. No workflow name/HEAD guessing."}else{"Exact retained job log only • 0 GH/SQL on Search • log lines are not Git/review coordinates."}).wrap(Wrap{trim:false}),Rect::new(inner.x,inner.y,inner.width,3));
    let labels = if filters {
        &[
            "Actor",
            "Branch",
            "Event",
            "Status",
            "Workflow ID",
            "Head SHA",
            "Created",
        ][..]
    } else {
        &["Query"][..]
    };
    for (i, label) in labels.iter().enumerate() {
        let y = inner.y + 3 + i as u16;
        let prefix = format!("{label}: ");
        let prefix_width =
            crate::editor::display_width(&prefix).min(usize::from(inner.width)) as u16;
        let rect = Rect::new(
            inner.x + prefix_width,
            y,
            inner.width.saturating_sub(prefix_width),
            1,
        );
        e.fields[i].ensure_visible(usize::from(rect.width), 1);
        let shown = crate::editor::display_line(
            &e.fields[i].text,
            e.fields[i].left_column,
            usize::from(rect.width),
        );
        frame.render_widget(
            Paragraph::new(format!("{prefix}{shown}"))
                .style(theme.style(crate::theme::SemanticToken::Normal)),
            Rect::new(inner.x, y, inner.width, 1),
        );
        e.field_hits.push((rect, i));
        if e.focus == i && rect.width > 0 {
            let (_, column) = e.fields[i].position();
            frame.set_cursor_position((
                rect.x
                    + (column.saturating_sub(e.fields[i].left_column) as u16).min(rect.width - 1),
                y,
            ));
        }
    }
    if let EditorKindV2::Search(mode) = e.kind {
        frame.render_widget(
            Paragraph::new(format!(
                "Mode: {mode:?} • literal/regex share one bounded matcher"
            )),
            Rect::new(inner.x, inner.y + 4, inner.width, 1),
        );
    }
    let controls: Vec<(usize, &str, ReadUiActionV2)> = if filters {
        vec![
            (7, "[Apply filters]", ReadUiActionV2::Apply),
            (8, "[Esc Close editor]", ReadUiActionV2::Close),
            (9, "[Ctrl-C Quit]", ReadUiActionV2::Quit),
        ]
    } else {
        vec![
            (
                1,
                "[Exact text]",
                ReadUiActionV2::Mode(LoadedLogSearchModeV2::ExactText),
            ),
            (
                2,
                "[Regex]",
                ReadUiActionV2::Mode(LoadedLogSearchModeV2::Regex),
            ),
            (3, "[Search captured logs]", ReadUiActionV2::Apply),
            (4, "[Esc Close editor]", ReadUiActionV2::Close),
            (5, "[Ctrl-C Quit]", ReadUiActionV2::Quit),
        ]
    };
    for (offset, (focus, label, ui)) in controls.into_iter().enumerate() {
        let y = inner.y + 4 + labels.len() as u16 + offset as u16;
        let rect = Rect::new(inner.x, y, inner.width, 1);
        frame.render_widget(
            Paragraph::new(label).style(
                theme
                    .style(crate::theme::SemanticToken::Reference)
                    .add_modifier(if e.focus == focus {
                        Modifier::UNDERLINED
                    } else {
                        Modifier::empty()
                    }),
            ),
            rect,
        );
        e.hits.push((rect, ui));
    }
    frame.render_widget(Paragraph::new("Tab/Shift-Tab • keyboard/mouse selection • bounded volatile input • explicit Apply, no draft"),Rect::new(inner.x,inner.bottom()-1,inner.width,1));
}

/// One row: label at `x`, returns the rect it occupied.
fn put_label(
    frame: &mut Frame,
    style: ratatui::style::Style,
    label: &str,
    x: u16,
    y: u16,
    right: u16,
) -> Option<Rect> {
    let width = crate::editor::display_width(label) as u16;
    if x >= right || x + width > right {
        return None;
    }
    let rect = Rect::new(x, y, width, 1);
    frame.render_widget(Paragraph::new(label.to_owned()).style(style), rect);
    Some(rect)
}
/// TASK-116 finder: query, mode toggles, live count, ranked results with the
/// matched characters highlighted, and the action row. Every control is both
/// a key (shown in its label or the hint) and a click target.
fn draw_finder(frame: &mut Frame, e: &mut ReadEditorV2, theme: crate::theme::Theme, inner: Rect) {
    use crate::theme::SemanticToken as T;
    let EditorKindV2::Search(mode) = e.kind else {
        return;
    };
    let right = inner.right();
    // Row 0: the query.
    let prefix = "Find: ";
    let prefix_width = prefix.len() as u16;
    let field = Rect::new(
        inner.x + prefix_width,
        inner.y,
        inner.width.saturating_sub(prefix_width),
        1,
    );
    e.fields[0].ensure_visible(usize::from(field.width), 1);
    let shown = crate::editor::display_line(
        &e.fields[0].text,
        e.fields[0].left_column,
        usize::from(field.width),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(prefix, theme.style(T::Accent).add_modifier(Modifier::BOLD)),
            Span::styled(shown, theme.style(T::Normal)),
        ])),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    e.field_hits.push((field, 0));
    if e.focus == 0 && field.width > 0 {
        let (_, column) = e.fields[0].position();
        frame.set_cursor_position((
            field.x + (column.saturating_sub(e.fields[0].left_column) as u16).min(field.width - 1),
            inner.y,
        ));
    }
    // Row 1: optional mode toggles and First error.
    let y = inner.y + 1;
    let mut x = inner.x;
    for (focus, label, ui) in [
        (
            1,
            "[Fuzzy]",
            ReadUiActionV2::Mode(LoadedLogSearchModeV2::Fuzzy),
        ),
        (
            2,
            "[Exact text]",
            ReadUiActionV2::Mode(LoadedLogSearchModeV2::ExactText),
        ),
        (
            3,
            "[Regex]",
            ReadUiActionV2::Mode(LoadedLogSearchModeV2::Regex),
        ),
        (4, "[e First error]", ReadUiActionV2::FirstError),
    ] {
        let active = matches!(ui, ReadUiActionV2::Mode(m) if m == mode);
        let mut style = theme.style(if active { T::Accent } else { T::Reference });
        if active {
            style = style.add_modifier(Modifier::BOLD | Modifier::REVERSED);
        }
        if e.focus == focus {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        if let Some(rect) = put_label(frame, style, label, x, y, right) {
            e.hits.push((rect, ui));
            x = rect.right() + 1;
        }
    }
    // Row 2: the live count (or why there is none).
    let ranking = &e.finder.ranking;
    let (count, role) = if let Some(error) = &e.finder.error {
        (format!("{error} • Mode: {mode:?}"), T::Failure)
    } else if e.fields[0].text.trim().is_empty() {
        (
            format!(
                "Type to filter {} log lines • Mode: {mode:?} • e jumps to the first error",
                ranking.lines
            ),
            T::Muted,
        )
    } else if ranking.total == 0 {
        (
            format!("0 of {} lines match • Mode: {mode:?}", ranking.lines),
            T::Pending,
        )
    } else {
        (
            format!(
                "{} of {} lines match{} • Mode: {mode:?}",
                ranking.total,
                ranking.lines,
                if ranking.total > ranking.hits.len() {
                    format!(" (best {} shown)", ranking.hits.len())
                } else {
                    String::new()
                }
            ),
            T::Success,
        )
    };
    frame.render_widget(
        Paragraph::new(count).style(theme.style(role)),
        Rect::new(inner.x, inner.y + 2, inner.width, 1),
    );
    // Rows 3..: ranked results; the selected one carries the ▌ bar.
    let list_top = inner.y + 3;
    let list_rows = inner.height.saturating_sub(5) as usize;
    let finder = &mut e.finder;
    if finder.selected < finder.top {
        finder.top = finder.selected;
    } else if list_rows > 0 && finder.selected >= finder.top + list_rows {
        finder.top = finder.selected + 1 - list_rows;
    }
    let number_width = finder.ranking.lines.max(1).to_string().len();
    let lines: Vec<&str> = e
        .page
        .loaded_log_snapshot_v2()
        .map(|snapshot| snapshot.text().lines().collect())
        .unwrap_or_default();
    for (offset, (index, hit)) in finder
        .ranking
        .hits
        .iter()
        .enumerate()
        .skip(finder.top)
        .take(list_rows)
        .enumerate()
    {
        let y = list_top + offset as u16;
        let rect = Rect::new(inner.x, y, inner.width, 1);
        let selected = index == finder.selected;
        let text = lines.get(hit.line).copied().unwrap_or_default();
        let gutter = format!(
            "{}{:>number_width$} │ ",
            if selected { "▌" } else { " " },
            hit.line + 1
        );
        let avail = usize::from(inner.width).saturating_sub(gutter.chars().count());
        let start = window_start_v1(text, &hit.positions, avail);
        let base = if selected {
            theme.style(T::Selection)
        } else {
            theme.style(T::Normal)
        };
        let mark = theme
            .style(T::Accent)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        let mut spans = vec![Span::styled(
            gutter,
            if selected {
                base
            } else {
                theme.style(T::Muted)
            },
        )];
        let mut run = String::new();
        let mut run_marked = false;
        let mut wanted = hit.positions.iter().peekable();
        for (i, ch) in text.chars().enumerate().skip(start).take(avail) {
            while wanted.peek().is_some_and(|p| **p < i) {
                wanted.next();
            }
            let marked = wanted.peek().is_some_and(|p| **p == i);
            if marked != run_marked && !run.is_empty() {
                spans.push(Span::styled(
                    std::mem::take(&mut run),
                    if run_marked { base.patch(mark) } else { base },
                ));
            }
            run_marked = marked;
            // Control characters (tabs, escapes) print as spaces.
            run.push(if ch.is_control() { ' ' } else { ch });
        }
        if !run.is_empty() {
            spans.push(Span::styled(
                run,
                if run_marked { base.patch(mark) } else { base },
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)).style(base), rect);
        e.hits.push((rect, ReadUiActionV2::Pick(index)));
    }
    // Last rows: actions and the key hint.
    let y = inner.bottom().saturating_sub(2);
    let mut x = inner.x;
    for (focus, label, ui) in [
        (5, "[Enter Jump to line]", ReadUiActionV2::Apply),
        (6, "[Esc Close editor]", ReadUiActionV2::Close),
        (7, "[Ctrl-C Quit]", ReadUiActionV2::Quit),
    ] {
        let mut style = theme.style(T::Reference);
        if e.focus == focus {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        if let Some(rect) = put_label(frame, style, label, x, y, right) {
            e.hits.push((rect, ui));
            x = rect.right() + 1;
        }
    }
    frame.render_widget(
        Paragraph::new("Type to filter • ↑↓ or click choose • Enter jumps • then n/N next/previous • Tab modes")
            .style(theme.style(T::Muted)),
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
    );
}
