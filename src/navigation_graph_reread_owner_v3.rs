//! NEW undeclared private child of navigation_local_owner_v1. Exact capsule
//! ownership BEFORE reads; no source constructors, new operation or authority.
use super::*;
use crate::repository_graph_v1::{
    restore_v2::GraphPositionV2, GraphPresentationV1, OriginalGraphStateV1,
};
use std::sync::Weak;
impl OriginalLocalNavigationOwnerV1 {
    pub(crate) fn matches_graph_original_v3(
        &self,
        origin: &Arc<OriginalLocalActorOriginV1>,
        state: &Weak<OriginalGraphStateV1>,
        display: &Weak<GraphPresentationV1>,
        position: &GraphPositionV2,
    ) -> bool {
        self.operation.check().is_ok()
            && Arc::ptr_eq(&self.origin, origin)
            && Arc::ptr_eq(&self.target_origin, origin)
            && self
                .graph
                .as_ref()
                .is_some_and(|current| Weak::ptr_eq(&Arc::downgrade(current), state))
            && self
                .graph_display
                .as_ref()
                .is_some_and(|current| Weak::ptr_eq(&Arc::downgrade(current), display))
            && self.revision == position.selected
            && self.selected.as_deref() == Some(position.selected.as_str())
            && self.graph_view == position.view
            && self.graph_horizontal == position.horizontal
            && self.graph_projection_start == position.projection_start
            && self.graph_comparison_parent_v3 == position.comparison_parent
    }
}
