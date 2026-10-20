//! Repository rendering consumes prepared identities, objects and graph rows only.
#[cfg(test)]
#[path = "repository_diff_header_v1_tests.rs"]
mod diff_header_v1_tests;
mod file_history;
pub mod history_filter;
pub(crate) mod history_table;
#[cfg(test)]
#[path = "repository_views_v1_tests.rs"]
mod repository_views_v1_tests;
#[cfg(test)]
#[path = "repository_toolbar_v2_tests.rs"]
mod toolbar_v2_tests;
use crate::{
    app::{safe_text, Action, App, HitRegion},
    footer::{ControlGroup, KeyControl},
    identity::GitRole,
    theme::SemanticToken,
    workspace::{RepositoryMode, RowAction, Workspace},
};
use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{Block, Clear, Paragraph},
    Frame,
};

/// The Repository sub-modes, drawn as the tab strip above the content. Their keys resolve
/// through [`footer_keys`]; the strip leaves out tabs that do not fit, and every mode keeps its
/// key and Commands entry.
pub fn mode_controls(current: RepositoryMode) -> Vec<KeyControl> {
    RepositoryMode::ALL
        .into_iter()
        .map(|(mode, label, key)| {
            let label = if mode == RepositoryMode::Uncommitted {
                label
            } else {
                label.split_whitespace().next().unwrap_or(label)
            };
            let control = KeyControl::new(key.to_string(), label, Action::RepositoryMode(mode))
                .in_group(ControlGroup::Mode)
                .with_selected(mode == current);
            // `e Edits` lets all eight tabs fit an 80-column pane.
            if mode == RepositoryMode::Uncommitted {
                control.with_compact("Edits")
            } else {
                control
            }
        })
        .collect()
}

const NO_REPOSITORY: &str = "Open a local Git repository first";

/// Whether the visible Graph claims a key first, as its own keys always have.
fn graph_claims(workspace: &Workspace, key: &str) -> bool {
    let mut characters = key.chars();
    match (characters.next(), characters.next()) {
        (Some(character), None) => {
            crate::repository_graph_ui_v1::key(workspace, KeyCode::Char(character)).is_some()
        }
        _ => false,
    }
}

