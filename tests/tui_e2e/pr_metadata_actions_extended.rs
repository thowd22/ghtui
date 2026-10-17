//! Additional native current/eligible/cancel/recovery boundaries. Expected
//! source facts and queued argv are fixed independently of the observed output.
#[test]
fn pr_metadata_actions_boundary_keyboard_partial_known_label_and_nullable_disabled_member() {
    partial(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_partial_known_label_and_nullable_disabled_member() {
    partial(true);
}
fn partial(mouse: bool) {
    super::boundaries::legacy_v282::partial(mouse);
}
#[test]
fn pr_metadata_actions_boundary_keyboard_known_empty_vs_forbidden_source_never_infers_clear() {
    empty(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_known_empty_vs_forbidden_source_never_infers_clear() {
    empty(true);
}
fn empty(mouse: bool) {
    super::boundaries::legacy_v282::empty_and_forbidden(mouse);
}
#[test]
fn pr_metadata_actions_boundary_keyboard_missing_current_array_is_unknown_not_empty() {
    unknown(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_missing_current_array_is_unknown_not_empty() {
    unknown(true);
}
fn unknown(mouse: bool) {
    super::boundaries::legacy_v282::missing_current_array(mouse);
}

#[test]
fn pr_metadata_actions_boundary_keyboard_prewrite_403_same_uid_hides_old_private_snapshot() {
    prewrite_access(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_prewrite_403_same_uid_hides_old_private_snapshot() {
    prewrite_access(true);
}
fn prewrite_access(mouse: bool) {
    super::boundaries::legacy_v282::recovery_v294::prewrite_access(mouse);
}
