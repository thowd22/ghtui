//! Real compiled-binary current Metadata approval, editor and restart journeys.
use super::*;

#[test]
fn pr_metadata_preview_keyboard_no_change_edit_editor_close_restart_and_fresh_confirmation() {
    current_v51::preview_restart(false);
}
#[test]
fn pr_metadata_preview_mouse_no_change_edit_editor_close_restart_and_fresh_confirmation() {
    current_v51::preview_restart(true);
}
