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
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::Paragraph,
    Frame,
};

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
    /// An ancestor of the selected commit: the projection's ancestry (TASK-28).
    ancestor: bool,
    /// Some while History filters: does this commit match (TASK-99.3)?
    matched: Option<bool>,
    /// The lowercase search text, underlined where it occurs in the subject.
    query: Option<String>,
    /// The path chip's file under another name in this commit ("was docs/guide.md").
    renamed: Option<String>,
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

/// The graph cut to `width` cells for a capped GRAPH column: the lanes that fit, then `…`.
/// The commit's own `●` always stays visible, after the `…` when its lane is further right,
/// so every row shows its dot in its lane color.
fn clip_graph(spans: &[(String, Style)], width: usize, theme: Theme) -> Vec<(String, Style)> {
    use unicode_segmentation::UnicodeSegmentation;
    let cells: Vec<(&str, Style)> = spans
        .iter()
        .flat_map(|(text, style)| text.graphemes(true).map(move |glyph| (glyph, *style)))
        .collect();
    let total: usize = cells
        .iter()
        .map(|(glyph, _)| crate::editor::grapheme_width(glyph))
        .sum();
    if total <= width || width < 2 {
        return spans.to_vec();
    }
    let dot = cells.iter().position(|(glyph, _)| *glyph == "●");
    let ellipsis = ("…", theme.role(Role::TextDim));
    let mut kept: Vec<(&str, Style)> = match dot {
        Some(at) if at + 1 >= width => {
            let mut kept = cells[..width - 2].to_vec();
            kept.push(ellipsis);
            kept.push(cells[at]);
            kept
        }
        _ => {
            let mut kept = cells[..width - 1].to_vec();
            kept.push(ellipsis);
            kept
        }
    };
    let mut out: Vec<(String, Style)> = Vec::new();
    for (glyph, style) in kept.drain(..) {
        match out.last_mut() {
            Some((text, last)) if *last == style => text.push_str(glyph),
            _ => out.push((glyph.to_owned(), style)),
        }
    }
    out
}

fn parts(workspace: &Workspace, index: usize, now: u64, theme: Theme) -> Option<Parts> {
    let display = workspace.display_rows_v1().get(index)?;
    let Some(RowAction::Commit(id)) = &display.action else {
        return None;
    };
    let state = workspace.graph_state_v1.as_ref()?;
    let observed = state.observed(id)?;
    let commit = observed.commit();
    let filter = &workspace.history_filter_v993;
    let matched = filter.matched(state, id);
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
        ancestor: display.role == crate::repository_view::SemanticRole::Reference,
        matched,
        query: filter
            .spec
            .text()
            .filter(|_| matched == Some(true))
            .map(str::to_lowercase),
        renamed: filter.renamed(state, id).map(str::to_owned),
    })
}

/// The subject with every occurrence of the (lowercase) query emphasised.
fn subject_spans(
    subject: &str,
    query: Option<&str>,
    base: Style,
    hit: Style,
) -> Vec<(String, Style)> {
    let Some(query) = query.filter(|query| !query.is_empty()) else {
        return vec![(subject.to_owned(), base)];
    };
    let lower = subject.to_lowercase();
    // A fuzzy match (TASK-122) highlights its matched characters, like every finder.
    if !lower.contains(query) {
        return match crate::fuzzy_finder::word_positions_v1(query, subject) {
            Some(positions) => crate::fuzzy_finder::highlight_spans_v1(
                subject,
                &positions,
                0,
                usize::MAX / 4,
                base,
                hit,
            )
            .into_iter()
            .map(|span| (span.content.into_owned(), span.style))
            .collect(),
            None => vec![(subject.to_owned(), base)],
        };
    }
    // Byte offsets only line up when lowercasing kept every length.
    if lower.len() != subject.len() {
        return vec![(subject.to_owned(), base)];
    }
    let mut spans = Vec::new();
    let mut at = 0;
    for (start, found) in lower.match_indices(query) {
        if start < at
            || !subject.is_char_boundary(start)
            || !subject.is_char_boundary(start + found.len())
        {
            continue;
        }
        if start > at {
            spans.push((subject[at..start].to_owned(), base));
        }
        spans.push((subject[start..start + found.len()].to_owned(), hit));
        at = start + found.len();
    }
    if at < subject.len() {
        spans.push((subject[at..].to_owned(), base));
    }
    spans
}

