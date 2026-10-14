use crate::app::{safe_text, Action, App, HitRegion, Overlay, View};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn notice_lines(text: &str, width: u16) -> Vec<Line<'static>> {
    let mut output = vec![];
    for line in text.lines() {
        let mut current = String::new();
        let mut cells = 0;
        for grapheme in line.graphemes(true) {
            let count = crate::editor::grapheme_width(grapheme);
            if !current.is_empty() && cells + count > usize::from(width.max(1)) {
                output.push(Line::from(std::mem::take(&mut current)));
                cells = 0;
            }
            current.push_str(grapheme);
            cells += count;
        }
        output.push(Line::from(current));
    }
    output
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    if crate::repository_related_pr_v1::draw(frame, app) {
        return;
    }
    if app.remote_repository.is_some() {
        crate::remote_repository_native_v1::draw(frame, app);
        return;
    }
    let area = frame.area();
    if crate::repository_tree_viewport_native_v2::folder_draw(frame, app, area) {
        return;
    }
    if app.overlay.is_none() && crate::repository_tree_native_v1::draw(frame, app, area) {
        return;
    }
    if app.overlay.is_none() && crate::repository_diff_sides_native_v1::draw(frame, app, area) {
        return;
    }
    let theme = app.theme();
    if app.view == View::Repository && app.overlay.is_none() && app.local_commit_actors.details_open
    {
        app.hits.clear();
        app.local_actor_layout
            .details(frame, area, &app.local_commit_actors, theme);
        app.hits
            .extend(app.local_actor_layout.hits().iter().map(|hit| HitRegion {
                rect: hit.rect,
                action: Action::LocalActor(hit.action.clone()),
            }));
        return;
    }
    frame.render_widget(Block::default().style(theme.base()), area);
    app.hits.clear();
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    let accent = theme
        .style(crate::theme::SemanticToken::Focus)
        .add_modifier(Modifier::BOLD);
    frame.render_widget(
        Paragraph::new("ghtui • Git and GitHub, clearly").style(accent),
        rows[0],
    );
    let mut x = rows[1].x;
    for (index, view) in View::ALL.into_iter().enumerate() {
        let label = format!(
            " {} {} ",
            app.shortcut(["inbox", "repository", "search", "actions", "dashboard"][index]),
            view.label()
        );
        let width = (label.len() as u16).min(rows[1].right().saturating_sub(x));
        let rect = Rect::new(x, rows[1].y, width, 1);
        let mut style = if app.view == view {
            accent
                .remove_modifier(Modifier::UNDERLINED)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        if app.focus == index {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        frame.render_widget(Paragraph::new(label).style(style), rect);
        app.hits.push(HitRegion {
            rect,
            action: Action::View(view),
        });
        x = x.saturating_add(width);
    }
    let source_control = app.view == View::Repository
        && app.repository_source_identity.is_some()
        && rows[2].width >= 18;
    let context_width = rows[2]
        .width
        .saturating_sub(if source_control { 18 } else { 0 });
    frame.render_widget(
        Paragraph::new(safe_text(&app.context)),
        Rect::new(rows[2].x, rows[2].y, context_width, 1),
    );
    if source_control {
        let rect = Rect::new(rows[2].right() - 17, rows[2].y, 17, 1);
        frame.render_widget(
            Paragraph::new("[Source identity]").style(
                theme
                    .style(
                        if app.focus == app.hits.len() && !app.repository_content_focused {
                            crate::theme::SemanticToken::Focus
                        } else {
                            crate::theme::SemanticToken::Reference
                        },
                    )
                    .remove_modifier(
                        if app.focus == app.hits.len() && !app.repository_content_focused {
                            Modifier::empty()
                        } else {
                            Modifier::UNDERLINED
                        },
                    ),
            ),
            rect,
        );
        app.hits.push(HitRegion {
            rect,
            action: Action::RepositorySource,
        });
    }
    crate::navigation_state::draw_navigation(frame, app, rows[0]);
    let content: Vec<Line> = app.content[app.view.index()]
        .iter()
        .map(|line| Line::from(safe_text(line)))
        .collect();
    frame.render_widget(
        Paragraph::new(content)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} • focused view ", app.view.label())),
            )
            .wrap(Wrap { trim: false })
            .scroll((app.scroll.min(u16::MAX as usize) as u16, 0)),
        rows[3],
    );
    if app.view == View::Repository {
        crate::repository_ui::draw(
            frame,
            app,
            rows[3].inner(ratatui::layout::Margin::new(1, 1)),
        );
    }
    if app.view == View::Inbox && app.github.is_some() {
        crate::github_ui::draw(
            frame,
            app,
            rows[3].inner(ratatui::layout::Margin::new(1, 1)),
        );
    }
    if app.view == View::Search {
        crate::search_ui::draw(
            frame,
            app,
            rows[3].inner(ratatui::layout::Margin::new(1, 1)),
        );
    }
    if app.view == View::Actions {
        let actions_area = crate::pr_checks_native::scoped::choices::ui::draw_launch(
            frame,
            app,
            rows[3].inner(ratatui::layout::Margin::new(1, 1)),
        );
        let actions_area =
            crate::pr_checks_native::scoped::choices::continuations_v2::ui::draw_controls(
                frame,
                app,
                actions_area,
            );
        crate::pr_checks_native::scoped::draw_pending_v1(frame, app, actions_area);
        if let Some(actions) = &mut app.actions {
            let rendered =
                crate::actions_ui::draw(frame, actions, actions_area, theme, &app.avatar_states);
            app.actions_visible_rows = rendered.visible_rows;
            // Focus is a state, not a decoration on every actionable label.
            for hit in &rendered.hits {
                frame.buffer_mut().set_style(
                    hit.rect,
                    Style::default().remove_modifier(Modifier::UNDERLINED),
                );
            }
            if !app.actions_content_focused {
                if let Some(hit) = app
                    .focus
                    .checked_sub(app.hits.len())
                    .and_then(|index| rendered.hits.get(index))
                {
                    frame.buffer_mut().set_style(
                        hit.rect,
                        Style::default().add_modifier(Modifier::UNDERLINED),
                    );
                }
            }
            app.hits
                .extend(rendered.hits.into_iter().map(|hit| HitRegion {
                    rect: hit.rect,
                    action: Action::Actions(hit.action),
                }));
        }
    }
    if app.view == View::Dashboard {
        if let Some(metrics) = &mut app.metrics {
            let rendered = crate::metrics_ui::draw(
                frame,
                metrics,
                rows[3].inner(ratatui::layout::Margin::new(1, 1)),
                theme,
                &app.avatar_states,
            );
            app.metrics_content = rendered.content;
            app.metrics_visible_rows = rendered.visible_rows;
            if !app.metrics_content_focused {
                if let Some(hit) = app
                    .focus
                    .checked_sub(app.hits.len())
                    .and_then(|index| rendered.hits.get(index))
                {
                    frame.buffer_mut().set_style(
                        hit.rect,
                        Style::default().add_modifier(Modifier::UNDERLINED),
                    );
                }
            }
            app.hits
                .extend(rendered.hits.into_iter().map(|hit| HitRegion {
                    rect: hit.rect,
                    action: Action::Metrics(hit.action),
                }));
        } else {
            app.metrics_content = Default::default();
        }
    }
    frame.render_widget(Paragraph::new(safe_text(&app.status)), rows[4]);
    let mut x = rows[5].x;
    let compact_footer = rows[5].width < 80;
    let mut buttons = vec![
        (format!("[{} Help]", app.shortcut("help")), Action::Help),
        (
            format!("[{} Commands]", app.shortcut("palette")),
            Action::Palette,
        ),
        (
            format!("[{} Refresh]", app.shortcut("refresh")),
            Action::Refresh,
        ),
        (
            format!("[{} Settings]", app.shortcut("settings")),
            Action::Settings,
        ),
        (format!("[{} Quit]", app.shortcut("quit")), Action::Quit),
    ];
    if compact_footer {
        // Preserve discoverable Help/Commands/Quit and active cancellation;
        // Refresh/Settings remain clickable through the same command palette.
        buttons.retain(|(_, action)| !matches!(action, Action::Refresh | Action::Settings));
        if let Some((label, _)) = buttons
            .iter_mut()
            .find(|(_, action)| *action == Action::Palette)
        {
            *label = format!("[{} Cmds]", app.shortcut("palette"));
        }
    }
    if app.loading[app.view.index()] || app.navigation_restore_inflight {
        buttons.push(("[Esc Cancel]".into(), Action::Cancel));
    }
    for (label, action) in buttons {
        let rect = Rect::new(
            x.min(rows[5].right()),
            rows[5].y,
            (label.len() as u16).min(rows[5].right().saturating_sub(x)),
            1,
        );
        let focused = app.focus == app.hits.len();
        frame.render_widget(
            Paragraph::new(label).style(if focused { accent } else { Style::default() }),
            rect,
        );
        app.hits.push(HitRegion { rect, action });
        x = x.saturating_add(rect.width + if compact_footer { 1 } else { 2 });
    }
    crate::navigation_state::restore_focus(app);
    if let Some(overlay) = app.overlay {
        crate::scoped_pr_observed_tabs_v1::resources::immutable_images::native::retire_invalid(app);
        if overlay == Overlay::PrFileImages {
            if app.overlay == Some(Overlay::PrFileImages) {
                crate::scoped_pr_observed_tabs_v1::resources::immutable_images::native::draw(
                    frame, app, area,
                );
            }
            return;
        }
        app.hits.clear();
        if overlay == Overlay::GraphRereadV3 {
            crate::repository_graph_reread_native_v3::draw(frame, app, area);
            return;
        }
        if overlay == Overlay::PrCheckContinuationV2 {
            crate::pr_checks_native::scoped::choices::continuations_v2::ui::draw(frame, app, area);
            return;
        }
        if overlay == Overlay::PrCheckChoices {
            crate::pr_checks_native::scoped::choices::ui::draw(frame, app, area);
            return;
        }
        if overlay == Overlay::RepositoryCatalogue {
            if let Some(modal) = &mut app.catalogue {
                let modal_area = Rect::new(
                    area.x,
                    area.y.saturating_add(2),
                    area.width,
                    area.height.saturating_sub(2),
                );
                crate::repository_catalogue_dialogs::draw(frame, modal_area, modal);
            }
            return;
        }
        if overlay == Overlay::RepositorySource {
            crate::repository_source_ui::draw(frame, app, area);
            return;
        }
        if overlay == Overlay::Palette {
            if let Some(palette) = &mut app.command_palette {
                let hits =
                    palette.draw(frame, area.inner(ratatui::layout::Margin::new(2, 2)), theme);
                app.hits.extend(hits.into_iter().map(|hit| HitRegion {
                    rect: hit.rect,
                    action: Action::PaletteInput(hit.action),
                }));
            }
            return;
        }
        if overlay == Overlay::Help {
            crate::navigation_ui::draw_help(frame, app, area);
            return;
        }
        if overlay == Overlay::Capabilities {
            app.capability_hits.clear();
            if let Some(inspector) = &mut app.capability_inspector {
                app.capability_hits =
                    inspector.draw(frame, area.inner(ratatui::layout::Margin::new(2, 2)), theme);
                app.hits
                    .extend(app.capability_hits.iter().map(|hit| HitRegion {
                        rect: hit.rect,
                        action: Action::Inspector(hit.action.clone()),
                    }));
            }
            return;
        }
        if overlay == Overlay::SearchBlend {
            crate::search_blend::draw(frame, app, area);
            return;
        }
        if overlay == Overlay::SearchSources {
            crate::search_sources::ui::draw(frame, app, area);
            return;
        }
        if overlay == Overlay::SearchContext {
            crate::search_sources::ui::draw_context(frame, app, area);
            return;
        }
        if overlay == Overlay::Form {
            crate::forms::draw(frame, app, area);
            return;
        }
        let width = area.width.saturating_sub(4).min(72);
        let height = area.height.saturating_sub(4).min(16);
        let rect = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, rect);
        let title = match overlay {
            Overlay::PrFileImages => unreachable!("Immutable images drawn above"),
            Overlay::PrCheckChoices => unreachable!("Owned check choices drawn above"),
            Overlay::PrCheckContinuationV2 => unreachable!("Owned check continuation drawn above"),
            Overlay::GraphRereadV3 => unreachable!("Original Graph reread drawn above"),
            Overlay::RepositoryCatalogue => unreachable!("Repository catalogue drawn above"),
            Overlay::RepositorySource => unreachable!("Repository source drawn above"),
            Overlay::Help => " Help • keyboard and mouse ",
            Overlay::Palette => " Commands • choose a view ",
            Overlay::Capabilities => unreachable!("Capabilities drawn above"),
            Overlay::Settings => " Settings • local preferences ",
            Overlay::PrActions => " PR actions • native workflows ",
            Overlay::Form => " Composer ",
            Overlay::LocalActions => " Local operations • explicit preview and confirmation ",
            Overlay::Notice => {
                crate::scoped_pr_observed_tabs_v1::resources::native::notice_title(&app.notice)
            }
            Overlay::PrReactions => " Comment reactions • read-only actors ",
            Overlay::SearchSources => " Search sources • read-only hydration ",
            Overlay::SearchContext => " Exact search source • read-only ",
            Overlay::SearchBlend => " Blend controls • explicit Ranked GitHub discovery ",
            Overlay::SearchRankDetails => " Rank sources • limits and coverage ",
        };
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(accent.remove_modifier(Modifier::UNDERLINED)),
            rect,
        );
        let inner = rect.inner(ratatui::layout::Margin::new(2, 1));
        match overlay {
            Overlay::PrFileImages => unreachable!("Immutable images drawn above"),
            Overlay::PrCheckChoices => unreachable!("Owned check choices drawn above"),
            Overlay::PrCheckContinuationV2 => unreachable!("Owned check continuation drawn above"),
            Overlay::GraphRereadV3 => unreachable!("Original Graph reread drawn above"),
            Overlay::RepositoryCatalogue => unreachable!("Repository catalogue drawn above"),
            Overlay::RepositorySource => unreachable!("Repository source drawn above"),
            Overlay::Capabilities => unreachable!("Capabilities drawn above"),
            Overlay::SearchBlend => unreachable!("Blend overlay drawn above"),
            Overlay::SearchSources => unreachable!("Search source overlay drawn above"),
            Overlay::SearchContext => unreachable!("Search context overlay drawn above"),
            Overlay::PrReactions => draw_pr_reactions(frame, app, inner),
            Overlay::Notice | Overlay::SearchRankDetails => {
                let body = Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(2),
                );
                let lines = notice_lines(&app.notice, body.width);
                app.notice_scroll = app
                    .notice_scroll
                    .min(lines.len().saturating_sub(body.height as usize));
                let paragraph =
                    Paragraph::new(lines).style(theme.style(crate::theme::SemanticToken::Pending));
                frame.render_widget(paragraph.scroll((app.notice_scroll as u16, 0)), body);
                let y = inner.bottom().saturating_sub(2);
                for (offset, label, delta) in [(0, "[PgUp Previous]", -8), (19, "[PgDn More]", 8)] {
                    let button = Rect::new(
                        inner.x + offset.min(inner.width),
                        y,
                        18.min(inner.width.saturating_sub(offset)),
                        1,
                    );
                    frame.render_widget(Paragraph::new(label), button);
                    if !button.is_empty() {
                        app.hits.push(HitRegion {
                            rect: button,
                            action: Action::NoticeScroll(delta),
                        });
                    }
                }
            }
            Overlay::LocalActions => {
                let visible = inner.height.saturating_sub(2) as usize;
                let start = app
                    .local_action_index
                    .saturating_sub(visible.saturating_sub(1));
                let context = app.workspace.as_ref().map(|workspace| {
                    crate::local_availability::LocalAvailability::from_workspace(
                        workspace,
                        app.selected,
                        app.repository_selection
                            .as_ref()
                            .map(|selection| &selection.start),
                    )
                });
                for (offset, (index, (command, label))) in crate::local_actions::LocalCommand::ALL
                    .iter()
                    .enumerate()
                    .skip(start)
                    .take(visible)
                    .enumerate()
                {
                    let row = Rect::new(inner.x, inner.y + offset as u16, inner.width, 1);
                    let reason = context
                        .as_ref()
                        .and_then(|context| context.disabled_reason(*command));
                    frame.render_widget(
                        Paragraph::new(format!(
                            "{} {label}{}",
                            if index == app.local_action_index {
                                ">"
                            } else {
                                " "
                            },
                            reason
                                .as_ref()
                                .map_or(String::new(), |reason| format!(" [disabled: {reason}]"))
                        ))
                        .style(if index == app.local_action_index {
                            accent
                        } else {
                            Style::default()
                        }),
                        row,
                    );
                    app.hits.push(HitRegion {
                        rect: row,
                        action: Action::LocalCommand(*command),
                    });
                }
                let y = inner.bottom().saturating_sub(2);
                let previous = Rect::new(inner.x, y, 18.min(inner.width), 1);
                let next = Rect::new(
                    inner.x + 20.min(inner.width),
                    y,
                    18.min(inner.width.saturating_sub(20)),
                    1,
                );
                frame.render_widget(Paragraph::new("[PgUp Previous]"), previous);
                frame.render_widget(Paragraph::new("[PgDn More]"), next);
                app.hits.push(HitRegion {
                    rect: previous,
                    action: Action::MenuScroll(-8),
                });
                app.hits.push(HitRegion {
                    rect: next,
                    action: Action::MenuScroll(8),
                });
            }
            Overlay::PrActions => {
                let visible = inner.height.saturating_sub(2) as usize;
                app.pr_menu_rows = visible.max(1);
                let start = app
                    .pr_action_index
                    .saturating_sub(visible.saturating_sub(1));
                for (offset, (index, (label, action))) in app
                    .pr_controls()
                    .into_iter()
                    .enumerate()
                    .skip(start)
                    .take(visible)
                    .enumerate()
                {
                    let row = Rect::new(inner.x, inner.y + offset as u16, inner.width, 1);
                    let disabled = matches!(action, Action::Explain(_));
                    frame.render_widget(
                        Paragraph::new(format!(
                            "{} {label}{}",
                            if index == app.pr_action_index {
                                ">"
                            } else {
                                " "
                            },
                            if disabled { " [disabled]" } else { "" }
                        ))
                        .style(if index == app.pr_action_index {
                            accent
                        } else {
                            Style::default()
                        }),
                        row,
                    );
                    app.hits.push(HitRegion { rect: row, action });
                }
                let y = inner.bottom().saturating_sub(2);
                for (offset, label, delta) in [
                    (0, "[PgUp Previous]", -(app.pr_menu_rows as i32)),
                    (20, "[PgDn More]", app.pr_menu_rows as i32),
                ] {
                    let button = Rect::new(
                        inner.x + offset.min(inner.width),
                        y,
                        18.min(inner.width.saturating_sub(offset)),
                        1,
                    );
                    frame.render_widget(Paragraph::new(label), button);
                    if !button.is_empty() {
                        app.hits.push(HitRegion {
                            rect: button,
                            action: Action::MenuScroll(delta),
                        });
                    }
                }
            }
            Overlay::Form => {}
            Overlay::Help => {
                frame.render_widget(Paragraph::new(format!("Views: {} Inbox • {} Repository • {} Search • {} Actions • {} Dashboard\nTab / Shift-Tab: focus controls • Enter: activate\nArrows / j k: scroll • mouse wheel: scroll\n{}: commands • {}: refresh • {}: settings\nEscape: close overlay • Ctrl+C: emergency quit\n\nGitHub authentication belongs to gh.\nLocal Git browsing preserves your working files.", app.shortcut("inbox"), app.shortcut("repository"), app.shortcut("search"), app.shortcut("actions"), app.shortcut("dashboard"), app.shortcut("palette"), app.shortcut("refresh"), app.shortcut("settings"))).wrap(Wrap { trim: false }), inner);
            }
            Overlay::Palette => {
                for (index, (label, action)) in app.palette_controls().into_iter().enumerate() {
                    let row = Rect::new(inner.x, inner.y + index as u16, inner.width, 1);
                    frame.render_widget(
                        Paragraph::new(format!(
                            "{} {}",
                            if index == app.palette_index { ">" } else { " " },
                            label
                        ))
                        .style(if index == app.palette_index {
                            accent
                        } else {
                            Style::default()
                        }),
                        row,
                    );
                    app.hits.push(HitRegion { rect: row, action });
                }
            }
            Overlay::Settings => {
                for (index, (label, value)) in [
                    ("Host", &app.settings_host),
                    ("Repository set", &app.settings_set),
                ]
                .into_iter()
                .enumerate()
                {
                    let y = inner.y + index as u16 * 2;
                    frame.render_widget(
                        Paragraph::new(format!(
                            "{label}: {}",
                            if value.is_empty() {
                                "(personal queues)"
                            } else {
                                value
                            }
                        )),
                        Rect::new(inner.x, y, inner.width, 1),
                    );
                    let value_offset = label.len() as u16 + 2;
                    let row = Rect::new(
                        inner.x + value_offset.min(inner.width),
                        y,
                        inner.width.saturating_sub(value_offset),
                        1,
                    );
                    app.hits.push(HitRegion {
                        rect: row,
                        action: Action::SettingsField(index),
                    });
                    if app.focus == index {
                        frame.set_cursor_position((
                            row.x + (app.settings_cursor as u16).min(row.width.saturating_sub(1)),
                            row.y,
                        ));
                    }
                }
                let theme = Rect::new(inner.x, inner.y + 4, inner.width, 1);
                frame.render_widget(
                    Paragraph::new(format!(
                        "[Theme: {}]",
                        ["Dark", "Light", "No color"][app.settings_theme]
                    ))
                    .style(if app.focus == 2 {
                        accent
                    } else {
                        Style::default()
                    }),
                    theme,
                );
                app.hits.push(HitRegion {
                    rect: theme,
                    action: Action::SettingsTheme,
                });
                frame.render_widget(Paragraph::new("Tab: next control • Enter: activate • Ctrl-U: clear field\nChoose a saved repository set, or leave blank for personal queues."), Rect::new(inner.x, inner.y + 6, inner.width, 2));
                let save = Rect::new(
                    inner.x + 18.min(inner.width),
                    rect.bottom().saturating_sub(2),
                    18.min(inner.width.saturating_sub(18)),
                    1,
                );
                frame.render_widget(
                    Paragraph::new("[Save settings]").style(if app.focus == 3 {
                        accent
                    } else {
                        Style::default()
                    }),
                    save,
                );
                app.hits.push(HitRegion {
                    rect: save,
                    action: Action::SettingsSave,
                });
            }
        }
        let close = Rect::new(
            inner.x,
            rect.bottom().saturating_sub(2),
            15.min(inner.width),
            1,
        );
        frame.render_widget(
            Paragraph::new("[Esc Close]").style(
                if overlay == Overlay::PrReactions && app.focus == app.hits.len() {
                    accent.add_modifier(Modifier::UNDERLINED)
                } else {
                    Style::default()
                },
            ),
            close,
        );
        app.hits.push(HitRegion {
            rect: close,
            action: Action::Close,
        });
    }
}

