//! The History commit detail (TASK-99.4): the selected commit beside the History table from
//! [`crate::ui_chrome::SPLIT_MIN_WIDTH`] columns, and the same content full width as the
//! `D Details` drill-down below that. It shows the short and full SHA, every ref chip, the
//! author and committer chips, relative and UTC dates, the ordered parents (each one a link),
//! the whole message wrapped at word boundaries and the changed files with `+`/`−` counts.
//!
//! Everything but the changed files comes from the loaded graph walk. The files are one
//! bounded `git diff-tree` per commit, read on a worker thread only after the cursor has rested
//! on the commit for [`DEBOUNCE_V994`], and cached per commit, so scrolling fast through the
//! table runs no Git. The detail never writes: diff, file history and blame open the same
//! read-only views the rest of Repository opens.
//!
//! Items are the parents, then the shown files. Keyboard and mouse share one meaning: ↑↓ (or a
//! click on a file) select an item, Enter (or the parent link) opens it, and `Enter diff`,
//! `f file history`, `Y blame` and `y copy SHA` are both keys and clickable tokens.
use crate::{
    app::{safe_text, Action, App, HitRegion, View},
    footer::{ControlGroup, KeyControl},
    git::{HistoryChangedFileV994, HistoryChangedFilesV994, OriginalGraphCommitV1},
    identity::{ActorIdentity, GitRole, InitialsChip},
    providers::runner::CancellationToken,
    repository_graph_v1::{GraphActionV1, GraphViewV1, HistoryDetailActV994 as Act},
    theme::{ColorMode, Role, SemanticToken, Theme},
    workspace::{RowAction, Workspace},
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::{
    collections::VecDeque,
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

/// How long the cursor rests on a commit before its changed files are read.
pub const DEBOUNCE_V994: Duration = Duration::from_millis(150);
/// Commits whose changed files stay cached.
const CACHE: usize = 32;
/// Width of the label column (`Committer`).
const LABEL: usize = 9;
/// Chips listed in the detail before the rest are counted.
const MAX_CHIPS: usize = 64;
/// Message rows shown before the rest are counted.
const MAX_MESSAGE_ROWS: usize = 1024;
/// The pane title.
pub const TITLE: &str = "Commit";

/// The detail's own actions; the rest are [`GraphActionV1::Detail`] row actions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DetailUiActionV994 {
    /// Put the detail cursor on an item (a parent or a changed file).
    Select(usize),
    /// Open the item under the detail cursor (what Enter does in the focused detail).
    Activate,
    /// Show the full SHA to copy: ghtui has no clipboard integration.
    CopySha,
}

type FilesResult = Result<Arc<HistoryChangedFilesV994>, String>;

/// The changed files of the shown commit.
#[derive(Clone, Debug)]
pub enum FilesV994 {
    /// Waiting for the cursor to rest, or for the read.
    Waiting,
    Ready(Arc<HistoryChangedFilesV994>),
    Failed(String),
}

struct Inflight {
    commit: String,
    // Behind a mutex so the App stays `Sync` (tests share it across scoped threads).
    receiver: std::sync::Mutex<mpsc::Receiver<FilesResult>>,
    cancel: CancellationToken,
}

/// Detail cursor, scroll and the per-commit changed-files cache.
#[derive(Default)]
pub struct HistoryDetailStateV994 {
    cache: VecDeque<(String, FilesResult)>,
    origin: usize,
    wanted: Option<(String, Instant)>,
    inflight: Option<Inflight>,
    /// The commit the cursor and scroll belong to.
    shown: String,
    /// The item under the detail cursor: parents first, then files.
    pub item: usize,
    /// Scroll the item into view on the next paint.
    reveal: bool,
    /// Drill-down scroll (the split pane uses the shared preview scroll).
    scroll: usize,
    /// The painted drill-down body, for the wheel.
    area: Option<Rect>,
    /// Changed-files reads started, for tests of the debounce.
    reads: usize,
}

impl HistoryDetailStateV994 {
    /// Changed-files reads started so far.
    pub fn reads(&self) -> usize {
        self.reads
    }

    /// The cached files of `commit`, without starting a read.
    pub fn cached(&self, commit: &str) -> FilesV994 {
        match self.cache.iter().find(|(id, _)| id == commit) {
            Some((_, Ok(files))) => FilesV994::Ready(files.clone()),
            Some((_, Err(error))) => FilesV994::Failed(error.clone()),
            None => FilesV994::Waiting,
        }
    }

    /// Stores a result (tests insert files this way).
    pub fn store(&mut self, commit: String, result: Result<HistoryChangedFilesV994, String>) {
        self.cache.retain(|(id, _)| *id != commit);
        self.cache.push_back((commit, result.map(Arc::new)));
        while self.cache.len() > CACHE {
            self.cache.pop_front();
        }
    }

    fn poll(&mut self) {
        let Some(inflight) = &self.inflight else {
            return;
        };
        let received = match inflight.receiver.lock() {
            Ok(receiver) => receiver.try_recv(),
            Err(_) => Err(mpsc::TryRecvError::Disconnected),
        };
        let result = match received {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("The changed-files read stopped without a result".into())
            }
        };
        let commit = inflight.commit.clone();
        self.inflight = None;
        self.cache.retain(|(id, _)| *id != commit);
        self.cache.push_back((commit, result));
        while self.cache.len() > CACHE {
            self.cache.pop_front();
        }
    }

    /// The files of `commit`, starting their read once the cursor has rested on it.
    fn request(
        &mut self,
        workspace: &Workspace,
        commit: &str,
        parent: Option<&str>,
        now: Instant,
    ) -> FilesV994 {
        let origin = workspace
            .actor_origin
            .as_ref()
            .map_or(0, |origin| Arc::as_ptr(origin) as usize);
        if origin != self.origin {
            self.cache.clear();
            self.origin = origin;
            self.wanted = None;
            if let Some(inflight) = self.inflight.take() {
                inflight.cancel.cancel();
            }
        }
        self.poll();
        let cached = self.cached(commit);
        if !matches!(cached, FilesV994::Waiting) {
            return cached;
        }
        if let Some(inflight) = &self.inflight {
            if inflight.commit == commit {
                return FilesV994::Waiting;
            }
            // The cursor moved on: the old read is no longer wanted.
            inflight.cancel.cancel();
            self.inflight = None;
        }
        let rested = match &self.wanted {
            Some((wanted, since)) if wanted == commit => {
                now.saturating_duration_since(*since) >= DEBOUNCE_V994
            }
            _ => {
                self.wanted = Some((commit.to_owned(), now));
                false
            }
        };
        if !rested {
            return FilesV994::Waiting;
        }
        let Some(origin) = workspace.actor_origin.clone() else {
            let error = "Changed files need the original local source • open History again";
            self.store(commit.to_owned(), Err(error.into()));
            return FilesV994::Failed(error.into());
        };
        let (sender, receiver) = mpsc::channel();
        let cancel = CancellationToken::default();
        let (id, parent_id, token) = (commit.to_owned(), parent.map(str::to_owned), cancel.clone());
        let spawned = std::thread::Builder::new()
            .name("ghtui-history-files".into())
            .spawn(move || {
                let _ = sender.send(read_files_v994(&origin, &id, parent_id.as_deref(), token));
            });
        self.wanted = None;
        match spawned {
            Ok(_) => {
                self.reads += 1;
                self.inflight = Some(Inflight {
                    commit: commit.to_owned(),
                    receiver: std::sync::Mutex::new(receiver),
                    cancel,
                });
                FilesV994::Waiting
            }
            Err(error) => {
                let error = format!("Changed files could not start a reader: {error}");
                self.store(commit.to_owned(), Err(error.clone()));
                FilesV994::Failed(error)
            }
        }
    }

    /// Follows a new commit: the cursor starts on its first file and the scroll at the top.
    fn follow(&mut self, commit: &str, parents: usize) -> bool {
        if self.shown == commit {
            return false;
        }
        self.shown = commit.to_owned();
        self.item = parents;
        self.scroll = 0;
        self.reveal = false;
        true
    }
}

