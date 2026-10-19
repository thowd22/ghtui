//! Native prepared GitHub view, using the same actions for mouse and keyboard.
use crate::{
    app::{safe_text, Action, App, HitRegion},
    footer::{ControlGroup, KeyControl},
    github_workspace::{GithubAction, InboxFilter, PrTab},
    identity::{ActorIdentity, AvatarState, ListAvatar},
    theme::SemanticToken,
};
use ratatui::{
    layout::Rect,
    widgets::{Clear, Paragraph},
    Frame,
};

/// Why `p` is refused on an observed (scoped) section that has no PR actions.
const OBSERVED_ACTIONS_REASON: &str =
    "Observed sections only • PR actions require their own fresh scoped read";

/// Shorter PR tab labels for a tab strip too narrow for the full ones (80 columns). Each is
/// part of its full label, so the short form still names the same tab.
fn pr_tab_compact(tab: PrTab) -> &'static str {
    match tab {
        PrTab::Summary => "Summary",
        PrTab::Conversation => "Conv",
        PrTab::Commits => "Commits",
        PrTab::Files => "Files",
        PrTab::Checks => "Checks",
        PrTab::Timeline => "Time",
        PrTab::Metadata => "Metadata",
        PrTab::Progress => "Prog",
    }
}

fn inbox_filter_compact(filter: InboxFilter) -> &'static str {
    match filter {
        InboxFilter::All => "All",
        InboxFilter::Authored => "Created",
        InboxFilter::Assigned => "Assigned",
        InboxFilter::ReviewRequested => "Review",
    }
}