fn draw_pr_reactions(frame: &mut Frame, app: &mut App, inner: Rect) {
    use crate::identity::{AvatarState, AvatarWidget};
    use crate::theme::SemanticToken;
    if app.pr_reactions.is_none() {
        return;
    }
    let theme = app.theme();
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(2),
    );
    app.pr_reaction_rows = usize::from(body.height).max(1);
    app.pr_reaction_scroll = app.pr_reaction_scroll.min(
        app.pr_reaction_display
            .len()
            .saturating_sub(app.pr_reaction_rows),
    );
    if body.width < 68 {
        frame.render_widget(Paragraph::new("Resize to at least 80 columns to inspect complete reaction identities. Close remains available.").wrap(Wrap {trim:false}), body);
    }
    for (offset, row) in app
        .pr_reaction_display
        .iter()
        .skip(app.pr_reaction_scroll)
        .take(if body.width < 68 {
            0
        } else {
            app.pr_reaction_rows
        })
        .enumerate()
    {
        let rect = Rect::new(body.x, body.y + offset as u16, body.width, 1);
        let text_rect = if let Some(identity) = &row.actor {
            let image = match app.avatar_states.get(identity) {
                Some(AvatarState::Ready(image)) => Some(image.as_ref()),
                _ => None,
            };
            frame.render_widget(
                AvatarWidget {
                    identity,
                    image,
                    theme,
                },
                Rect::new(rect.x, rect.y, 3.min(rect.width), 1),
            );
            Rect::new(
                rect.x + 4.min(rect.width),
                rect.y,
                rect.width.saturating_sub(4),
                1,
            )
        } else {
            rect
        };
        frame.render_widget(
            Paragraph::new(row.line.clone()).style(theme.style(SemanticToken::Normal)),
            text_rect,
        );
    }
    let y = inner.bottom().saturating_sub(2);
    for (offset, label, delta) in [
        (0, "[PgUp Previous]", -(app.pr_reaction_rows as i32)),
        (20, "[PgDn More]", app.pr_reaction_rows as i32),
    ] {
        let rect = Rect::new(
            inner.x + offset.min(inner.width),
            y,
            18.min(inner.width.saturating_sub(offset)),
            1,
        );
        if !rect.is_empty() {
            let style = if app.focus == app.hits.len() {
                theme
                    .style(SemanticToken::Focus)
                    .add_modifier(Modifier::UNDERLINED)
            } else {
                Style::default()
            };
            frame.render_widget(Paragraph::new(label).style(style), rect);
            app.hits.push(HitRegion {
                rect,
                action: Action::PrReactionScroll(delta),
            });
        }
    }
}

