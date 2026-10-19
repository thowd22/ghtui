//! TASK-93: the Repository Overview as persistent panels. A Files panel holds the working-tree
//! status (`workspace_rows`: branch, upstream and paused-operation prose, then one row per
//! changed file) with untracked files collapsed to a count and a toggle, and a Recent commits
//! panel holds the saved versions. From [`SPLIT_MIN_WIDTH`] columns the selected row is
//! previewed beside them (see [`crate::list_preview_v92`]); the preview only shows what the
//! Overview read already holds, so moving the cursor never runs Git.
//!
//! The panels share the Repository list state: one selection walks Files then Commits, every
//! row keeps its click target and action, and stage, unstage and commit act on the selected
//! row through their usual preview and confirmation. Each panel keeps its own scroll offset.
use crate::{
    app::{Action, App, HitRegion, View},
    footer::KeyControl,
    git::StatusEntry,
    theme::{Role, SemanticToken, Theme},
    ui_chrome::{label_value, pane_block},
    ui_table::{Cell, Column, Table, TableRow},
    workspace::{DisplayRow, RowAction, Workspace},
};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    Frame,
};

/// The Repository preview needs room for paths and full ids: the panels split from this
/// width (wider than the Inbox and Actions split, so 120-column sessions keep full-width
/// panels).
pub const SPLIT_MIN_WIDTH: u16 = 130;
/// The Commits panel title (the Overview's section heading before TASK-93).
pub const COMMITS_TITLE: &str = "Recent commits — saved versions";
/// The Overview row that headed the commits before they had their own panel.
const RECENT_HEADING: &str = "Recent commits — saved versions";
/// Width of the label column in preview rows.
const LABEL: usize = 9;

/// The Files panel columns: change, where it is (index, working tree, both), and the path.
pub const FILE_COLUMNS: [Column; 3] = [
    Column::new("STATUS", 9).priority(1),
    Column::new("STAGE", 8).priority(2),
    Column::new("PATH", 12).flex(1).priority(0),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelActionV93 {
    /// Show or collapse the untracked files in the Files panel.
    ToggleUntracked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Files,
    Commits,
}
impl Panel {
    fn index(self) -> usize {
        match self {
            Self::Files => 0,
            Self::Commits => 1,
        }
    }
}

/// Panel scroll offsets and the untracked toggle. The selection itself stays in
/// `App::repository_list`.
#[derive(Clone, Debug, Default)]
pub struct PanelsV93 {
    pub show_untracked: bool,
    tops: [usize; 2],
    lens: [usize; 2],
    visible: [usize; 2],
    /// Painted panel areas (empty when a panel is not drawn).
    pub areas: [Rect; 2],
    last_selected: Option<usize>,
}

/// One painted line of a panel: a display row or the untracked toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Line93 {
    Row(usize),
    Untracked,
}

/// The Overview rows sorted into panels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sorted {
    /// Display-row indexes shown in the Files panel, in order.
    pub files: Vec<usize>,
    /// Display-row indexes shown in the Commits panel, in order.
    pub commits: Vec<usize>,
    /// Untracked file rows, shown or not.
    pub untracked: usize,
    /// Per display row: hidden from the panels (collapsed untracked files, the old heading).
    pub hidden: Vec<bool>,
}

/// Whether the Repository shows its panels: the plain Overview of a local repository.
pub fn active(app: &App) -> bool {
    app.view == View::Repository
        && app.remote_repository.is_none()
        && app.workspace.as_ref().is_some_and(|workspace| {
            crate::repository_ui::overview_table_v91(workspace)
                && workspace
                    .display_rows_v1()
                    .iter()
                    .any(|row| row.action.is_none() && row.label == RECENT_HEADING)
        })
}

/// The status entry of each file row: the Overview emits one row per entry, in order.
fn entries(workspace: &Workspace) -> Vec<Option<&StatusEntry>> {
    let mut next = workspace.status.entries.iter();
    workspace
        .display_rows_v1()
        .iter()
        .map(|row| match &row.action {
            Some(RowAction::WorkingFile(path) | RowAction::Conflict(path)) => {
                next.next().filter(|entry| entry.path == *path)
            }
            _ => None,
        })
        .collect()
}

