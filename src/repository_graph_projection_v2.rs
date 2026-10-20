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
        Self::prepare_window_v993(state, selected, view, start, None)
    }
    /// The timeline window, keeping only the `visible` loaded commits when History hides
    /// non-matching ones (TASK-99.3). The selected commit is always shown.
    pub(crate) fn prepare_window_v993(
        state: &OriginalGraphStateV1,
        selected: &str,
        view: GraphViewV1,
        start: usize,
        visible: Option<&[bool]>,
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
        // One summary row: read-only, how much is loaded and which window shows. The History
        // table paints it as its status line; the selected commit's explanation stays in
        // Details (D) and lane meaning in the Legend (l).
        builder.push(
            history_summary_v992(state, start, end),
            SemanticRole::Reference,
            None,
            Vec::new(),
        )?;
        let shown = visible.map(|flags| {
            let selected_node = state
                .loaded_member_v2(&selected.id)
                .map_or(usize::MAX, |member| member.node as usize);
            flags
                .iter()
                .enumerate()
                .map(|(index, flag)| *flag || index == selected_node)
                .collect::<Vec<_>>()
        });
        let lanes = match &shown {
            Some(shown) => state.lane_rows_v993(start, end, shown)?,
            None => state.lane_rows_v991(start, end)?,
        };
        // One graph column for the window: every message starts at the same cell.
        let graph_width = lanes.iter().map(|row| row.cells.len()).max().unwrap_or(0);
        let nodes = state.topology.rows()[start..end]
            .iter()
            .enumerate()
            .filter(|(offset, _)| shown.as_ref().is_none_or(|shown| shown[start + offset]))
            .map(|(_, node)| node);
        for (node, lane) in nodes.zip(&lanes) {
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
            let observed = state
                .topology
                .observed(&state.pages, node.member)
                .ok_or("Compact graph actual member lost")?;
            let label = format!(
                "{}{} {}{} • {}",
                if is_selected { "▶ selected " } else { "" },
                &commit.id[..8],
                commit.subject,
                short_names_v1(observed),
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

/// `Read-only · 230 commits loaded · showing 201–230 · < > page · scroll for more`, plus the
/// refs past the 4096-name cap when there are any: they are neither named nor walked.
fn history_summary_v992(state: &OriginalGraphStateV1, start: usize, end: usize) -> String {
    let loaded = state.loaded();
    let mut text = format!(
        "Read-only · {loaded} {} loaded",
        if loaded == 1 { "commit" } else { "commits" }
    );
    if loaded > GRAPH_PROJECTION_COMMITS_V2 {
        text.push_str(&format!(" · showing {}–{end} · < > page", start + 1));
    }
    match state.coverage {
        GraphCoverageV1::MoreAvailable => text.push_str(" · scroll for more"),
        GraphCoverageV1::Capped => text.push_str(" · 10000-commit limit"),
        _ => {}
    }
    let overflow = state.last_page().ref_overflow();
    if overflow > 0 {
        text.push_str(&format!(
            " · {overflow} refs past the 4096-ref cap not shown or walked"
        ));
    }
    text
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