/// The Repository key bar: the mode tabs (strip only), one Back, the actions for the current
/// diff, graph or file, then the repository-wide actions and Refresh/Settings. Each key appears
/// once; an unavailable action leaves the bar and its key explains why.
pub fn footer_keys(app: &App) -> Vec<KeyControl> {
    let Some(workspace) = app.workspace.as_ref() else {
        // Nothing is open yet: the modes still choose what to open, and the repository
        // actions explain why they wait.
        let mut keys = mode_controls(app.repository_mode);
        for (key, label) in [
            ("m", "Local operations"),
            ("J", "Jump"),
            ("l", "Legend"),
            ("a", "Author"),
            ("c", "Committer"),
        ] {
            keys.push(
                KeyControl::new(key, label, Action::Refresh)
                    .disabled_by(Some(NO_REPOSITORY.to_string())),
            );
        }
        keys.extend(crate::footer::transitional_keys(app));
        return keys;
    };
    let busy = app.loading[crate::app::View::Repository.index()];
    let detail = workspace
        .file_provenance
        .as_ref()
        .is_some_and(|state| state.view == crate::workspace::ProvenanceView::Detail);
    let diff_side = workspace
        .original_diff_side_v1
        .as_ref()
        .filter(|side| side.visible_for(workspace));
    let saved_comparison = workspace.comparison_v1().is_some_and(|comparison| {
        matches!(
            comparison.comparison,
            crate::git::Comparison::Snapshots { .. }
                | crate::git::Comparison::BranchChanges { .. }
                | crate::git::Comparison::CommitChanges { .. }
        )
    }) && workspace.file_provenance.is_none();
    let mutable_comparison = workspace.actor_profile.is_none()
        && workspace.file_provenance.is_none()
        && workspace.comparison.as_ref().is_some_and(|comparison| {
            matches!(
                comparison.comparison,
                crate::git::Comparison::Working
                    | crate::git::Comparison::Staged
                    | crate::git::Comparison::Uncommitted
            )
        });
    let mut graph = crate::repository_graph_ui_v1::controls(workspace, busy);
    let mut files = if !saved_comparison
        && (!workspace.graph_retention_visible_v2()
            || workspace.graph_view_v1 == crate::repository_graph_v1::GraphViewV1::Timeline)
    {
        file_history::controls(workspace, busy)
    } else {
        Vec::new()
    };
    // History's commit detail (TASK-99.4) owns Enter, f, Y, y and D for the shown commit; `P`
    // stays an alias of its drill-down.
    let history_keys = crate::history::detail::footer_keys(app);
    if !history_keys.is_empty() {
        graph.retain(|control| !matches!(control.key.as_str(), "D" | "P"));
        files.retain(|control| !history_keys.iter().any(|key| key.key == control.key));
    }
    // One Back, in the order its key has always been answered: the graph comparison, the saved
    // tree a file came from, the comparison a diff side came from, then the file history.
    let take_back = |controls: &mut Vec<KeyControl>, tree: bool| {
        let index = controls
            .iter()
            .position(|control| control.key == "u" && (control.label == "Back to tree") == tree)?;
        Some(controls.remove(index))
    };
    let graph_back = take_back(&mut graph, false);
    let tree_back = take_back(&mut files, true);
    // The panels' untracked toggle owns `u` while the unavailable File back would only explain.
    let panel_keys = crate::repository_panels_v93::footer_keys(app);
    let file_back = take_back(&mut files, false)
        .filter(|back| panel_keys.is_empty() || back.disabled.is_none());
    let side_back = diff_side.map(|side| {
        KeyControl::new(
            "u",
            "Back to comparison",
            Action::RepositoryRow(RowAction::OriginalDiffReturn(side.return_ticket())),
        )
        .with_compact("Back")
        .with_priority(5)
        .in_group(ControlGroup::Navigate)
    });
    let mut keys = mode_controls(workspace.mode);
    keys.extend(
        graph_back
            .or(tree_back)
            .or(side_back)
            .or(file_back.filter(|_| diff_side.is_none())),
    );
    let mut context = Vec::new();
    // Staging applies to the working tree and index, never to a saved comparison.
    if workspace.file_provenance.is_none()
        && workspace.actor_profile.is_none()
        && !saved_comparison
        && matches!(
            workspace.mode,
            RepositoryMode::Overview
                | RepositoryMode::Working
                | RepositoryMode::Staged
                | RepositoryMode::Uncommitted
        )
    {
        for (priority, (quick, label, key)) in
            (10u8..).zip(crate::local_actions::QuickLocal::CONTROLS)
        {
            context.push(
                KeyControl::new(key.to_string(), label, Action::LocalQuick(quick))
                    .with_priority(priority),
            );
        }
    }
    if matches!(
        workspace.mode,
        RepositoryMode::Working | RepositoryMode::Staged
    ) {
        context.push(KeyControl::new("h", "Hunk", Action::RepositoryHunk).with_priority(13));
    }
    if saved_comparison || mutable_comparison {
        context.push(
            KeyControl::new(
                "v",
                "Native diff",
                if mutable_comparison {
                    Action::NativeMutableDiffV2(
                        crate::repository_native_diff_ui_v1::NativeDiffActionV1::Open,
                    )
                } else {
                    Action::NativeDiffV1(
                        crate::repository_native_diff_ui_v1::NativeDiffActionV1::Open,
                    )
                },
            )
            .with_compact("Diff")
            .with_priority(9),
        );
    }
    if saved_comparison {
        context.push(
            KeyControl::new(
                "f",
                "Changed files",
                Action::OriginalDiffSidesV1(
                    crate::repository_diff_sides_v1::OriginalDiffActionV1::Open,
                ),
            )
            .with_compact("Files")
            .with_priority(8),
        );
    }
    // History's search, match, hide and scope keys lead its own (TASK-99.3).
    context.extend(history_filter::controls(app));
    context.extend(history_keys);
    context.extend(graph);
    context.extend(files);
    if diff_side.is_none() && !detail {
        context.push(KeyControl::new("C", "Compare", Action::Compare).with_priority(24));
    }
    context.extend(panel_keys);
    let jump = if busy {
        Some("Wait for the current repository read or cancel first".to_string())
    } else {
        crate::git::OriginalTreeIntentV1::available(workspace).err()
    };
    context.extend([
        KeyControl::new("m", "Local operations", Action::LocalMenu)
            .with_compact("Git ops")
            .with_priority(7),
        KeyControl::new(
            "J",
            "Jump",
            Action::OriginalTreeV1(crate::repository_tree_jump_v1::OriginalTreeJumpActionV1::Open),
        )
        // Jump is only available in a saved Tree, where it is the main action.
        .with_priority(12)
        .disabled_by(jump),
        KeyControl::new(
            "l",
            "Legend",
            Action::RepositoryRow(
                if workspace.graph_visible_v1() || graph_claims(workspace, "l") {
                    RowAction::Graph(crate::repository_graph_v1::GraphActionV1::Legend)
                } else {
                    RowAction::Legend
                },
            ),
        )
        .with_priority(34),
        KeyControl::new(
            "a",
            "Author",
            Action::RepositoryRow(RowAction::Actor(GitRole::Author)),
        )
        .with_priority(36),
        KeyControl::new(
            "c",
            "Committer",
            Action::RepositoryRow(RowAction::Actor(GitRole::Committer)),
        )
        .with_priority(37),
    ]);
    for control in context {
        let claimed = keys.iter().any(|kept| kept.key == control.key)
            || (!control_is_graph(&control) && graph_claims(workspace, &control.key));
        if !claimed {
            keys.push(control);
        }
    }
    // Re-reading the repository after an outside Git change is a daily action: Refresh stays
    // ahead of the repository-wide keys; Settings still leaves first.
    keys.extend(crate::footer::transitional_keys(app).into_iter().map(
        |control| match control.action {
            Action::Refresh => control.with_priority(25),
            _ => control,
        },
    ));
    keys
}

