//! Repository rendering consumes prepared identities, objects and graph rows only.
#[cfg(test)]
#[path = "repository_diff_header_v1_tests.rs"]
mod diff_header_v1_tests;
mod file_history;
#[cfg(test)]
#[path = "repository_toolbar_v2_tests.rs"]
mod toolbar_v2_tests;
use crate::{
    app::{safe_text, Action, App, HitRegion},
    identity::GitRole,
    theme::SemanticToken,
    workspace::{RepositoryMode, RowAction},
};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
    Frame,
};

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
    let mut x = area.x;
    for (mode, label, key) in RepositoryMode::ALL {
        let label = format!(
            "[{key} {}]",
            label.split_whitespace().next().unwrap_or(label)
        );
        let rect = Rect::new(
            x.min(area.right()),
            area.y,
            (label.len() as u16).min(area.right().saturating_sub(x)),
            1,
        );
        frame.render_widget(
            Paragraph::new(label).style(if mode == workspace.mode {
                theme.selected(SemanticToken::Reference)
            } else {
                theme.base()
            }),
            rect,
        );
        if !rect.is_empty() {
            app.hits.push(HitRegion {
                rect,
                action: Action::RepositoryMode(mode),
            });
        }
        x = x.saturating_add(rect.width + 1);
    }
    if area.height < 2 {
        return;
    }
    let mut x = area.x;
    for (label, action) in [
        (
            "[l Legend]",
            Action::RepositoryRow(if workspace.graph_visible_v1() {
                RowAction::Graph(crate::repository_graph_v1::GraphActionV1::Legend)
            } else {
                RowAction::Legend
            }),
        ),
        (
            "[a Author]",
            Action::RepositoryRow(RowAction::Actor(GitRole::Author)),
        ),
        (
            "[c Committer]",
            Action::RepositoryRow(RowAction::Actor(GitRole::Committer)),
        ),
        ("[m Local operations]", Action::LocalMenu),
        (
            "[J Jump]",
            app.workspace.as_ref().map_or_else(
                || Action::Explain("Open an original immutable Tree first".into()),
                |workspace| {
                    if app.loading[crate::app::View::Repository.index()] {
                        Action::Explain(
                            "Wait for the current repository read or cancel first".into(),
                        )
                    } else {
                        crate::git::OriginalTreeIntentV1::available(workspace).map_or_else(
                            Action::Explain,
                            |_| {
                                Action::OriginalTreeV1(
                                    crate::repository_tree_jump_v1::OriginalTreeJumpActionV1::Open,
                                )
                            },
                        )
                    }
                },
            ),
        ),
    ] {
        let rect = Rect::new(
            x.min(area.right()),
            area.y + 1,
            (label.len() as u16).min(area.right().saturating_sub(x)),
            1,
        );
        frame.render_widget(
            Paragraph::new(label).style(theme.style(SemanticToken::Actor)),
            rect,
        );
        if !rect.is_empty() {
            app.hits.push(HitRegion { rect, action });
        }
        x = x.saturating_add(rect.width + if area.width < 100 { 1 } else { 2 });
    }
    if matches!(
        workspace.mode,
        RepositoryMode::Working | RepositoryMode::Staged
    ) {
        let label = "[h Hunk]";
        let rect = Rect::new(
            x.min(area.right()),
            area.y + 1,
            (label.len() as u16).min(area.right().saturating_sub(x)),
            1,
        );
        frame.render_widget(
            Paragraph::new(label).style(theme.style(SemanticToken::Changed)),
            rect,
        );
        if !rect.is_empty() {
            app.hits.push(HitRegion {
                rect,
                action: Action::RepositoryHunk,
            });
        }
    }
    if area.height >= 3 {
        let detail = workspace
            .file_provenance
            .as_ref()
            .is_some_and(|state| state.view == crate::workspace::ProvenanceView::Detail);
        let mut x = area.x;
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
        if let Some(side) = diff_side {
            let label = "[u Back to comparison]";
            let rect = Rect::new(area.x, area.y + 2, (label.len() as u16).min(area.width), 1);
            frame.render_widget(
                Paragraph::new(label).style(theme.style(SemanticToken::Reference)),
                rect,
            );
            if !rect.is_empty() {
                app.hits.push(HitRegion {
                    rect,
                    action: Action::RepositoryRow(RowAction::OriginalDiffReturn(
                        side.return_ticket(),
                    )),
                });
            }
            x = rect.right().saturating_add(1);
        } else if !detail {
            let rect = Rect::new(area.x, area.y + 2, 11.min(area.width), 1);
            frame.render_widget(
                Paragraph::new("[C Compare]").style(theme.style(SemanticToken::Changed)),
                rect,
            );
            app.hits.push(HitRegion {
                rect,
                action: Action::Compare,
            });
            x = rect.right().saturating_add(1);
        }
        if saved_comparison {
            let label = "[f Changed files]";
            let rect = Rect::new(
                x.min(area.right()),
                area.y + 2,
                (label.len() as u16).min(area.right().saturating_sub(x)),
                1,
            );
            frame.render_widget(
                Paragraph::new(label).style(theme.style(SemanticToken::Changed)),
                rect,
            );
            if !rect.is_empty() {
                app.hits.push(HitRegion {
                    rect,
                    action: Action::OriginalDiffSidesV1(
                        crate::repository_diff_sides_v1::OriginalDiffActionV1::Open,
                    ),
                });
            }
            x = rect.right().saturating_add(1);
        }
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
        if saved_comparison || mutable_comparison {
            let label = "[v Native diff]";
            let rect = Rect::new(
                x.min(area.right()),
                area.y + 2,
                (label.len() as u16).min(area.right().saturating_sub(x)),
                1,
            );
            frame.render_widget(
                Paragraph::new(label).style(theme.style(SemanticToken::Changed)),
                rect,
            );
            if !rect.is_empty() {
                app.hits.push(HitRegion {
                    rect,
                    action: if mutable_comparison {
                        Action::NativeMutableDiffV2(
                            crate::repository_native_diff_ui_v1::NativeDiffActionV1::Open,
                        )
                    } else {
                        Action::NativeDiffV1(
                            crate::repository_native_diff_ui_v1::NativeDiffActionV1::Open,
                        )
                    },
                });
            }
            x = rect.right().saturating_add(1);
        }
        for control in
            file_history::controls(workspace, app.loading[crate::app::View::Repository.index()])
                .into_iter()
                .filter(|_| {
                    !saved_comparison
                        && (!workspace.graph_retention_visible_v2()
                            || workspace.graph_view_v1
                                == crate::repository_graph_v1::GraphViewV1::Timeline)
                })
                .filter(|control| diff_side.is_none() || control.label != "[u File back]")
        {
            let rect = Rect::new(
                x.min(area.right()),
                area.y + 2,
                (control.label.len() as u16).min(area.right().saturating_sub(x)),
                1,
            );
            frame.render_widget(
                Paragraph::new(control.label).style(if control.disabled {
                    theme.style(SemanticToken::Muted)
                } else if control.selected {
                    theme.selected(SemanticToken::Reference)
                } else {
                    theme.style(SemanticToken::Reference)
                }),
                rect,
            );
            if !rect.is_empty() {
                app.hits.push(HitRegion {
                    rect,
                    action: control.action,
                });
            }
            x = x.saturating_add(rect.width + 1);
        }
    }
    if area.height >= 5 {
        if let Some(commit) = &workspace.selected_commit {
            let author =
                crate::git_actor_labels_v1::display_identity(&commit.author, GitRole::Author);
            let committer =
                crate::git_actor_labels_v1::display_identity(&commit.committer, GitRole::Committer);
            let (author_area, committer_area) = if area.width >= 100 {
                let half = area.width / 2;
                (
                    Rect::new(area.x, area.y + 3, half, 2),
                    Rect::new(area.x + half, area.y + 3, area.width - half, 2),
                )
            } else {
                (
                    Rect::new(area.x, area.y + 3, area.width, 1),
                    Rect::new(area.x, area.y + 4, area.width, 1),
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
                Rect::new(area.x, area.y + 3, area.width, 1),
            );
        }
    }
    if area.height >= 6 {
        if let Some(source) = &app.repository_source_identity {
            // Full 40/64 IDs remain readable independently of long directories.
            for (index, row) in source.compact().iter().take(2).enumerate() {
                let y = area.y + 5 + index as u16;
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
                Rect::new(area.x, area.y + 5, area.width, 1),
            );
        }
    }
    let source_rows = if app.repository_source_identity.is_some() {
        1u16
    } else {
        0
    };
    let history_notice = workspace.status.has_history_overrides();
    if area.height >= 7 + source_rows && history_notice {
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
            Rect::new(area.x, area.y + 6 + source_rows, area.width, 1),
        );
    }
    let mut content_offset =
        ((if history_notice { 7u16 } else { 6u16 }) + source_rows).min(area.height);
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
    content_offset += crate::repository_graph_ui_v1::draw_controls(
        frame,
        workspace,
        &mut app.hits,
        Rect::new(
            area.x,
            area.y + content_offset,
            area.width,
            area.height.saturating_sub(content_offset),
        ),
        theme,
        app.loading[crate::app::View::Repository.index()],
        crate::repository_graph_ui_v1::GraphControlFocusV2 {
            hit: app.focus,
            content_focused: app.repository_content_focused,
        },
    );
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
    let visible_rows = usize::from(area.height.saturating_sub(content_offset));
    // A resize or newly installed graph changes the real body height after the
    // worker used the previous view's height. Keep the actual selected member
    // visible, without undoing deliberate scrolling on unchanged geometry.
    if workspace.graph_visible_v1()
        && app.repository_content_focused
        && visible_rows != app.repository_visible_rows
        && visible_rows != 0
    {
        if app.selected < app.scroll {
            app.scroll = app.selected;
        } else if app.selected >= app.scroll.saturating_add(visible_rows) {
            app.scroll = app.selected + 1 - visible_rows;
        }
    }
    app.repository_visible_rows = visible_rows;
    for (visible, index) in (app.scroll..workspace.display_rows_v1().len())
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
        let style = if range_selected || (index == app.selected && app.repository_content_focused) {
            theme.selected(token)
        } else {
            theme.style(token)
        };
        let line = if let Some(line) =
            crate::repository_graph_ui_v1::line(workspace, index, theme, usize::from(area.width))
        {
            line
        } else if workspace.mode == RepositoryMode::Graph {
            if let Some(RowAction::Commit(id)) = &row.action {
                if let Some(graph_row) =
                    workspace.graph.rows.iter().find(|row| &row.commit.id == id)
                {
                    let mut spans = graph_row
                        .colors_before
                        .iter()
                        .enumerate()
                        .map(|(index, color)| {
                            Span::styled(
                                if index == graph_row.column {
                                    "● "
                                } else {
                                    "│ "
                                },
                                theme.graph(*color),
                            )
                        })
                        .collect::<Vec<_>>();
                    if graph_row.commit.parents.len() > 1 {
                        spans.push(Span::styled("╲", theme.style(SemanticToken::Diverged)));
                    }
                    spans.push(Span::raw(safe_text(
                        row.label
                            .strip_prefix(&graph_row.glyphs)
                            .unwrap_or(&row.label),
                    )));
                    Line::from(spans)
                } else {
                    Line::from(safe_text(&row.label))
                }
            } else {
                Line::from(safe_text(&row.label))
            }
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
            } else {
                Action::RepositoryRow(action.clone())
            };
            app.hits.push(HitRegion { rect, action });
        }
    }
}