/// The Inbox and PR detail controls: PR tabs or Inbox queues as Mode controls for the tab
/// strip, then the footer keys by priority. Keys resolve to the same actions the tokens and
/// tabs carry; a refused action keeps its key and explains why.
pub fn footer_keys(app: &App) -> Vec<KeyControl> {
    let Some(github) = app.github.as_ref() else {
        return Vec::new();
    };
    let gh = Action::Github;
    let scoped = github.scoped_pr_navigation.is_some();
    let back = KeyControl::new("b", "Back", gh(GithubAction::Back))
        .with_priority(5)
        .in_group(ControlGroup::Navigate);
    let refresh = |priority: u8| {
        KeyControl::new(app.shortcut("refresh"), "Refresh", Action::Refresh).with_priority(priority)
    };
    let clear = |priority: u8| {
        KeyControl::new("x", "Clear cache", gh(GithubAction::ClearCache))
            .with_compact("Clear")
            .with_priority(priority)
    };
    let Some(page) = github.pr.as_ref() else {
        if scoped {
            // A pending observed section: Back cancels it; nothing else is offered yet.
            return vec![back];
        }
        let mut keys = vec![KeyControl::new(
            "Enter",
            "Open",
            github.selected.clone().map_or_else(
                || Action::Explain("Select a pull request first.".into()),
                |key| gh(GithubAction::Open(key)),
            ),
        )
        .with_priority(5)
        .in_group(ControlGroup::Navigate)
        .disabled_by(
            github
                .visible()
                .is_empty()
                .then(|| "No pull request in this queue.".to_string()),
        )];
        keys.extend([
            refresh(10),
            KeyControl::new(
                "F8",
                "Notifications",
                Action::NotificationsV63(crate::notifications_ui_v63::NotificationActionV63::Open),
            )
            .with_priority(20),
            clear(40),
        ]);
        keys.extend(InboxFilter::ALL.iter().map(|(filter, label, key)| {
            KeyControl::new(key.to_string(), *label, gh(GithubAction::Filter(*filter)))
                .with_compact(inbox_filter_compact(*filter))
                .in_group(ControlGroup::Mode)
                .with_selected(github.filter == *filter)
        }));
        return keys;
    };
    let tab = page.tab;
    let mut keys = vec![back];
    if tab == PrTab::Checks {
        let action = crate::scoped_pr_observed_tabs_v1::resources::native::check_action(app);
        let reason = match &action {
            Action::Explain(reason) => Some(reason.clone()),
            _ => None,
        };
        keys.push(
            // The primary action of the Checks tab outlasts Back on the narrowest rows.
            KeyControl::new("Enter", "Open check", action)
                .with_compact("Check")
                .with_priority(4)
                .disabled_by(reason),
        );
    }
    if crate::pr_checks_native::scoped::choices::has_candidates(app)
        && !crate::pr_checks_native::scoped::choices::pending(app)
    {
        keys.push(
            KeyControl::new(
                "C",
                "Candidates",
                Action::PrCheckChoice(
                    crate::pr_checks_native::scoped::choices::PrCheckChoiceUiV1::Open,
                ),
            )
            .with_priority(9),
        );
    }
    // The scoped gate matches the dispatcher. Opening this menu is not publication
    // authority; its chosen operation proves that fresh.
    let actions_offered = !scoped
        || matches!(tab, PrTab::Summary | PrTab::Conversation)
        || tab == PrTab::Metadata && page.metadata_display_v51.is_some();
    keys.push(
        KeyControl::new("p", "PR actions", gh(GithubAction::Actions))
            .with_compact("Actions")
            .with_priority(10)
            .disabled_by((!actions_offered).then(|| OBSERVED_ACTIONS_REASON.to_string())),
    );
    if scoped && matches!(tab, PrTab::Summary | PrTab::Timeline) {
        keys.push(
            KeyControl::new("T", "All threads", gh(GithubAction::ReviewThreadsV4))
                .with_priority(20),
        );
    }
    if tab == PrTab::Summary {
        keys.push(
            KeyControl::new("J", "Thread history", gh(GithubAction::ThreadHistoryV1))
                .with_compact("History")
                .with_priority(22),
        );
        keys.push(
            KeyControl::new(
                "F9",
                "Review history",
                gh(GithubAction::ReviewOperationHistoryV3),
            )
            .with_compact("Review")
            .with_priority(24),
        );
        if scoped {
            keys.push(
                KeyControl::new("F8", "Actions history", gh(GithubAction::ActionsHistoryV5))
                    .with_priority(26),
            );
        }
    }
    // Resource keys come before the tabs so `v` and `T` keep their Files meaning.
    if github.scoped_pr_resource.is_some() && tab == PrTab::Files {
        keys.extend(resource_keys());
        if scoped {
            keys.push(
                KeyControl::new("K", "FILE comment", gh(GithubAction::FileCommentV75))
                    .with_priority(17),
            );
        }
        if github.scoped_pr_resource.as_ref().is_some_and(|view| {
            view.observed().kind() == crate::providers::gh::PrResourceKindV1::Files
        }) {
            keys.push(
                KeyControl::new("F10", "Reviews", Action::OpenScopedReviewsV2).with_priority(14),
            );
        }
    }
    keys.extend(crate::pr_metadata_quick_v1::footer_keys(app));
    // While the page is loading, `Esc Cancel` joins the bar and Refresh replaces the read in
    // flight; it outranks the history overlays so a click still reaches the replacement.
    let refresh_priority = if app.loading[crate::app::View::Inbox.index()] {
        21
    } else {
        45
    };
    keys.extend([refresh(refresh_priority), clear(70)]);
    keys.extend(
        PrTab::ALL
            .iter()
            .filter(|(candidate, _, _)| {
                !scoped
                    || crate::scoped_pr_observed_tabs_v1::ObservedPrTabV1::from_tab(*candidate)
                        .is_some()
                    || matches!(candidate, PrTab::Files | PrTab::Checks)
                    || *candidate == PrTab::Metadata
                        && matches!(tab, PrTab::Summary | PrTab::Conversation | PrTab::Metadata)
            })
            .map(|(candidate, label, key)| {
                KeyControl::new(key.to_string(), *label, gh(GithubAction::PrTab(*candidate)))
                    .with_compact(pr_tab_compact(*candidate))
                    .in_group(ControlGroup::Mode)
                    .with_selected(*candidate == tab)
            }),
    );
    keys
}