/// One read-only `diff-tree` of `commit` under a fresh bounded local read operation.
pub fn read_files_v994(
    origin: &crate::git::OriginalLocalActorOriginV1,
    commit: &str,
    parent: Option<&str>,
    cancel: CancellationToken,
) -> FilesResult {
    let operation =
        crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1::begin(
            origin, cancel,
        )?;
    let files = origin.with_saved_navigation_read_v1(&operation, |repository| {
        repository
            .history_changed_files_v994(commit, parent)
            .map_err(|error| error.to_string())
    });
    let sealed = operation.seal();
    let files = files?;
    sealed?;
    Ok(Arc::new(files))
}

/// Whether the History table is shown, so its commit detail is offered beside it.
pub fn offered(app: &App) -> bool {
    app.view == View::Repository
        && app.remote_repository.is_none()
        && app
            .workspace
            .as_ref()
            .is_some_and(crate::repository_ui::history_table::active)
}

/// Whether the full-width detail (the drill-down) replaces the History table.
pub fn drilldown(workspace: &Workspace) -> bool {
    workspace.graph_visible_v1() && workspace.graph_view_v1 == GraphViewV1::Details
}

fn drilldown_shown(app: &App) -> bool {
    app.view == View::Repository
        && app.remote_repository.is_none()
        && app.workspace.as_ref().is_some_and(drilldown)
}

/// The commit the detail shows: the History cursor row, or the drill-down's commit.
pub fn commit_id(app: &App) -> Option<String> {
    let workspace = app.workspace.as_ref()?;
    if drilldown_shown(app) {
        return Some(workspace.revision.clone());
    }
    if !offered(app) {
        return None;
    }
    match &workspace
        .display_rows_v1()
        .get(app.repository_list.selected)?
        .action
    {
        Some(RowAction::Commit(id)) => Some(id.clone()),
        _ => None,
    }
}

fn observed<'a>(workspace: &'a Workspace, id: &str) -> Option<&'a OriginalGraphCommitV1> {
    workspace.graph_state_v1.as_ref()?.observed(id)
}

fn detail(commit: &str, act: Act) -> Action {
    Action::RepositoryRow(RowAction::Graph(GraphActionV1::Detail {
        commit: commit.to_owned(),
        act,
    }))
}

/// An action with the reason it is unavailable; the reason wins when dispatched.
#[derive(Clone, Debug)]
struct Target {
    action: Action,
    reason: Option<String>,
}
impl Target {
    fn dispatched(&self) -> Action {
        self.reason
            .clone()
            .map_or_else(|| self.action.clone(), Action::Explain)
    }
}