/// Sorts the rows into the Files and Commits panels.
pub fn sort(workspace: &Workspace, show_untracked: bool) -> Sorted {
    let rows = workspace.display_rows_v1();
    let entries = entries(workspace);
    let heading = rows
        .iter()
        .position(|row| row.action.is_none() && row.label == RECENT_HEADING);
    let mut sorted = Sorted {
        hidden: vec![false; rows.len()],
        ..Sorted::default()
    };
    for (index, entry) in entries.iter().enumerate().take(rows.len()) {
        match heading {
            Some(heading) if index == heading => sorted.hidden[index] = true,
            Some(heading) if index > heading => sorted.commits.push(index),
            _ => {
                if entry.is_some_and(StatusEntry::untracked) {
                    sorted.untracked += 1;
                    if !show_untracked {
                        sorted.hidden[index] = true;
                        continue;
                    }
                }
                sorted.files.push(index);
            }
        }
    }
    sorted
}

/// Display rows the panels hide (keyboard movement skips them), when the panels are shown.
pub fn hidden_rows(app: &App) -> Option<Vec<bool>> {
    active(app).then(|| {
        sort(
            app.workspace.as_ref().unwrap(),
            app.repository_panels_v93.show_untracked,
        )
        .hidden
    })
}

/// Whether the selected row is collapsed out of view (an action must not reach it).
pub fn selection_hidden(app: &App) -> bool {
    hidden_rows(app).is_some_and(|hidden| {
        hidden
            .get(app.repository_list.selected)
            .copied()
            .unwrap_or(false)
    })
}

pub fn dispatch(app: &mut App, action: PanelActionV93) {
    match action {
        PanelActionV93::ToggleUntracked => {
            let panels = &mut app.repository_panels_v93;
            panels.show_untracked = !panels.show_untracked;
            panels.last_selected = None;
            let count = app
                .workspace
                .as_ref()
                .map_or(0, |workspace| sort(workspace, true).untracked);
            app.status = if app.repository_panels_v93.show_untracked {
                format!("Showing {count} untracked files • u hides them • nothing changed")
            } else {
                format!("{count} untracked files collapsed • u shows them • nothing changed")
            };
            // A collapsed selection moves to the first visible row that has an action.
            if selection_hidden(app) {
                if let (Some(hidden), Some(workspace)) = (hidden_rows(app), app.workspace.as_ref())
                {
                    if let Some(index) = workspace
                        .display_rows_v1()
                        .iter()
                        .enumerate()
                        .position(|(index, row)| row.action.is_some() && !hidden[index])
                    {
                        app.repository_list.selected = index;
                    }
                }
            }
        }
    }
}

/// The panels' own keys, added to the Repository key bar: the untracked toggle.
pub fn footer_keys(app: &App) -> Vec<KeyControl> {
    if !active(app) {
        return vec![];
    }
    let untracked = sort(app.workspace.as_ref().unwrap(), true).untracked;
    if untracked == 0 {
        return vec![];
    }
    let shown = app.repository_panels_v93.show_untracked;
    vec![KeyControl::new(
        "u",
        if shown {
            "Hide untracked"
        } else {
            "Show untracked"
        },
        Action::RepositoryPanelsV93(PanelActionV93::ToggleUntracked),
    )
    .with_compact("Untracked")
    // After Compare: the panel's untracked row is its own click target at 80 columns.
    .with_priority(25)]
}

/// The App action of a row's click target, as the Overview list always mapped it.
pub fn row_action(app: &App, action: RowAction) -> Action {
    match action {
        RowAction::FileProvenance(file_action) => {
            let reason = if app.loading[View::Repository.index()] {
                Some("Wait for the current repository read, or cancel it first".into())
            } else {
                app.workspace
                    .as_ref()
                    .and_then(|workspace| workspace.provenance_available(&file_action).err())
            };
            reason.map_or(
                Action::RepositoryRow(RowAction::FileProvenance(file_action)),
                Action::Explain,
            )
        }
        RowAction::Local(command) => Action::LocalCommand(command),
        action => Action::RepositoryRow(action),
    }
}

