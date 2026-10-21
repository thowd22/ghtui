//! TASK-93/119: the Repository Overview as persistent panels, one per section of the Overview
//! read (see [`crate::workspace::overview_v119`]): Branch, Changes (untracked files collapsed
//! to a count and a toggle), Worktrees (the stack collapsed behind its summary row, TASK-118),
//! Stashes, Recent commits and, when the reflogs hold any, Recover lost commits (collapsed
//! behind its own entry). From [`SPLIT_MIN_WIDTH`] columns the selected row is previewed
//! beside them (see [`crate::list_preview_v92`]); the preview only shows what the Overview
//! read already holds, so moving the cursor never runs Git.
//!
//! The panels share the Repository list state: one selection walks the sections in order,
//! every row keeps its click target and action, and stage, unstage and commit act on the
//! selected row through their usual preview and confirmation. Each panel keeps its own
//! scroll offset.
use crate::{
    app::{Action, App, HitRegion, View},
    footer::KeyControl,
    git::StatusEntry,
    theme::{Role, SemanticToken, Theme},
    ui_chrome::{label_value, pane_block},
    ui_table::{Cell, Column, Table, TableRow},
    workspace::{overview_v119, DisplayRow, OverviewLinkV119, RowAction, Workspace},
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
/// The Commits panel title, also the hidden heading row that starts its section.
pub const COMMITS_TITLE: &str = overview_v119::RECENT;
/// Width of the label column in preview rows.
const LABEL: usize = 9;

/// The Changes panel columns: change, where it is (index, working tree, both), and the path.
pub const FILE_COLUMNS: [Column; 3] = [
    Column::new("STATUS", 9).priority(1),
    Column::new("STAGE", 8).priority(2),
    Column::new("PATH", 12).flex(1).priority(0),
];
/// Prose panels (Branch, Worktrees, Stashes, Recover) draw one text cell per row.
const TEXT_COLUMNS: [Column; 1] = [Column::new("ITEM", 12).flex(1).priority(0)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelActionV93 {
    /// Show or collapse the untracked files in the Changes panel.
    ToggleUntracked,
    /// Show or collapse the worktree stack (TASK-118).
    ToggleWorktrees,
    /// Show or collapse the commits the reflogs can recover (TASK-119).
    ToggleRecovery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Branch,
    Changes,
    Worktrees,
    Stashes,
    Commits,
    Recover,
}
/// The panels in drawing order.
pub const PANELS: [Panel; 6] = [
    Panel::Branch,
    Panel::Changes,
    Panel::Worktrees,
    Panel::Stashes,
    Panel::Commits,
    Panel::Recover,
];
impl Panel {
    pub fn index(self) -> usize {
        self as usize
    }
    /// The panel a hidden heading row starts.
    fn from_heading(label: &str) -> Option<Self> {
        Some(match label {
            overview_v119::CHANGES => Self::Changes,
            overview_v119::WORKTREES => Self::Worktrees,
            overview_v119::STASHES => Self::Stashes,
            overview_v119::RECENT => Self::Commits,
            overview_v119::RECOVER => Self::Recover,
            _ => return None,
        })
    }
}

/// Panel scroll offsets and the collapse toggles. The selection itself stays in
/// `App::repository_list`.
#[derive(Clone, Debug, Default)]
pub struct PanelsV93 {
    pub show_untracked: bool,
    pub show_worktrees: bool,
    pub show_recovery: bool,
    pub tops: [usize; 6],
    lens: [usize; 6],
    visible: [usize; 6],
    /// Painted panel areas, by [`Panel::index`] (empty when a panel is not drawn).
    pub areas: [Rect; 6],
    last_selected: Option<usize>,
}

/// One painted line of a panel: a display row, the untracked toggle, or the way back to the
/// worktree the Repository view was showing before another one was opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Line93 {
    Row(usize),
    Untracked,
    Back,
    /// A link the panel draws itself (a button, a count, an empty section's hint): it is
    /// clickable but outside the keyboard order. An index into the draw's link actions.
    Link(usize),
}

/// The Overview rows sorted into panels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sorted {
    /// Display-row indexes shown in each panel, by [`Panel::index`], in order.
    pub sections: [Vec<usize>; 6],
    /// Whether the read has the section at all (Recover only when the reflogs hold any).
    pub present: [bool; 6],
    /// Untracked file rows, shown or not.
    pub untracked: usize,
    /// Per display row: hidden from the panels (collapsed rows, the section headings).
    pub hidden: Vec<bool>,
}
impl Sorted {
    /// Display rows of the Changes panel.
    pub fn files(&self) -> &[usize] {
        &self.sections[Panel::Changes.index()]
    }
    /// Display rows of the Recent commits panel.
    pub fn commits(&self) -> &[usize] {
        &self.sections[Panel::Commits.index()]
    }
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
                    .any(|row| row.action.is_none() && row.label == COMMITS_TITLE)
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

/// Sorts the rows into the panels, leaving out the collapsed ones.
pub fn sort(workspace: &Workspace, panels: &PanelsV93) -> Sorted {
    let rows = workspace.display_rows_v1();
    let entries = entries(workspace);
    let mut sorted = Sorted {
        hidden: vec![false; rows.len()],
        ..Sorted::default()
    };
    sorted.present[Panel::Branch.index()] = true;
    let mut panel = Panel::Branch;
    for (index, row) in rows.iter().enumerate() {
        if let Some(next) = Panel::from_heading(&row.label).filter(|_| row.action.is_none()) {
            panel = next;
            sorted.present[panel.index()] = true;
            sorted.hidden[index] = true;
            continue;
        }
        let shown = match panel {
            Panel::Changes => {
                if entries[index].is_some_and(StatusEntry::untracked) {
                    sorted.untracked += 1;
                    panels.show_untracked
                } else {
                    true
                }
            }
            Panel::Worktrees => {
                panels.show_worktrees
                    || row.action
                        == Some(RowAction::OverviewV119(OverviewLinkV119::ToggleWorktrees))
            }
            Panel::Recover => {
                panels.show_recovery
                    || row.action == Some(RowAction::OverviewV119(OverviewLinkV119::ToggleRecovery))
            }
            Panel::Branch | Panel::Stashes | Panel::Commits => true,
        };
        if shown {
            sorted.sections[panel.index()].push(index);
        } else {
            sorted.hidden[index] = true;
        }
    }
    sorted
}

/// Display rows the panels hide (keyboard movement skips them), when the panels are shown.
pub fn hidden_rows(app: &App) -> Option<Vec<bool>> {
    active(app).then(|| sort(app.workspace.as_ref().unwrap(), &app.repository_panels_v93).hidden)
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
    let panels = &mut app.repository_panels_v93;
    panels.last_selected = None;
    match action {
        PanelActionV93::ToggleUntracked => {
            panels.show_untracked = !panels.show_untracked;
            let count = app.workspace.as_ref().map_or(0, |workspace| {
                sort(
                    workspace,
                    &PanelsV93 {
                        show_untracked: true,
                        ..PanelsV93::default()
                    },
                )
                .untracked
            });
            app.status = if app.repository_panels_v93.show_untracked {
                format!("Showing {count} untracked files • u hides them • nothing changed")
            } else {
                format!("{count} untracked files collapsed • u shows them • nothing changed")
            };
        }
        PanelActionV93::ToggleWorktrees => {
            panels.show_worktrees = !panels.show_worktrees;
            app.status = if panels.show_worktrees {
                "Worktrees shown • Enter or click one to open it here • nothing changed".into()
            } else {
                "Worktrees collapsed • nothing changed".into()
            };
        }
        PanelActionV93::ToggleRecovery => {
            panels.show_recovery = !panels.show_recovery;
            app.status = if panels.show_recovery {
                "Recoverable commits shown • Enter inspects one • nothing changed".into()
            } else {
                "Recoverable commits collapsed • nothing changed".into()
            };
        }
    }
    // A collapsed selection moves to the nearest earlier visible row that has an action
    // (the toggle row it was opened from), or the first one.
    if selection_hidden(app) {
        if let (Some(hidden), Some(workspace)) = (hidden_rows(app), app.workspace.as_ref()) {
            let rows = workspace.display_rows_v1();
            let usable = |index: &usize| rows[*index].action.is_some() && !hidden[*index];
            let selected = app.repository_list.selected.min(rows.len());
            if let Some(index) = (0..selected)
                .rev()
                .find(usable)
                .or_else(|| (0..rows.len()).find(usable))
            {
                app.repository_list.selected = index;
            }
        }
    }
}

/// The panels' own keys, added to the Repository key bar: the untracked toggle and the way
/// back to the worktree shown before another one was opened.
pub fn footer_keys(app: &App) -> Vec<KeyControl> {
    if !active(app) {
        return vec![];
    }
    let mut keys = vec![];
    let untracked = sort(
        app.workspace.as_ref().unwrap(),
        &PanelsV93 {
            show_untracked: true,
            ..PanelsV93::default()
        },
    )
    .untracked;
    if untracked > 0 {
        let shown = app.repository_panels_v93.show_untracked;
        keys.push(
            KeyControl::new(
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
            .with_priority(25),
        );
    }
    // The Branch section's Push and Pull buttons by key (TASK-119); each opens its Local
    // operation form with preview and confirmation.
    for (_, command) in overview_v119::branch_buttons(&app.workspace.as_ref().unwrap().status) {
        let (key, label, priority) = match command {
            // Lowest priorities: the buttons in the Branch section stay clickable at 80 columns.
            crate::local_actions::LocalCommand::Pull => ("L", "Pull", 41),
            _ => ("p", "Push", 40),
        };
        keys.push(
            KeyControl::new(key, label, Action::LocalCommand(command)).with_priority(priority),
        );
    }
    if crate::repository_worktrees_v118::away(app).is_some() {
        keys.push(
            KeyControl::new(
                "B",
                "Back to first worktree",
                Action::WorktreeV118(crate::repository_worktrees_v118::WorktreeActionV118::Back),
            )
            .with_compact("Back")
            .with_priority(8),
        );
    }
    keys
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

/// A prose row; the stack and recovery summary rows show whether their list is open.
fn text_row(row: &DisplayRow, panels: &PanelsV93) -> TableRow<Line93> {
    let label = match row.action {
        Some(RowAction::OverviewV119(OverviewLinkV119::ToggleWorktrees)) => format!(
            "{} {}",
            if panels.show_worktrees { "▾" } else { "▸" },
            row.label
        ),
        Some(RowAction::OverviewV119(OverviewLinkV119::ToggleRecovery)) => format!(
            "{} {}",
            if panels.show_recovery { "▾" } else { "▸" },
            row.label
        ),
        _ => row.label.clone(),
    };
    TableRow::text(label).with_tone(SemanticToken::from(row.role))
}

fn panel_rows(
    app: &App,
    workspace: &Workspace,
    sorted: &Sorted,
    panel: Panel,
    now: u64,
    links: &mut Vec<Action>,
) -> Vec<TableRow<Line93>> {
    let rows = workspace.display_rows_v1();
    let panels = &app.repository_panels_v93;
    let indexes = &sorted.sections[panel.index()];
    let mut output = vec![];
    if panel == Panel::Branch {
        if let Some(back) = crate::repository_worktrees_v118::away(app) {
            output.push(
                TableRow::text(format!("◂ {back} · B"))
                    .with_tone(SemanticToken::Reference)
                    .with_action(Some(Line93::Back)),
            );
        }
    }
    if panel == Panel::Commits {
        output.extend(indexes.iter().map(|index| {
            let row = crate::repository_ui::overview_row_v91(&rows[*index], now);
            TableRow {
                kind: row.kind,
                cells: row.cells,
                tone: row.tone,
                glyph: row.glyph,
                actor: row.actor,
                action: Some(Line93::Row(*index)),
            }
        }));
        return output;
    }
    let mut link = |output: &mut Vec<TableRow<Line93>>, text: String, tone, action| {
        output.push(
            TableRow::text(text)
                .with_tone(tone)
                .with_action(Some(Line93::Link(links.len()))),
        );
        links.push(action);
    };
    if panel == Panel::Changes {
        for (text, mode) in overview_v119::change_links(&workspace.status) {
            link(
                &mut output,
                text,
                SemanticToken::Reference,
                Action::RepositoryMode(mode),
            );
        }
    }
    let entries = entries(workspace);
    for (position, index) in indexes.iter().enumerate() {
        let row = &rows[*index];
        output.push(
            match entries[*index] {
                Some(entry) => file_row(row, entry),
                None => text_row(row, panels),
            }
            .with_action(Some(Line93::Row(*index))),
        );
        // The Branch row is followed by its Push and Pull buttons and the last commit's full
        // id and UTC time (a click opens it; the keyboard reaches it in Recent commits).
        if panel == Panel::Branch && position == 0 {
            for (text, command) in overview_v119::branch_buttons(&workspace.status) {
                link(
                    &mut output,
                    format!("[{text}]"),
                    SemanticToken::Reference,
                    Action::LocalCommand(command),
                );
            }
            if let Some(commit) = &workspace.selected_commit {
                let last = workspace.status.head.as_deref() == Some(commit.id.as_str());
                for text in overview_v119::commit_lines(commit, last) {
                    link(
                        &mut output,
                        text,
                        SemanticToken::Muted,
                        Action::RepositoryRow(RowAction::Commit(commit.id.clone())),
                    );
                }
            }
        }
    }
    if indexes.is_empty() {
        match panel {
            Panel::Worktrees => link(
                &mut output,
                "Only this checkout · add a linked worktree to work on another branch beside it"
                    .into(),
                SemanticToken::Muted,
                Action::LocalCommand(crate::local_actions::LocalCommand::AddWorktree),
            ),
            Panel::Stashes => link(
                &mut output,
                "No stashes · a stash puts unfinished changes aside for later".into(),
                SemanticToken::Muted,
                Action::LocalCommand(crate::local_actions::LocalCommand::StashSave),
            ),
            _ => {}
        }
    }
    if panel == Panel::Changes && sorted.untracked > 0 {
        let text = if panels.show_untracked {
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

/// Rows of two panels that share `height`, each with its rule: each gets what it needs when
/// both fit; otherwise the smaller need is met first and the other takes the rest.
pub fn heights(height: u16, files: usize, commits: usize) -> (u16, u16) {
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
        half.max(2).min(height)
    };
    (files, height - files)
}

/// A panel's rows plus its titled rule.
fn need(rows: usize) -> u16 {
    (rows.min(usize::from(u16::MAX - 2)) as u16).max(1) + 1
}

/// Heights of the panels in `height`, by [`Panel::index`]. Branch, Stashes, Worktrees and
/// Recover take what they need up to a share; Changes and Recent commits split the rest.
pub fn layout(height: u16, rows: [usize; 6], present: [bool; 6]) -> [u16; 6] {
    let mut heights_out = [0u16; 6];
    let mut left = height;
    // The Branch section leaves Changes and Recent commits at least a rule and a row each;
    // the other sections leave Changes all it needs and Recent commits a few rows (on a short
    // terminal they give way first).
    let changes_first = need(rows[Panel::Changes.index()])
        .min(height / 2)
        .saturating_add(need(rows[Panel::Commits.index()]).min(4))
        .max(4);
    for (panel, cap) in [
        (Panel::Branch, 9u16),
        (Panel::Stashes, 2),
        (Panel::Worktrees, (height / 3).max(2)),
        (Panel::Recover, (height / 3).max(2)),
    ] {
        if !present[panel.index()] {
            continue;
        }
        let reserve = if panel == Panel::Branch {
            4
        } else {
            changes_first
        };
        let room = left.saturating_sub(reserve);
        let take = need(rows[panel.index()]).min(cap).min(room);
        if take >= 2 {
            heights_out[panel.index()] = take;
            left -= take;
        }
    }
    let (changes, commits) = heights(
        left,
        rows[Panel::Changes.index()],
        rows[Panel::Commits.index()],
    );
    heights_out[Panel::Changes.index()] = changes;
    heights_out[Panel::Commits.index()] = commits;
    heights_out
}

fn title(app: &App, workspace: &Workspace, panel: Panel) -> String {
    match panel {
        Panel::Branch => match crate::repository_worktrees_v118::here_label(app) {
            Some(here) => format!("Branch · Worktree: {here}"),
            None => "Branch".into(),
        },
        Panel::Changes => {
            let counts = crate::git::ChangeCountsV118::from_status(&workspace.status);
            if counts.clean() {
                "Changes · clean".into()
            } else {
                let mut parts = vec!["Changes".to_owned()];
                for (count, word) in [
                    (counts.conflicted, "conflicted"),
                    (counts.staged, "staged"),
                    (counts.unstaged, "unstaged"),
                    (counts.untracked, "untracked"),
                ] {
                    if count > 0 {
                        parts.push(format!("{count} {word}"));
                    }
                }
                parts.join(" · ")
            }
        }
        Panel::Worktrees => overview_v119::WORKTREES.into(),
        Panel::Stashes => overview_v119::STASHES.into(),
        Panel::Commits => COMMITS_TITLE.into(),
        Panel::Recover => overview_v119::RECOVER.into(),
    }
}

/// Paints the panels into `area` and registers one click target per row.
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(workspace) = app.workspace.as_ref() else {
        return;
    };
    let theme = app.theme();
    let sorted = sort(workspace, &app.repository_panels_v93);
    let now = crate::storage::now();
    let mut links = vec![];
    let contents: Vec<(Panel, String, Vec<TableRow<Line93>>)> = PANELS
        .iter()
        .map(|panel| {
            (
                *panel,
                title(app, workspace, *panel),
                panel_rows(app, workspace, &sorted, *panel, now, &mut links),
            )
        })
        .collect();
    let selected = app.repository_list.selected;
    let list_focused = app.repository_content_focused && crate::list_preview_v92::list_focused(app);
    let mut present = sorted.present;
    let mut counts = [0usize; 6];
    for (panel, _, rows) in &contents {
        counts[panel.index()] = rows.len();
        present[panel.index()] &= !rows.is_empty() || *panel != Panel::Recover;
    }
    let heights = layout(area.height, counts, present);
    let selection_moved = app.repository_panels_v93.last_selected != Some(selected);
    app.repository_panels_v93.last_selected = Some(selected);
    let mut hits = vec![];
    let mut y = area.y;
    for (panel, title, rows) in contents {
        let height = heights[panel.index()];
        let rect = Rect::new(area.x, y, area.width, height);
        y += height;
        let state = &mut app.repository_panels_v93;
        state.areas[panel.index()] = Rect::default();
        if rect.height < 2 || rect.width < 8 {
            continue;
        }
        let columns: &[Column] = match panel {
            Panel::Changes => &FILE_COLUMNS[..],
            Panel::Commits => &crate::repository_ui::OVERVIEW_COLUMNS[..],
            _ => &TEXT_COLUMNS[..],
        };
        let position = rows
            .iter()
            .position(|row| row.action == Some(Line93::Row(selected)));
        let focused = list_focused && position.is_some();
        // A panel is a titled rule inside the Repository pane, not a box nested in its frame
        // (`│╭─`/`╯││`): the rule meets the pane's side borders as `├`/`┤` junctions.
        let block = pane_block(theme, &[&title], focused).borders(ratatui::widgets::Borders::TOP);
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        join_rule(frame, rect);
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
                Panel::Changes => "Working files are clean",
                Panel::Commits => "No commits yet",
                _ => "Nothing here",
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
                Line93::Back => {
                    Action::WorktreeV118(crate::repository_worktrees_v118::WorktreeActionV118::Back)
                }
                Line93::Link(index) => links.get(index)?.clone(),
                // The Branch row (always the first) opens History by click.
                Line93::Row(0)
                    if app.workspace.as_ref()?.display_rows_v1()[0]
                        .action
                        .is_none() =>
                {
                    Action::RepositoryMode(crate::workspace::RepositoryMode::Graph)
                }
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

/// Turns the pane border cells beside a panel rule at `rect.y` into `├`/`┤` junctions.
pub(crate) fn join_rule(frame: &mut Frame, rect: Rect) {
    let screen = frame.area();
    let buffer = frame.buffer_mut();
    for (x, junction) in [
        (rect.x.checked_sub(1), "├"),
        (Some(rect.right()).filter(|x| *x < screen.right()), "┤"),
    ] {
        let Some(x) = x else { continue };
        if let Some(cell) = buffer.cell_mut((x, rect.y)) {
            if cell.symbol() == "│" {
                cell.set_symbol(junction);
            }
        }
    }
}

/// The wheel over a panel scrolls that panel only. Returns whether the event was used.
pub fn mouse(app: &mut App, column: u16, row: u16, wheel: Option<isize>) -> bool {
    let Some(delta) = wheel else {
        return false;
    };
    if app.overlay.is_some() || !active(app) {
        return false;
    }
    let Some(panel) = PANELS.into_iter().find(|panel| {
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
    let sorted = sort(app.workspace.as_ref().unwrap(), &app.repository_panels_v93);
    let selected = app.repository_list.selected;
    let panel = PANELS
        .into_iter()
        .find(|panel| sorted.sections[panel.index()].contains(&selected))
        .unwrap_or(Panel::Changes);
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
    if let Some(RowAction::OpenWorktree(path)) = &row.action {
        let lines = vec![
            Line::from(Span::styled(row.label.clone(), bold)),
            Line::default(),
            label_value(theme, "Folder", &path.display().to_string(), LABEL),
            Line::default(),
            muted(
                theme,
                "Enter opens this worktree here; nothing is checked out or changed",
            ),
        ];
        return crate::list_preview_v92::Preview {
            item,
            title: "Worktree".into(),
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

/// The Repository viewers that own every event while open (native diff, saved-folder detail,
/// saved-path picker, changed-files picker) drawn into `area`, in the order they own it.
/// Returns whether one was drawn. [`takeover::draw`] gives them the content pane.
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

#[path = "repository_panels_v93_takeover.rs"]
pub mod takeover;

#[cfg(test)]
#[path = "repository_panels_v93_tests.rs"]
mod tests;