/// The selected file of `commit`: the file under the cursor, else the first file.
fn selected_file(
    files: &FilesV994,
    parents: usize,
    item: usize,
) -> Result<HistoryChangedFileV994, String> {
    match files {
        FilesV994::Waiting => Err("Changed files are still loading • try again in a moment".into()),
        FilesV994::Failed(error) => Err(format!("Changed files unavailable: {error}")),
        FilesV994::Ready(files) if files.files.is_empty() => {
            Err("This commit changes no files • nothing to follow".into())
        }
        FilesV994::Ready(files) => Ok(files
            .files
            .get(item.saturating_sub(parents))
            .unwrap_or(&files.files[0])
            .clone()),
    }
}

/// What the detail's keys and tokens do for the shown commit.
struct Targets {
    commit: String,
    diff: Target,
    file_history: Target,
    blame: Target,
}

fn targets(app: &App) -> Option<Targets> {
    let commit = commit_id(app)?;
    let workspace = app.workspace.as_ref()?;
    let parents = observed(workspace, &commit)?.commit().parents.len();
    let diff = GraphActionV1::Detail {
        commit: commit.clone(),
        act: Act::OpenDiff,
    };
    let item = if app.history_detail_v994.shown == commit {
        app.history_detail_v994.item
    } else {
        parents
    };
    let file = selected_file(&app.history_detail_v994.cached(&commit), parents, item);
    let file_target = |act: fn(crate::git::RawPath) -> Act, blame: bool| match &file {
        Ok(file) => Target {
            action: detail(&commit, act(file.path.clone())),
            reason: (blame && file.status == 'D').then(|| {
                format!(
                    "{} is deleted in this commit • select its parent to blame the last version",
                    safe_text(&file.path.display())
                )
            }),
        },
        Err(reason) => Target {
            action: detail(&commit, Act::Details),
            reason: Some(reason.clone()),
        },
    };
    Some(Targets {
        diff: Target {
            reason: workspace.graph_action_reason_v1(&diff),
            action: Action::RepositoryRow(RowAction::Graph(diff)),
        },
        file_history: file_target(Act::FileHistory, false),
        blame: file_target(Act::Blame, true),
        commit,
    })
}

/// The detail's key-bar controls while History shows a commit.
pub fn footer_keys(app: &App) -> Vec<KeyControl> {
    let Some(targets) = targets(app) else {
        return Vec::new();
    };
    // Detail actions are row-level intents: like choosing a row they replace a running read,
    // so they stay available while one runs.
    let drill = drilldown_shown(app);
    let mut keys = Vec::new();
    if drill {
        keys.push(
            KeyControl::new(
                "Enter",
                "Open",
                Action::HistoryDetailV994(DetailUiActionV994::Activate),
            )
            .with_priority(3),
        );
    } else if app.repository_content_focused {
        keys.push(
            KeyControl::new("Enter", "Diff", targets.diff.action.clone())
                .with_priority(3)
                .disabled_by(targets.diff.reason.clone()),
        );
    }
    keys.push(
        KeyControl::new("f", "File history", targets.file_history.action.clone())
            .with_compact("File hist")
            .with_priority(6)
            .disabled_by(targets.file_history.reason.clone()),
    );
    keys.push(
        KeyControl::new("Y", "Blame", targets.blame.action.clone())
            .with_priority(8)
            .disabled_by(targets.blame.reason.clone()),
    );
    keys.push(
        KeyControl::new(
            "y",
            "Copy SHA",
            Action::HistoryDetailV994(DetailUiActionV994::CopySha),
        )
        .with_compact("SHA")
        .with_priority(9),
    );
    if !drill {
        // Below the split width the drill-down is the only way to the detail.
        let pane = app.preview_area_v92.is_some();
        keys.push(
            KeyControl::new("D", "Details", detail(&targets.commit, Act::Details))
                .with_priority(if pane { 40 } else { 4 })
                .in_group(ControlGroup::Navigate),
        );
    }
    keys
}

/// Keys of the focused detail (the split pane or the drill-down): ↑↓ move the item cursor,
/// PgUp/PgDn scroll, Enter opens the item. `P` in the History table opens the drill-down like
/// `D`. Returns whether the key was used.
pub fn key(app: &mut App, key: KeyEvent) -> bool {
    if app.view != View::Repository
        || app.overlay.is_some()
        || !key.modifiers.is_empty() && key.modifiers != crossterm::event::KeyModifiers::SHIFT
    {
        return false;
    }
    let drill = drilldown_shown(app) && app.repository_content_focused;
    let pane = offered(app) && crate::list_preview_v92::preview_focused(app);
    if !drill && !pane {
        if offered(app) && app.repository_content_focused && key.code == KeyCode::Char('P') {
            if let Some(commit) = commit_id(app) {
                app.dispatch(detail(&commit, Act::Details));
                return true;
            }
        }
        return false;
    }
    let Some(commit) = commit_id(app) else {
        return false;
    };
    let Some(items) = item_count(app, &commit) else {
        return false;
    };
    let state = &mut app.history_detail_v994;
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            state.item = state.item.min(items.saturating_sub(1)).saturating_sub(1);
            state.reveal = true;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.item = (state.item + 1).min(items.saturating_sub(1));
            state.reveal = true;
        }
        KeyCode::Home => {
            state.item = 0;
            state.reveal = true;
        }
        KeyCode::End => {
            state.item = items.saturating_sub(1);
            state.reveal = true;
        }
        KeyCode::PageUp | KeyCode::PageDown => {
            let delta: isize = if key.code == KeyCode::PageUp { -10 } else { 10 };
            if pane {
                app.preview_scroll_v92 = app.preview_scroll_v92.saturating_add_signed(delta);
            } else {
                state.scroll = state.scroll.saturating_add_signed(delta);
            }
        }
        KeyCode::Enter => dispatch(app, DetailUiActionV994::Activate),
        _ => return false,
    }
    true
}