/// A changed file as cells: status, stage and path (with its rename source and the conflict
/// hint).
fn file_row(row: &DisplayRow, entry: &StatusEntry) -> TableRow<Line93> {
    let stage = if entry.conflict {
        "conflicted"
    } else if entry.staged() && entry.worktree != '.' {
        "both"
    } else if entry.staged() {
        "staged"
    } else if entry.untracked() {
        "untracked"
    } else {
        "unstaged"
    };
    let mut path = entry.path.display().to_string();
    if let Some(original) = &entry.original_path {
        path.push_str(&format!(" (from {})", original.display()));
    }
    if entry.conflict {
        path.push_str(" • Enter: compare base/ours/theirs");
    }
    TableRow::cells(vec![
        entry.label().into(),
        Cell::styled(
            stage,
            if entry.staged() {
                Role::Success
            } else {
                Role::TextMuted
            },
        ),
        Cell::styled(path, Role::Text),
    ])
    .with_tone(SemanticToken::from(row.role))
}

fn files_rows(
    workspace: &Workspace,
    sorted: &Sorted,
    show_untracked: bool,
) -> Vec<TableRow<Line93>> {
    let rows = workspace.display_rows_v1();
    let entries = entries(workspace);
    let mut output = sorted
        .files
        .iter()
        .map(|index| {
            let row = &rows[*index];
            match entries[*index] {
                Some(entry) => file_row(row, entry),
                None => TableRow::text(row.label.as_str()).with_tone(SemanticToken::from(row.role)),
            }
            .with_action(Some(Line93::Row(*index)))
        })
        .collect::<Vec<_>>();
    if sorted.untracked > 0 {
        let text = if show_untracked {
            format!("untracked: {} shown · u hides them", sorted.untracked)
        } else {
            format!("untracked: {} hidden · u shows them", sorted.untracked)
        };
        output.push(
            TableRow::text(text)
                .with_tone(SemanticToken::Muted)
                .with_action(Some(Line93::Untracked)),
        );
    }
    output
}

fn commit_rows(workspace: &Workspace, sorted: &Sorted, now: u64) -> Vec<TableRow<Line93>> {
    let rows = workspace.display_rows_v1();
    sorted
        .commits
        .iter()
        .map(|index| {
            let row = crate::repository_ui::overview_row_v91(&rows[*index], now);
            TableRow {
                kind: row.kind,
                cells: row.cells,
                tone: row.tone,
                glyph: row.glyph,
                actor: row.actor,
                action: Some(Line93::Row(*index)),
            }
        })
        .collect()
}

/// Rows of two panels that share `height`: each gets what it needs when both fit; otherwise
/// the smaller need is met first and the other takes the rest.
pub fn heights(height: u16, files: usize, commits: usize) -> (u16, u16) {
    let need = |rows: usize| (rows.min(usize::from(u16::MAX - 2)) as u16).max(1) + 2;
    let (files, commits) = (need(files), need(commits));
    if files.saturating_add(commits) <= height {
        return (files, height - files);
    }
    let half = height / 2;
    let files = if files <= half {
        files
    } else if commits <= height - half {
        height - commits
    } else {
        half.max(3).min(height)
    };
    (files, height - files)
}

