//! Worker-only original Graph coordination. Display rows are immutable Arcs,
//! not foreground copies of the loaded DAG. Parent hooks are unapplied.
use super::*;
use crate::{
    git::OriginalGraphReadV1,
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_graph_v1::{GraphActionV1, GraphPresentationV1, GraphViewV1, OriginalGraphStateV1},
};

impl Workspace {
    pub(crate) fn initialize_graph_v1(&mut self, repository: &GitRepository) -> Result<(), String> {
        if self.status.head.is_none() {
            self.rows.clear();
            self.push(
                "No saved versions yet • Graph has no guessed HEAD/root",
                SemanticRole::Boundary,
                None,
            );
            return Ok(());
        }
        let origin = self
            .actor_origin
            .as_ref()
            .ok_or("Original local graph source unavailable")?
            .clone();
        let page = if let Some(deadline) = repository.original_graph_deadline_v1() {
            repository
                .original_graph_page_v1(OriginalGraphReadV1::Start(&self.revision), deadline)
                .map_err(|error| error.to_string())?
        } else {
            // Direct legacy library calls are not queued native authority. The
            // actual native route always forwards its already captured window.
            let operation = LocalGitNavigationReadV1::begin(
                &origin,
                repository.original_graph_cancellation_v1(),
            )?;
            let page = origin.with_saved_navigation_read_v1(&operation, |handle| {
                handle
                    .original_graph_page_v1(
                        OriginalGraphReadV1::Start(&self.revision),
                        operation.deadline(),
                    )
                    .map_err(|error| error.to_string())
            })?;
            operation.seal()?;
            page
        };
        let graph = OriginalGraphStateV1::start(origin, page)?;
        self.install_graph_v1(graph, GraphViewV1::Timeline)
    }
    fn install_graph_v1(
        &mut self,
        state: Arc<OriginalGraphStateV1>,
        view: GraphViewV1,
    ) -> Result<(), String> {
        if !self
            .actor_origin
            .as_ref()
            .is_some_and(|origin| Arc::ptr_eq(origin, state.origin()))
        {
            return Err("Original Graph source owner changed before preparation".into());
        }
        let selected = state
            .selected_commit(&self.revision)
            .ok_or("Original Graph selected saved version absent")?
            .clone();
        let member = state
            .loaded_member_v2(&selected.id)
            .ok_or("Original loaded member missing")?;
        let projection_start = member.node as usize / 200 * 200;
        let presentation =
            GraphPresentationV1::prepare_window_v2(&state, &selected.id, view, projection_start)?;
        self.graph_projection_start_v2 = projection_start;
        self.graph_state_v1 = Some(state);
        self.graph_display_v1 = Some(presentation);
        self.graph_view_v1 = view;
        self.selected_commit = Some(selected);
        self.mode = RepositoryMode::Graph;
        self.actor_profile = None;
        if self
            .file_provenance
            .as_ref()
            .is_none_or(|state| state.view != ProvenanceView::Graph)
        {
            self.file_provenance = None;
        }
        self.preview = None;
        self.tree = None;
        self.tree_navigation_v1 = None;
        self.tree_breadcrumbs_v2 = None;
        self.comparison = None;
        // Current Main clones Workspace metadata, never these old graph bodies.
        self.rows.clear();
        self.graph = CommitGraph::default();
        Ok(())
    }
    pub(crate) fn graph_cached_commit_v1(&mut self, id: &str) -> Result<bool, String> {
        let Some(state) = self.graph_state_v1.clone() else {
            return Ok(false);
        };
        if self.actor_profile.is_some()
            || self.mode != RepositoryMode::Graph
            || self
                .file_provenance
                .as_ref()
                .is_some_and(|state| state.view != ProvenanceView::Graph)
        {
            return Ok(false);
        }
        if state.selected_commit(id).is_none() {
            return Err("Selected Graph row is not an original loaded member".into());
        }
        self.revision = id.into();
        self.install_graph_v1(state, GraphViewV1::Timeline)?;
        Ok(true)
    }
    pub fn graph_focus_row_v1(&self) -> Option<usize> {
        self.graph_visible_v1()
            .then(|| {
                self.graph_display_v1
                    .as_ref()
                    .and_then(|display| display.focus())
            })
            .flatten()
    }
    pub fn graph_visible_v1(&self) -> bool {
        self.mode == RepositoryMode::Graph
            && self.actor_profile.is_none()
            && self
                .file_provenance
                .as_ref()
                .is_none_or(|state| state.view == ProvenanceView::Graph)
            && self.preview.is_none()
            && self.graph_state_v1.as_ref().is_some_and(|state| {
                self.actor_origin
                    .as_ref()
                    .is_some_and(|origin| Arc::ptr_eq(origin, state.origin()))
                    && state.selected_commit(&self.revision).is_some()
            })
    }
    pub fn graph_paint_v1(
        &self,
        index: usize,
    ) -> Option<&[crate::repository_graph_v1::GraphSpanV1]> {
        if !self.graph_visible_v1() {
            return None;
        }
        self.graph_display_v1.as_ref()?.paint(index)
    }
    pub fn graph_action_index_v1(&self, action: &RowAction) -> Option<usize> {
        if !self.graph_visible_v1() {
            return None;
        }
        match action {
            RowAction::Commit(id) => self.graph_display_v1.as_ref()?.commit_row(id),
            _ => None,
        }
    }
    pub fn graph_navigation_row_v1(&self, key: &[u8]) -> Option<usize> {
        if !self.graph_visible_v1() {
            return None;
        }
        self.graph_display_v1.as_ref()?.navigation_row(key)
    }
    pub fn graph_action_reason_v1(&self, action: &GraphActionV1) -> Option<String> {
        let state = self.graph_state_v1.as_ref()?;
        match action {
            GraphActionV1::Window(direction) => {
                let start = self.graph_projection_start_v2;
                use crate::repository_graph_v1::GraphWindowActionV2::*;
                match direction {
                    Newer if start == 0 => {
                        Some("Already at newest loaded projection • no wider read".into())
                    }
                    Older if start + 200 >= state.loaded() => Some(
                        "Already at oldest loaded projection • use More for unknown history".into(),
                    ),
                    _ => None,
                }
            }
            GraphActionV1::More if !state.last_page().has_more() => {
                Some(state.coverage().label().into())
            }
            GraphActionV1::ChooseParent(index) | GraphActionV1::CompareParent(index) => {
                let selected = state.selected_commit(&self.revision)?;
                if selected.parents.get(*index).is_none() {
                    Some("No original parent at this index • root has no guessed parent".into())
                } else {
                    state
                        .parent_reason(&self.revision, *index)
                        .map(str::to_owned)
                }
            }
            _ => None,
        }
    }
    pub(crate) fn graph_action_v1(
        &mut self,
        action: GraphActionV1,
        repository: Option<&GitRepository>,
    ) -> Result<(), String> {
        if let Some(reason) = self.graph_action_reason_v1(&action) {
            return Err(reason);
        }
        let state = self
            .graph_state_v1
            .as_ref()
            .ok_or("No genuine original Graph state")?
            .clone();
        if !self
            .actor_origin
            .as_ref()
            .is_some_and(|origin| Arc::ptr_eq(origin, state.origin()))
        {
            return Err("Original Graph owner lost; no new source inferred".into());
        }
        match action {
            GraphActionV1::Window(direction) => return self.graph_window_action_v2(direction),
            GraphActionV1::DiscardBack => {
                return Err("Discard graph Back requires the shared current App control".into())
            }
            GraphActionV1::More => {
                let repository =
                    repository.ok_or("Graph continuation requires its actual original handle")?;
                let deadline = repository
                    .original_graph_deadline_v1()
                    .ok_or("Graph continuation lost original whole-operation clock")?;
                let page = repository
                    .original_graph_page_v1(
                        OriginalGraphReadV1::Continue(state.last_page()),
                        deadline,
                    )
                    .map_err(|error| error.to_string())?;
                let next = OriginalGraphStateV1::append(&state, page)?;
                self.install_graph_v1(next, GraphViewV1::Timeline)?;
                repository
                    .validate_graph_epoch_v1(
                        self.graph_state_v1.as_ref().unwrap().last_page(),
                        deadline,
                    )
                    .map_err(|error| error.to_string())?;
            }
            GraphActionV1::Parents => self.install_graph_v1(state, GraphViewV1::Parents)?,
            GraphActionV1::Details => self.install_graph_v1(state, GraphViewV1::Details)?,
            GraphActionV1::Legend => self.install_graph_v1(state, GraphViewV1::Legend)?,
            GraphActionV1::Back => self.install_graph_v1(state, GraphViewV1::Timeline)?,
            GraphActionV1::ChooseParent(index) => {
                let selected = state
                    .selected_commit(&self.revision)
                    .ok_or("Original selected Graph commit absent")?;
                self.revision = selected
                    .parents
                    .get(index)
                    .ok_or("No original parent index")?
                    .clone();
                self.install_graph_v1(state, GraphViewV1::Timeline)?;
            }
            GraphActionV1::CompareParent(index) => {
                let repository =
                    repository.ok_or("Exact parent comparison needs the actual original handle")?;
                let deadline = repository
                    .original_graph_deadline_v1()
                    .ok_or("Parent comparison lost original clock")?;
                repository
                    .validate_graph_epoch_v1(state.last_page(), deadline)
                    .map_err(|error| error.to_string())?;
                let selected = state
                    .selected_commit(&self.revision)
                    .ok_or("Original exact selected member absent")?;
                repository
                    .original_graph_member_v1(
                        state.last_page(),
                        state
                            .observed(&selected.id)
                            .ok_or("Original member object observation unavailable")?,
                        deadline,
                    )
                    .map_err(|error| error.to_string())?;
                self.set_commit_changes(repository, index)?;
                self.mode = RepositoryMode::CommitChanges;
                self.graph_view_v1 = GraphViewV1::Comparison;
                repository
                    .validate_graph_epoch_v1(state.last_page(), deadline)
                    .map_err(|error| error.to_string())?;
            }
            GraphActionV1::Horizontal(delta) => {
                self.graph_horizontal_v1 = self
                    .graph_horizontal_v1
                    .saturating_add_signed(delta as isize)
                    .min(2048)
            }
        }
        Ok(())
    }
    pub(crate) fn restore_graph_v1(
        &mut self,
        repository: &GitRepository,
        route: &crate::navigation_local_source_v1::OriginalGraphRouteV1,
    ) -> Result<(), String> {
        let origin = self
            .actor_origin
            .as_ref()
            .ok_or("Original Graph visit source unavailable")?
            .clone();
        let deadline = repository
            .original_graph_deadline_v1()
            .ok_or("Original Graph Back lost its fresh request clock")?;
        repository
            .validate_graph_fingerprint_v1(route.epoch, deadline)
            .map_err(|error| error.to_string())?;
        let first = repository
            .original_graph_page_v1(OriginalGraphReadV1::Start(&route.walk_tip), deadline)
            .map_err(|error| error.to_string())?;
        if first.epoch().fingerprint() != route.epoch || route.loaded == 0 || route.loaded > 10_000
        {
            return Err("Original Graph visit epoch/count changed; no later source adopted".into());
        }
        let mut state = OriginalGraphStateV1::start(origin, first)?;
        while state.loaded() < route.loaded {
            let next = repository
                .original_graph_page_v1(OriginalGraphReadV1::Continue(state.last_page()), deadline)
                .map_err(|error| error.to_string())?;
            state = OriginalGraphStateV1::append(&state, next)?;
        }
        if state.loaded() != route.loaded {
            return Err("Original Graph Back page size changed; no truncation accepted".into());
        }
        self.revision = route.selected.clone();
        self.install_graph_v1(
            state,
            if route.view == GraphViewV1::Comparison {
                GraphViewV1::Timeline
            } else {
                route.view
            },
        )?;
        if route.view == GraphViewV1::Comparison {
            self.graph_action_v1(
                GraphActionV1::CompareParent(
                    route
                        .comparison_parent
                        .ok_or("Original graph parent comparison index unavailable")?,
                ),
                Some(repository),
            )?;
        }
        self.graph_horizontal_v1 = route.horizontal;
        repository
            .validate_graph_epoch_v1(self.graph_state_v1.as_ref().unwrap().last_page(), deadline)
            .map_err(|error| error.to_string())
    }
    pub(crate) fn graph_route_v1(
        &self,
    ) -> Result<Option<crate::navigation_local_source_v1::OriginalGraphRouteV1>, String> {
        if self.file_provenance.is_some() {
            return Ok(None);
        }
        if !(self.graph_visible_v1()
            || (self.mode == RepositoryMode::CommitChanges
                && self.graph_view_v1 == GraphViewV1::Comparison
                && self.actor_profile.is_none()
                && self.file_provenance.is_none()))
        {
            return Ok(None);
        }
        let state = self
            .graph_state_v1
            .as_ref()
            .ok_or("Original graph route state unavailable")?;
        if !self
            .actor_origin
            .as_ref()
            .is_some_and(|origin| Arc::ptr_eq(origin, state.origin()))
            || state.selected_commit(&self.revision).is_none()
            || self.graph_horizontal_v1 > 2048
        {
            return Err("Original Graph visit lacks exact source/member/bounded viewport".into());
        }
        let comparison_parent = if self.graph_view_v1 == GraphViewV1::Comparison {
            match &self
                .comparison
                .as_ref()
                .ok_or("Original Graph comparison unavailable")?
                .comparison
            {
                Comparison::CommitChanges { commit, parent } if commit == &self.revision => {
                    Some(*parent)
                }
                _ => {
                    return Err(
                        "Original Graph comparison endpoints do not match selected member".into(),
                    )
                }
            }
        } else {
            None
        };
        Ok(Some(
            crate::navigation_local_source_v1::OriginalGraphRouteV1 {
                walk_tip: state.walk_tip().into(),
                selected: self.revision.clone(),
                loaded: state.loaded(),
                epoch: state.last_page().epoch().fingerprint(),
                view: self.graph_view_v1,
                horizontal: self.graph_horizontal_v1,
                comparison_parent,
            },
        ))
    }
}
