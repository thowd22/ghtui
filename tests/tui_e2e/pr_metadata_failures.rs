//! Genuine current Metadata receipt, protected context failure and numeric-account privacy.
use super::*;

#[test]
fn pr_metadata_keyboard_confirmed_context_denial_hides_old_rows_without_resend() {
    current_v51::failure(false, false);
}

#[test]
fn pr_metadata_mouse_confirmed_context_denial_hides_old_rows_without_resend() {
    current_v51::failure(true, false);
}

#[test]
fn pr_metadata_keyboard_confirmed_final_reused_login_new_uid_hides_old_rows_without_resend() {
    current_v51::failure(false, true);
}

#[test]
fn pr_metadata_mouse_confirmed_final_reused_login_new_uid_hides_old_rows_without_resend() {
    current_v51::failure(true, true);
}