/// Paints the Files and Commits panels into `area` and registers one click target per row.
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(workspace) = app.workspace.as_ref() else {
        return;
    };
    let theme = app.theme();
    let show_untracked = app.repository_panels_v93.show_untracked;
    let sorted = sort(workspace, show_untracked);
    let files = files_rows(workspace, &sorted, show_untracked);
    let commits = commit_rows(workspace, &sorted, crate::storage::now());
    let selected = app.repository_list.selected;
    let list_focused = app.repository_content_focused && crate::list_preview_v92::list_focused(app);
    let files_title = match workspace.status.entries.len() {
        0 => "Files".to_string(),
        count => format!("Files · {count} changed"),
    };
    let (files_height, commits_height) = heights(area.height, files.len(), commits.len());
    let panels = [
        (
            Panel::Files,
            files_title,
            &FILE_COLUMNS[..],
            files,
            Rect::new(area.x, area.y, area.width, files_height),
        ),
        (
            Panel::Commits,
            COMMITS_TITLE.to_string(),
            &crate::repository_ui::OVERVIEW_COLUMNS[..],
            commits,
            Rect::new(area.x, area.y + files_height, area.width, commits_height),
        ),
    ];
    let selection_moved = app.repository_panels_v93.last_selected != Some(selected);
    app.repository_panels_v93.last_selected = Some(selected);
    let mut hits = vec![];
    for (panel, title, columns, rows, rect) in panels {
        let state = &mut app.repository_panels_v93;
        state.areas[panel.index()] = Rect::default();
        if rect.height < 3 || rect.width < 8 {
            continue;
        }
        let position = rows
            .iter()
            .position(|row| row.action == Some(Line93::Row(selected)));
        let focused = list_focused && position.is_some();
        let block = pane_block(theme, &[&title], focused);
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let visible = usize::from(inner.height);
        let top = &mut state.tops[panel.index()];
        let geometry =
            state.visible[panel.index()] != visible || state.lens[panel.index()] != rows.len();
        if let Some(position) = position.filter(|_| selection_moved || geometry) {
            let mut list = crate::ui_table::ListState {
                selected: position,
                top: *top,
            };
            list.ensure_visible(visible);
            *top = list.top;
        }
        *top = (*top).min(rows.len().saturating_sub(visible));
        let top = *top;
        state.lens[panel.index()] = rows.len();
        state.visible[panel.index()] = visible;
        state.areas[panel.index()] = rect;
        let natural = crate::ui_table::natural_widths(columns, &rows, theme.mode);
        let table = Table {
            columns,
            header: false,
            focused: list_focused,
            glyphs: false,
            empty: Some(match panel {
                Panel::Files => "Working files are clean",
                Panel::Commits => "No saved versions yet",
            }),
        };
        let rendered = crate::ui_table::draw_table_at(
            frame,
            inner,
            &table,
            &natural,
            rows.len(),
            |index| rows[index].clone(),
            position,
            top,
            theme,
        );
        hits.extend(rendered.hits.into_iter().filter_map(|hit| {
            let action = match hit.action? {
                Line93::Untracked => Action::RepositoryPanelsV93(PanelActionV93::ToggleUntracked),
                Line93::Row(index) => {
                    let action = app.workspace.as_ref()?.display_rows_v1()[index]
                        .action
                        .clone()?;
                    row_action(app, action)
                }
            };
            Some(HitRegion {
                rect: hit.rect,
                action,
            })
        }));
    }
    // Views opened from here (the graph) size their first viewport from the whole body.
    app.repository_visible_rows = usize::from(area.height);
    app.hits.extend(hits);
}

/// The wheel over a panel scrolls that panel only. Returns whether the event was used.
pub fn mouse(app: &mut App, column: u16, row: u16, wheel: Option<isize>) -> bool {
    let Some(delta) = wheel else {
        return false;
    };
    if app.overlay.is_some() || !active(app) {
        return false;
    }
    let Some(panel) = [Panel::Files, Panel::Commits].into_iter().find(|panel| {
        app.repository_panels_v93.areas[panel.index()].contains((column, row).into())
    }) else {
        return false;
    };
    scroll(app, panel, delta);
    true
}

fn scroll(app: &mut App, panel: Panel, delta: isize) {
    let state = &mut app.repository_panels_v93;
    let end = state.lens[panel.index()].saturating_sub(state.visible[panel.index()]);
    let top = &mut state.tops[panel.index()];
    *top = top.saturating_add_signed(delta).min(end);
}

/// Keyboard paging (`Action::Scroll`) scrolls the panel that holds the selection.
pub fn scroll_selected(app: &mut App, delta: isize) -> bool {
    if !active(app) {
        return false;
    }
    let sorted = sort(
        app.workspace.as_ref().unwrap(),
        app.repository_panels_v93.show_untracked,
    );
    let panel = if sorted.commits.contains(&app.repository_list.selected) {
        Panel::Commits
    } else {
        Panel::Files
    };
    scroll(app, panel, delta);
    true
}

fn heading(theme: Theme, text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_uppercase(),
        theme
            .role(Role::Accent)
            .add_modifier(ratatui::style::Modifier::BOLD),
    ))
}

fn muted(theme: Theme, text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme.role(Role::TextMuted)))
}