#[cfg(test)]
#[path = "ui_compact_footer_v1_tests.rs"]
mod compact_footer_v1_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn bootstrap_empty_layout_and_mouse_regions_resize() {
        for (width, height) in [(120, 40), (80, 24), (40, 12)] {
            let mut app = App::default();
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            assert_eq!(app.hits.len(), if width < 80 { 9 } else { 11 });
            assert!(app.hits.iter().any(|hit| hit.action
                == Action::Navigation(crate::navigation_trail::NavigationAction::Back)));
            assert!(app
                .hits
                .iter()
                .all(|hit| hit.rect.right() <= width && hit.rect.bottom() <= height));
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(text.contains("ghtui"));
            app.dispatch(Action::Help);
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            assert_eq!(app.hits.len(), 3);
            assert_eq!(app.hits[2].action, Action::Close);
            assert!(app.context_help.is_some());
            assert!(app.hits.iter().all(|hit| hit.rect.right() <= width
                && hit.rect.bottom() <= height
                && !hit.rect.is_empty()));
            for expected_focus in [0, 1] {
                if expected_focus == 1 {
                    app.handle_event(crossterm::event::Event::Key(
                        crossterm::event::KeyEvent::new(
                            crossterm::event::KeyCode::Tab,
                            crossterm::event::KeyModifiers::NONE,
                        ),
                    ));
                    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
                }
                assert_eq!(app.focus, expected_focus);
                for (index, hit) in app.hits.iter().enumerate() {
                    assert_eq!(
                        terminal
                            .backend()
                            .buffer()
                            .cell((hit.rect.x, hit.rect.y))
                            .unwrap()
                            .modifier
                            .contains(Modifier::UNDERLINED),
                        index == expected_focus,
                        "Help paging/Close underlines represent actual focus only"
                    );
                }
            }
        }
    }
}
