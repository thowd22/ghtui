//! Actual process death/receipt-store failure. The original ordered reference
//! survives; read-only restart never grants candidate authority or resends.
#[test]
fn pr_metadata_actions_recovery_keyboard_sigkill_invoked_child_retains_original_pending_without_resend(
) {
    crash(false);
}
#[test]
fn pr_metadata_actions_recovery_mouse_sigkill_invoked_child_retains_original_pending_without_resend(
) {
    crash(true);
}
fn crash(mouse: bool) {
    super::boundaries::legacy_v282::recovery_v294::crash(mouse);
}
#[test]
fn pr_metadata_actions_recovery_keyboard_actual_remote_confirmation_receipt_sql_failure_cannot_be_resent(
) {
    receipt(false);
}
#[test]
fn pr_metadata_actions_recovery_mouse_actual_remote_confirmation_receipt_sql_failure_cannot_be_resent(
) {
    receipt(true);
}
fn receipt(mouse: bool) {
    super::boundaries::legacy_v282::recovery_v294::receipt_failure(mouse);
}