fn control_is_graph(control: &KeyControl) -> bool {
    matches!(
        control.action,
        Action::RepositoryRow(RowAction::Graph(_)) | Action::GraphRereadV3(_)
    )
}

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(workspace) = app.workspace.as_ref() else {
        return;
    };
    let theme = app.theme();
    // The generic/loading shell can already have drawn text in this pane. Styling a Block
    // changes colors, not symbols: clear the complete dedicated pane before prepared rows.
    frame.render_widget(Clear, area);
    frame.render_widget(Block::default().style(theme.base()), area);
    if area.height == 0 || area.width == 0 {
        return;
    }
    let busy = app.loading[crate::app::View::Repository.index()];
    let tabs = mode_controls(workspace.mode);
    let graph_status = (!crate::repository_graph_ui_v1::controls(workspace, busy).is_empty())
        .then(|| crate::repository_graph_ui_v1::horizontal_status(workspace));
    // Row 0: the mode tabs. The graph pan position sits at its right edge when it fits.
    crate::ui_chrome::tab_strip(frame, app, Rect::new(area.x, area.y, area.width, 1), &tabs);
    let tabs_width = tabs
        .iter()
        .map(|tab| crate::editor::display_width(&format!(" {} {} ", tab.key, tab.label)) + 1)
        .sum::<usize>();
    let graph_status = graph_status.and_then(|status| {
        let width = crate::editor::display_width(&status);
        if tabs_width + 1 + width <= usize::from(area.width) {
            let width = width as u16;
            frame.render_widget(
                Paragraph::new(status).style(theme.style(SemanticToken::Muted)),
                Rect::new(area.right() - width, area.y, width, 1),
            );
            None
        } else {
            Some(status)
        }
    });
    let Some(workspace) = app.workspace.as_ref() else {
        return;
    };
    if area.height >= 3 {
        if let Some(commit) = &workspace.selected_commit {
            let author =
                crate::git_actor_labels_v1::display_identity(&commit.author, GitRole::Author);
            let committer =
                crate::git_actor_labels_v1::display_identity(&commit.committer, GitRole::Committer);
            let (author_area, committer_area) = if area.width >= 100 {
                let half = area.width / 2;
                (
                    Rect::new(area.x, area.y + 1, half, 2),
                    Rect::new(area.x + half, area.y + 1, area.width - half, 2),
                )
            } else {
                (
                    Rect::new(area.x, area.y + 1, area.width, 1),
                    Rect::new(area.x, area.y + 2, area.width, 1),
                )
            };
            let report = app
                .local_commit_actors
                .report_for_workspace(workspace)
                .map(std::sync::Arc::as_ref);
            crate::repository_commit_actor_ui_v1::draw_role(
                frame,
                author_area,
                GitRole::Author,
                &author,
                report,
                &app.avatar_states,
                theme,
            );
            crate::repository_commit_actor_ui_v1::draw_role(
                frame,
                committer_area,
                GitRole::Committer,
                &committer,
                report,
                &app.avatar_states,
                theme,
            );
            app.hits.push(HitRegion {
                rect: author_area,
                action: Action::RepositoryRow(RowAction::Actor(GitRole::Author)),
            });
            app.hits.push(HitRegion {
                rect: committer_area,
                action: Action::RepositoryRow(RowAction::Actor(GitRole::Committer)),
            });
        } else {
            frame.render_widget(
                Paragraph::new("No saved version yet — Git actor identity is unavailable")
                    .style(theme.style(SemanticToken::Muted)),
                Rect::new(area.x, area.y + 1, area.width, 1),
            );
        }
    }
    if area.height >= 4 {
        if let Some(source) = &app.repository_source_identity {
            // Full 40/64 IDs remain readable independently of long directories.
            for (index, row) in source.compact().iter().take(2).enumerate() {
                let y = area.y + 3 + index as u16;
                if y < area.bottom() {
                    frame.render_widget(
                        Paragraph::new(row.text.as_str()).style(theme.style(row.role)),
                        Rect::new(area.x, y, area.width, 1),
                    );
                }
            }
        } else if let Some(commit) = &workspace.selected_commit {
            frame.render_widget(
                Paragraph::new(format!("Saved: {}", commit.id))
                    .style(theme.style(SemanticToken::Reference)),
                Rect::new(area.x, area.y + 3, area.width, 1),
            );
        }
    }
    let source_rows = if app.repository_source_identity.is_some() {
        1u16
    } else {
        0
    };
    let history_notice = workspace.status.has_history_overrides();
    if area.height >= 5 + source_rows && history_notice {
        frame.render_widget(
            Paragraph::new(format!(
                "Original snapshots • replacements: {} • legacy grafts: {}",
                workspace.status.replacement_refs,
                if workspace.status.grafted {
                    "present"
                } else {
                    "absent"
                }
            ))
            .style(theme.style(SemanticToken::Changed)),
            Rect::new(area.x, area.y + 4 + source_rows, area.width, 1),
        );
    }
    let mut content_offset =
        ((if history_notice { 5u16 } else { 4u16 }) + source_rows).min(area.height);
    if workspace.actor_profile.is_some() {
        let control_rows = (if area.width < 100 { 2u16 } else { 1u16 })
            .min(area.height.saturating_sub(content_offset));
        app.local_actor_layout.controls(
            frame,
            Rect::new(area.x, area.y + content_offset, area.width, control_rows),
            &app.local_commit_actors,
            theme,
        );
        app.hits
            .extend(app.local_actor_layout.hits().iter().map(|hit| HitRegion {
                rect: hit.rect,
                action: Action::LocalActor(hit.action.clone()),
            }));
        content_offset += control_rows;
    }
    if let Some(status) = graph_status.filter(|_| content_offset < area.height) {
        frame.render_widget(
            Paragraph::new(status).style(theme.style(SemanticToken::Muted)),
            Rect::new(area.x, area.y + content_offset, area.width, 1),
        );
        content_offset += 1;
    }
    if let Some(tree) = workspace
        .tree_navigation_v1
        .as_ref()
        .filter(|tree| tree.visible_for(workspace))
    {
        if let Some(strip) = workspace
            .tree_breadcrumbs_v2
            .as_ref()
            .filter(|strip| std::sync::Arc::ptr_eq(strip.source(), tree))
        {
            let plan = strip.layout(Rect::new(
                area.x,
                area.y + content_offset,
                area.width,
                area.height.saturating_sub(content_offset),
            ));
            for cell in &plan.cells {
                let action = match &cell.target {
                    crate::repository_tree_viewport_v2::TreeCrumbTargetV2::Directory(action) => {
                        Action::RepositoryRow(RowAction::Tree(action.clone()))
                    }
                    crate::repository_tree_viewport_v2::TreeCrumbTargetV2::Commands => {
                        Action::OriginalTreeViewportV2(
                            crate::repository_tree_viewport_v2::TreeViewportActionV2::Commands,
                        )
                    }
                };
                let style = crate::repository_tree_viewport_v2::focused_style_v2(
                    theme,
                    SemanticToken::Reference,
                    false,
                    !app.repository_content_focused && app.hits.len() == app.focus,
                );
                frame.render_widget(Paragraph::new(strip.label(cell)).style(style), cell.rect);
                app.hits.push(HitRegion {
                    rect: cell.rect,
                    action,
                });
            }
            content_offset += plan.used_rows;
        } else {
            content_offset += crate::repository_tree_jump_ui_v1::draw_breadcrumbs(
                frame,
                tree,
                &mut app.hits,
                Rect::new(
                    area.x,
                    area.y + content_offset,
                    area.width,
                    area.height.saturating_sub(content_offset),
                ),
                theme,
                if app.repository_content_focused {
                    usize::MAX
                } else {
                    app.focus
                },
            );
        }
    }
    // A source-bound title is fixed while its body scrolls. Read a single
    // already prepared row; never scan/copy the comparison or resolve paths.
    if (workspace
        .original_diff_side_v1
        .as_ref()
        .is_some_and(|side| side.visible_for(workspace))
        || workspace
            .original_comparison_v1
            .as_ref()
            .is_some_and(|source| source.visible_for(workspace)))
        && area.height.saturating_sub(content_offset) > 1
    {
        if let Some(title) = workspace.display_rows_v1().first() {
            frame.render_widget(
                Paragraph::new(title.label.as_str()).style(theme.style(SemanticToken::Reference)),
                Rect::new(area.x, area.y + content_offset, area.width, 1),
            );
            content_offset += 1;
        }
    }
    if overview_table_v91(workspace) {
        let body = Rect::new(
            area.x,
            area.y + content_offset,
            area.width,
            area.height.saturating_sub(content_offset),
        );
        if crate::repository_panels_v93::active(app) {
            crate::repository_panels_v93::draw(frame, app, body);
        } else {
            draw_overview_v91(frame, app, body);
        }
        return;
    }
    if crate::history::detail::drilldown(workspace) && app.remote_repository.is_none() {
        let body = Rect::new(
            area.x,
            area.y + content_offset,
            area.width,
            area.height.saturating_sub(content_offset),
        );
        crate::history::detail::draw_full(frame, app, body);
        return;
    }
    if history_table::active(workspace) {
        let body = Rect::new(
            area.x,
            area.y + content_offset,
            area.width,
            area.height.saturating_sub(content_offset),
        );
        history_table::draw(frame, app, body);
        return;
    }
    let visible_rows = usize::from(area.height.saturating_sub(content_offset));
    // A resize or newly installed graph changes the real body height after the
    // worker used the previous view's height. Keep the actual selected member
    // visible, without undoing deliberate scrolling on unchanged geometry.
    if workspace.graph_visible_v1()
        && app.repository_content_focused
        && visible_rows != app.repository_visible_rows
        && visible_rows != 0
    {
        if app.repository_list.selected < app.repository_list.top {
            app.repository_list.top = app.repository_list.selected;
        } else if app.repository_list.selected
            >= app.repository_list.top.saturating_add(visible_rows)
        {
            app.repository_list.top = app.repository_list.selected + 1 - visible_rows;
        }
    }
    app.repository_visible_rows = visible_rows;
    for (visible, index) in (app.repository_list.top..workspace.display_rows_v1().len())
        .take(app.repository_visible_rows)
        .enumerate()
    {
        let row = &workspace.display_rows_v1()[index];
        let rect = Rect::new(
            area.x,
            area.y + content_offset + visible as u16,
            area.width,
            1,
        );
        let token = SemanticToken::from(row.role);
        let range_selected = matches!(&row.action, Some(RowAction::DiffLine(anchor)) if app.repository_anchor_selected(anchor));
        let style = if range_selected
            || (index == app.repository_list.selected && app.repository_content_focused)
        {
            theme.selected(token)
        } else {
            theme.style(token)
        };
        let line = if let Some(line) =
            crate::repository_graph_ui_v1::line(workspace, index, theme, usize::from(area.width))
        {
            line
        } else {
            Line::from(safe_text(&row.label))
        };
        let line = if row.label == "GitHub account association: unavailable" {
            workspace.actor_profile.and_then(|role| app.local_commit_actors.report_for_workspace(workspace)
                .map(|report| match report.role(role) {
                    crate::providers::gh::LocalCommitRoleReport::Reported(_) =>
                        Line::from("Read-only host account report available • F7 details; not repository mapping"),
                    crate::providers::gh::LocalCommitRoleReport::Unavailable(_) =>
                        Line::from("Host account unreported for this role • original local Git identity remains"),
                })).unwrap_or(line)
        } else {
            line
        };
        frame.render_widget(Paragraph::new(line).style(style), rect);
        if let Some(action) = &row.action {
            let action = if let RowAction::FileProvenance(file_action) = action {
                let reason = if app.loading[crate::app::View::Repository.index()] {
                    Some("Wait for the current repository read, or cancel it first".into())
                } else {
                    workspace.provenance_available(file_action).err()
                };
                reason.map_or(Action::RepositoryRow(action.clone()), Action::Explain)
            } else if let RowAction::Local(command) = action {
                Action::LocalCommand(*command)
            } else {
                Action::RepositoryRow(action.clone())
            };
            app.hits.push(HitRegion { rect, action });
        }
    }
}