/// The preview of the selected row: a changed file, a saved version or a status line, from
/// the Overview read only (the diff loads when the row opens).
pub fn preview(app: &App, theme: Theme) -> crate::list_preview_v92::Preview {
    let empty = |text: &str| crate::list_preview_v92::Preview {
        item: String::new(),
        title: "Preview".into(),
        badge: None,
        lines: vec![muted(theme, text)],
    };
    let Some(workspace) = app.workspace.as_ref() else {
        return empty("No repository open");
    };
    let index = app.repository_list.selected;
    let Some(row) = workspace
        .display_rows_v1()
        .get(index)
        .filter(|_| !selection_hidden(app))
    else {
        return empty("No row selected");
    };
    let bold = theme
        .role(Role::Text)
        .add_modifier(ratatui::style::Modifier::BOLD);
    let item = format!("repo\0{index}\0{}", row.label);
    if let Some(entry) = entries(workspace).get(index).copied().flatten() {
        let mut lines = vec![
            Line::from(Span::styled(entry.path.display().to_string(), bold)),
            Line::default(),
            label_value(theme, "Status", entry.label(), LABEL),
            label_value(
                theme,
                "Index",
                if entry.staged() {
                    "staged"
                } else {
                    "not staged"
                },
                LABEL,
            ),
            label_value(
                theme,
                "Worktree",
                if entry.untracked() {
                    "untracked"
                } else if matches!(entry.worktree, '.' | ' ') {
                    "unchanged"
                } else {
                    "changed"
                },
                LABEL,
            ),
        ];
        if let Some(original) = &entry.original_path {
            lines.push(label_value(
                theme,
                "From",
                &original.display().to_string(),
                LABEL,
            ));
        }
        lines.push(Line::default());
        lines.push(heading(theme, "Actions"));
        lines.push(muted(
            theme,
            if entry.conflict {
                "Enter compares base, ours and theirs"
            } else {
                "Enter opens the working diff · + Stage · - Unstage · K Commit"
            },
        ));
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "The diff loads when the file opens; this preview runs no Git command",
            theme.role(Role::TextDim),
        )));
        return crate::list_preview_v92::Preview {
            item,
            title: "File".into(),
            badge: Some((entry.label().to_string(), SemanticToken::from(row.role))),
            lines,
        };
    }
    if let Some(commit) = row.cells.as_deref() {
        let lines = vec![
            Line::from(Span::styled(commit.subject.clone(), bold)),
            Line::default(),
            label_value(theme, "Commit", &commit.id, LABEL),
            label_value(theme, "Author", &commit.author, LABEL),
            label_value(
                theme,
                "Authored",
                &crate::git_actor_labels_v1::utc_datetime(commit.authored),
                LABEL,
            ),
            label_value(theme, "Committer", &commit.committer, LABEL),
            label_value(
                theme,
                "Committed",
                &crate::git_actor_labels_v1::utc_datetime(commit.committed),
                LABEL,
            ),
            Line::default(),
            Line::from(Span::styled(
                "Enter shows the changes this version introduced",
                theme.role(Role::TextDim),
            )),
        ];
        return crate::list_preview_v92::Preview {
            item,
            title: format!("Commit {}", crate::ui_table::short_sha(&commit.id)),
            badge: None,
            lines,
        };
    }
    let mut lines = vec![Line::from(Span::styled(
        row.label.clone(),
        theme.style(SemanticToken::from(row.role)),
    ))];
    if row.action.is_some() {
        lines.push(Line::default());
        lines.push(muted(theme, "Enter opens it"));
    }
    crate::list_preview_v92::Preview {
        item,
        title: "Details".into(),
        badge: None,
        lines,
    }
}

/// The action that opens the previewed row: the one its list row's click target carries.
pub fn open_action(app: &App) -> Option<Action> {
    if selection_hidden(app) {
        return None;
    }
    let action = app
        .workspace
        .as_ref()?
        .display_rows_v1()
        .get(app.repository_list.selected)?
        .action
        .clone()?;
    Some(row_action(app, action))
}

/// The full-screen Repository viewers (native diff, saved-folder detail, saved-path picker,
/// changed-files picker) drawn into `area`, in the order they own the screen. Returns whether
/// one was drawn; each still owns every event while it is open.
pub fn draw_takeover(frame: &mut Frame, app: &mut App, area: Rect) -> bool {
    if app.overlay.is_none() && crate::repository_native_diff_native_v1::draw(frame, app, area) {
        return true;
    }
    if crate::repository_tree_viewport_native_v2::folder_draw(frame, app, area) {
        return true;
    }
    if app.overlay.is_none() && crate::repository_tree_native_v1::draw(frame, app, area) {
        return true;
    }
    app.overlay.is_none() && crate::repository_diff_sides_native_v1::draw(frame, app, area)
}

#[cfg(test)]
#[path = "repository_panels_v93_tests.rs"]
mod tests;
