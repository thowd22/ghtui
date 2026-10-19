//! Prepared native Dashboard rendering/input; no IO or formula work in draw.
use crate::{
    app::Action,
    footer::{ControlGroup, KeyControl},
    identity::{ActorWidget, AvatarState, ListAvatar},
    metrics::MetricsAction,
    metrics_workspace::{MetricsView, MetricsWorkspace, MetricsWorkspaceAction},
    theme::{SemanticToken, Theme},
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::{
    layout::{Position, Rect},
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
    // The view's controls live in the footer key bar (`footer_keys`), so content starts at
    // the top of the pane.
    let mut y = area.y;
    if workspace.view == MetricsView::Layout && y < area.bottom() {
        frame.render_widget(
            Paragraph::new(
                "Layout only • one widget must stay visible • Save replaces this account's default",
            )
            .style(theme.style(SemanticToken::Reference)),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }
    let headers = [
        (&workspace.scope_label, SemanticToken::Reference),
        (&workspace.cohort_label, SemanticToken::Muted),
        (&workspace.window_label, SemanticToken::Normal),
        (&workspace.as_of_label, SemanticToken::Muted),
        (&workspace.source_coverage_label, SemanticToken::Pending),
    ];
    for (label, role) in headers {
        if y >= area.bottom() {
            break;
        }
        let rect = Rect::new(area.x, y, area.width, 1);
        if std::ptr::eq(label, &workspace.as_of_label) {
            // The as-of time is provenance: a dimmed detail on the metrics section header.
            frame.render_widget(
                Paragraph::new(crate::ui_chrome::section_header(
                    theme,
                    "Metrics",
                    label,
                    crate::theme::Role::TextMuted,
                    area.width,
                )),
                rect,
            );
        } else {
            frame.render_widget(
                Paragraph::new(label.as_str()).style(theme.style(role)),
                rect,
            );
        }
        y += 1;
    }
    if let Some(status) = &workspace.status {
        if y < area.bottom() {
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
                if y < area.bottom() {
                    frame.render_widget(
                        Paragraph::new(label.as_str()).style(theme.style(role)),
                        Rect::new(area.x, y, area.width, 1),
                    );
                    y += 1;
                }
            }
        }
        if workspace.view == MetricsView::Repositories && y < area.bottom() {
            frame.render_widget(Paragraph::new("Choose ALL declared repositories or exactly one; no organization/global fallback").style(theme.style(SemanticToken::Head)),Rect::new(area.x,y,area.width,1));
            y += 1;
        }
    }
    output.content = Rect::new(area.x, y, area.width, area.bottom().saturating_sub(y));
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
                // Chip, exactly one blank cell, then the label cut with an ellipsis.
                ListAvatar {
                    identity: actor,
                    image,
                    theme,
                }
                .render_with_label(
                    actor_area,
                    frame.buffer_mut(),
                    &row.label,
                    style,
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
    output
}

/// Selection and movement keys, plus Esc as an alias of Back. Keys painted in the footer
/// resolve through [`footer_keys`] and `footer::resolve_key` instead.
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
        KeyCode::Esc => MetricsWorkspaceAction::Metric(MetricsAction::Back),
        _ => return None,
    };
    Some(if workspace.pending.is_some() {
        MetricsWorkspaceAction::Explain(PREPARING_REASON.into())
    } else {
        action
    })
}
/// The Dashboard's key bar. While a read is preparing every control is disabled and its key
/// explains why; a window the cached scope cannot recompute is disabled with that reason.
pub fn footer_keys(workspace: &MetricsWorkspace) -> Vec<KeyControl> {
    use MetricsWorkspaceAction as W;
    let layout = workspace.view == MetricsView::Layout;
    let metric = |action: MetricsAction| Action::Metrics(W::Metric(action));
    let window = |key: &str, days: u64, label: &str| {
        let (action, reason) = match workspace.window_action(days) {
            W::Explain(reason) => (Action::Explain(reason.clone()), Some(reason)),
            action => (Action::Metrics(action), None),
        };
        let window = &workspace.snapshot.window;
        let selected = window.end == workspace.snapshot.as_of
            && window.end.checked_sub(window.start) == Some(days.saturating_mul(86_400));
        KeyControl::new(key, label, action)
            .with_priority(if layout { 60 } else { 22 })
            .disabled_by(reason)
            .with_selected(selected)
    };
    let mut keys = Vec::new();
    if layout {
        keys.extend([
            KeyControl::new(
                "Space",
                "Show/hide",
                Action::Metrics(W::ToggleWidget(workspace.selected)),
            )
            .with_priority(10),
            KeyControl::new("<", "Earlier", Action::Metrics(W::MoveWidget(false)))
                .with_priority(11),
            KeyControl::new(">", "Later", Action::Metrics(W::MoveWidget(true))).with_priority(12),
        ]);
    }
    keys.extend([
        KeyControl::new("r", "Refresh", metric(MetricsAction::Refresh)).with_priority(if layout {
            50
        } else {
            10
        }),
        KeyControl::new(
            "i",
            "Details",
            Action::Metrics(W::Inspect(workspace.selected)),
        )
        .with_priority(if layout { 52 } else { 12 }),
        KeyControl::new("b", "Back", metric(MetricsAction::Back))
            .with_priority(13)
            .in_group(ControlGroup::Navigate),
        window("7", 7, "7 days"),
        window("0", 30, "30 days"),
        KeyControl::new("R", "Repositories", Action::Metrics(W::Repositories))
            .with_compact("Repos")
            .with_priority(if layout { 62 } else { 24 }),
        KeyControl::new("L", "Layout", Action::Metrics(W::Layout)).with_priority(if layout {
            14
        } else {
            30
        }),
        KeyControl::new("S", "Save", metric(MetricsAction::SaveDashboard))
            .with_priority(if layout { 20 } else { 40 }),
        KeyControl::new("O", "Open saved", metric(MetricsAction::RestoreDashboard))
            .with_priority(42),
        KeyControl::new("D", "Defaults", metric(MetricsAction::DefaultDashboard)).with_priority(44),
    ]);
    if workspace.pending.is_some() {
        for key in &mut keys {
            key.disabled = Some(PREPARING_REASON.into());
        }
    }
    keys
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