/// Overview's recent commits as aligned columns (TASK-91), drawn in the Recent commits panel
/// (TASK-93). The full id and the UTC times stay in the Saved and actor detail rows, the DATE
/// column and the commit preview.
pub const OVERVIEW_COLUMNS: [crate::ui_table::Column; 5] = [
    crate::ui_table::Column::new("COMMIT", 7).priority(1),
    crate::ui_table::Column::new("SUBJECT", 12)
        .flex(3)
        .priority(0),
    crate::ui_table::Column::new("AUTHOR", 6)
        .flex(1)
        .priority(3)
        .actor(),
    crate::ui_table::Column::new("AGE", 3).priority(2).right(),
    crate::ui_table::Column::new("DATE", 10).priority(4),
];

/// The plain Overview list: no actor profile, preview or file history replaces its rows; it is
/// drawn as the Repository panels (TASK-93).
pub(crate) fn overview_table_v91(workspace: &Workspace) -> bool {
    workspace.mode == RepositoryMode::Overview
        && workspace.actor_profile.is_none()
        && workspace.preview.is_none()
        && workspace.file_provenance.is_none()
        && std::ptr::eq(workspace.display_rows_v1(), workspace.rows.as_slice())
}

/// One Overview row as a table row: commit cells, or prose from the first column.
pub fn overview_row_v91(
    row: &crate::workspace::DisplayRow,
    now: u64,
) -> crate::ui_table::TableRow<RowAction> {
    use crate::ui_table::{Cell, TableRow};
    let tone = SemanticToken::from(row.role);
    let table_row = match &row.cells {
        Some(commit) => {
            let age = u64::try_from(commit.authored)
                .map(|authored| crate::ui_table::age_from_seconds(now.saturating_sub(authored)))
                .unwrap_or_else(|_| crate::ui_table::NONE.into());
            TableRow::cells(vec![
                Cell::styled(
                    crate::ui_table::short_sha(&commit.id),
                    crate::theme::Role::Commit,
                ),
                commit.subject.as_str().into(),
                Cell::styled(commit.actors.as_str(), crate::theme::Role::TextMuted),
                Cell::styled(age, crate::theme::Role::TextMuted),
                Cell::styled(
                    crate::git_actor_labels_v1::utc_date(commit.authored),
                    crate::theme::Role::TextMuted,
                ),
            ])
            // One local actor shows its initials chip and name, like actors in the Inbox and
            // Actions tables; author and committer that differ keep both roles as text.
            .with_actor(commit.actor.as_ref().map(|actor| {
                crate::identity::ActorIdentity::local(actor, crate::identity::GitRole::Author)
            }))
        }
        None => TableRow::text(row.label.as_str()),
    };
    table_row.with_tone(tone).with_action(row.action.clone())
}

