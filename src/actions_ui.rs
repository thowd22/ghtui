//! Pure prepared Actions rendering and keyboard mapping. No subprocess, network or disk I/O.
use crate::{
    actions_workspace::{
        ActionsAction, ActionsCells, ActionsControl, ActionsRow, ActionsView, ActionsWorkspace,
    },
    app::Action,
    footer::{ControlGroup, KeyControl},
    identity::ActorIdentity,
    providers::runner::sanitize_terminal,
    theme::{Hue, Role, SemanticToken, Theme},
    ui_table::{Cell, ListState, RowKind, Table, TableRow},
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

#[derive(Clone, Debug)]
pub struct ActionsHit {
    pub rect: Rect,
    pub action: ActionsAction,
}
#[derive(Clone, Debug, Default)]
pub struct ActionsRender {
    pub hits: Vec<ActionsHit>,
    pub visible_rows: usize,
    pub content: Rect,
}

/// Common application Tab/Shift-Tab/Enter focus traversal should consume these same hits.
pub fn draw(
    frame: &mut Frame,
    workspace: &mut ActionsWorkspace,
    area: Rect,
    theme: Theme,
    focused: bool,
) -> ActionsRender {
    let mut output = ActionsRender::default();
    if area.width == 0 || area.height == 0 {
        return output;
    }
    frame.render_widget(Clear, area);
    let mut y = area.y;
    // Catalogue sub-modes are one row of tabs; every other control is in the key bar.
    if draw_tabs(
        frame,
        Rect::new(area.x, y, area.width, 1),
        theme,
        workspace,
        &mut output,
    ) {
        y = y.saturating_add(1);
    }
    if y < area.bottom() {
        frame.render_widget(
            Paragraph::new(sanitize_terminal(&workspace.context()))
                .style(theme.style(SemanticToken::Head)),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }
    if y < area.bottom() {
        let freshness = workspace.fetched_at.map_or("Not loaded".into(), |at| {
            format!("Fetched {}", crate::metrics_workspace::utc_label(at))
        });
        let watch = if workspace.watching && workspace.watch_applies() {
            format!(
                " • Watching: refreshes every 5s{} • w stops",
                workspace
                    .watch_note
                    .as_ref()
                    .map(|note| format!(" • {}", sanitize_terminal(note)))
                    .unwrap_or_default()
            )
        } else if workspace.watching {
            " • Watch paused here; it resumes on Runs or the run".into()
        } else {
            workspace
                .watch_note
                .as_ref()
                .map(|note| format!(" • {}", sanitize_terminal(note)))
                .unwrap_or_default()
        };
        let status = format!("{freshness}{watch}");
        // Freshness and watch state are provenance on the section header, never the heading.
        frame.render_widget(
            Paragraph::new(crate::ui_chrome::section_header(
                theme,
                workspace.view.label(),
                &status,
                if workspace.fetched_at.is_some() {
                    crate::theme::Role::TextMuted
                } else {
                    crate::theme::Role::Warning
                },
                area.width,
            )),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }
    if workspace.view == ActionsView::Runs && y < area.bottom() {
        let coverage = format!(
            "{} runs of {} reported • {}{}",
            workspace.coverage.loaded,
            workspace.coverage.total_matches,
            if workspace.coverage.complete {
                "complete"
            } else {
                "PARTIAL"
            },
            if workspace.coverage.reasons.is_empty() {
                String::new()
            } else {
                format!(" • {}", workspace.coverage.reasons.join(" • "))
            }
        );
        // A narrow (split) list ends the coverage row with an ellipsis rather than a cut word.
        frame.render_widget(
            Paragraph::new(crate::ui_chrome::fit(
                &sanitize_terminal(&coverage),
                usize::from(area.width),
            ))
            .style(theme.style(if workspace.coverage.complete {
                SemanticToken::Normal
            } else {
                SemanticToken::Pending
            })),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }
    if let Some(warning) = workspace.warnings.last().filter(|_| y < area.bottom()) {
        frame.render_widget(
            Paragraph::new(sanitize_terminal(warning)).style(theme.style(SemanticToken::Pending)),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }
    let content_height = area.bottom().saturating_sub(y);
    output.content = Rect::new(area.x, y, area.width, content_height);
    // Runs and Workflows are tables under a sticky header; a run keeps its detail rows above an
    // inline jobs header; every other page is prose in the same rows (TASK-91).
    let columns = workspace.columns();
    let glyphs = !matches!(workspace.view, ActionsView::Workflows);
    let table = Table {
        columns: &columns,
        header: matches!(workspace.view, ActionsView::Runs | ActionsView::Workflows)
            && workspace
                .rows
                .iter()
                .any(|row| matches!(row.cells, ActionsCells::Row(_))),
        focused,
        glyphs,
        empty: None,
    };
    let host = workspace.identity.host.clone();
    let table_row = |index: usize, row: &ActionsRow| {
        let actor = row
            .actor
            .as_ref()
            .map(|actor| ActorIdentity::github(&host, actor));
        let base = match &row.cells {
            ActionsCells::Row(cells) if !columns.is_empty() => {
                TableRow::cells(cells.iter().map(|cell| Cell::from(cell.as_str())).collect())
            }
            ActionsCells::Header if !columns.is_empty() => TableRow::header(),
            _ => TableRow::text(row.label.as_str()),
        };
        // Prose keeps only real status glyphs; a muted table row (a skipped job) shows `○`.
        let glyph =
            (base.kind != RowKind::Text || !row.role.symbol().is_empty()).then_some(row.role);
        // One click opens the same exact prepared action as Enter on the selected row.
        base.with_tone(row.role)
            .with_glyph(glyph)
            .with_actor(actor)
            .with_action(Some(
                row.action.clone().unwrap_or(ActionsAction::Select(index)),
            ))
    };
    let natural = crate::ui_table::natural_widths(
        &columns,
        workspace
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row.cells, ActionsCells::Row(_)))
            .map(|(index, row)| table_row(index, row))
            .collect::<Vec<_>>()
            .iter(),
        theme.mode,
    );
    let mut state = ListState {
        selected: workspace.selected,
        top: workspace.top,
    };
    let rendered = crate::ui_table::draw_table_with(
        frame,
        output.content,
        &table,
        &natural,
        workspace.rows.len(),
        |index| table_row(index, &workspace.rows[index]),
        &mut state,
        theme,
    );
    workspace.top = state.top;
    output.visible_rows = rendered.visible_rows;
    output.content = rendered.body;
    output
        .hits
        .extend(rendered.hits.into_iter().map(|hit| ActionsHit {
            rect: hit.rect,
            action: hit.action.unwrap_or(ActionsAction::Select(hit.index)),
        }));
    if let Some((image, start, count)) = workspace.artifact_image() {
        let visible_start = start.max(workspace.top);
        let visible_end = (start + count).min(workspace.top + output.visible_rows);
        if visible_end > visible_start {
            let image_area = Rect::new(
                area.x,
                output.content.y + (visible_start - workspace.top) as u16,
                area.width,
                (visible_end - visible_start) as u16,
            );
            frame.render_widget(Clear, image_area);
            frame.render_widget(crate::identity::ImageWidget { image, theme }, image_area);
        }
    }
    output
}
/// The catalogue tab strip (Runs, Workflows, Caches…): the current catalogue filled with the
/// Actions hue, the others with the key in the accent. Tabs that do not fit leave from the
/// right, never the current one; they stay one key or palette entry away. Returns whether a
/// row was used.
fn draw_tabs(
    frame: &mut Frame,
    row: Rect,
    theme: Theme,
    workspace: &ActionsWorkspace,
    output: &mut ActionsRender,
) -> bool {
    let tabs = workspace
        .controls
        .iter()
        .filter(|control| control.is_catalog_tab())
        .collect::<Vec<_>>();
    if tabs.is_empty() || row.is_empty() {
        return false;
    }
    let selected = |control: &ActionsControl| control.action == ActionsAction::View(workspace.view);
    let text = |control: &ActionsControl| {
        let (key, text) = control.key_and_text();
        sanitize_terminal(&key.map_or_else(|| format!(" {text} "), |key| format!(" {key} {text} ")))
    };
    let width = |control: &ActionsControl| Line::from(text(control)).width() as u16;
    let mut shown = (0..tabs.len()).collect::<Vec<_>>();
    // When tabs leave, two columns stay for a dim `…` so the row does not look complete.
    while shown.len() > 1
        && shown.iter().map(|index| width(tabs[*index])).sum::<u16>()
            + shown.len().saturating_sub(1) as u16
            + if shown.len() < tabs.len() { 2 } else { 0 }
            > row.width
    {
        let Some(position) = shown.iter().rposition(|index| !selected(tabs[*index])) else {
            break;
        };
        shown.remove(position);
    }
    let overflow = shown.len() < tabs.len();
    let mut x = row.x;
    for index in shown {
        let control = tabs[index];
        let tab_width = width(control).min(row.right().saturating_sub(x));
        if tab_width == 0 {
            break;
        }
        let rect = Rect::new(x, row.y, tab_width, 1);
        let line = if control.disabled_reason.is_some() {
            Line::from(Span::styled(text(control), theme.role(Role::TextDim)))
        } else if selected(control) {
            Line::from(Span::styled(
                text(control),
                theme.filled(Hue::for_view(crate::app::View::Actions)),
            ))
        } else {
            let (key, label) = control.key_and_text();
            let mut spans = vec![Span::raw(" ")];
            if let Some(key) = key {
                spans.push(Span::styled(
                    sanitize_terminal(&key),
                    theme.role(Role::Accent).add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(
                format!("{} ", sanitize_terminal(&label)),
                theme.role(Role::TextMuted),
            ));
            Line::from(spans)
        };
        frame.render_widget(Paragraph::new(line), rect);
        output.hits.push(ActionsHit {
            rect,
            action: unavailable_or(control),
        });
        x = x.saturating_add(tab_width).saturating_add(1);
    }
    if overflow && x < row.right() {
        frame.render_widget(
            Paragraph::new(Span::styled("…", theme.role(Role::TextDim))),
            Rect::new(x, row.y, 1, 1),
        );
    }
    true
}
/// The control's action, or its visible explanation when it is disabled.
fn unavailable_or(control: &ActionsControl) -> ActionsAction {
    control.disabled_reason.as_ref().map_or_else(
        || control.action.clone(),
        |reason| ActionsAction::Unavailable(format!("{}: {reason}", control.label)),
    )
}
/// The Actions key bar: every prepared control in declaration order, with its key, compact
/// caption and priority. Catalogue entries are Mode controls (the tab strip); disabled
/// controls leave the bar and their key explains why. Repository switching joins them when
/// more than one repository is in scope.
pub fn footer_keys(workspace: &ActionsWorkspace) -> Vec<KeyControl> {
    let mut keys = workspace
        .controls
        .iter()
        .map(|control| {
            let (key, text) = control.key_and_text();
            // The refresh key is the global Refresh binding, which runs before view keys;
            // its token carries the same global action so click and key agree.
            let action = match control.action {
                ActionsAction::Refresh => Action::Refresh,
                ref action => Action::Actions(action.clone()),
            };
            let mut key_control =
                KeyControl::new(key.unwrap_or_default(), sanitize_terminal(&text), action)
                    .with_priority(control.priority())
                    .in_group(if control.is_catalog_tab() {
                        ControlGroup::Mode
                    } else if matches!(
                        control.action,
                        ActionsAction::Back | ActionsAction::Select(_)
                    ) {
                        ControlGroup::Navigate
                    } else {
                        ControlGroup::Act
                    })
                    .with_selected(control.action == ActionsAction::View(workspace.view))
                    .disabled_by(
                        control
                            .disabled_reason
                            .as_ref()
                            .map(|reason| format!("{}: {reason}", control.label)),
                    );
            if let Some(compact) = control.compact_text() {
                key_control = key_control.with_compact(compact);
            }
            key_control
        })
        .collect::<Vec<_>>();
    let len = workspace.repositories.len();
    if len > 1 {
        for (key, label, compact, index) in [
            (
                "{",
                "Previous repo",
                "Prev repo",
                (workspace.selected_repository + len - 1) % len,
            ),
            (
                "}",
                "Next repo",
                "Next repo",
                (workspace.selected_repository + 1) % len,
            ),
        ] {
            keys.push(
                KeyControl::new(
                    key,
                    label,
                    Action::Actions(ActionsAction::Repository(index)),
                )
                .with_compact(compact)
                .with_priority(70)
                .in_group(ControlGroup::Navigate),
            );
        }
    }
    // One key, one action: the first enabled bearer keeps a shared key (Enable and Disable
    // workflow both use X); a different action on the same key becomes click/palette only.
    for index in 0..keys.len() {
        if keys[index].key.is_empty() {
            continue;
        }
        let bearer = keys
            .iter()
            .position(|other| other.key == keys[index].key && other.disabled.is_none())
            .or_else(|| keys.iter().position(|other| other.key == keys[index].key))
            .unwrap_or(index);
        if bearer != index && keys[bearer].action != keys[index].action {
            keys[index].key.clear();
        }
    }
    keys
}
/// The row `delta` rows from the selection, clamped to the list. An inline column-header row
/// is never a stop: movement passes over it in its own direction (or back when it is the end).
pub(crate) fn row_step(workspace: &ActionsWorkspace, delta: isize) -> usize {
    let last = workspace.rows.len().saturating_sub(1);
    let target = workspace.selected.saturating_add_signed(delta).min(last);
    let header = |index: usize| {
        workspace
            .rows
            .get(index)
            .is_some_and(|row| row.cells == ActionsCells::Header)
    };
    if !header(target) {
        return target;
    }
    let forward = delta > 0;
    let beyond = if forward {
        (target + 1..=last).find(|index| !header(*index))
    } else {
        (0..target).rev().find(|index| !header(*index))
    };
    beyond
        .or_else(|| {
            if forward {
                (0..target).rev().find(|index| !header(*index))
            } else {
                (target + 1..=last).find(|index| !header(*index))
            }
        })
        .unwrap_or(target)
}
/// Selection, movement and the Enter/Esc aliases, plus view keys for routes whose prepared
/// controls omit their tab or page. Keys of prepared controls (Refresh, Logs, failure jumps,
/// paging, workflow commands, Cancel…) resolve first through [`footer_keys`], so a key does
/// exactly what its painted control does, or explains why it is unavailable.
pub fn keyboard_action(event: KeyEvent, workspace: &ActionsWorkspace) -> Option<ActionsAction> {
    if event.kind != KeyEventKind::Press
        || event
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return None;
    }
    match event.code {
        KeyCode::Up => Some(ActionsAction::Select(row_step(workspace, -1))),
        KeyCode::Down => Some(ActionsAction::Select(row_step(workspace, 1))),
        KeyCode::PageUp => Some(ActionsAction::Select(row_step(workspace, -10))),
        KeyCode::PageDown => Some(ActionsAction::Select(row_step(workspace, 10))),
        KeyCode::Home => Some(ActionsAction::Select(row_step(workspace, isize::MIN))),
        KeyCode::End => Some(ActionsAction::Select(row_step(workspace, isize::MAX))),
        // An information row (run header, actor, log line) has nothing to
        // open; keep Enter here instead of activating an unrelated global hit.
        KeyCode::Enter => workspace
            .selected_action()
            .or(Some(ActionsAction::Select(workspace.selected))),
        KeyCode::Esc | KeyCode::Char('b') => Some(ActionsAction::Back),
        KeyCode::Char('r') => Some(ActionsAction::Refresh),
        KeyCode::Char('P') => Some(ActionsAction::PublicationList),
        KeyCode::Char('E') => Some(ActionsAction::EditWorkflowLocalV9),
        KeyCode::Char('w') => Some(ActionsAction::Watch),
        KeyCode::Char('R') => Some(ActionsAction::View(ActionsView::Runs)),
        KeyCode::Char('W') => Some(ActionsAction::View(ActionsView::Workflows)),
        KeyCode::Char('/') if workspace.view == ActionsView::Logs => Some(ActionsAction::Compose(
            crate::actions_workspace::ActionsCommand::SearchLogs,
        )),
        KeyCode::Char('c') => Some(ActionsAction::View(ActionsView::Caches)),
        KeyCode::Char('S') => Some(ActionsAction::View(ActionsView::Secrets)),
        KeyCode::Char('v') => Some(ActionsAction::View(ActionsView::Variables)),
        KeyCode::Char('e') => Some(ActionsAction::View(ActionsView::Environments)),
        KeyCode::Char('p') => Some(ActionsAction::View(ActionsView::Policies)),
        KeyCode::Char('n') => Some(ActionsAction::View(ActionsView::Runners)),
        KeyCode::Char('g') => Some(ActionsAction::View(ActionsView::RunnerGroups)),
        KeyCode::Char('G') if workspace.run.is_some() => {
            Some(ActionsAction::View(ActionsView::Approvals))
        }
        KeyCode::Char('N')
            if workspace
                .run
                .as_ref()
                .is_some_and(|page| page.selected_job.is_some()) =>
        {
            Some(ActionsAction::Annotations(
                workspace.run.as_ref().unwrap().selected_job.unwrap(),
            ))
        }
        KeyCode::Char('u') => Some(ActionsAction::View(ActionsView::Usage)),
        KeyCode::Char('j') if workspace.run.is_some() => {
            Some(ActionsAction::View(ActionsView::Jobs))
        }
        KeyCode::Char('l') if workspace.run.is_some() => Some(ActionsAction::Logs(
            workspace.run.as_ref().and_then(|run| run.selected_job),
        )),
        KeyCode::Char('a') if workspace.run.is_some() => Some(ActionsAction::Artifacts),
        _ => None,
    }
}
#[cfg(test)]
#[path = "actions_footer_v90_tests.rs"]
mod design_footer_v90_tests;
