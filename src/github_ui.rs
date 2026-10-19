//! Native prepared GitHub view, using the same actions for mouse and keyboard.
use crate::{
    app::{safe_text, Action, App, HitRegion},
    github_workspace::{GithubAction, InboxFilter, PrTab},
    identity::{ActorIdentity, AvatarState, ListAvatar},
    theme::SemanticToken,
};
use ratatui::{
    layout::Rect,
    style::Modifier,
    widgets::{Clear, Paragraph},
    Frame,
};

fn buttons(frame: &mut Frame, app: &mut App, area: Rect, buttons: Vec<(String, Action)>) -> u16 {
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
        app.hits.push(HitRegion { rect, action });
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
            if github
                .pr
                .as_ref()
                .is_some_and(|page| matches!(page.tab, PrTab::Summary | PrTab::Timeline))
            {
                controls.push(("[T All threads]".into(), GithubAction::ReviewThreadsV4));
            }
            if github
                .pr
                .as_ref()
                .is_some_and(|page| page.tab == PrTab::Summary)
            {
                controls.push(("[J Thread history]".into(), GithubAction::ThreadHistoryV1));
                controls.push((
                    "[F9 Review history]".into(),
                    GithubAction::ReviewOperationHistoryV3,
                ));
                controls.push((
                    "[F8 Actions history]".into(),
                    GithubAction::ActionsHistoryV5,
                ));
            }
            controls.extend(
                PrTab::ALL
                    .iter()
                    .filter(|(tab, _, _)| {
                        crate::scoped_pr_observed_tabs_v1::ObservedPrTabV1::from_tab(*tab).is_some()
                            || matches!(tab, PrTab::Files | PrTab::Checks)
                            || *tab == PrTab::Metadata
                                && github.pr.as_ref().is_some_and(|p| {
                                    matches!(
                                        p.tab,
                                        PrTab::Summary | PrTab::Conversation | PrTab::Metadata
                                    )
                                })
                    })
                    .map(|(tab, label, key)| {
                        (format!("[{key} {label}]"), GithubAction::PrTab(*tab))
                    }),
            );
            // Match the existing scoped keyboard gate. Opening this menu is
            // not publication authority; its chosen operation proves that fresh.
            if github.pr.as_ref().is_some_and(|page| {
                matches!(page.tab, PrTab::Summary | PrTab::Conversation)
                    || page.tab == PrTab::Metadata && page.metadata_display_v51.is_some()
            }) {
                controls.push(("[p PR actions]".into(), GithubAction::Actions));
            }
            if github.scoped_pr_resource.is_some()
                && github
                    .pr
                    .as_ref()
                    .is_some_and(|page| page.tab == PrTab::Files)
            {
                controls.push(("[K FILE comment]".into(), GithubAction::FileCommentV75));
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
        let mut controls: Vec<_> = PrTab::ALL
            .iter()
            .map(|(tab, label, key)| (format!("[{key} {label}]"), GithubAction::PrTab(*tab)))
            .chain([
                (String::from("[p PR actions]"), GithubAction::Actions),
                (String::from("[b Back]"), GithubAction::Back),
            ])
            .chain(
                github
                    .pr
                    .as_ref()
                    .filter(|page| page.tab == PrTab::Summary)
                    .map(|_| {
                        (
                            String::from("[F9 Review history]"),
                            GithubAction::ReviewOperationHistoryV3,
                        )
                    }),
            )
            .collect();
        if github
            .pr
            .as_ref()
            .is_some_and(|page| page.tab == PrTab::Summary)
        {
            controls.push(("[J Thread history]".into(), GithubAction::ThreadHistoryV1));
        }
        controls
    } else {
        InboxFilter::ALL
            .iter()
            .map(|(filter, label, key)| (format!("[{key} {label}]"), GithubAction::Filter(*filter)))
            .collect()
    };
    let notifications_available = github.pr.is_none() && !scoped;
    let mut tabs: Vec<(String, Action)> = tabs
        .into_iter()
        .map(|(label, action)| (label, Action::Github(action)))
        .collect();
    // Metadata tab: one-step label/assignee/reviewer/milestone controls.
    tabs.extend(crate::pr_metadata_quick_v1::controls(app, area.width));
    let mut y = area.y + buttons(frame, app, area, tabs);
    if notifications_available && y < area.bottom() {
        let label = "[F8 Notifications]";
        let rect = Rect::new(area.x, y, (label.len() as u16).min(area.width), 1);
        frame.render_widget(
            Paragraph::new(label).style(theme.style(SemanticToken::Focus)),
            rect,
        );
        app.hits.push(HitRegion {
            rect,
            action: Action::NotificationsV63(
                crate::notifications_ui_v63::NotificationActionV63::Open,
            ),
        });
        y += 1;
    }
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
        // Summary/Conversation prose rows (PR/comment bodies) wrap; selectable and actor rows
        // stay one line so their hit regions and selection index are unchanged.
        // Visible rows = logical rows that actually fit, for paging/selection.
        let wrap_prose = matches!(page.tab, PrTab::Summary | PrTab::Conversation);
        let mut line = y;
        let mut rendered = 0usize;
        for (offset, row) in page.rows.iter().skip(app.scroll).enumerate() {
            if line >= area.bottom() {
                break;
            }
            let index = app.scroll + offset;
            let selectable = github.pr_row_is_interactive(index);
            if wrap_prose && !selectable && row.actor.is_none() {
                let text = safe_text(&row.label);
                let pieces = wrap_cells(&text, usize::from(area.width.saturating_sub(2)).max(1));
                let room = usize::from(area.bottom() - line);
                for piece in pieces.iter().take(room) {
                    frame.render_widget(
                        Paragraph::new(format!("  {piece}")).style(theme.style(row.role)),
                        Rect::new(area.x, line, area.width, 1),
                    );
                    line += 1;
                }
                // A clipped row is shown but not counted, so the next page
                // starts on it (unless it alone fills the pane).
                if pieces.len() <= room || rendered == 0 {
                    rendered += 1;
                }
                continue;
            }
            let row_area = Rect::new(area.x, line, area.width, 1);
            line += 1;
            rendered += 1;
            let selected = selectable && index == app.selected && app.github_content_focused;
            let marker = if selected { "> " } else { "  " };
            let style = if selected {
                theme.style(SemanticToken::Focus)
            } else {
                theme.style(row.role)
            };
            if let Some(actor) = &row.actor {
                // Marker, initials chip, one blank cell, then `@login text` cut with an ellipsis.
                // The row text is never painted under the chip, so nothing shows through it.
                frame.render_widget(Paragraph::new(marker).style(style), row_area);
                let identity = ActorIdentity::github(&github.identity.host, actor);
                let avatar = ListAvatar {
                    identity: &identity,
                    image: match app.avatar_states.get(&identity) {
                        Some(AvatarState::Ready(image)) => Some(image.as_ref()),
                        _ => None,
                    },
                    theme,
                };
                let indent = 2.min(row_area.width);
                avatar.render_with_label(
                    Rect::new(row_area.x + indent, row_area.y, row_area.width - indent, 1),
                    frame.buffer_mut(),
                    &format!("{} {}", identity.label(), safe_text(&row.label)),
                    style,
                );
            } else {
                frame.render_widget(
                    Paragraph::new(format!("{marker}{}", safe_text(&row.label))).style(style),
                    row_area,
                );
            }
            if selectable {
                app.hits.push(HitRegion {
                    rect: row_area,
                    action: Action::GithubRow(index),
                });
            }
        }
        app.github_visible_rows = rendered.max(1);
        return;
    }
    let filter_label = InboxFilter::ALL
        .iter()
        .find(|(filter, _, _)| *filter == github.filter)
        .map_or("All", |(_, label, _)| *label);
    let queue_line = if github.state_membership_unavailable_v68() {
        format!("Queue: {filter_label}")
    } else {
        crate::github_workspace::queue_summary_v1(filter_label, github.queue_counts())
    };
    frame.render_widget(
        Paragraph::new(queue_line).style(theme.style(SemanticToken::Head)),
        Rect::new(area.x, y, area.width, 1),
    );
    y += 1;
    // Freshness is provenance: a dimmed detail on the list's section header.
    frame.render_widget(
        Paragraph::new(crate::ui_chrome::section_header(
            theme,
            "Pull requests",
            &github.freshness.label(),
            crate::theme::Role::TextMuted,
            area.width,
        )),
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
            Paragraph::new(if github.state_membership_unavailable_v68() {
                "Queue membership unavailable • use a genuine fresh scope read.".to_string()
            } else {
                crate::github_workspace::empty_queue_hint_v1(github.filter, github.queue_counts())
            }),
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
            Paragraph::new(crate::ui_chrome::fit(&title, usize::from(rect.width))).style(base),
            Rect::new(rect.x, rect.y, rect.width, 1),
        );
        // The author line sits under the title, past the two-cell selection marker column.
        let indent = 2.min(rect.width);
        let rect = Rect::new(rect.x + indent, rect.y, rect.width - indent, rect.height);
        let actor = ActorIdentity::github(&github.identity.host, &pr.author);
        let avatar = ListAvatar {
            identity: &actor,
            image: match app.avatar_states.get(&actor) {
                Some(AvatarState::Ready(image)) => Some(image.as_ref()),
                _ => None,
            },
            theme,
        };
        let memberships = pr
            .queues
            .iter()
            .map(|queue| queue.label())
            .collect::<Vec<_>>()
            .join(" + ");
        let author_line = format!(
            "{} • {} • {} • review/checks: open PR to load",
            actor.label(),
            crate::github_workspace::age_label(&pr.created_at),
            memberships
        );
        // Chip, exactly one blank cell, then the author line cut with an ellipsis.
        avatar.render_with_label(
            Rect::new(rect.x, rect.y + 1, rect.width, 1),
            frame.buffer_mut(),
            &author_line,
            base,
        );
        app.hits.push(HitRegion {
            rect: Rect::new(area.x, rect.y, area.width, 2),
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

/// Splits display text into rows of at most `width` visible cells, breaking
/// after the last whitespace when a word would cross the edge. Overlong words
/// still hard-wrap, and every character is kept (rows concatenate to `text`).
fn wrap_cells(text: &str, width: usize) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut rows = Vec::new();
    let mut current = String::new();
    let mut cells = 0usize;
    // Byte offset just after the row's last whitespace, and its cell count.
    let mut boundary: Option<(usize, usize)> = None;
    for glyph in text.graphemes(true) {
        let count = crate::editor::grapheme_width(glyph);
        let space = glyph.chars().all(char::is_whitespace);
        // Trailing whitespace may pass the edge (it is invisible); the next
        // word then starts a row instead of a stray leading space.
        if !space && cells.saturating_add(count) > width && !current.is_empty() {
            match boundary {
                Some((at, before)) if at < current.len() => {
                    let word = current.split_off(at);
                    rows.push(std::mem::replace(&mut current, word));
                    cells -= before;
                }
                _ => {
                    rows.push(std::mem::take(&mut current));
                    cells = 0;
                }
            }
            boundary = None;
            if cells.saturating_add(count) > width && !current.is_empty() {
                rows.push(std::mem::take(&mut current));
                cells = 0;
            }
        }
        current.push_str(glyph);
        cells += count;
        if space {
            boundary = Some((current.len(), cells));
        }
    }
    if !current.is_empty() || rows.is_empty() {
        rows.push(current);
    }
    rows
}

#[cfg(test)]
mod pr_body_wrap_tests {
    use super::wrap_cells;

    #[test]
    fn pr_body_wrap_cells_respect_width_wide_glyphs_and_keep_every_character() {
        let text = "abc 界界 defgh";
        let rows = wrap_cells(text, 4);
        assert!(rows
            .iter()
            .all(|row| crate::editor::display_width(row.trim_end()) <= 4));
        assert_eq!(rows.concat(), text);
        assert_eq!(wrap_cells("", 10), vec![String::new()]);
        assert_eq!(
            wrap_cells("we observe the run", 10),
            ["we observe ", "the run"].map(str::to_owned)
        );
        assert_eq!(
            wrap_cells("a verylongtokenhere b", 6),
            ["a ", "verylo", "ngtoke", "nhere ", "b"].map(str::to_owned)
        );
        assert_eq!(wrap_cells("exact", 5), vec!["exact".to_owned()]);
        // A glyph wider than the pane still makes progress on its own row.
        assert_eq!(wrap_cells("界", 1), vec!["界".to_owned()]);
    }
}
