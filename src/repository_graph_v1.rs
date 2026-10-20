//! Immutable prepared DAG state. Constructors consume real original Git pages;
//! nothing here authenticates a host or restores a completed read operation.
use crate::{
    git::{Commit, OriginalGraphCommitV1, OriginalGraphPageV1, OriginalLocalActorOriginV1},
    repository_view::SemanticRole,
    workspace::{DisplayRow, RowAction},
};
use std::{collections::HashMap, sync::Arc};

#[path = "repository_graph_compact_v2.rs"]
pub(crate) mod compact_v2;
#[path = "repository_graph_projection_v2.rs"]
pub(crate) mod projection_v2;
#[path = "repository_graph_restore_v2.rs"]
pub(crate) mod restore_v2;
#[path = "repository_graph_state_v2.rs"]
mod state_v2;
pub use projection_v2::GraphWindowActionV2;
pub use restore_v2::GraphPositionV2;
#[cfg(test)]
#[path = "repository_graph_fixture_v2.rs"]
pub(crate) mod fixture_v2;

const MAX_NODES: usize = 10_000;
const MAX_LANES: usize = 128;
/// Lane columns drawn before a row ends in `…`, so very wide histories keep
/// their messages on screen. The full lane frontier is still bounded by
/// `MAX_LANES` and overflowing it is an error, never a dropped path.
pub(crate) const HISTORY_VISIBLE_LANES_V991: usize = 24;
const MAX_RETAINED: usize = 8 * 1024 * 1024;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub enum GraphViewV1 {
    #[default]
    Timeline,
    Parents,
    Details,
    Legend,
    Comparison,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum GraphActionV1 {
    More,
    Parents,
    Details,
    Legend,
    Back,
    ChooseParent(usize),
    CompareParent(usize),
    Horizontal(i32),
    Window(GraphWindowActionV2),
    DiscardBack,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphCoverageV1 {
    MoreAvailable,
    AvailableWalkExhausted,
    ShallowBoundary,
    UnavailableParents,
    Capped,
}
impl GraphCoverageV1 {
    pub fn label(&self) -> &'static str {
        match self {
            Self::MoreAvailable => "More history available • original saved walk continues",
            Self::AvailableWalkExhausted => "All ancestors in this original saved walk are loaded",
            Self::ShallowBoundary => {
                "Shallow history boundary • original parent IDs retained • no automatic fetch"
            }
            Self::UnavailableParents => {
                "Original walk stopped with unavailable parents • not a root or empty history"
            }
            Self::Capped => "Graph viewer limit reached • remaining history unknown, not complete",
        }
    }
}
pub struct OriginalGraphStateV1 {
    origin: Arc<OriginalLocalActorOriginV1>,
    topology: compact_v2::GraphTopologyV2,
    shared_charges_v2: Arc<[restore_v2::GraphMemoryChargeV2]>,
    snapshot_v2: std::sync::OnceLock<Arc<restore_v2::GraphSnapshotV2>>,
    pages: Arc<[Arc<OriginalGraphPageV1>]>,
    retained_bytes: usize,
    coverage: GraphCoverageV1,
}
impl std::fmt::Debug for OriginalGraphStateV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OriginalGraphStateV1")
            .field("walk_tip", &self.walk_tip())
            .field("loaded", &self.loaded())
            .field("coverage", &self.coverage)
            .finish()
    }
}
impl OriginalGraphStateV1 {
    pub fn origin(&self) -> &Arc<OriginalLocalActorOriginV1> {
        &self.origin
    }
    pub fn walk_tip(&self) -> &str {
        self.pages[0].walk_tip()
    }
    pub fn loaded(&self) -> usize {
        self.topology.rows().len()
    }
    pub fn coverage(&self) -> &GraphCoverageV1 {
        &self.coverage
    }
    pub fn last_page(&self) -> &Arc<OriginalGraphPageV1> {
        self.pages.last().expect("genuine graph has first page")
    }
    pub fn observed(&self, sha: &str) -> Option<&OriginalGraphCommitV1> {
        self.topology
            .observed(&self.pages, self.topology.member(sha)?)
    }
    pub fn selected_commit(&self, sha: &str) -> Option<&Commit> {
        self.observed(sha).map(OriginalGraphCommitV1::commit)
    }
    pub fn parent_reason(&self, selected: &str, index: usize) -> Option<&'static str> {
        let commit = self.selected_commit(selected)?;
        let parent = commit.parents.get(index)?;
        if self.last_page().epoch().is_shallow_commit(selected) {
            Some("Shallow parent is outside this original walk • no fetch or empty diff guessed")
        } else if self.selected_commit(parent).is_none() {
            Some("Parent is not loaded • use More history first; its exact ID remains visible")
        } else {
            None
        }
    }
}
#[derive(Clone, Debug)]
pub struct GraphSpanV1 {
    pub text: String,
    pub color: Option<usize>,
}
pub struct GraphPresentationV1 {
    rows: Arc<[DisplayRow]>,
    paint: Arc<[Arc<[GraphSpanV1]>]>,
    commit_rows: HashMap<String, usize>,
    navigation_rows: HashMap<Vec<u8>, usize>,
    focus: Option<usize>,
}
impl std::fmt::Debug for GraphPresentationV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphPresentationV1")
            .field("rows", &self.rows.len())
            .field("focus", &self.focus)
            .finish()
    }
}
impl GraphPresentationV1 {
    pub fn rows(&self) -> &[DisplayRow] {
        &self.rows
    }
    pub fn paint(&self, index: usize) -> Option<&[GraphSpanV1]> {
        self.paint.get(index).map(AsRef::as_ref)
    }
    pub fn focus(&self) -> Option<usize> {
        self.focus
    }
    pub fn commit_row(&self, id: &str) -> Option<usize> {
        self.commit_rows.get(id).copied()
    }
    pub fn navigation_row(&self, key: &[u8]) -> Option<usize> {
        self.navigation_rows.get(key).copied()
    }
    #[cfg(test)]
    pub(crate) fn with_prepared_heading_for_test(&self) -> Result<Arc<Self>, String> {
        // Derive a bounded test layout from genuine freshly read rows through
        // the SAME canonical builder/indexes; never invent a source/member.
        let mut builder = ProjectionBuilderV1 {
            limit: MAX_RETAINED,
            ..Default::default()
        };
        builder.push(
            "new prepared heading".into(),
            SemanticRole::Reference,
            None,
            Vec::new(),
        )?;
        for (row, paint) in self.rows.iter().zip(self.paint.iter()) {
            builder.push(
                row.label.clone(),
                row.role,
                row.action.clone(),
                paint.to_vec(),
            )?;
        }
        builder.focus = self.focus.map(|index| index + 1);
        builder.finish()
    }
    pub(crate) fn prepare(
        state: &OriginalGraphStateV1,
        selected: &str,
        view: GraphViewV1,
    ) -> Result<Arc<Self>, String> {
        if view == GraphViewV1::Timeline {
            let member = state
                .loaded_member_v2(selected)
                .ok_or("Original timeline member absent")?;
            return Self::prepare_window_v2(
                state,
                selected,
                view,
                member.node as usize / 200 * 200,
            );
        }
        let selected = state
            .selected_commit(selected)
            .ok_or("Selected exact saved version is not a member of this graph")?;
        let mut builder = ProjectionBuilderV1 {
            limit: MAX_RETAINED.saturating_sub(state.retained_bytes),
            ..Default::default()
        };
        match view {
            GraphViewV1::Timeline => unreachable!("Delegated bounded timeline projection"),
            GraphViewV1::Parents => {
                builder.push(
                    format!("Ordered original parents • {}", selected.id),
                    SemanticRole::Reference,
                    None,
                    Vec::new(),
                )?;
                if selected.parents.is_empty() {
                    builder.push(
                        "Root commit • no parents; first saved version in this ancestry".into(),
                        SemanticRole::Boundary,
                        None,
                        Vec::new(),
                    )?;
                }
                for (index, parent) in selected.parents.iter().enumerate() {
                    builder.push(
                        format!("Parent {}: {parent}", index + 1),
                        SemanticRole::Reference,
                        Some(RowAction::Graph(GraphActionV1::ChooseParent(index))),
                        Vec::new(),
                    )?;
                    builder.push(
                        format!(
                            "[Compare parent {}]{}",
                            index + 1,
                            state
                                .parent_reason(&selected.id, index)
                                .map_or(String::new(), |reason| format!(
                                    " • unavailable: {reason}"
                                ))
                        ),
                        SemanticRole::Changed,
                        Some(RowAction::Graph(GraphActionV1::CompareParent(index))),
                        Vec::new(),
                    )?;
                }
            }
            GraphViewV1::Details => {
                let row = state
                    .observed(&selected.id)
                    .expect("selected genuine member");
                for line in [
                    format!("Saved commit: {}", selected.id),
                    graph_explanation_v1(selected),
                    format!(
                        "Author: {}",
                        crate::repository_view::actor_label(&selected.author)
                    ),
                    format!(
                        "Committer: {}",
                        crate::repository_view::actor_label(&selected.committer)
                    ),
                    format!(
                        "Authored: {}",
                        crate::git_actor_labels_v1::utc_datetime(selected.author.timestamp)
                    ),
                    format!(
                        "Committed: {}",
                        crate::git_actor_labels_v1::utc_datetime(selected.committer.timestamp)
                    ),
                ] {
                    if line.len() > 120 {
                        for part in wrap_graph_line_v1(&line, 68) {
                            builder.push(part, SemanticRole::Reference, None, Vec::new())?;
                        }
                    } else {
                        builder.push(line, SemanticRole::Reference, None, Vec::new())?;
                    }
                }
                for name in selected.refs.iter().take(64) {
                    let shown = name.chars().take(1024).collect::<String>();
                    builder.push(
                        format!("Name observed at graph load: {shown}"),
                        SemanticRole::Reference,
                        None,
                        Vec::new(),
                    )?;
                }
                if selected.refs.len() > 64 {
                    builder.push(
                        format!(
                            "{} additional original names not displayed; no name absence inferred",
                            selected.refs.len() - 64
                        ),
                        SemanticRole::Boundary,
                        None,
                        Vec::new(),
                    )?;
                }
                builder.push(
                    if row.message_complete() {
                        "Original commit message"
                    } else {
                        "Original message preview16KiB • further message bytes not displayed"
                    }
                    .into(),
                    SemanticRole::Boundary,
                    None,
                    Vec::new(),
                )?;
                let mut displayed = 0usize;
                let mut limited = false;
                'message: for line in row.message().split('\n') {
                    for part in wrap_graph_line_v1(line, 68) {
                        if displayed == 1024 {
                            limited = true;
                            break 'message;
                        }
                        builder.push(part, SemanticRole::Normal, None, Vec::new())?;
                        displayed += 1;
                    }
                }
                if limited {
                    builder.push("Original message display limit1024 rows • more logical lines not displayed".into(), SemanticRole::Boundary, None, Vec::new())?;
                }
            }
            GraphViewV1::Legend => {
                for line in crate::theme::Theme::from_settings(0, false).legend() {
                    builder.push(line, SemanticRole::Reference, None, Vec::new())?;
                }
                builder.push(
                    "▶ selected • highlighted: ancestors of the selection • ●─╮ merge joins a parent lane • ●─╯ a lane ends in its parent".into(),
                    SemanticRole::Reference,
                    None,
                    Vec::new(),
                )?;
            }
            GraphViewV1::Comparison => {
                return Err(
                    "Comparison uses existing genuine diff rows, not a copied graph pane".into(),
                )
            }
        }
        // Subviews start at their first actual action, not a stale Timeline
        // index which may be beyond an earlier parent/control after Tab.
        builder.focus = builder
            .rows
            .iter()
            .position(|row| row.action.is_some())
            .or(Some(0));
        builder.finish()
    }
}
fn graph_explanation_v1(commit: &Commit) -> String {
    let history = match commit.parents.len() {
        0 => "This is a root commit: the first saved version in this history.".into(),
        1 => format!(
            "This saved version follows parent {}.",
            &commit.parents[0][..8]
        ),
        count => format!(
            "This merge joins {count} histories. Its ordered parents are {}.",
            commit
                .parents
                .iter()
                .map(|id| &id[..8])
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let names = short_names_v1(commit);
    let names = names.trim().trim_start_matches('(').trim_end_matches(')');
    if names.is_empty() {
        history
    } else {
        format!("{history} Branches and tags here: {names}.")
    }
}
fn wrap_graph_line_v1(line: &str, width: usize) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut rows = Vec::new();
    let mut current = String::new();
    let mut cells = 0usize;
    for glyph in line.graphemes(true) {
        let count = crate::editor::grapheme_width(glyph);
        if cells.saturating_add(count) > width && !current.is_empty() {
            rows.push(std::mem::take(&mut current));
            cells = 0;
        }
        current.push_str(glyph);
        cells += count;
    }
    if !current.is_empty() || rows.is_empty() {
        rows.push(current);
    }
    rows
}
/// Compact `git log --decorate` style names for one graph row, e.g.
/// ` (HEAD, main, origin/main)`. Full names stay in Details.
pub(crate) fn short_names_v1(commit: &Commit) -> String {
    if commit.refs.is_empty() {
        return String::new();
    }
    let names = commit
        .refs
        .iter()
        .take(3)
        .map(|name| {
            let short = if name == "HEAD at graph load" {
                "HEAD".to_owned()
            } else if let Some(tag) = name.strip_prefix("refs/tags/") {
                format!("tag {tag}")
            } else {
                name.strip_prefix("refs/heads/")
                    .or_else(|| name.strip_prefix("refs/remotes/"))
                    .unwrap_or(name)
                    .to_owned()
            };
            short.chars().take(64).collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(", ");
    let more = commit.refs.len().saturating_sub(3);
    if more > 0 {
        format!(" ({names}, +{more})")
    } else {
        format!(" ({names})")
    }
}
#[derive(Default)]
struct ProjectionBuilderV1 {
    rows: Vec<DisplayRow>,
    paint: Vec<Arc<[GraphSpanV1]>>,
    bytes: usize,
    focus: Option<usize>,
    commit_rows: HashMap<String, usize>,
    navigation_rows: HashMap<Vec<u8>, usize>,
    limit: usize,
}
#[cfg(test)]
#[path = "repository_graph_lanes_v991_tests.rs"]
mod lanes_v991_tests;
#[cfg(test)]
#[path = "repository_graph_v1_tests.rs"]
mod tests;
impl ProjectionBuilderV1 {
    fn push(
        &mut self,
        label: String,
        role: SemanticRole,
        action: Option<RowAction>,
        spans: Vec<GraphSpanV1>,
    ) -> Result<usize, String> {
        self.bytes = self
            .bytes
            .checked_add(
                label.len()
                    + spans.iter().map(|span| span.text.len() + 64).sum::<usize>()
                    + 128
                    + if action.is_some() { 512 } else { 0 },
            )
            .filter(|n| *n <= self.limit)
            .ok_or("Graph prepared connector/details labels exceed8MiB")?;
        if self.rows.len() >= MAX_NODES * 4 || label.len() > 64 * 1024 {
            return Err("Graph prepared row/display cap reached, no silent parent loss".into());
        }
        let index = self.rows.len();
        if let Some(action) = &action {
            self.navigation_rows
                .insert(crate::navigation_state::row_key(action)?, index);
        }
        if let Some(RowAction::Commit(id)) = &action {
            self.commit_rows.entry(id.clone()).or_insert(index);
        }
        self.rows.push(DisplayRow {
            label,
            role,
            action,
            cells: None,
        });
        self.paint.push(Arc::from(spans));
        Ok(index)
    }
    fn finish(self) -> Result<Arc<GraphPresentationV1>, String> {
        let result = Arc::new(GraphPresentationV1 {
            rows: Arc::from(self.rows),
            paint: Arc::from(self.paint),
            focus: self.focus,
            commit_rows: self.commit_rows,
            navigation_rows: self.navigation_rows,
        });
        if result.retained_bytes_v2() > self.limit {
            return Err("Actual allocated graph projection exceeds shared8MiB budget".into());
        }
        Ok(result)
    }
}
