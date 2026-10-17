//! Native source/publication boundary journeys; strict queued transport and
//! independent durable/Git state are checked, never inferred from screenshots.
#[path = "pr_metadata_members_boundary_legacy_v282.rs"]
pub(super) mod legacy_v282;

#[test]
fn pr_metadata_actions_boundary_keyboard_same_login_numeric_reassignment_hides_choices() {
    source_identity(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_same_login_numeric_reassignment_hides_choices() {
    source_identity(true);
}
fn source_identity(mouse: bool) {
    legacy_v282::source_identity(mouse);
}
#[test]
fn pr_metadata_actions_boundary_keyboard_raw_stderr_capture_exhaustion_cannot_produce_source() {
    capture(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_raw_stderr_capture_exhaustion_cannot_produce_source() {
    capture(true);
}
fn capture(mouse: bool) {
    legacy_v282::raw_capture_limit(mouse);
}
#[test]
fn pr_metadata_actions_boundary_keyboard_complete_opposite_review_set_drift_refuses_before_claim() {
    drift(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_complete_opposite_review_set_drift_refuses_before_claim() {
    drift(true);
}
fn drift(mouse: bool) {
    legacy_v282::opposite_review_drift(mouse);
}

#[test]
fn pr_metadata_actions_boundary_keyboard_confirmed_cleanup_failure_restart_views_exact_original_without_choices_or_resend(
) {
    confirmed_restart(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_confirmed_cleanup_failure_restart_views_exact_original_without_choices_or_resend(
) {
    confirmed_restart(true);
}
fn confirmed_restart(mouse: bool) {
    legacy_v282::recovery_v294::confirmed_restart(mouse);
}