/// A plain Overview list that is not the Overview listing itself (a conflict or stash
/// detail opened from it) keeps the single table.
fn draw_overview_v91(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(workspace) = app.workspace.as_ref() else {
        return;
    };
    let theme = app.theme();
    let rows = workspace.display_rows_v1();
    let now = crate::storage::now();
    let table_rows = rows
        .iter()
        .map(|row| overview_row_v91(row, now))
        .collect::<Vec<_>>();
    let natural = crate::ui_table::natural_widths(&OVERVIEW_COLUMNS, &table_rows, theme.mode);
    let table = crate::ui_table::Table {
        columns: &OVERVIEW_COLUMNS,
        header: false,
        focused: app.repository_content_focused,
        glyphs: false,
        empty: None,
    };
    // The wheel moves the viewport without the selection, so the list keeps `app.scroll`;
    // keyboard movement already scrolls the selected row into view.
    let top = app.repository_list.top.min(rows.len().saturating_sub(1));
    let selected =
        (app.repository_list.selected < rows.len()).then_some(app.repository_list.selected);
    let rendered = crate::ui_table::draw_table_at(
        frame,
        area,
        &table,
        &natural,
        table_rows.len(),
        |index| table_rows[index].clone(),
        selected,
        top,
        theme,
    );
    let busy = app.loading[crate::app::View::Repository.index()];
    let mut hits = Vec::new();
    for hit in rendered.hits {
        let Some(action) = hit.action else {
            continue;
        };
        let action = match action {
            RowAction::FileProvenance(file_action) => {
                let reason = if busy {
                    Some("Wait for the current repository read, or cancel it first".into())
                } else {
                    workspace.provenance_available(&file_action).err()
                };
                reason.map_or(
                    Action::RepositoryRow(RowAction::FileProvenance(file_action)),
                    Action::Explain,
                )
            }
            RowAction::Local(command) => Action::LocalCommand(command),
            action => Action::RepositoryRow(action),
        };
        hits.push(HitRegion {
            rect: hit.rect,
            action,
        });
    }
    app.repository_list.top = top;
    app.repository_visible_rows = rendered.visible_rows;
    app.hits.extend(hits);
}
