//! Native prepared GitHub view, using the same actions for mouse and keyboard.
use crate::{
    app::{safe_text, Action, App, HitRegion},
    github_workspace::{GithubAction, InboxFilter, PrTab},
    identity::{ActorIdentity, AvatarState, AvatarWidget},
    theme::SemanticToken,
};
use ratatui::{
    layout::Rect,
    style::Modifier,
    widgets::{Clear, Paragraph},
    Frame,
};

fn buttons(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    buttons: Vec<(String, GithubAction)>,
) -> u16 {
    let theme = app.theme();
    let mut x = area.x;
    let mut y = area.y;
    for (label, action) in buttons {
        let width = (label.len() as u16).min(area.width);
        if x > area.x && x + width > area.right() {
            x = area.x;
            y += 1;
        }
        if y >= area.bottom() {
            break;
        }
        let rect = Rect::new(x, y, width, 1);
        let mut style = theme
            .style(SemanticToken::Focus)
            .remove_modifier(Modifier::UNDERLINED);
        if !app.github_content_focused && app.focus == app.hits.len() {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        frame.render_widget(Paragraph::new(label).style(style), rect);
        app.hits.push(HitRegion {
            rect,
            action: Action::Github(action),
        });
        x += width + 1;
    }
    y.saturating_sub(area.y) + 1
}

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(github) = app.github.as_ref() else {
        return;
    };
    let theme = app.theme();
    frame.render_widget(Clear, area);
    // Stable FIRST Back slot for both loaded and pending scoped pages.
    let scoped = github.scoped_pr_navigation.is_some();
    let pending_observed = scoped && github.pr.is_none();
    let tabs = if scoped {
        let mut controls = vec![(String::from("[b Back]"), GithubAction::Back)];
        if !pending_observed {
            controls.extend(
                PrTab::ALL
                    .iter()
                    .filter(|(tab, _, _)| {
                        crate::scoped_pr_observed_tabs_v1::ObservedPrTabV1::from_tab(*tab).is_some()
                            || matches!(tab, PrTab::Files | PrTab::Checks)
                    })
                    .map(|(tab, label, key)| {
                        (format!("[{key} {label}]"), GithubAction::PrTab(*tab))
                    }),
            );
            if github.scoped_pr_resource.is_some()
                && github
                    .pr
                    .as_ref()
                    .is_some_and(|page| page.tab == PrTab::Files)
            {
                controls.extend(
                    crate::scoped_pr_observed_tabs_v1::resources::PrResourceUiActionV1::CONTROLS
                        .iter()
                        .map(|(action, label, key)| {
                            (format!("[{key} {label}]"), GithubAction::Resource(*action))
                        }),
                );
            }
        }
        controls
    } else if github.pr.is_some() {
        PrTab::ALL
            .iter()
            .map(|(tab, label, key)| (format!("[{key} {label}]"), GithubAction::PrTab(*tab)))
            .chain([
                (String::from("[p PR actions]"), GithubAction::Actions),
                (String::from("[b Back]"), GithubAction::Back),
            ])
            .collect()
    } else {
        InboxFilter::ALL
            .iter()
            .map(|(filter, label, key)| (format!("[{key} {label}]"), GithubAction::Filter(*filter)))
            .collect()
    };
    let mut y = area.y + buttons(frame, app, area, tabs);
    if pending_observed {
        let message = if app.loading[crate::app::View::Inbox.index()] {
            "Preparing observed PR section • Back cancels • permission/counts not inferred"
        } else {
            "Observed page unavailable • Back returns to the previous cached Inbox"
        };
        frame.render_widget(
            Paragraph::new(message).style(theme.style(SemanticToken::Pending)),
            Rect::new(area.x, y, area.width, 1),
        );
        app.github_visible_rows = 0;
        return;
    }
    if app
        .github
        .as_ref()
        .and_then(|github| github.pr.as_ref())
        .is_some_and(|page| page.tab == PrTab::Checks)
        && y < area.bottom()
    {
        let rect = Rect::new(area.x, y, 18.min(area.width), 1);
        let action = crate::scoped_pr_observed_tabs_v1::resources::native::check_action(app);
        let mut style = theme
            .style(if matches!(action, Action::Explain(_)) {
                SemanticToken::Muted
            } else {
                SemanticToken::Reference
            })
            .remove_modifier(Modifier::UNDERLINED);
        if !app.github_content_focused && app.focus == app.hits.len() {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        frame.render_widget(Paragraph::new("[Enter Open check]").style(style), rect);
        app.hits.push(HitRegion { rect, action });
        y += 1;
    }
    y = crate::pr_checks_native::scoped::choices::ui::draw_launch(
        frame,
        app,
        Rect::new(area.x, y, area.width, area.bottom().saturating_sub(y)),
    )
    .y;
    let github = app.github.as_ref().unwrap();
    if let Some(page) = &github.pr {
        let summary = &page.details.summary;
        let heading = format!(
            "{}#{} • {} • {}{}",
            summary.pr.repository,
            summary.pr.number,
            summary.pr.title,
            summary.pr.state,
            if summary.pr.draft { " [draft]" } else { "" }
        );
        frame.render_widget(
            Paragraph::new(safe_text(&heading)).style(theme.style(SemanticToken::Head)),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
        app.github_visible_rows = area.bottom().saturating_sub(y) as usize;
        for (offset, row) in page
            .rows
            .iter()
            .skip(app.scroll)
            .take(app.github_visible_rows)
            .enumerate()
        {
            let row_area = Rect::new(area.x, y + offset as u16, area.width, 1);
            let index = app.scroll + offset;
            let selectable = github.pr_row_is_interactive(index);
            let selected = selectable && index == app.selected && app.github_content_focused;
            let marker = if selected { "> " } else { "  " };
            let style = if selected {
                theme.style(SemanticToken::Focus)
            } else {
                theme.style(row.role)
            };
            frame.render_widget(
                Paragraph::new(format!("{marker}{}", safe_text(&row.label))).style(style),
                row_area,
            );
            if let Some(actor) = &row.actor {
                let identity = ActorIdentity::github(&github.identity.host, actor);
                let avatar = app.avatar_states.get(&identity);
                frame.render_widget(
                    AvatarWidget {
                        identity: &identity,
                        image: match avatar {
                            Some(AvatarState::Ready(image)) => Some(image.as_ref()),
                            _ => None,
                        },
                        theme,
                    },
                    Rect::new(
                        area.x + 2.min(area.width),
                        row_area.y,
                        4.min(area.width.saturating_sub(2)),
                        1,
                    ),
                );
                frame.render_widget(
                    Paragraph::new(format!("{} {}", identity.label(), safe_text(&row.label)))
                        .style(style),
                    Rect::new(
                        area.x + 6.min(area.width),
                        row_area.y,
                        area.width.saturating_sub(6),
                        1,
                    ),
                );
            }
            if selectable {
                app.hits.push(HitRegion {
                    rect: row_area,
                    action: Action::GithubRow(index),
                });
            }
        }
        return;
    }
    let filter_label = InboxFilter::ALL
        .iter()
        .find(|(filter, _, _)| *filter == github.filter)
        .map_or("All", |(_, label, _)| *label);
    frame.render_widget(
        Paragraph::new(format!("Queue: {filter_label}")).style(theme.style(SemanticToken::Head)),
        Rect::new(area.x, y, area.width, 1),
    );
    y += 1;
    frame.render_widget(
        Paragraph::new(safe_text(&github.freshness.label()))
            .style(theme.style(SemanticToken::Muted)),
        Rect::new(area.x, y, area.width, 1),
    );
    y += 1;
    frame.render_widget(
        Paragraph::new(github.coverage_label()).style(theme.style(
            if github.inbox.coverage.complete {
                SemanticToken::Normal
            } else {
                SemanticToken::Pending
            },
        )),
        Rect::new(area.x, y, area.width, 1),
    );
    y += 1;
    let warning = github.inbox.coverage.reasons.join(" • ");
    if !warning.is_empty() {
        frame.render_widget(
            Paragraph::new(safe_text(&warning)).style(theme.style(SemanticToken::Pending)),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }
    app.github_visible_rows = area.bottom().saturating_sub(y + 1) as usize / 2;
    let visible = github.visible();
    if visible.is_empty() {
        frame.render_widget(
            Paragraph::new("No pull requests in this queue / scope."),
            Rect::new(area.x, y, area.width, 1),
        );
    }
    for (offset, index) in (app.scroll..visible.len())
        .take(app.github_visible_rows)
        .enumerate()
    {
        let pr = visible[index];
        let rect = Rect::new(area.x, y + offset as u16 * 2, area.width, 2);
        let selected = github.selected_index() == index && app.github_content_focused;
        let base = if selected {
            theme.selected(SemanticToken::Normal)
        } else {
            theme.style(SemanticToken::Normal)
        };
        let title = format!(
            "{} {}#{} {}{}",
            if selected { ">" } else { " " },
            pr.repository,
            pr.number,
            safe_text(&pr.title),
            if pr.draft { " [draft]" } else { "" }
        );
        frame.render_widget(
            Paragraph::new(title).style(base),
            Rect::new(rect.x, rect.y, rect.width, 1),
        );
        let actor = ActorIdentity::github(&github.identity.host, &pr.author);
        frame.render_widget(
            AvatarWidget {
                identity: &actor,
                image: match app.avatar_states.get(&actor) {
                    Some(AvatarState::Ready(image)) => Some(image.as_ref()),
                    _ => None,
                },
                theme,
            },
            Rect::new(rect.x, rect.y + 1, 4.min(rect.width), 1),
        );
        let memberships = pr
            .queues
            .iter()
            .map(|queue| queue.label())
            .collect::<Vec<_>>()
            .join(" + ");
        frame.render_widget(
            Paragraph::new(format!(
                "{} • {} • {} • review/checks: open PR to load",
                actor.label(),
                crate::github_workspace::age_label(&pr.created_at),
                memberships
            ))
            .style(base),
            Rect::new(
                rect.x + 4.min(rect.width),
                rect.y + 1,
                rect.width.saturating_sub(4),
                1,
            ),
        );
        app.hits.push(HitRegion {
            rect,
            action: Action::Github(GithubAction::Open(pr.into())),
        });
    }
    let clear = Rect::new(
        area.x,
        area.bottom().saturating_sub(1),
        15.min(area.width),
        1,
    );
    frame.render_widget(
        Paragraph::new("[x Clear cache]").style(theme.style(SemanticToken::Muted)),
        clear,
    );
    app.hits.push(HitRegion {
        rect: clear,
        action: Action::Github(GithubAction::ClearCache),
    });
}