/// The loaded Files resource controls (`PrResourceUiActionV1::CONTROLS`). At 80 columns the
/// diff, comment entry (Old image), Reviews, file paging and threads stay; the others keep
/// their keys. Compact labels are part of the full ones.
fn resource_keys() -> Vec<KeyControl> {
    use crate::scoped_pr_observed_tabs_v1::resources::PrResourceUiActionV1 as R;
    R::CONTROLS
        .iter()
        .map(|(action, label, key)| {
            let (priority, compact) = match action {
                R::NativeDiff => (12, "Diff"),
                R::OldImage => (13, "Old"),
                R::NextFile => (15, "Next"),
                R::Threads => (16, "Threads"),
                R::CommentLine => (18, "Comment"),
                R::NextHunk => (19, "Hunk"),
                R::Layout => (30, "Layout"),
                R::PatchContext => (32, "Context"),
            };
            KeyControl::new(
                key.to_string(),
                *label,
                Action::Github(GithubAction::Resource(*action)),
            )
            .with_compact(compact)
            .with_priority(priority)
        })
        .collect()
}

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(github) = app.github.as_ref() else {
        return;
    };
    let theme = app.theme();
    frame.render_widget(Clear, area);
    let pending_observed = github.scoped_pr_navigation.is_some() && github.pr.is_none();
    // PR tabs or Inbox queues on one tab-strip row; the other controls are footer keys.
    let controls = footer_keys(app);
    let mut y = area.y;
    if area.height > 0
        && controls
            .iter()
            .any(|control| control.group == ControlGroup::Mode)
    {
        crate::ui_chrome::tab_strip(frame, app, Rect::new(area.x, y, area.width, 1), &controls);
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
        // Commits, Files and Checks rows with cells are table rows (TASK-91); every other row
        // is prose. The page keeps its own scroll and cursor, so lines are painted one by one.
        let host = github.identity.host.clone();
        let columns = page.columns();
        let cell_rows = columns.map_or_else(Vec::new, |(_, glyphs)| {
            page.rows
                .iter()
                .filter_map(|row| table_row(row, &host, glyphs))
                .collect::<Vec<_>>()
        });
        let table = columns.map(|(columns, glyphs)| crate::ui_table::Table {
            columns,
            header: false,
            focused: app.github_content_focused,
            glyphs,
            empty: None,
        });
        let natural = columns.map_or_else(Vec::new, |(columns, _)| {
            crate::ui_table::natural_widths(columns, cell_rows.iter(), theme.mode)
        });
        let painter = table
            .as_ref()
            .map(|table| crate::ui_table::TablePainter::new(table, &natural, area, theme));
        // Checks are one table: the exact head they ran on, then sticky column titles.
        if let Some(painter) = painter
            .as_ref()
            .filter(|_| page.tab == PrTab::Checks && !cell_rows.is_empty() && area.bottom() > y + 1)
        {
            frame.render_widget(
                Paragraph::new(format!("  Head {}", safe_text(&summary.head_sha)))
                    .style(theme.style(SemanticToken::Muted)),
                Rect::new(area.x, y, area.width, 1),
            );
            painter.header(frame.buffer_mut(), y + 1);
            y += 2;
        }
        let mut line = y;
        let mut rendered = 0usize;
        for (offset, row) in page.rows.iter().skip(app.inbox_list.top).enumerate() {
            if line >= area.bottom() {
                break;
            }
            let index = app.inbox_list.top + offset;
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
            let style = theme.style(row.role);
            if let (Some(painter), Some(item)) = (
                &painter,
                columns.and_then(|(_, glyphs)| table_row(row, &host, glyphs)),
            ) {
                painter.row(frame.buffer_mut(), row_area.y, &item);
            } else if let Some(actor) = &row.actor {
                // Bar slot, initials chip (or photo), one blank cell, then `@login text` cut
                // with an ellipsis. The row text is never painted under the chip.
                let identity = ActorIdentity::github(&host, actor);
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
                    Paragraph::new(format!("  {}", safe_text(&row.label))).style(style),
                    row_area,
                );
            }
            if selectable {
                // The cursor rests only on interactive rows: the `▌` bar on the Selection
                // background, accent while the page content has focus.
                if index == app.inbox_list.selected {
                    crate::ui_table::select_line(
                        frame.buffer_mut(),
                        row_area,
                        app.github_content_focused,
                        theme,
                    );
                }
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
    if crate::ui_chrome::density().is_compact() {
        // Compact (TASK-96): the queue summary is the section header, so freshness keeps its
        // dimmed right-hand place and the separate `PULL REQUESTS` row goes to the list.
        frame.render_widget(
            Paragraph::new(compact_queue_header(
                theme,
                &queue_line,
                &github.freshness.label(),
                area.width,
            )),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    } else {
        // A narrow list pane (beside a preview) cuts the summary with an ellipsis.
        frame.render_widget(
            Paragraph::new(crate::ui_chrome::fit(&queue_line, usize::from(area.width)))
                .style(theme.style(SemanticToken::Head)),
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
    }
    frame.render_widget(
        Paragraph::new(crate::ui_chrome::fit(
            &github.coverage_label(),
            usize::from(area.width),
        ))
        .style(theme.style(if github.inbox.coverage.complete {
            SemanticToken::Normal
        } else {
            SemanticToken::Pending
        })),
        Rect::new(area.x, y, area.width, 1),
    );
    y += 1;
    let warning = github.inbox.coverage.reasons.join(" • ");
    if !warning.is_empty() {
        frame.render_widget(
            Paragraph::new(crate::ui_chrome::fit(
                &safe_text(&warning),
                usize::from(area.width),
            ))
            .style(theme.style(SemanticToken::Pending)),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }
    // One table row per pull request under sticky column titles (TASK-91). The Inbox keeps
    // its own scroll (`app.inbox_list.top`; the wheel may leave the cursor off screen).
    let visible = github.visible();
    let host = github.identity.host.clone();
    let rows = visible
        .iter()
        .map(|pr| inbox_row(pr, &host))
        .collect::<Vec<_>>();
    let table = crate::ui_table::Table {
        columns: &INBOX_COLUMNS,
        header: true,
        focused: app.github_content_focused && crate::list_preview_v92::list_focused(app),
        glyphs: false,
        empty: None,
    };
    let natural = crate::ui_table::natural_widths(&INBOX_COLUMNS, rows.iter(), theme.mode);
    let painter = crate::ui_table::TablePainter::new(&table, &natural, area, theme);
    if y < area.bottom() {
        painter.header(frame.buffer_mut(), y);
        y += 1;
    }
    app.github_visible_rows = usize::from(area.bottom().saturating_sub(y));
    if visible.is_empty() && y < area.bottom() {
        painter.empty(
            frame.buffer_mut(),
            y,
            &if github.state_membership_unavailable_v68() {
                "Queue membership unavailable • use a genuine fresh scope read.".to_string()
            } else {
                crate::github_workspace::empty_queue_hint_v1(github.filter, github.queue_counts())
            },
        );
    }
    let selected = github.selected_index();
    for (offset, index) in (app.inbox_list.top..visible.len())
        .take(app.github_visible_rows)
        .enumerate()
    {
        let y = y + offset as u16;
        painter.row(frame.buffer_mut(), y, &rows[index]);
        if index == selected {
            painter.select(frame.buffer_mut(), y);
        }
        app.hits.push(HitRegion {
            rect: painter.line(y),
            action: Action::Github(GithubAction::Open(visible[index].into())),
        });
    }
}

/// The Compact Inbox's one summary row: the queue summary on the left (cut with an ellipsis
/// first) and the freshness right-aligned and muted, which is never cut for the summary.
pub(crate) fn compact_queue_header(
    theme: crate::theme::Theme,
    summary: &str,
    freshness: &str,
    width: u16,
) -> ratatui::text::Line<'static> {
    use ratatui::text::{Line, Span};
    let width = usize::from(width);
    let freshness = crate::ui_chrome::fit(&safe_text(freshness), width);
    let detail = crate::editor::display_width(&freshness);
    let room = width.saturating_sub(detail + 2);
    let muted = theme.role(crate::theme::Role::TextMuted);
    if room < 8 {
        return Line::from(Span::styled(freshness, muted));
    }
    let summary = crate::ui_chrome::fit(summary, room);
    let gap = width - detail - crate::editor::display_width(&summary);
    Line::from(vec![
        Span::styled(summary, theme.style(SemanticToken::Head)),
        Span::raw(" ".repeat(gap)),
        Span::styled(freshness, muted),
    ])
}

/// Inbox queue columns. The PR reference never leaves; title, author and age share a priority
/// so a long title is cut before author or age leave; queues go first. The title takes most of
/// the spare width (a long login is cut before the title is).
const INBOX_COLUMNS: [crate::ui_table::Column; 5] = [
    crate::ui_table::Column::new("PR", 6).priority(0),
    crate::ui_table::Column::new("TITLE", 10)
        .flex(4)
        .priority(1),
    crate::ui_table::Column::new("AUTHOR", 6)
        .flex(1)
        .priority(1)
        .actor(),
    crate::ui_table::Column::new("AGE", 3).priority(1).right(),
    crate::ui_table::Column::new("QUEUE", 5).priority(3),
];

/// One Inbox table row: `owner/repo#N`, title (`[draft]` kept), author chip, age, queues.
fn inbox_row(pr: &crate::providers::gh::PullRequest, host: &str) -> crate::ui_table::TableRow<()> {
    use crate::providers::gh::Queue;
    let queues = pr
        .queues
        .iter()
        .map(|queue| match queue {
            Queue::Authored => "Created",
            Queue::Assigned => "Assigned",
            Queue::ReviewRequested => "Review",
        })
        .collect::<Vec<_>>()
        .join(", ");
    crate::ui_table::TableRow::cells(vec![
        format!("{}#{}", pr.repository, pr.number).into(),
        format!("{}{}", pr.title, if pr.draft { " [draft]" } else { "" }).into(),
        String::new().into(),
        crate::ui_table::relative_age(&pr.created_at).into(),
        crate::ui_table::Cell::styled(queues, crate::theme::Role::TextMuted),
    ])
    .with_actor(Some(ActorIdentity::github(host, &pr.author)))
}

/// A PR page row with cells as a table row (`None` for prose rows). The status glyph comes
/// from the row's role when the tab's table has a glyph slot.
fn table_row(
    row: &crate::github_workspace::PrRow,
    host: &str,
    glyphs: bool,
) -> Option<crate::ui_table::TableRow<()>> {
    use crate::{github_workspace::PrCells, ui_table::TableRow};
    match row.cells.as_ref()? {
        PrCells::Header => Some(TableRow::header()),
        PrCells::Row(cells) => Some(
            TableRow::cells(cells.iter().map(|cell| cell.as_str().into()).collect())
                .with_tone(row.role)
                .with_glyph(glyphs.then_some(row.role))
                .with_actor(
                    row.actor
                        .as_ref()
                        .map(|actor| ActorIdentity::github(host, actor)),
                ),
        ),
    }
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

#[cfg(test)]
#[path = "github_ui_footer_v90_tests.rs"]
mod footer_v90_tests;

#[cfg(test)]
#[path = "github_table_v91_tests.rs"]
mod table_v91_tests;