/// The wheel over the drill-down scrolls it. Returns whether the event was used.
pub fn mouse(app: &mut App, column: u16, row: u16, wheel: Option<isize>) -> bool {
    let Some(delta) = wheel else {
        return false;
    };
    if app.overlay.is_some() || !drilldown_shown(app) {
        return false;
    }
    let state = &mut app.history_detail_v994;
    if !state
        .area
        .is_some_and(|area| area.contains((column, row).into()))
    {
        return false;
    }
    state.scroll = state.scroll.saturating_add_signed(delta);
    true
}

/// Items of `commit`: its parents, then its shown files.
fn item_count(app: &App, commit: &str) -> Option<usize> {
    let parents = observed(app.workspace.as_ref()?, commit)?
        .commit()
        .parents
        .len();
    let files = match app.history_detail_v994.cached(commit) {
        FilesV994::Ready(files) => files.files.len(),
        _ => 0,
    };
    Some(parents + files)
}

pub fn dispatch(app: &mut App, action: DetailUiActionV994) {
    let Some(commit) = commit_id(app) else {
        app.dispatch(Action::Explain(
            "No History commit is selected • move the bar to a commit row first".into(),
        ));
        return;
    };
    match action {
        DetailUiActionV994::Select(item) => {
            let state = &mut app.history_detail_v994;
            state.item = item;
            state.reveal = true;
            if offered(app) {
                crate::list_preview_v92::focus(app, crate::list_preview_v92::Pane::Preview);
            } else {
                app.repository_content_focused = true;
                app.status = "Commit detail focused • ↑↓ select • Enter opens • u back".into();
            }
        }
        DetailUiActionV994::Activate => {
            let Some(parents) = app
                .workspace
                .as_ref()
                .and_then(|workspace| observed(workspace, &commit))
                .map(|observed| observed.commit().parents.len())
            else {
                return;
            };
            let item = app.history_detail_v994.item;
            let action = if item < parents {
                detail(&commit, Act::Parent(item))
            } else {
                match targets(app) {
                    Some(targets) => targets.diff.dispatched(),
                    None => return,
                }
            };
            app.dispatch(action);
        }
        DetailUiActionV994::CopySha => {
            app.dispatch(Action::Explain(format!(
                "Commit SHA\n{commit}\n\nghtui has no clipboard integration: select the SHA above \
                 in your terminal to copy it. Nothing was changed."
            )));
        }
    }
}

// ---------------------------------------------------------------------------
// Content
// ---------------------------------------------------------------------------

/// One painted row: its line, click targets (`x` offset, width, action) and its item.
struct Row {
    line: Line<'static>,
    hits: Vec<(u16, u16, Action)>,
    item: Option<usize>,
}
impl Row {
    fn new(line: Line<'static>) -> Self {
        Self {
            line,
            hits: Vec::new(),
            item: None,
        }
    }
    fn blank() -> Self {
        Self::new(Line::default())
    }
}

fn muted(theme: Theme, text: impl Into<String>) -> Row {
    Row::new(Line::from(Span::styled(
        text.into(),
        theme.role(Role::TextMuted),
    )))
}

/// A section heading, with its muted summary beside it or, when narrow, on the next row.
fn heading(theme: Theme, text: &str, rest: Option<String>, width: usize) -> Vec<Row> {
    let title = Span::styled(
        text.to_uppercase(),
        theme.role(Role::Accent).add_modifier(Modifier::BOLD),
    );
    let Some(rest) = rest else {
        return vec![Row::new(Line::from(title))];
    };
    let beside = format!("  {rest}");
    if crate::editor::display_width(text) + crate::editor::display_width(&beside) <= width {
        return vec![Row::new(Line::from(vec![
            title,
            Span::styled(beside, theme.role(Role::TextMuted)),
        ]))];
    }
    let mut rows = vec![Row::new(Line::from(title))];
    rows.extend(
        wrap(&rest, width)
            .into_iter()
            .map(|part| muted(theme, part)),
    );
    rows
}

fn label(theme: Theme, text: &str) -> Span<'static> {
    Span::styled(format!("{text:<LABEL$} "), theme.role(Role::TextMuted))
}

/// `6h ago · 18 Oct 14:01 UTC` (the year is added when it is not the current one).
pub fn when(timestamp: i64, now: u64) -> String {
    let age = u64::try_from(timestamp).map_or_else(
        |_| crate::ui_table::NONE.to_owned(),
        |at| crate::ui_table::age_from_seconds(now.saturating_sub(at)),
    );
    format!("{age} ago · {}", utc_short(timestamp, now))
}

