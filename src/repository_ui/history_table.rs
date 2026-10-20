//! The History table (TASK-99.2): one row per loaded commit as GRAPH | COMMIT | MESSAGE with
//! ref chips | AUTHOR chip | AGE under a `History › all branches` rule, with the shared `▌`
//! selection bar. Cells come from the prepared graph window and the typed chips the original
//! walk captured; drawing performs no Git read.
use crate::{
    app::{Action, App, HitRegion},
    git::HistoryRefKindV1 as Kind,
    history::refs::{self, RefChip},
    repository_graph_v1::GraphViewV1,
    theme::{Role, SemanticToken, Theme},
    ui_table::{Cell, Column, Table, TableRow},
    workspace::{RowAction, Workspace},
};
use ratatui::{layout::Rect, style::Style, widgets::Paragraph, Frame};

/// Lower priority stays longer: at 80 columns AGE leaves first, then AUTHOR, before MESSAGE
/// is cut below its minimum.
pub const HISTORY_COLUMNS: [Column; 5] = [
    Column::new("GRAPH", 5).priority(1),
    // Flexible at its natural width, so a long message never pushes the SHA out.
    Column::new("COMMIT", 7).flex(1).priority(2),
    Column::new("MESSAGE", 24).flex(4).priority(0),
    Column::new("AUTHOR", 10).flex(1).priority(3).actor(),
    Column::new("AGE", 3).flex(1).priority(4).right(),
];
const MESSAGE: usize = 2;
/// Cells of the subject kept beside the chips before chips give way to `+N`.
const SUBJECT_KEEP: usize = 16;
pub const TITLE: &str = "History";
pub const SCOPE: &str = "all branches";

/// The History table replaces the plain row list while the graph timeline is shown.
pub(crate) fn active(workspace: &Workspace) -> bool {
    workspace.graph_visible_v1()
        && workspace.graph_view_v1 == GraphViewV1::Timeline
        && workspace.graph_display_v1.is_some()
}

/// The chip style for each ref kind: HEAD, local and tag chips are filled with their role,
/// remote branches are the softer variant of the branch hue.
pub fn chip_style(theme: Theme, kind: Kind) -> Style {
    match kind {
        Kind::Head => theme.chip(Role::Head),
        Kind::Local => theme.chip(Role::Branch),
        Kind::Remote => theme.soft_chip(Role::Remote),
        Kind::Tag => theme.chip(Role::Tag),
    }
}

/// One commit row's parts before the MESSAGE width is known.
struct Parts {
    graph: Vec<(String, Style)>,
    id: String,
    chips: Vec<RefChip>,
    subject: String,
    author: crate::identity::ActorIdentity,
    age: String,
    focused: bool,
}

fn graph_spans(
    workspace: &Workspace,
    index: usize,
    horizontal: usize,
    theme: Theme,
) -> Vec<(String, Style)> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut spans = Vec::new();
    let mut skipped = 0usize;
    for part in workspace
        .graph_paint_v1(index)
        .unwrap_or_default()
        .iter()
        .filter(|part| part.color.is_some())
    {
        let mut text = String::new();
        for glyph in part.text.graphemes(true) {
            if skipped < horizontal {
                skipped += crate::editor::grapheme_width(glyph);
                continue;
            }
            text.push_str(glyph);
        }
        if !text.is_empty() {
            spans.push((text, theme.graph(part.color.unwrap_or(0))));
        }
    }
    // The window pads every graph to one width; the column does that now.
    while let Some((text, _)) = spans.last_mut() {
        let trimmed = text.trim_end().len();
        if trimmed == 0 {
            spans.pop();
        } else {
            text.truncate(trimmed);
            break;
        }
    }
    spans
}

fn parts(workspace: &Workspace, index: usize, now: u64, theme: Theme) -> Option<Parts> {
    let Some(RowAction::Commit(id)) = &workspace.display_rows_v1().get(index)?.action else {
        return None;
    };
    let observed = workspace.graph_state_v1.as_ref()?.observed(id)?;
    let commit = observed.commit();
    let age = u64::try_from(commit.author.timestamp)
        .map(|authored| crate::ui_table::age_from_seconds(now.saturating_sub(authored)))
        .unwrap_or_else(|_| crate::ui_table::NONE.into());
    Some(Parts {
        graph: graph_spans(workspace, index, workspace.graph_horizontal_v1, theme),
        id: commit.id.clone(),
        chips: refs::chips(observed.chips()),
        subject: commit.subject.clone(),
        author: crate::identity::ActorIdentity::local(
            &commit.author,
            crate::identity::GitRole::Author,
        ),
        age,
        focused: workspace.graph_focus_row_v1() == Some(index),
    })
}

/// The MESSAGE cell: chips (one blank between them and before the subject), then the subject.
/// `width` limits the chips to what leaves the subject [`SUBJECT_KEEP`] cells; the rest are
/// counted in a `+N` chip.
fn message(parts: &Parts, width: Option<usize>, theme: Theme) -> Cell {
    let base = theme.role(Role::Text);
    let subject_cells = crate::editor::display_width(&parts.subject).min(SUBJECT_KEEP);
    let (shown, hidden) = match width {
        Some(width) => refs::fit(&parts.chips, width.saturating_sub(subject_cells + 1)),
        None => (parts.chips.len(), 0),
    };
    let mut spans = Vec::new();
    for chip in &parts.chips[..shown] {
        spans.push((
            format!(
                " {} ",
                crate::ui_chrome::fit(&chip.label, refs::MAX_CHIP_CELLS)
            ),
            chip_style(theme, chip.kind),
        ));
        spans.push((" ".into(), base));
    }
    if hidden > 0 {
        spans.push((
            format!(" {} ", refs::overflow_label(hidden)),
            theme.badge(SemanticToken::Normal),
        ));
        spans.push((" ".into(), base));
    }
    spans.push((parts.subject.clone(), base));
    Cell::spans(spans)
}

