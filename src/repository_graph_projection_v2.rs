//! SAME shared borrowed DisplayRow renderer, bounded worker preparation only.
//! Full actual topology remains retained; this range is not full history.
use super::*;
pub(crate) const GRAPH_PROJECTION_COMMITS_V2: usize = 200;

impl GraphPresentationV1 {
    pub(crate) fn prepare_window_v2(
        state: &OriginalGraphStateV1,
        selected: &str,
        view: GraphViewV1,
        start: usize,
    ) -> Result<Arc<Self>, String> {
        if view != GraphViewV1::Timeline {
            let result = Self::prepare(state, selected, view)?;
            if state
                .retained_bytes
                .checked_add(result.retained_bytes_v2())
                .is_none_or(|n| n > MAX_RETAINED)
            {
                return Err(
                    "Graph raw+compact+actual prepared details exceeds unchanged8MiB".into(),
                );
            }
            return Ok(result);
        }
        if start >= state.loaded() || !start.is_multiple_of(GRAPH_PROJECTION_COMMITS_V2) {
            return Err("Graph projection range is not a loaded200-record window".into());
        }
        let selected = state
            .selected_commit(selected)
            .ok_or("Graph projection selected original member absent")?;
        let end = (start + GRAPH_PROJECTION_COMMITS_V2).min(state.loaded());
        let ancestors = state.topology.ancestors(&selected.id)?;
        let mut builder = ProjectionBuilderV1 {
            limit: MAX_RETAINED.saturating_sub(state.retained_bytes),
            ..Default::default()
        };
        builder.push(
            format!(
                "Visible commits {}–{} • {} saved versions loaded /10000",
                start + 1,
                end,
                state.loaded()
            ),
            SemanticRole::Reference,
            None,
            Vec::new(),
        )?;
        builder.push(
            format!("Original walk tip: {}", state.walk_tip()),
            SemanticRole::Reference,
            None,
            Vec::new(),
        )?;
        builder.push(
            "● commit • lines connect each commit to its parents".into(),
            SemanticRole::Reference,
            None,
            Vec::new(),
        )?;
        builder.push(
            "Showing up to 200 commits at a time • < > page through loaded history".into(),
            SemanticRole::Boundary,
            None,
            Vec::new(),
        )?;
        builder.push("Colors trace parent paths, never permanent branches. Branch names are as of graph load".into(),
            SemanticRole::Normal,None,Vec::new())?;
        builder.push(
            graph_explanation_v1(selected),
            SemanticRole::Reference,
            None,
            Vec::new(),
        )?;
        for node in &state.topology.rows()[start..end] {
            let commit = state
                .topology
                .commit(&state.pages, node.member)
                .ok_or("Compact graph actual member lost")?;
            let related = ancestors[node.member.node as usize];
            let is_selected = commit.id == selected.id;
            let role = if is_selected {
                SemanticRole::Head
            } else if related {
                SemanticRole::Reference
            } else {
                SemanticRole::Normal
            };
            let mut paint = state
                .topology
                .before(node)
                .iter()
                .enumerate()
                .map(|(col, id)| GraphSpanV1 {
                    text: if col == usize::from(node.column) {
                        "● "
                    } else {
                        "│ "
                    }
                    .into(),
                    color: state.topology.color(*id),
                })
                .collect::<Vec<_>>();
            let label = format!(
                "{}{} {} • {}{}",
                if is_selected {
                    "▶ selected "
                } else if related {
                    "↑ ancestor "
                } else {
                    "  "
                },
                &commit.id[..8],
                commit.subject,
                crate::git_actor_labels_v1::commit_summary(commit),
                bounded_names_v1(commit)
            );
            paint.push(GraphSpanV1 {
                text: label.clone(),
                color: None,
            });
            let row = builder.push(
                label,
                role,
                Some(RowAction::Commit(commit.id.clone())),
                paint,
            )?;
            builder.commit_rows.insert(commit.id.clone(), row);
            if is_selected {
                builder.focus = Some(row);
            }
            let edges = state.topology.edges(node);
            if edges.len() != commit.parents.len() {
                return Err("Original ordered compact edge missing".into());
            }
            for (order, parent) in commit.parents.iter().enumerate() {
                let edge = &edges[order];
                if usize::from(edge.order) != order
                    || state.topology.id(&state.pages, edge.parent) != Some(parent.as_str())
                {
                    return Err("Original compact parent order/identity differs".into());
                }
                let from = usize::from(edge.from);
                let to = usize::from(edge.to);
                let columns = state
                    .topology
                    .before(node)
                    .len()
                    .max(state.topology.after(node).len());
                let mut paint = (0..columns)
                    .map(|column| GraphSpanV1 {
                        text: if column == from && from == to {
                            "│ "
                        } else if column == from && to > from {
                            "╲─"
                        } else if column == from {
                            "╱─"
                        } else if column == to {
                            "└ "
                        } else if column > from.min(to) && column < from.max(to) {
                            "──"
                        } else {
                            "  "
                        }
                        .into(),
                        color: Some(usize::from(edge.color)),
                    })
                    .collect::<Vec<_>>();
                let loaded = state.selected_commit(parent).is_some();
                let label = format!(
                    "↳ parent {} → {}{}",
                    order + 1,
                    &parent[..8],
                    if loaded {
                        ""
                    } else {
                        " • outside loaded walk"
                    }
                );
                paint.push(GraphSpanV1 {
                    text: label.clone(),
                    color: None,
                });
                builder.push(
                    label,
                    SemanticRole::Boundary,
                    loaded.then(|| RowAction::Commit(parent.clone())),
                    paint,
                )?;
            }
        }
        builder.push(
            state.coverage.label().into(),
            SemanticRole::Boundary,
            None,
            Vec::new(),
        )?;
        let result = builder.finish()?;
        // Actual allocated row/action/paint/map capacities, not label lengths
        // alone. Sparse octopus metadata may honestly refuse this projection.
        if state
            .retained_bytes
            .checked_add(result.retained_bytes_v2())
            .is_none_or(|n| n > MAX_RETAINED)
        {
            return Err(
                "Graph raw+compact+actual prepared projection exceeds unchanged8MiB".into(),
            );
        }
        Ok(result)
    }
    pub(crate) fn retained_bytes_v2(&self) -> usize {
        use std::mem::size_of;
        let rows = self
            .rows
            .iter()
            .map(|r| {
                size_of::<DisplayRow>()
                    + r.label.capacity()
                    + match &r.action {
                        Some(RowAction::Commit(id)) => id.capacity(),
                        _ => 0,
                    }
            })
            .sum::<usize>();
        let paint = self
            .paint
            .iter()
            .map(|line| {
                32 + line.len() * size_of::<GraphSpanV1>()
                    + line.iter().map(|s| s.text.capacity()).sum::<usize>()
            })
            .sum::<usize>();
        size_of::<Self>()
            + rows
            + paint
            + 32
            + self.paint.len() * size_of::<Arc<[GraphSpanV1]>>()
            + self.commit_rows.capacity() * (size_of::<(String, usize)>() + 8)
            + self.commit_rows.keys().map(String::capacity).sum::<usize>()
            + self.navigation_rows.capacity() * (size_of::<(Vec<u8>, usize)>() + 8)
            + self
                .navigation_rows
                .keys()
                .map(Vec::capacity)
                .sum::<usize>()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum GraphWindowActionV2 {
    Newer,
    Older,
    First,
    Last,
}
impl OriginalGraphStateV1 {
    /// Pure worker operation over THIS immutable full state. No SQL/Git/gh.
    /// Result position selects a real global member, never a display-row ID.
    pub(crate) fn projection_position_v2(
        &self,
        old: &restore_v2::GraphPositionV2,
        action: GraphWindowActionV2,
    ) -> Result<restore_v2::GraphPositionV2, String> {
        self.snapshot_v2().validate_position(old)?;
        let start = match action {
            GraphWindowActionV2::Newer => old
                .projection_start
                .saturating_sub(GRAPH_PROJECTION_COMMITS_V2),
            GraphWindowActionV2::Older => (old.projection_start + GRAPH_PROJECTION_COMMITS_V2).min(
                (self.loaded() - 1) / GRAPH_PROJECTION_COMMITS_V2 * GRAPH_PROJECTION_COMMITS_V2,
            ),
            GraphWindowActionV2::First => 0,
            GraphWindowActionV2::Last => {
                (self.loaded() - 1) / GRAPH_PROJECTION_COMMITS_V2 * GRAPH_PROJECTION_COMMITS_V2
            }
        };
        if start == old.projection_start
            && !matches!(
                action,
                GraphWindowActionV2::First | GraphWindowActionV2::Last
            )
        {
            return Err("No more loaded projection records in this direction".into());
        }
        let member = self
            .member_at_v2(start)
            .ok_or("Graph projection first actual member unavailable")?;
        self.position_v2(
            &member.id,
            GraphViewV1::Timeline,
            start,
            old.horizontal,
            None,
        )
    }
}

#[cfg(test)]
#[path = "repository_graph_projection_v2_tests.rs"]
mod tests;