fn utc_short(timestamp: i64, now: u64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let date = crate::git_actor_labels_v1::utc_date(timestamp);
    let this_year = crate::git_actor_labels_v1::utc_date(i64::try_from(now).unwrap_or(i64::MAX));
    let mut parts = date.split('-');
    let (Some(year), Some(month), Some(day)) = (parts.next(), parts.next(), parts.next()) else {
        return crate::git_actor_labels_v1::utc_datetime(timestamp);
    };
    let month = month
        .parse::<usize>()
        .ok()
        .and_then(|month| MONTHS.get(month.wrapping_sub(1)))
        .copied()
        .unwrap_or("?");
    let day = day.trim_start_matches('0');
    let seconds = timestamp.rem_euclid(86_400);
    let time = format!("{:02}:{:02}", seconds / 3600, seconds % 3600 / 60);
    if this_year.starts_with(year) {
        format!("{day} {month} {time} UTC")
    } else {
        format!("{day} {month} {year} {time} UTC")
    }
}

/// Word-wrapped rows of `text`, at most `width` cells each.
fn wrap(text: &str, width: usize) -> Vec<String> {
    crate::github_ui::wrap_cells(text, width.max(1))
        .into_iter()
        .map(|row| row.trim_end().to_owned())
        .collect()
}

/// A path cut to `width` cells keeping its file name: `tests/…/name.rs`, `…/name.rs`, then an
/// end ellipsis.
pub fn fit_path(path: &str, width: usize) -> String {
    use crate::editor::display_width;
    if display_width(path) <= width {
        return path.to_owned();
    }
    if let Some((directory, name)) = path.rsplit_once('/') {
        let first = directory.split('/').next().unwrap_or_default();
        for candidate in [format!("{first}/…/{name}"), format!("…/{name}")] {
            if display_width(&candidate) <= width {
                return candidate;
            }
        }
    }
    crate::ui_chrome::fit(path, width)
}

fn actor_spans(theme: Theme, identity: &ActorIdentity, width: usize) -> Vec<Span<'static>> {
    let chip = if theme.mode == ColorMode::NoColor {
        theme.style(SemanticToken::Actor)
    } else {
        theme.identity_chip(InitialsChip::slot(identity))
    };
    let text = InitialsChip::text(identity, theme.mode);
    let room = width.saturating_sub(crate::editor::display_width(&text) + 1);
    vec![
        Span::styled(text, chip),
        Span::raw(" "),
        Span::styled(
            crate::ui_chrome::fit(&safe_text(&crate::ui_table::actor_text(identity)), room),
            theme.role(Role::Text),
        ),
    ]
}

fn status_style(theme: Theme, status: char) -> Style {
    theme.style(match status {
        'A' | 'C' => SemanticToken::Added,
        'D' => SemanticToken::Removed,
        'R' => SemanticToken::Reference,
        'U' => SemanticToken::Conflict,
        _ => SemanticToken::Changed,
    })
}

/// `+42 −1`, `+14`, `−3`, `±0` or `bin` as spans and their width.
fn count_spans(theme: Theme, counts: Option<(usize, usize)>) -> Vec<Span<'static>> {
    let added = theme.style(SemanticToken::Added);
    let removed = theme.style(SemanticToken::Removed);
    match counts {
        None => vec![Span::styled("bin", theme.role(Role::TextMuted))],
        Some((0, 0)) => vec![Span::styled("±0", theme.role(Role::TextMuted))],
        Some((plus, 0)) => vec![Span::styled(format!("+{plus}"), added)],
        Some((0, minus)) => vec![Span::styled(format!("−{minus}"), removed)],
        Some((plus, minus)) => vec![
            Span::styled(format!("+{plus}"), added),
            Span::raw(" "),
            Span::styled(format!("−{minus}"), removed),
        ],
    }
}

fn spans_width(spans: &[Span]) -> usize {
    spans
        .iter()
        .map(|span| crate::editor::display_width(&span.content))
        .sum()
}

