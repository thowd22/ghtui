//! SOURCE-ONLY test forwarding for existing private Background predicates.
//! No receipt/owner/source/result constructor or alternate test queue.
use super::*;

pub(crate) fn pending(background: &Background, view: View) -> bool {
    background.current.contains_key(&view.index())
}
pub(crate) fn original_tree_only(prepared: &PreparedView) -> bool {
    original_tree_background_v1::original_tree_prepared_only_v1(prepared)
}
pub(crate) fn original_local_only(prepared: &PreparedView) -> bool {
    local_saved_navigation_v1::original_local_prepared_only_v1(prepared)
}