/// The MESSAGE cell: chips (one blank between them and before the subject), then the subject.
/// `width` limits the chips to what leaves the subject [`SUBJECT_KEEP`] cells; the rest are
/// counted in a `+N` chip.
fn message(parts: &Parts, width: Option<usize>, theme: Theme) -> Cell {
    let dimmed = parts.matched == Some(false);
    let base = theme.role(if dimmed { Role::TextDim } else { Role::Text });
    let subject_cells = crate::editor::display_width(&parts.subject).min(SUBJECT_KEEP);
    let budget = width.map(|width| width.saturating_sub(subject_cells + 1));
    let (shown, hidden) = match budget {
        Some(budget) => refs::fit(&parts.chips, budget),
        None => (parts.chips.len(), 0),
    };
    // A lone chip wider than its budget (a long branch name) is cut with `…` so the subject
    // keeps its cells, never below a readable stub.
    let overflow = if hidden > 0 {
        crate::editor::display_width(&refs::overflow_label(hidden)) + 3
    } else {
        0
    };
    let label_cells = match budget {
        Some(budget) if shown == 1 => budget
            .saturating_sub(overflow + 2)
            .clamp(6, refs::MAX_CHIP_CELLS),
        _ => refs::MAX_CHIP_CELLS,
    };
    let mut spans = Vec::new();
    for chip in &parts.chips[..shown] {
        spans.push((
            format!(" {} ", crate::ui_chrome::fit(&chip.label, label_cells)),
            if dimmed {
                theme.role(Role::TextDim)
            } else {
                chip_style(theme, chip.kind)
            },
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
    // A followed file's other name leads the subject, so a narrow MESSAGE never cuts it.
    if let Some(renamed) = &parts.renamed {
        spans.push((
            renamed.clone(),
            theme
                .role(if dimmed {
                    Role::TextDim
                } else {
                    Role::TextMuted
                })
                .add_modifier(Modifier::ITALIC),
        ));
        spans.push(("  ".into(), base));
    }
    spans.extend(subject_spans(
        &parts.subject,
        parts.query.as_deref(),
        base,
        theme
            .role(Role::Warning)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    ));
    Cell::spans(spans)
}

fn row(parts: &Parts, width: Option<usize>, theme: Theme) -> TableRow<RowAction> {
    let sha = crate::ui_table::short_sha(&parts.id);
    // While History filters, a match keeps full ink with a bold SHA and its text underlined;
    // a commit that does not match stays for context, dimmed (TASK-99.3).
    let dimmed = parts.matched == Some(false);
    // The SHA is subtext, never a lane or chip hue, so a yellow or teal lane and the tag or
    // branch chips beside it never run together; the selected row reads in full ink.
    let sha_role = if parts.focused {
        Role::Text
    } else {
        Role::TextMuted
    };
    // Unfiltered, the selected commit and its ancestry carry a bold full-ink SHA, so the
    // path that led to the selection reads at a glance (also without color) (TASK-28).
    let sha = match parts.matched {
        Some(true) => Cell::spans(vec![(
            sha.to_owned(),
            theme.role(sha_role).add_modifier(Modifier::BOLD),
        )]),
        Some(false) => Cell::styled(sha, Role::TextDim),
        None if parts.focused || parts.ancestor => Cell::spans(vec![(
            sha.to_owned(),
            theme.role(Role::Text).add_modifier(Modifier::BOLD),
        )]),
        None => Cell::styled(sha, sha_role),
    };
    TableRow::cells(vec![
        Cell::spans(parts.graph.clone()),
        sha,
        message(parts, width, theme),
        if dimmed {
            // The initials and name of the chip, in dim ink so the row reads as context.
            Cell::styled(
                format!(
                    "{} {}",
                    crate::identity::InitialsChip::text(&parts.author, theme.mode),
                    crate::ui_table::actor_text(&parts.author)
                ),
                Role::TextDim,
            )
        } else {
            String::new().into()
        },
        Cell::styled(
            parts.age.as_str(),
            if dimmed {
                Role::TextDim
            } else {
                Role::TextMuted
            },
        ),
    ])
    .with_actor((!dimmed).then(|| parts.author.clone()))
    .with_action(Some(RowAction::Commit(parts.id.clone())))
}

/// Lanes the capped GRAPH column always fits.
const GRAPH_MIN_LANES: usize = 4;

/// Cells of the GRAPH column at most: a fifth of the table, never fewer than four lanes.
pub(crate) fn graph_cap(width: u16) -> usize {
    usize::from(width / 5).max(GRAPH_MIN_LANES * 2)
}

/// The summary with the filter's mode when History filters.
fn summary_text(workspace: &Workspace, label: &str, width: u16) -> String {
    let filter = &workspace.history_filter_v993;
    let mut summary = label.to_owned();
    if filter.spec.matching() {
        // The long wording only when it fits; the filter row already names the mode.
        let room = usize::from(width.saturating_sub(6));
        let long = if filter.spec.hide {
            " · non-matching commits hidden"
        } else {
            " · non-matching commits stay visible for context"
        };
        let short = if filter.spec.hide {
            " · non-matches hidden"
        } else {
            " · non-matches dimmed"
        };
        let fits =
            crate::editor::display_width(&summary) + crate::editor::display_width(long) <= room;
        summary.push_str(if fits { long } else { short });
        if filter.chip_capped() {
            summary.push_str(" · matches stop at the 10,000-commit walk");
        }
    }
    crate::app::safe_text(&summary)
}

/// `╰─ Read-only · N commits loaded · … ─╯`: the status on the pane's bottom border, one
/// border cell kept at each end.
fn draw_border_status(frame: &mut Frame, theme: Theme, summary: &str, inner: Rect) {
    let room = usize::from(inner.width.saturating_sub(4));
    let text = format!(" {} ", crate::ui_chrome::fit(summary, room));
    let width = (crate::editor::display_width(&text) as u16).min(inner.width.saturating_sub(2));
    frame.render_widget(
        Paragraph::new(text).style(theme.role(Role::TextMuted)),
        Rect::new(inner.x + 1, inner.bottom(), width, 1),
    );
}

/// `↑ 200 newer commits · < page` above the column titles.
fn draw_newer_boundary(frame: &mut Frame, theme: Theme, newer: usize, rect: Rect) {
    use ratatui::text::{Line, Span};
    let mut spans = vec![Span::styled(
        format!(
            "  ↑ {newer} newer {} · ",
            if newer == 1 { "commit" } else { "commits" }
        ),
        theme.role(Role::TextMuted),
    )];
    spans.extend(crate::ui_chrome::key_token_line(theme, "<", "page", false).spans);
    frame.render_widget(Paragraph::new(Line::from(spans)), rect);
}

/// The repository's folder name for the `History › <repo> › <scope>` title.
pub(crate) fn repository_name(workspace: &crate::workspace::Workspace) -> String {
    crate::app::safe_text(&workspace.directory.file_name().map_or_else(
        || workspace.directory.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    ))
}

/// The `History › <repo> › <last>` rule. The repository crumb gives way first on a narrow
/// pane, so the title never runs under the match count on the right.
pub(crate) fn title_block(
    theme: crate::theme::Theme,
    repository: &str,
    last: &str,
    width: u16,
    focused: bool,
) -> ratatui::widgets::Block<'static> {
    let full = [TITLE, repository, last];
    let fits = !repository.is_empty()
        && crate::editor::display_width(&crate::ui_chrome::crumb_title(&full, focused)) + 28
            <= usize::from(width);
    let crumbs: &[&str] = if fits { &full } else { &[TITLE, last] };
    crate::ui_chrome::pane_block(theme, crumbs, focused).borders(ratatui::widgets::Borders::TOP)
}

/// Draws the History rule, table and status line into `area` and registers one click target
/// per painted row with the same action Enter takes.
pub(crate) fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(workspace) = app.workspace.as_ref() else {
        return;
    };
    let theme = app.theme();
    // Beside the commit detail the list keeps its bar dim while the detail owns the focus.
    let focused = app.repository_content_focused && crate::list_preview_v92::list_focused(app);
    if area.height < 3 || area.width < 8 {
        return;
    }
    let scope = super::history_filter::title_scope(workspace);
    let repository = repository_name(workspace);
    let block = title_block(theme, &repository, &scope, area.width, focused);
    let mut inner = block.inner(area);
    frame.render_widget(block, area);
    crate::repository_panels_v93::join_rule(frame, area);
    // The filter row (and, with room, a rule under it) sits above the table header.
    super::history_filter::draw_count(frame, app, Rect::new(area.x, area.y, area.width, 1));
    // The filter row sits directly above the column titles at every size (no rule).
    if super::history_filter::shown(app) && inner.height >= 6 {
        super::history_filter::draw_row(frame, app, Rect::new(inner.x, inner.y, inner.width, 1));
        inner = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1);
    }
    let Some(workspace) = app.workspace.as_ref() else {
        return;
    };
    let rows = workspace.display_rows_v1();
    // Row 0 is the window summary (`Read-only · N commits loaded`): History's one status
    // line, drawn on the pane's bottom border below the table (TASK-100).
    let summary = rows
        .first()
        .filter(|row| row.action.is_none())
        .map(|row| summary_text(workspace, row.label.as_str(), inner.width));
    // The border row under the table; without one (a bare test area) the summary keeps the
    // table's last row instead.
    let on_border = inner.bottom() < frame.area().bottom() && inner.width >= 12;
    let status_rows = u16::from(summary.is_some() && !on_border && inner.height >= 4);
    let mut body = Rect::new(inner.x, inner.y, inner.width, inner.height - status_rows);
    if let Some(summary) = &summary {
        if on_border {
            draw_border_status(frame, theme, summary, inner);
        } else if status_rows == 1 {
            frame.render_widget(
                Paragraph::new(crate::ui_chrome::fit(
                    summary,
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
    }
    // A window that does not start at the newest loaded commit says how many are newer, as
    // a row above the column titles that pages to them like `<` (TASK-100).
    let newer = workspace.graph_projection_start_v2;
    let mut boundary = None;
    if newer > 0 && body.height >= 5 {
        let rect = Rect::new(body.x, body.y, body.width, 1);
        draw_newer_boundary(frame, theme, newer, rect);
        boundary = Some(HitRegion {
            rect,
            action: Action::RepositoryRow(RowAction::Graph(
                crate::repository_graph_v1::GraphActionV1::Window(
                    crate::repository_graph_v1::projection_v2::GraphWindowActionV2::Newer,
                ),
            )),
        });
        body = Rect::new(body.x, body.y + 1, body.width, body.height - 1);
    }
    let first = usize::from(summary.is_some());
    let now = crate::storage::now();
    let mut parts: Vec<Option<Parts>> = (first..rows.len())
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
    // Never leave blank rows below the last commit while earlier ones are scrolled away (a
    // taller terminal, or a shorter filtered window).
    top = top.min(parts.len().saturating_sub(visible));
    let mut natural = crate::ui_table::natural_widths(&HISTORY_COLUMNS, &full, theme.mode);
    // GRAPH grows to the widest lanes shown so far and never shrinks while History stays
    // open, so COMMIT and MESSAGE do not jump sideways when scrolling back past a merge, and
    // a burst of parallel branches further down does not squeeze every message up front
    // (TASK-99). It never takes more than a fifth of the table (but always four lanes), so
    // messages stay readable beside many lanes; wider rows end in `…` before their `●` and
    // pan with `←`/`→` (TASK-100).
    let lanes = parts
        .iter()
        .skip(top)
        .take(visible.max(1))
        .flatten()
        .map(|part| {
            part.graph
                .iter()
                .map(|(text, _)| crate::editor::display_width(text))
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0)
        .max(app.history_graph_width_v99);
    app.history_graph_width_v99 = lanes;
    let cap = graph_cap(body.width);
    natural[0] = u16::try_from(lanes.min(cap))
        .unwrap_or(u16::MAX)
        .max(crate::editor::display_width(HISTORY_COLUMNS[0].title) as u16);
    for part in parts.iter_mut().flatten() {
        part.graph = clip_graph(&part.graph, usize::from(natural[0]), theme);
    }
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
    app.hits.extend(boundary);
    if app.preview_area_v92.is_none() {
        crate::history::detail::prefetch(app);
    }
}

#[cfg(test)]
#[path = "history_table_tests.rs"]
mod history_refs_v992_table_tests;