/// Flows `tokens` (spans with optional click targets) into rows of `width`, separated by
/// `separator`; continuation rows start at `indent`.
fn flow(
    rows: &mut Vec<Row>,
    first: Vec<Span<'static>>,
    tokens: Vec<(Vec<Span<'static>>, Option<Action>, Option<usize>)>,
    separator: Span<'static>,
    indent: usize,
    width: usize,
) {
    let mut row = Row::new(Line::from(first));
    let mut used = row.line.width();
    let mut empty = true;
    let gap = crate::editor::display_width(&separator.content);
    for (spans, action, item) in tokens {
        let token = spans_width(&spans);
        if !empty && used + gap + token > width {
            rows.push(std::mem::replace(
                &mut row,
                Row::new(Line::from(" ".repeat(indent))),
            ));
            used = indent;
            empty = true;
        }
        if !empty {
            row.line.spans.push(separator.clone());
            used += gap;
        }
        if let Some(action) = action {
            row.hits.push((used as u16, token as u16, action));
        }
        if item.is_some() {
            row.item = item;
        }
        row.line.spans.extend(spans);
        used += token;
        empty = false;
    }
    rows.push(row);
}

/// Every row of the detail of `commit` at `width` cells. `item` is the detail cursor and
/// `focused` whether the detail owns the focus.
#[allow(clippy::too_many_arguments)]
fn content(
    workspace: &Workspace,
    observed: &OriginalGraphCommitV1,
    files: &FilesV994,
    width: usize,
    theme: Theme,
    now: u64,
    item: usize,
    focused: bool,
) -> Vec<Row> {
    let commit = observed.commit();
    let id = commit.id.as_str();
    let mut rows = Vec::new();
    // `abc1234  HEAD  main ⇅  v1.0`: the short SHA, then every chip, wrapped.
    let chips = crate::history::refs::chips(observed.chips());
    let mut tokens: Vec<(Vec<Span<'static>>, Option<Action>, Option<usize>)> = chips
        .iter()
        .take(MAX_CHIPS)
        .map(|chip| {
            (
                vec![Span::styled(
                    format!(
                        " {} ",
                        crate::ui_chrome::fit(&safe_text(&chip.label), width.saturating_sub(2))
                    ),
                    crate::repository_ui::history_table::chip_style(theme, chip.kind),
                )],
                None,
                None,
            )
        })
        .collect();
    if chips.len() > MAX_CHIPS {
        tokens.push((
            vec![Span::styled(
                format!(
                    " {} ",
                    crate::history::refs::overflow_label(chips.len() - MAX_CHIPS)
                ),
                theme.badge(SemanticToken::Normal),
            )],
            None,
            None,
        ));
    }
    flow(
        &mut rows,
        vec![Span::styled(
            crate::ui_table::short_sha(id).to_owned(),
            theme.role(Role::Commit).add_modifier(Modifier::BOLD),
        )],
        tokens,
        Span::raw(" "),
        0,
        width,
    );
    if !chips.is_empty() {
        // The first chip sits two cells after the SHA, like the table's MESSAGE column.
        if let Some(first) = rows.first_mut() {
            if first.line.spans.len() > 1 {
                first.line.spans.insert(1, Span::raw(" "));
            }
        }
    }
    let bold = theme.role(Role::Text).add_modifier(Modifier::BOLD);
    for part in wrap(&safe_text(&commit.subject), width) {
        rows.push(Row::new(Line::from(Span::styled(part, bold))));
    }
    rows.push(Row::blank());

    // The full SHA stays visible: beside its label when it fits, else on its own row.
    let full = Span::styled(id.to_owned(), theme.role(Role::Commit));
    if LABEL + 1 + id.len() <= width {
        rows.push(Row::new(Line::from(vec![label(theme, "SHA"), full])));
    } else {
        rows.push(Row::new(Line::from(label(theme, "SHA"))));
        rows.push(Row::new(Line::from(vec![Span::raw("  "), full])));
    }
    let author = ActorIdentity::local(&commit.author, GitRole::Author);
    let committer = ActorIdentity::local(&commit.committer, GitRole::Committer);
    for (name, identity, actor) in [
        ("Author", &author, &commit.author),
        ("Committer", &committer, &commit.committer),
    ] {
        let mut spans = vec![label(theme, name)];
        spans.extend(actor_spans(
            theme,
            identity,
            width.saturating_sub(LABEL + 1),
        ));
        let email = format!("  <{}>", safe_text(&actor.email));
        if !actor.email.is_empty()
            && spans_width(&spans) + crate::editor::display_width(&email) <= width
        {
            spans.push(Span::styled(email, theme.role(Role::TextMuted)));
        }
        rows.push(Row::new(Line::from(spans)));
    }
    // `Date  6h ago · 18 Oct 14:01 UTC`; a narrow detail puts the UTC time on the next row.
    let mut dated = |name: &str, timestamp: i64| {
        let value = when(timestamp, now);
        if LABEL + 1 + crate::editor::display_width(&value) <= width {
            rows.push(Row::new(Line::from(vec![
                label(theme, name),
                Span::styled(value, theme.role(Role::Text)),
            ])));
            return;
        }
        let (relative, utc) = value.split_once(" · ").unwrap_or((value.as_str(), ""));
        rows.push(Row::new(Line::from(vec![
            label(theme, name),
            Span::styled(relative.to_owned(), theme.role(Role::Text)),
        ])));
        rows.push(Row::new(Line::from(vec![
            Span::raw(" ".repeat(LABEL + 1)),
            Span::styled(utc.to_owned(), theme.role(Role::Text)),
        ])));
    };
    dated("Date", commit.author.timestamp);
    if commit.committer.timestamp != commit.author.timestamp {
        dated("Committed", commit.committer.timestamp);
    }
    // Parents: each one a link to that commit (the item cursor reaches them too).
    let parent_label = if commit.parents.len() > 1 {
        "Parents"
    } else {
        "Parent"
    };
    if commit.parents.is_empty() {
        rows.push(Row::new(Line::from(vec![
            label(theme, parent_label),
            Span::styled("none · root commit", theme.role(Role::TextMuted)),
        ])));
    } else {
        let tokens = commit
            .parents
            .iter()
            .enumerate()
            .map(|(index, parent)| {
                let mut style = theme.role(Role::Commit).add_modifier(Modifier::UNDERLINED);
                if index == item {
                    style = style.patch(if focused {
                        crate::ui_table::selection_style(theme).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().add_modifier(Modifier::BOLD)
                    });
                }
                let mut spans = vec![Span::styled(
                    crate::ui_table::short_sha(parent).to_owned(),
                    style,
                )];
                if workspace
                    .graph_state_v1
                    .as_ref()
                    .is_some_and(|state| state.selected_commit(parent).is_none())
                {
                    spans.push(Span::styled(" (not loaded)", theme.role(Role::TextMuted)));
                }
                (spans, Some(detail(id, Act::Parent(index))), Some(index))
            })
            .collect();
        flow(
            &mut rows,
            vec![label(theme, parent_label)],
            tokens,
            Span::raw("  "),
            LABEL + 1,
            width,
        );
    }

    rows.push(Row::blank());
    rows.extend(heading(theme, "Message", None, width));
    let body: Vec<&str> = observed
        .message()
        .split('\n')
        .skip(1)
        .skip_while(|line| line.trim().is_empty())
        .collect();
    let end = body
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map_or(0, |at| at + 1);
    if end == 0 {
        rows.push(muted(theme, "No message body"));
    } else {
        let mut shown = 0usize;
        'message: for line in &body[..end] {
            for part in wrap(&safe_text(line), width) {
                if shown == MAX_MESSAGE_ROWS {
                    rows.push(muted(
                        theme,
                        "Message display limit · further rows not shown",
                    ));
                    break 'message;
                }
                rows.push(Row::new(Line::from(Span::styled(
                    part,
                    theme.role(Role::Text),
                ))));
                shown += 1;
            }
        }
    }
    if !observed.message_complete() {
        rows.push(muted(
            theme,
            "Message preview of 16 KiB · the rest is not shown",
        ));
    }

    rows.push(Row::blank());
    let parents = commit.parents.len();
    match files {
        FilesV994::Waiting => {
            rows.extend(heading(theme, "Changed files", None, width));
            rows.push(muted(theme, "Reading changed files…"));
        }
        FilesV994::Failed(error) => {
            rows.extend(heading(theme, "Changed files", None, width));
            for part in wrap(&safe_text(error), width) {
                rows.push(Row::new(Line::from(Span::styled(
                    part,
                    theme.style(SemanticToken::Failure),
                ))));
            }
        }
        FilesV994::Ready(files) => {
            let mut summary = format!(
                "{} file{}",
                files.total,
                if files.total == 1 { "" } else { "s" }
            );
            if files.added > 0 || files.removed > 0 {
                summary.push_str(&format!(" · +{} −{}", files.added, files.removed));
            }
            if parents > 1 {
                summary.push_str(" · vs parent 1");
            }
            rows.extend(heading(theme, "Changed files", Some(summary), width));
            if files.files.is_empty() {
                rows.push(muted(theme, "No file changes"));
            }
            let counts: Vec<Vec<Span<'static>>> = files
                .files
                .iter()
                .map(|file| count_spans(theme, file.counts))
                .collect();
            let counts_width = counts
                .iter()
                .map(|spans| spans_width(spans))
                .max()
                .unwrap_or(0);
            for (index, (file, counts)) in files.files.iter().zip(counts).enumerate() {
                let item_index = parents + index;
                let path = match &file.from {
                    Some(from) => format!(
                        "{} → {}",
                        safe_text(&from.display()),
                        safe_text(&file.path.display())
                    ),
                    None => safe_text(&file.path.display()),
                };
                // `▌ M path…  +4 −1`: bar slot, status, path, right-aligned counts.
                let path_width = width.saturating_sub(4 + counts_width + 2);
                let path = fit_path(&path, path_width);
                let pad = path_width.saturating_sub(crate::editor::display_width(&path));
                let count_pad = counts_width - spans_width(&counts);
                let mut spans = vec![
                    Span::raw("  "),
                    Span::styled(file.status.to_string(), status_style(theme, file.status)),
                    Span::raw(" "),
                    Span::styled(path, theme.role(Role::Text)),
                    Span::raw(" ".repeat(pad + 2 + count_pad)),
                ];
                spans.extend(counts);
                let mut row = Row::new(Line::from(spans));
                row.item = Some(item_index);
                row.hits.push((
                    0,
                    width as u16,
                    Action::HistoryDetailV994(DetailUiActionV994::Select(item_index)),
                ));
                rows.push(row);
            }
            if files.total > files.files.len() {
                rows.push(muted(
                    theme,
                    format!(
                        "  +{} more files · enter opens the whole diff",
                        files.total - files.files.len()
                    ),
                ));
            }
        }
    }
    rows.push(Row::blank());
    rows
}

/// The `enter diff · f file history · Y blame · y copy SHA` row, every token clickable.
fn hints(app: &App, theme: Theme, width: usize) -> Vec<Row> {
    let Some(targets) = targets(app) else {
        return Vec::new();
    };
    let drill = drilldown_shown(app);
    let open = if drill {
        Action::HistoryDetailV994(DetailUiActionV994::Activate)
    } else {
        targets.diff.dispatched()
    };
    let mut tokens = vec![(
        vec![
            Span::styled(
                "enter",
                theme.role(Role::Accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if drill { " open" } else { " diff" },
                theme.role(Role::TextMuted),
            ),
        ],
        Some(open),
        None,
    )];
    for (key, text, target) in [
        ("f", " file history", Some(&targets.file_history)),
        ("Y", " blame", Some(&targets.blame)),
        ("y", " copy SHA", None),
    ] {
        tokens.push((
            vec![
                Span::styled(key, theme.role(Role::Accent).add_modifier(Modifier::BOLD)),
                Span::styled(text, theme.role(Role::TextMuted)),
            ],
            Some(target.map_or(
                Action::HistoryDetailV994(DetailUiActionV994::CopySha),
                Target::dispatched,
            )),
            None,
        ));
    }
    let mut rows = Vec::new();
    flow(
        &mut rows,
        Vec::new(),
        tokens,
        Span::styled(" · ", theme.role(Role::TextMuted)),
        0,
        width,
    );
    rows
}

/// Paints the detail rows into `body` from `scroll` and registers their click targets.
/// Returns the scroll after clamping and revealing the item.
fn paint(frame: &mut Frame, app: &mut App, body: Rect, scroll: usize, focused: bool) -> usize {
    let theme = app.theme();
    let Some(commit) = commit_id(app) else {
        return 0;
    };
    let Some(workspace) = app.workspace.as_ref() else {
        return 0;
    };
    let Some(observed) = observed(workspace, &commit) else {
        frame.render_widget(
            Paragraph::new("The selected commit is not loaded").style(theme.role(Role::TextMuted)),
            body,
        );
        return 0;
    };
    let parents = observed.commit().parents.first().cloned();
    let parent_count = observed.commit().parents.len();
    let files =
        app.history_detail_v994
            .request(workspace, &commit, parents.as_deref(), Instant::now());
    let followed = app.history_detail_v994.follow(&commit, parent_count);
    let items = parent_count
        + match &files {
            FilesV994::Ready(files) => files.files.len(),
            _ => 0,
        };
    let state = &mut app.history_detail_v994;
    if matches!(files, FilesV994::Ready(_)) && items > 0 && state.item >= items {
        // The files are known: keep the cursor on a real item (it waits on the first file).
        state.item = if parent_count < items {
            parent_count
        } else {
            items - 1
        };
    }
    let item = state.item;
    let width = usize::from(body.width);
    let mut rows = content(
        workspace,
        observed,
        &files,
        width,
        theme,
        crate::storage::now(),
        item,
        focused,
    );
    rows.extend(hints(app, theme, width));
    let height = usize::from(body.height);
    let mut scroll = if followed { 0 } else { scroll };
    if std::mem::take(&mut app.history_detail_v994.reveal) {
        if let Some(at) = rows.iter().position(|row| row.item == Some(item)) {
            if at < scroll {
                scroll = at;
            } else if at >= scroll + height {
                scroll = at + 1 - height;
            }
        }
    }
    scroll = scroll.min(rows.len().saturating_sub(height));
    let buffer_area = frame.area();
    for (offset, row) in rows.iter().skip(scroll).take(height).enumerate() {
        let rect = Rect::new(body.x, body.y + offset as u16, body.width, 1);
        frame.render_widget(Paragraph::new(row.line.clone()).style(theme.base()), rect);
        // The selected file wears the table's selection: the `▌` bar and the row fill.
        if row.item == Some(item) && item >= parent_count {
            frame
                .buffer_mut()
                .set_style(rect, crate::ui_table::selection_style(theme));
            let bar = theme
                .role(if focused { Role::Accent } else { Role::TextDim })
                .patch(crate::ui_table::selection_style(theme));
            if let Some(cell) = frame.buffer_mut().cell_mut((rect.x, rect.y)) {
                cell.set_symbol(crate::ui_table::SELECTION_BAR);
                cell.set_style(bar);
            }
        }
        for (x, w, action) in &row.hits {
            let hit = Rect::new(
                rect.x + x,
                rect.y,
                (*w).min(body.width.saturating_sub(*x)),
                1,
            )
            .intersection(buffer_area);
            if !hit.is_empty() {
                app.hits.push(HitRegion {
                    rect: hit,
                    action: action.clone(),
                });
            }
        }
    }
    scroll
}

/// The split pane beside the History table.
pub fn draw_pane(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme();
    let focused = crate::list_preview_v92::preview_focused(app);
    let block = crate::ui_chrome::pane_block(theme, &[TITLE], focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let body = Rect::new(
        inner.x.saturating_add(1),
        inner.y,
        inner.width.saturating_sub(2),
        inner.height,
    );
    if body.is_empty() {
        return;
    }
    let item = commit_id(app).map(|commit| format!("history\0{commit}"));
    if let Some(item) = item.filter(|item| *item != app.preview_item_v92) {
        app.preview_item_v92 = item;
        app.preview_scroll_v92 = 0;
    }
    let scroll = app.preview_scroll_v92;
    app.preview_scroll_v92 = paint(frame, app, body, scroll, focused);
}

/// The full-width drill-down (`D Details`) in the Repository pane body.
pub fn draw_full(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme();
    if area.height < 3 || area.width < 8 {
        return;
    }
    let focused = app.repository_content_focused;
    let short = commit_id(app)
        .map(|commit| crate::ui_table::short_sha(&commit).to_owned())
        .unwrap_or_default();
    let title = format!("{TITLE} {short}");
    let block = crate::ui_chrome::pane_block(
        theme,
        &[crate::repository_ui::history_table::TITLE, &title],
        focused,
    )
    .borders(ratatui::widgets::Borders::TOP);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    crate::repository_panels_v93::join_rule(frame, area);
    let body = Rect::new(
        inner.x.saturating_add(1),
        inner.y,
        inner.width.saturating_sub(2),
        inner.height,
    );
    app.history_detail_v994.area = Some(body);
    let scroll = app.history_detail_v994.scroll;
    app.history_detail_v994.scroll = paint(frame, app, body, scroll, focused);
    app.repository_visible_rows = usize::from(body.height);
}

#[cfg(test)]
#[path = "detail_tests.rs"]
mod history_detail_v994_tests;
