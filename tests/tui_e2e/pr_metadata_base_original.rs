//! The two ORIGINAL protected Base/restart journeys, on the current protocol.
//! Each owns its full publication/history/fresh-open scenario, not a newer test alias.
use super::*;

#[test]
fn pr_metadata_base_keyboard_confirmed_original_restart_edits_and_new_intent_never_resend_or_release_receipt(
) {
    current_legacy_v267::recovery_v303::original(false);
}
#[test]
fn pr_metadata_base_mouse_confirmed_original_restart_edits_and_new_intent_never_resend_or_release_receipt(
) {
    current_legacy_v267::recovery_v303::original(true);
}
