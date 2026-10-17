//! Original cached-label query repair before any original/history viewer.
use super::*;

#[test]
fn pr_metadata_actions_label_filter_repair_keyboard_query_before_original_view_repairs_sql_failure_without_resend(
) {
    boundaries::legacy_v282::query_v306::label_query(false);
}
#[test]
fn pr_metadata_actions_label_filter_repair_mouse_query_before_original_view_repairs_sql_failure_without_resend(
) {
    boundaries::legacy_v282::query_v306::label_query(true);
}
