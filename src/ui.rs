use crate::app::{safe_text, Action, App, HitRegion, Overlay, View};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Clear, Paragraph, Wrap},
    Frame,
};

pub(crate) fn notice_lines(text: &str, width: u16) -> Vec<Line<'static>> {
    let mut output = vec![];
    crate::prepared_form_text_v1::visit_wrapped_ranges_v1(
        text,
        width,
        None,
        || Ok(()),
        |range, _| {
            output.push(Line::from(text[range].to_owned()));
            Ok(())
        },
    )
    .expect("legacy uncapped wrapping is infallible");
    output
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    draw_screen(frame, app);
    // Views clear their areas to the terminal default; the theme background fills those cells
    // so gaps between controls and empty rows never show a different terminal color.
    crate::ui_chrome::fill_default_colors(frame.buffer_mut(), app.theme());
}

fn draw_screen(frame: &mut Frame, app: &mut App) {
    app.pointer_region.clear();
    crate::notification_pr_v63::retire_invalid(app);
    crate::notifications_native_v63::retire_invalid(app);
    if app.overlay == Some(Overlay::NotificationsV63) {
        let area = frame.area();
        crate::notifications_ui_v63::draw(frame, app, area);
        return;
    }
    crate::scoped_pr_observed_tabs_v1::resources::threads_v1::comments_v2::publication::ui::retire_invalid_form(app);
    crate::scoped_pr_observed_tabs_v1::resources::threads_v1::comments_v2::reactions_v3::form::retire_invalid_form(app);
    crate::scoped_pr_observed_tabs_v1::resources::threads_v1::ui_v1::retire_invalid(app);
    crate::repository_native_diff_native_v1::synchronize_pr_source_v1(app);
    crate::scoped_pr_observed_tabs_v1::resources::threads_v1::history_v1::ui::retire_invalid(app);
    if app.overlay == Some(Overlay::PrThreadHistoryV1) {
        let area = frame.area();
        crate::scoped_pr_observed_tabs_v1::resources::threads_v1::history_v1::ui::draw(
            frame, app, area,
        );
        return;
    }
    if app.overlay == Some(Overlay::PrThreadsV1) {
        let area = frame.area();
        crate::scoped_pr_observed_tabs_v1::resources::threads_v1::ui_v1::draw(frame, app, area);
        return;
    }
    if crate::repository_stash_recovery_v1::draw(frame, app) {
        return;
    }
    if crate::repository_related_pr_v1::draw(frame, app) {
        return;
    }
    if app.remote_repository.is_some() {
        crate::remote_repository_native_v1::draw(frame, app);
        return;
    }
    let area = frame.area();
    if app.overlay.is_none() && crate::repository_native_diff_native_v1::draw(frame, app, area) {
        return;
    }
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
    let header = crate::ui_chrome::header_rows(app, area.width);
    let rows = Layout::vertical([
        Constraint::Length(header),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    let accent = theme
        .style(crate::theme::SemanticToken::Focus)
        .add_modifier(Modifier::BOLD);
    // Logo, view tabs, Source identity, Back and breadcrumbs share the header; the scope
    // context moved into the content pane's top border.
    crate::ui_chrome::draw_header(frame, app, rows[0]);
    let chrome_hits = app.hits.len();
    // `rows` keeps the content/status/footer indices the view adapters below expect.
    let rows = [rows[0], rows[0], rows[0], rows[1], rows[2], rows[3]];
    // A list with a preview splits its pane at wide widths (TASK-92); overlays keep the
    // whole content area.
    let content_full = rows[3];
    let split = if crate::list_preview_v92::offered(app) {
        crate::ui_chrome::split_list_preview(content_full)
    } else {
        crate::ui_chrome::ListPreview {
            list: content_full,
            preview: None,
        }
    };
    app.preview_area_v92 = split.preview;
    if split.preview.is_none() {
        app.content_pane = crate::list_preview_v92::Pane::List;
    }
    let rows = [rows[0], rows[1], rows[2], split.list, rows[4], rows[5]];
    if app.overlay.is_none() {
        app.pointer_region.capture(app.view, app.overlay, rows[3]);
    }
    let content: Vec<Line> = app.content[app.view.index()]
        .iter()
        .map(|line| Line::from(safe_text(line)))
        .collect();
    let focused = crate::ui_chrome::content_focused(app, chrome_hits)
        && crate::list_preview_v92::list_focused(app);
    frame.render_widget(
        Paragraph::new(content)
            .block(crate::ui_chrome::main_pane(app, theme, rows[3], focused))
            .wrap(Wrap { trim: false })
            .scroll((app.view_list().top.min(u16::MAX as usize) as u16, 0)),
        rows[3],
    );
    if app.view == View::Repository {
        crate::repository_ui::draw(
            frame,
            app,
            rows[3].inner(ratatui::layout::Margin::new(1, 1)),
        );
    }
    if app.view == View::Inbox
        && app.github.is_some()
        && !crate::scoped_pr_collaboration_native_v17::retired_presentation(app)
    {
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
        let actions_area = if app.scoped_actions_v1.is_some() {
            actions_area
        } else {
            crate::pr_checks_native::scoped::choices::continuations_v2::ui::draw_controls(
                frame,
                app,
                actions_area,
            )
        };
        crate::pr_checks_native::scoped::draw_pending_v1(frame, app, actions_area);
        let list_focused =
            app.actions_content_focused && crate::list_preview_v92::list_focused(app);
        if let Some(actions) = &mut app.actions {
            let rendered =
                crate::actions_ui::draw(frame, actions, actions_area, theme, list_focused);
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
                app.metrics_content_focused,
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
    if let Some(preview) = app.preview_area_v92 {
        crate::list_preview_v92::draw(frame, app, preview);
    }
    frame.render_widget(
        Paragraph::new(crate::ui_chrome::fit(
            &safe_text(&app.status),
            usize::from(rows[4].width),
        )),
        rows[4],
    );
    // The key bar is registered last, after every view hit, so Tab order ends with it.
    let controls = crate::footer::main_controls(app);
    crate::footer::draw(frame, app, rows[5], &controls);
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
        if overlay == Overlay::ScopedActionsReadV2 {
            crate::scoped_actions_native_v1::read_ui_v2::draw(frame, app, area);
            return;
        }
        if overlay == Overlay::ScopedReviewsV2 {
            crate::scoped_pr_observed_tabs_v1::resources::immutable_images::reviews_v2::native_v2::draw(frame,app,area);
            return;
        }
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
                let region = crate::ui_chrome::overlay_region(area, content_full);
                crate::ui_chrome::clear_overlay_gutter(frame, theme, region, region);
                let hits = palette.draw(frame, region, theme);
                app.hits.extend(hits.into_iter().map(|hit| HitRegion {
                    rect: hit.rect,
                    action: Action::PaletteInput(hit.action),
                }));
            }
            return;
        }
        if overlay == Overlay::Help {
            let region = crate::ui_chrome::overlay_region(area, content_full);
            crate::navigation_ui::draw_help(frame, app, region);
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
            Overlay::NotificationsV63 => unreachable!("Owned Notifications drawn above"),
            Overlay::PrThreadsV1 | Overlay::PrThreadHistoryV1 => {
                unreachable!("Owned threads drawn above")
            }
            Overlay::PrFileImages => unreachable!("Immutable images drawn above"),
            Overlay::ScopedReviewsV2 => unreachable!("Original reviews drawn above"),
            Overlay::PrCheckChoices => unreachable!("Owned check choices drawn above"),
            Overlay::PrCheckContinuationV2 => unreachable!("Owned check continuation drawn above"),
            Overlay::ScopedActionsReadV2 => unreachable!("Owned Actions editor drawn above"),
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
        frame.render_widget(crate::ui_chrome::overlay_block(theme, title), rect);
        let inner = rect.inner(ratatui::layout::Margin::new(2, 1));
        app.pointer_region.capture(app.view, app.overlay, rect);
        match overlay {
            Overlay::NotificationsV63 => unreachable!("Owned Notifications drawn above"),
            Overlay::PrThreadsV1 | Overlay::PrThreadHistoryV1 => {
                unreachable!("Owned threads drawn above")
            }
            Overlay::PrFileImages => unreachable!("Immutable images drawn above"),
            Overlay::ScopedReviewsV2 => unreachable!("Original reviews drawn above"),
            Overlay::PrCheckChoices => unreachable!("Owned check choices drawn above"),
            Overlay::PrCheckContinuationV2 => unreachable!("Owned check continuation drawn above"),
            Overlay::ScopedActionsReadV2 => unreachable!("Owned Actions editor drawn above"),
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
                        app.repository_list.selected,
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
                            theme.base()
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
                            theme.base()
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
                            theme.base()
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
                let theme_row = Rect::new(inner.x, inner.y + 4, inner.width, 1);
                let mut spans = vec![ratatui::text::Span::styled(
                    format!(
                        "[Theme: {}]",
                        crate::config::ThemeName::from_index(app.settings_theme).label()
                    ),
                    if app.focus == 2 { accent } else { theme.base() },
                )];
                // A swatch of the five view hues previews the palette before saving.
                if theme.mode != crate::theme::ColorMode::NoColor {
                    spans.push(ratatui::text::Span::styled(" ", theme.base()));
                    spans.extend(View::ALL.into_iter().map(|view| {
                        ratatui::text::Span::styled(
                            " ●",
                            theme.with_view(view).role(crate::theme::Role::Accent),
                        )
                    }));
                }
                frame.render_widget(
                    Paragraph::new(Line::from(spans)).style(theme.base()),
                    theme_row,
                );
                app.hits.push(HitRegion {
                    rect: theme_row,
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
                        theme.base()
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
                    theme.base()
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
    use crate::identity::{AvatarState, ListAvatar};
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
            // The chip and exactly one blank cell, then the reaction line.
            let lead = ListAvatar {
                identity,
                image,
                theme,
            }
            .render_lead(rect, frame.buffer_mut(), theme.style(SemanticToken::Normal));
            Rect::new(rect.x + lead, rect.y, rect.width - lead, 1)
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
                theme.base()
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
            // Six chrome hits, then the key bar: Refresh, Settings and the Global tail; Settings
            // leaves the 40-column bar first.
            assert_eq!(app.hits.len(), if width < 80 { 10 } else { 11 });
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
