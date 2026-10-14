//! Prepared native Dashboard rendering/input; no IO or formula work in draw.
use crate::{
    identity::{ActorWidget, AvatarState, AvatarWidget},
    metrics::MetricsAction,
    metrics_workspace::{MetricsView, MetricsWorkspace, MetricsWorkspaceAction},
    theme::{SemanticToken, Theme},
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::{
    layout::{Position, Rect},
    style::Modifier,
    widgets::{Clear, Paragraph},
    Frame,
};
const PREPARING_REASON: &str = "Dashboard is preparing; previous values/Open are hidden";

#[derive(Clone, Debug)]
pub struct MetricsHit {
    pub rect: Rect,
    pub action: MetricsWorkspaceAction,
}
#[derive(Clone, Debug, Default)]
pub struct MetricsRender {
    pub hits: Vec<MetricsHit>,
    pub content: Rect,
    pub visible_rows: usize,
}

pub fn draw(
    frame: &mut Frame,
    workspace: &mut MetricsWorkspace,
    area: Rect,
    theme: Theme,
    avatars: &crate::actor_avatar_states_v1::ActorAvatarStatesV1,
) -> MetricsRender {
    let mut output = MetricsRender::default();
    if area.is_empty() {
        return output;
    }
    frame.render_widget(Clear, area);
    let mut y = area.y;
    let mut x = area.x;
    let buttons = [
        ("[7 Last7 days]", workspace.window_action(7)),
        ("[0 Last30 days]", workspace.window_action(30)),
        ("[R Repositories]", MetricsWorkspaceAction::Repositories),
        (
            "[r Refresh]",
            MetricsWorkspaceAction::Metric(MetricsAction::Refresh),
        ),
        (
            "[i Details]",
            MetricsWorkspaceAction::Inspect(workspace.selected),
        ),
        (
            "[b Back]",
            MetricsWorkspaceAction::Metric(MetricsAction::Back),
        ),
    ];
    for (label, action) in buttons {
        let width = (label.len() as u16).min(area.width);
        if x > area.x && x.saturating_add(width) > area.right() {
            x = area.x;
            y = y.saturating_add(1);
        }
        if y >= area.bottom().saturating_sub(1) {
            break;
        }
        let rect = Rect::new(x, y, width, 1);
        let (action, role) = if workspace.pending.is_some() {
            (
                MetricsWorkspaceAction::Explain(PREPARING_REASON.into()),
                SemanticToken::Muted,
            )
        } else {
            (action, SemanticToken::Focus)
        };
        frame.render_widget(
            Paragraph::new(label).style(theme.style(role).remove_modifier(Modifier::UNDERLINED)),
            rect,
        );
        output.hits.push(MetricsHit { rect, action });
        x = x.saturating_add(width + 1);
    }
    y = y.saturating_add(1);
    let headers = [
        (&workspace.scope_label, SemanticToken::Reference),
        (&workspace.cohort_label, SemanticToken::Muted),
        (&workspace.window_label, SemanticToken::Normal),
        (&workspace.as_of_label, SemanticToken::Muted),
        (&workspace.source_coverage_label, SemanticToken::Pending),
    ];
    for (label, role) in headers {
        if y >= area.bottom().saturating_sub(1) {
            break;
        }
        frame.render_widget(
            Paragraph::new(label.as_str()).style(theme.style(role)),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }
    if let Some(status) = &workspace.status {
        if y < area.bottom().saturating_sub(1) {
            frame.render_widget(
                Paragraph::new(status.as_str()).style(theme.style(SemanticToken::Pending)),
                Rect::new(area.x, y, area.width, 1),
            );
            y += 1;
        }
    }
    if workspace.pending.is_none() {
        let card = match workspace.view {
            MetricsView::Contributors(card)
            | MetricsView::Details { card, .. }
            | MetricsView::CardInfo(card) => Some(card),
            _ => None,
        };
        if let Some(card) = card {
            for (label, role) in [
                (&workspace.cards[card].title, SemanticToken::Head),
                (&workspace.cards[card].summary, SemanticToken::Normal),
            ] {
                if y < area.bottom().saturating_sub(1) {
                    frame.render_widget(
                        Paragraph::new(label.as_str()).style(theme.style(role)),
                        Rect::new(area.x, y, area.width, 1),
                    );
                    y += 1;
                }
            }
        }
        if workspace.view == MetricsView::Repositories && y < area.bottom().saturating_sub(1) {
            frame.render_widget(Paragraph::new("Choose ALL declared repositories or exactly one; no organization/global fallback").style(theme.style(SemanticToken::Head)),Rect::new(area.x,y,area.width,1));
            y += 1;
        }
    }
    output.content = Rect::new(
        area.x,
        y,
        area.width,
        area.bottom().saturating_sub(y.saturating_add(1)),
    );
    output.visible_rows = usize::from(output.content.height);
    if output.visible_rows > 0 {
        if workspace.selected < workspace.top {
            workspace.top = workspace.selected;
        }
        if workspace.selected >= workspace.top.saturating_add(output.visible_rows) {
            workspace.top = workspace.selected.saturating_sub(output.visible_rows - 1);
        }
    }
    let rows = workspace.rows();
    if rows.is_empty() && workspace.pending.is_none() && output.visible_rows > 0 {
        frame.render_widget(Paragraph::new("No contributing records • inspect Details for source coverage; absence is not zero").style(theme.style(SemanticToken::Muted)),Rect::new(area.x,y,area.width,1));
    }
    for (offset, (index, row)) in rows
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
        if let Some(actor) = &row.actor {
            frame.render_widget(
                Paragraph::new(if selected { "> " } else { "  " }).style(style),
                Rect::new(rect.x, rect.y, 2.min(rect.width), 1),
            );
            let image = avatars.get(actor).and_then(|state| match state {
                AvatarState::Ready(image) => Some(image.as_ref()),
                _ => None,
            });
            let actor_area = Rect::new(
                rect.x.saturating_add(2).min(rect.right()),
                rect.y,
                rect.width.saturating_sub(2),
                1,
            );
            if row.avatar_only {
                let avatar_width = 4.min(actor_area.width);
                frame.render_widget(
                    AvatarWidget {
                        identity: actor,
                        image,
                        theme,
                    },
                    Rect::new(actor_area.x, actor_area.y, avatar_width, 1),
                );
                frame.render_widget(
                    Paragraph::new(row.label.as_str()).style(style),
                    Rect::new(
                        actor_area.x.saturating_add(avatar_width),
                        actor_area.y,
                        actor_area.width.saturating_sub(avatar_width),
                        1,
                    ),
                );
            } else {
                frame.render_widget(
                    ActorWidget {
                        identity: actor,
                        image,
                        theme,
                    },
                    actor_area,
                );
            }
        } else {
            frame.render_widget(
                Paragraph::new(format!(
                    "{}{} {}",
                    if selected { "> " } else { "  " },
                    row.role.symbol(),
                    row.label
                ))
                .style(style),
                rect,
            );
        }
        if let Some(action) = &row.action {
            output.hits.push(MetricsHit {
                rect,
                action: action.clone(),
            });
        } else {
            output.hits.push(MetricsHit {
                rect,
                action: MetricsWorkspaceAction::Select(index),
            });
        }
    }
    frame.render_widget(
        Paragraph::new(
            "↑/↓ j/k select • Enter open • i details • Tab buttons • wheel/page scroll • b back",
        )
        .style(theme.style(SemanticToken::Muted)),
        Rect::new(area.x, area.bottom() - 1, area.width, 1),
    );
    output
}

pub fn keyboard_action(
    event: KeyEvent,
    workspace: &MetricsWorkspace,
) -> Option<MetricsWorkspaceAction> {
    if event.kind != KeyEventKind::Press
        || event
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return None;
    }
    let action = match event.code {
        KeyCode::Up | KeyCode::Char('k') => {
            MetricsWorkspaceAction::Select(workspace.selected.saturating_sub(1))
        }
        KeyCode::Down | KeyCode::Char('j') => {
            MetricsWorkspaceAction::Select(workspace.selected.saturating_add(1))
        }
        KeyCode::PageUp => MetricsWorkspaceAction::Select(workspace.selected.saturating_sub(8)),
        KeyCode::PageDown => MetricsWorkspaceAction::Select(workspace.selected.saturating_add(8)),
        KeyCode::Home => MetricsWorkspaceAction::Select(0),
        KeyCode::End => MetricsWorkspaceAction::Select(workspace.rows().len().saturating_sub(1)),
        KeyCode::Enter => return workspace.selected_action(),
        KeyCode::Esc | KeyCode::Char('b') => MetricsWorkspaceAction::Metric(MetricsAction::Back),
        KeyCode::Char('7') => workspace.window_action(7),
        KeyCode::Char('0') => workspace.window_action(30),
        KeyCode::Char('R') => MetricsWorkspaceAction::Repositories,
        KeyCode::Char('r') => MetricsWorkspaceAction::Metric(MetricsAction::Refresh),
        KeyCode::Char('i') => MetricsWorkspaceAction::Inspect(workspace.selected),
        _ => return None,
    };
    Some(if workspace.pending.is_some() {
        MetricsWorkspaceAction::Explain(PREPARING_REASON.into())
    } else {
        action
    })
}
/// Common App Tab focus should consume the returned hits; stale render hits are never retained.
pub fn mouse_action(
    event: Event,
    workspace: &MetricsWorkspace,
    rendered: &MetricsRender,
) -> Option<MetricsWorkspaceAction> {
    let Event::Mouse(event) = event else {
        return None;
    };
    let position = Position::new(event.column, event.row);
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => rendered
            .hits
            .iter()
            .find(|hit| hit.rect.contains(position))
            .map(|hit| hit.action.clone()),
        MouseEventKind::ScrollUp if rendered.content.contains(position) => Some(
            MetricsWorkspaceAction::Select(workspace.selected.saturating_sub(3)),
        ),
        MouseEventKind::ScrollDown if rendered.content.contains(position) => Some(
            MetricsWorkspaceAction::Select(workspace.selected.saturating_add(3)),
        ),
        _ => None,
    }
}