fn row(parts: &Parts, width: Option<usize>, theme: Theme) -> TableRow<RowAction> {
    TableRow::cells(vec![
        Cell::spans(parts.graph.clone()),
        Cell::styled(
            crate::ui_table::short_sha(&parts.id),
            if parts.focused {
                Role::Head
            } else {
                Role::Commit
            },
        ),
        message(parts, width, theme),
        String::new().into(),
        Cell::styled(parts.age.as_str(), Role::TextMuted),
    ])
    .with_actor(Some(parts.author.clone()))
    .with_action(Some(RowAction::Commit(parts.id.clone())))
}

/// Draws the History rule, table and status line into `area` and registers one click target
/// per painted row with the same action Enter takes.
pub(crate) fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(workspace) = app.workspace.as_ref() else {
        return;
    };
    let theme = app.theme();
    let focused = app.repository_content_focused;
    if area.height < 3 || area.width < 8 {
        return;
    }
    let block = crate::ui_chrome::pane_block(theme, &[TITLE, SCOPE], focused)
        .borders(ratatui::widgets::Borders::TOP);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    crate::repository_panels_v93::join_rule(frame, area);
    let rows = workspace.display_rows_v1();
    // Row 0 is the window summary (`Read-only · N commits loaded`): the status line.
    let summary = rows
        .first()
        .filter(|row| row.action.is_none())
        .map(|row| row.label.as_str());
    let status_rows = u16::from(summary.is_some() && inner.height >= 4);
    let body = Rect::new(inner.x, inner.y, inner.width, inner.height - status_rows);
    if let (Some(summary), 1) = (summary, status_rows) {
        frame.render_widget(
            Paragraph::new(crate::ui_chrome::fit(
                &crate::app::safe_text(summary),
                usize::from(inner.width.saturating_sub(2)),
            ))
            .style(theme.role(Role::TextMuted)),
            Rect::new(
                inner.x + 2,
                inner.bottom() - 1,
                inner.width.saturating_sub(2),
                1,
            ),
        );
    }
    let first = usize::from(summary.is_some());
    let now = crate::storage::now();
    let parts: Vec<Option<Parts>> = (first..rows.len())
        .map(|index| parts(workspace, index, now, theme))
        .collect();
    let text_row = |index: usize| {
        let row = &rows[index];
        TableRow::text(row.label.as_str())
            .with_tone(SemanticToken::from(row.role))
            .with_action(row.action.clone())
    };
    let full: Vec<TableRow<RowAction>> = parts
        .iter()
        .enumerate()
        .map(|(offset, part)| match part {
            Some(part) => row(part, None, theme),
            None => text_row(first + offset),
        })
        .collect();
    let natural = crate::ui_table::natural_widths(&HISTORY_COLUMNS, &full, theme.mode);
    let lead = crate::ui_table::lead(false).min(body.width);
    let message_width = crate::ui_table::layout(&HISTORY_COLUMNS, &natural, body.width - lead)
        .into_iter()
        .find(|placed| placed.column == MESSAGE)
        .map(|placed| usize::from(placed.width));
    let table = Table {
        columns: &HISTORY_COLUMNS,
        header: true,
        focused,
        glyphs: false,
        empty: Some("No commits loaded"),
    };
    let visible = usize::from(body.height.saturating_sub(1));
    let selected = app
        .repository_list
        .selected
        .checked_sub(first)
        .filter(|index| *index < parts.len());
    let given = app
        .repository_list
        .top
        .saturating_sub(first)
        .min(parts.len().saturating_sub(1));
    let mut top = given;
    // A resize or a new window keeps the selected commit visible without undoing deliberate
    // wheel scrolling on unchanged geometry.
    if let Some(selected) = selected.filter(|_| visible != app.repository_visible_rows) {
        let mut list = crate::ui_table::ListState { selected, top };
        list.ensure_visible(visible);
        top = list.top;
    }
    let rendered = crate::ui_table::draw_table_at(
        frame,
        body,
        &table,
        &natural,
        parts.len(),
        |index| match &parts[index] {
            Some(part) => row(part, message_width, theme),
            None => text_row(first + index),
        },
        selected,
        top,
        theme,
    );
    let hits: Vec<HitRegion> = rendered
        .hits
        .into_iter()
        .filter_map(|hit| {
            Some(HitRegion {
                rect: hit.rect,
                action: Action::RepositoryRow(hit.action?),
            })
        })
        .collect();
    // The list's top counts the summary row; an unchanged viewport keeps the given value.
    if top != given {
        app.repository_list.top = top + first;
    }
    app.repository_visible_rows = rendered.visible_rows;
    app.hits.extend(hits);
}

#[cfg(test)]
#[path = "history_table_tests.rs"]
mod history_refs_v992_table_tests;
