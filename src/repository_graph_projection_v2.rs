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
        // Two compact heading rows: where you are, and what the selection is.
        // Lane meaning and paging details stay in the Legend (l).
        builder.push(
            format!(
                "Visible commits {}–{} of {} loaded (limit 10000) • < > page • N load more",
                start + 1,
                end,
                state.loaded()
            ),
            SemanticRole::Reference,
            None,
            Vec::new(),
        )?;
        builder.push(
            graph_explanation_v1(selected),
            SemanticRole::Reference,
            None,
            Vec::new(),
        )?;
        let lanes = state.lane_rows_v991(start, end)?;
        // One graph column for the window: every message starts at the same cell.
        let graph_width = lanes.iter().map(|row| row.cells.len()).max().unwrap_or(0);
        for (node, lane) in state.topology.rows()[start..end].iter().zip(&lanes) {
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
            }
            // Lane glyphs keep their lane's palette slot; blanks and the
            // padding stay in the pannable graph column (slot 0 is invisible
            // on a space).
            let mut paint = lane
                .spans()
                .into_iter()
                .map(|(text, slot)| GraphSpanV1 {
                    text,
                    color: Some(slot.unwrap_or(0)),
                })
                .collect::<Vec<_>>();
            paint.push(GraphSpanV1 {
                text: " ".repeat(graph_width.saturating_sub(lane.cells.len()) + 2),
                color: Some(0),
            });
            let label = format!(
                "{}{} {}{} • {}",
                if is_selected { "▶ selected " } else { "" },
                &commit.id[..8],
                commit.subject,
                short_names_v1(commit),
                crate::git_actor_labels_v1::commit_summary(commit),
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
