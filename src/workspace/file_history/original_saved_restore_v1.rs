//! Fresh exact route/member restoration from the SAME actual original source.
//! Child of file_history so existing prepared rendering/parsers stay shared.
use super::*;
use crate::navigation_local_source_v1::{decode_mode, OriginalLocalRouteV1};

impl Workspace {
    pub(crate) fn restore_original_route_v1(
        repository: &GitRepository,
        origin: Arc<crate::git::OriginalLocalActorOriginV1>,
        route: &OriginalLocalRouteV1,
        cancellation: CancellationToken,
    ) -> Result<Self, String> {
        let mode = decode_mode(route.mode)?;
        let mut workspace = if let Some(graph) = &route.graph {
            let mut workspace = Self::load_from_repository_v1(
                repository,
                RepositoryMode::Tree,
                &graph.selected,
                None,
                Some(origin.clone()),
            )?
            .ok_or("Original Graph source unavailable")?;
            workspace.restore_graph_v1(repository, graph)?;
            workspace
        } else if let Some(provenance) = &route.provenance {
            let mut workspace = Self::open_search_file_from_repository_v1(
                repository,
                &provenance.path,
                Some(&provenance.revision),
                1,
                cancellation.clone(),
                Some(origin.clone()),
            )?;
            let mut state =
                FileProvenance::new(provenance.revision.clone(), provenance.path.clone());
            if provenance.view != 0 {
                let source = if provenance.selection.is_some() {
                    provenance.return_view
                } else {
                    provenance.view
                };
                match source {
                    1 => {
                        let offset = provenance
                            .history_offset
                            .ok_or("Original history page is unavailable")?;
                        state.history = Some(
                            repository
                                .file_history_page(
                                    &state.revision,
                                    &state.path,
                                    offset,
                                    HISTORY_PAGE,
                                )
                                .map_err(|error| error.to_string())?,
                        );
                        state.view = ProvenanceView::History;
                        workspace.render_history(repository, &state)?;
                    }
                    2 => {
                        if !workspace.load_blame(repository, &mut state, provenance.blame_start)? {
                            return Err(
                                "Original exact blame page is unavailable; no line invented".into(),
                            );
                        }
                        workspace.render_blame(repository, &state)?;
                    }
                    _ => return Err("Original nested provenance source is unavailable".into()),
                }
                if let Some(expected) = &provenance.selection {
                    if source == 1 {
                        let entry = state
                            .history
                            .as_ref()
                            .and_then(|page| {
                                page.entries
                                    .iter()
                                    .find(|entry| entry.commit.id == expected.commit)
                            })
                            .ok_or(
                                "Original history member no longer belongs to exact loaded page",
                            )?;
                        state.selection = Some(FileSelection {
                            commit: entry.commit.id.clone(),
                            path: entry.path.clone(),
                            original_line: None,
                            final_line: None,
                            boundary: false,
                            exists: entry.exists,
                        });
                        state.return_view = ProvenanceView::History;
                        state.view = ProvenanceView::Detail;
                        workspace.render_selection(repository, &state)?;
                    } else {
                        workspace.select_blame(
                            repository,
                            &mut state,
                            expected
                                .final_line
                                .ok_or("Original selected blame line unavailable")?,
                        )?;
                    }
                    if state
                        .selection
                        .as_ref()
                        .is_none_or(|actual| !expected.matches(actual))
                    {
                        return Err(
                            "Original responsible SHA/path/line/boundary changed; no retarget"
                                .into(),
                        );
                    }
                }
                workspace.file_provenance = Some(state);
                match provenance.view {
                    1..=3 => {}
                    4 => workspace.provenance_action_from_repository_v1(
                        repository,
                        FileAction::Graph,
                        cancellation.clone(),
                    )?,
                    5 => workspace.provenance_action_from_repository_v1(
                        repository,
                        FileAction::Diff,
                        cancellation.clone(),
                    )?,
                    6 => workspace.provenance_action_from_repository_v1(
                        repository,
                        FileAction::OriginalFile,
                        cancellation.clone(),
                    )?,
                    _ => return Err("Original file route is unsupported".into()),
                }
                if provenance.view == 5 {
                    if let Some(comparison) = &route.comparison {
                        if comparison.kind == 3
                            && comparison.parent.is_some_and(|parent| parent != 0)
                        {
                            workspace.open_from_repository_v1(
                                RowAction::ComparisonParent {
                                    commit: comparison.head.clone(),
                                    parent: comparison.parent.unwrap(),
                                },
                                cancellation.clone(),
                                Some(repository),
                            )?;
                        }
                    }
                }
            }
            workspace
        } else if let Some((revision, path, _)) = &route.preview {
            Self::open_search_file_from_repository_v1(
                repository,
                path,
                Some(revision),
                1,
                cancellation.clone(),
                Some(origin.clone()),
            )?
        } else if let Some(comparison) = &route.comparison {
            let mut workspace = Self::load_from_repository_v1(
                repository,
                RepositoryMode::Tree,
                &comparison.head,
                None,
                Some(origin.clone()),
            )?
            .ok_or("Original comparison source unavailable")?;
            let requested = match comparison.kind {
                1 => Comparison::Snapshots {
                    base: comparison
                        .base
                        .clone()
                        .ok_or("Original comparison base unavailable")?,
                    head: comparison.head.clone(),
                },
                2 => Comparison::BranchChanges {
                    base: comparison
                        .base
                        .clone()
                        .ok_or("Original proposed base unavailable")?,
                    head: comparison.head.clone(),
                },
                3 => Comparison::CommitChanges {
                    commit: comparison.head.clone(),
                    parent: comparison
                        .parent
                        .ok_or("Original selected parent unavailable")?,
                },
                _ => return Err("Original comparison kind unavailable".into()),
            };
            let result = repository
                .compare(&requested)
                .map_err(|error| error.to_string())?;
            if result.base != comparison.base
                || result.head.as_deref() != Some(comparison.head.as_str())
            {
                return Err("Original exact comparison endpoints changed".into());
            }
            workspace.rows.clear();
            workspace.push(&result.description, SemanticRole::Reference, None);
            workspace.rows.extend(
                diff_rows(&result.diff, DiffLayout::Unified, true)
                    .into_iter()
                    .map(|row| DisplayRow {
                        label: row.left.label,
                        role: row.left.role,
                        action: row.left.anchor.map(RowAction::DiffLine),
                        cells: None,
                    }),
            );
            workspace.comparison = Some(result);
            workspace.mode = mode;
            workspace.tree = None;
            workspace
        } else {
            Self::load_from_repository_v1(
                repository,
                mode,
                &route.revision,
                None,
                Some(origin.clone()),
            )?
            .ok_or("Original saved repository route unavailable")?
        };
        if route.provenance.is_none() && route.preview.is_none() && route.comparison.is_none() {
            for path in &route.expanded {
                workspace.open_from_repository_v1(
                    RowAction::Directory(path.clone()),
                    cancellation.clone(),
                    Some(repository),
                )?;
            }
        }
        if let Some((revision, path, blob)) = &route.preview {
            let preview = workspace
                .preview
                .as_ref()
                .ok_or("Original immutable preview unavailable")?;
            if preview.revision != *revision || preview.path != *path || preview.object_id != *blob
            {
                return Err("Original saved preview object/path changed; no substitute".into());
            }
        }
        if let Some(selected) = &route.selected {
            if workspace
                .selected_commit
                .as_ref()
                .is_none_or(|commit| commit.id != *selected)
            {
                if route
                    .provenance
                    .as_ref()
                    .is_some_and(|provenance| provenance.view != 4)
                    || route.provenance.as_ref().is_some_and(|provenance| {
                        provenance.view == 4
                            && !workspace
                                .graph
                                .rows
                                .iter()
                                .any(|row| row.commit.id == *selected)
                    })
                {
                    return Err("Original responsible commit/graph member unavailable".into());
                }
                workspace.open_from_repository_v1(
                    RowAction::Commit(selected.clone()),
                    cancellation.clone(),
                    Some(repository),
                )?;
            }
        }
        if let Some(expected) = &route.comparison {
            if workspace.comparison.as_ref().is_none_or(|actual| {
                actual.base != expected.base
                    || actual.head.as_deref() != Some(expected.head.as_str())
            }) {
                return Err("Original comparison endpoints are unavailable".into());
            }
        }
        if workspace.revision != route.revision {
            return Err("Original chosen saved revision changed; no HEAD fallback".into());
        }
        if let Some(expected) = route.mutable_source {
            if workspace.comparison.as_ref().and_then(|comparison| {
                comparison
                    .observed
                    .map(|observed| observed.navigation_identity())
            }) != Some(expected)
            {
                return Err(
                    "Original mutable files/index state changed; no old diff restored".into(),
                );
            }
        }
        if let Some(role) = route.role {
            let role = match role {
                0 => GitRole::Author,
                1 => GitRole::Committer,
                _ => return Err("Original Git role unavailable".into()),
            };
            workspace.open_from_repository_v1(
                RowAction::Actor(role),
                cancellation.clone(),
                None,
            )?;
        }
        if cancellation.is_cancelled() {
            return Err("Original route restoration cancelled".into());
        }
        if workspace
            .actor_origin
            .as_ref()
            .is_none_or(|actual| !Arc::ptr_eq(actual, &origin))
        {
            return Err("Original saved source was replaced during restore".into());
        }
        Ok(workspace)
    }
}
