//! Pure prepared Actions rendering and keyboard mapping. No subprocess, network or disk I/O.
use crate::{
    actions_workspace::{ActionsAction, ActionsView, ActionsWorkspace},
    identity::{ActorIdentity, AvatarState, AvatarWidget},
    providers::runner::sanitize_terminal,
    theme::{SemanticToken, Theme},
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::Line,
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
    avatars: &crate::actor_avatar_states_v1::ActorAvatarStatesV1,
) -> ActionsRender {
    let mut output = ActionsRender::default();
    if area.width == 0 || area.height == 0 {
        return output;
    }
    frame.render_widget(Clear, area);
    let mut y = area.y;
    let mut x = area.x;
    // Bound header height even for large sets; every repository remains accessible.
    if !workspace.repositories.is_empty() {
        let len = workspace.repositories.len();
        for (label, index) in [
            (
                "[← Previous repo]",
                (workspace.selected_repository + len - 1) % len,
            ),
            ("[→ Next repo]", (workspace.selected_repository + 1) % len),
        ] {
            button(
                frame,
                area,
                &mut x,
                &mut y,
                label,
                theme,
                false,
                &mut output,
                ActionsAction::Repository(index),
            );
        }
    }
    if x > area.x {
        y = y.saturating_add(1);
        x = area.x;
    }
    // All controls are reachable, not silently dropped at narrow terminal sizes.
    let mut positioned = Vec::new();
    let mut row = 0u16;
    let mut column = 0u16;
    for (index, control) in workspace.controls.iter().enumerate() {
        let label = format!(
            "[{}{}]",
            sanitize_terminal(&control.label),
            control
                .disabled_reason
                .as_ref()
                .map(|reason| format!(" unavailable: {}", sanitize_terminal(reason)))
                .unwrap_or_default()
        );
        let width = (Line::from(label.as_str()).width() as u16).min(area.width);
        if column > 0 && column.saturating_add(width) > area.width {
            column = 0;
            row = row.saturating_add(1);
        }
        positioned.push((index, column, row, width, label));
        column = column.saturating_add(width).saturating_add(1);
    }
    let rows_per_page = (area.height.saturating_sub(8) / 2).clamp(1, 4);
    let page_count = usize::from(row + 1)
        .div_ceil(usize::from(rows_per_page))
        .max(1);
    workspace.controls_page = workspace.controls_page.min(page_count - 1);
    for (label, delta, disabled) in [
        ("[← Controls]", -1, workspace.controls_page == 0),
        ("[Controls →]", 1, workspace.controls_page + 1 == page_count),
    ] {
        button(
            frame,
            area,
            &mut x,
            &mut y,
            label,
            theme,
            disabled,
            &mut output,
            ActionsAction::ControlsPage(delta),
        );
    }
    if y < area.bottom() {
        let label = format!(
            " {}/{} • [] change control page",
            workspace.controls_page + 1,
            page_count
        );
        frame.render_widget(
            Paragraph::new(label).style(theme.style(SemanticToken::Muted)),
            Rect::new(x, y, area.right().saturating_sub(x), 1),
        );
    }
    y = y.saturating_add(1);
    let controls_top = y;
    let start = workspace.controls_page * usize::from(rows_per_page);
    let mut drawn_rows = 0u16;
    for (index, column, row, width, label) in positioned {
        if usize::from(row) < start || usize::from(row) >= start + usize::from(rows_per_page) {
            continue;
        }
        let relative = row - (start as u16);
        let rect = Rect::new(area.x + column, controls_top + relative, width, 1);
        if rect.y >= area.bottom() {
            continue;
        }
        let control = &workspace.controls[index];
        let disabled = control.disabled_reason.is_some();
        frame.render_widget(
            Paragraph::new(label).style(
                theme
                    .style(if disabled {
                        SemanticToken::Muted
                    } else {
                        SemanticToken::Focus
                    })
                    .remove_modifier(Modifier::UNDERLINED),
            ),
            rect,
        );
        output.hits.push(ActionsHit {
            rect,
            action: control.disabled_reason.as_ref().map_or_else(
                || control.action.clone(),
                |reason| ActionsAction::Unavailable(format!("{}: {reason}", control.label)),
            ),
        });
        drawn_rows = drawn_rows.max(relative + 1);
    }
    y = y.saturating_add(drawn_rows);
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
            format!("Fetched {at} • refresh uses bounded gh requests")
        });
        let status = format!(
            "{freshness}{}",
            if workspace.watching {
                " • watching: bounded polling, not a live stream"
            } else {
                ""
            }
        );
        frame.render_widget(
            Paragraph::new(status).style(theme.style(SemanticToken::Muted)),
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
        frame.render_widget(
            Paragraph::new(sanitize_terminal(&coverage)).style(theme.style(
                if workspace.coverage.complete {
                    SemanticToken::Normal
                } else {
                    SemanticToken::Pending
                },
            )),
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
    let content_height = area.bottom().saturating_sub(y.saturating_add(1));
    output.visible_rows = usize::from(content_height);
    output.content = Rect::new(area.x, y, area.width, content_height);
    if workspace.selected < workspace.top {
        workspace.top = workspace.selected;
    }
    if workspace.selected >= workspace.top.saturating_add(output.visible_rows)
        && output.visible_rows > 0
    {
        workspace.top = workspace.selected.saturating_sub(output.visible_rows - 1);
    }
    for (offset, (index, row)) in workspace
        .rows
        .iter()
        .enumerate()
        .skip(workspace.top)
        .take(output.visible_rows)
        .enumerate()
    {
        let rect = Rect::new(area.x, y + offset as u16, area.width, 1);
        let selected = index == workspace.selected;
        let style = if selected {
            theme.selected(row.role)
        } else {
            theme.style(row.role)
        };
        let mut row_x = rect.x;
        if let Some(actor) = &row.actor {
            let identity = ActorIdentity::github(&workspace.identity.host, actor);
            let image = match avatars.get(&identity) {
                Some(AvatarState::Ready(image)) => Some(image.as_ref()),
                _ => None,
            };
            let width = 4.min(rect.width);
            frame.render_widget(
                AvatarWidget {
                    identity: &identity,
                    image,
                    theme,
                },
                Rect::new(row_x, rect.y, width, 1),
            );
            row_x += width;
            let label = format!("{} ", identity.label());
            let actor_width =
                (Line::from(label.as_str()).width() as u16).min(rect.right().saturating_sub(row_x));
            frame.render_widget(
                Paragraph::new(label).style(theme.style(SemanticToken::Actor)),
                Rect::new(row_x, rect.y, actor_width, 1),
            );
            row_x += actor_width;
        }
        let text = format!(
            "{}{} {}",
            if selected { "> " } else { "  " },
            row.role.symbol(),
            sanitize_terminal(&row.label)
        );
        frame.render_widget(
            Paragraph::new(text).style(style),
            Rect::new(row_x, rect.y, rect.right().saturating_sub(row_x), 1),
        );
        // One click opens the same exact prepared action as Enter on the selected row.
        output.hits.push(ActionsHit {
            rect,
            action: row.action.clone().unwrap_or(ActionsAction::Select(index)),
        });
    }
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
    if area.height > 0 {
        frame.render_widget(Paragraph::new("↑/↓ select • Enter open • Tab buttons • r refresh • b back • R runs • W workflows • [] controls").style(theme.style(SemanticToken::Muted)),Rect::new(area.x,area.bottom()-1,area.width,1));
    }
    output
}
#[allow(clippy::too_many_arguments)]
fn button(
    frame: &mut Frame,
    area: Rect,
    x: &mut u16,
    y: &mut u16,
    label: &str,
    theme: Theme,
    disabled: bool,
    output: &mut ActionsRender,
    action: ActionsAction,
) {
    let width = (Line::from(label).width() as u16).min(area.width);
    if *x > area.x && x.saturating_add(width) > area.right() {
        *x = area.x;
        *y = y.saturating_add(1);
    }
    if *y >= area.bottom() {
        return;
    }
    let rect = Rect::new(*x, *y, width, 1);
    frame.render_widget(
        // Accent styling is not focus: App adds underline only to the active hit.
        Paragraph::new(label).style(
            theme
                .style(if disabled {
                    SemanticToken::Muted
                } else {
                    SemanticToken::Focus
                })
                .remove_modifier(Modifier::UNDERLINED),
        ),
        rect,
    );
    if !disabled {
        output.hits.push(ActionsHit { rect, action });
    }
    *x = x.saturating_add(width).saturating_add(1);
}
pub fn keyboard_action(event: KeyEvent, workspace: &ActionsWorkspace) -> Option<ActionsAction> {
    if event.kind != KeyEventKind::Press
        || event
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return None;
    }
    match event.code {
        KeyCode::Up => Some(ActionsAction::Select(workspace.selected.saturating_sub(1))),
        KeyCode::Down => Some(ActionsAction::Select(workspace.selected.saturating_add(1))),
        KeyCode::PageUp => Some(ActionsAction::Select(workspace.selected.saturating_sub(10))),
        KeyCode::PageDown => Some(ActionsAction::Select(workspace.selected.saturating_add(10))),
        KeyCode::Home => Some(ActionsAction::Select(0)),
        KeyCode::End => Some(ActionsAction::Select(
            workspace.rows.len().saturating_sub(1),
        )),
        KeyCode::Enter => workspace.selected_action(),
        KeyCode::Esc | KeyCode::Char('b') => Some(ActionsAction::Back),
        KeyCode::Char('r') => Some(ActionsAction::Refresh),
        KeyCode::Char('P') => Some(ActionsAction::PublicationList),
        KeyCode::Char('E') => Some(ActionsAction::EditWorkflowLocalV9),
        KeyCode::Char('w') => Some(ActionsAction::Watch),
        KeyCode::Char('R') => Some(ActionsAction::View(ActionsView::Runs)),
        KeyCode::Char(key @ ('(' | ')' | 'A' | 'F' | 'I')) => {
            use crate::scoped_actions_resources_v2::ScopedActionsReadActionV2 as Read;
            let action = ActionsAction::ScopedReadV2(match key {
                '(' => Read::RunPage(-1),
                ')' => Read::RunPage(1),
                'A' => Read::AllRunStatuses,
                'F' => Read::FailedRuns,
                _ => Read::ActiveRuns,
            });
            workspace
                .controls
                .iter()
                .any(|control| control.action == action)
                .then_some(action)
        }
        KeyCode::Char('W') => Some(ActionsAction::View(ActionsView::Workflows)),
        KeyCode::Char('[') => Some(ActionsAction::ControlsPage(-1)),
        KeyCode::Char(']') => Some(ActionsAction::ControlsPage(1)),
        KeyCode::Char('{') if !workspace.repositories.is_empty() => {
            Some(ActionsAction::Repository(
                (workspace.selected_repository + workspace.repositories.len() - 1)
                    % workspace.repositories.len(),
            ))
        }
        KeyCode::Char('}') if !workspace.repositories.is_empty() => {
            Some(ActionsAction::Repository(
                (workspace.selected_repository + 1) % workspace.repositories.len(),
            ))
        }
        KeyCode::Char('/') if workspace.view == ActionsView::Logs => Some(ActionsAction::Compose(
            crate::actions_workspace::ActionsCommand::SearchLogs,
        )),
        KeyCode::Char('c') => Some(ActionsAction::View(ActionsView::Caches)),
        KeyCode::Char('C')
            if workspace.controls.iter().any(|control| {
                control.action
                    == ActionsAction::Compose(crate::actions_workspace::ActionsCommand::Cancel)
                    && control.disabled_reason.is_none()
            }) =>
        {
            Some(ActionsAction::Compose(
                crate::actions_workspace::ActionsCommand::Cancel,
            ))
        }
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
        KeyCode::Char('j')
            if workspace.run.is_some()
                || workspace
                    .controls
                    .iter()
                    .any(|control| control.action == ActionsAction::View(ActionsView::Jobs)) =>
        {
            Some(ActionsAction::View(ActionsView::Jobs))
        }
        KeyCode::Char('t')
            if workspace
                .controls
                .iter()
                .any(|control| control.label == "[t Run]") =>
        {
            Some(ActionsAction::View(ActionsView::Run))
        }
        KeyCode::Char('l') if workspace.run.is_some() => Some(ActionsAction::Logs(
            workspace.run.as_ref().and_then(|run| run.selected_job),
        )),
        KeyCode::Char('a') if workspace.run.is_some() => Some(ActionsAction::Artifacts),
        _ => None,
    }
}
