//! SOURCE-ONLY unregistered native Base-picker journeys. These strict protocols
//! and control labels require Root's actual integration review before execution;
//! planned requests, source preparation and snapshots are not native evidence.
use super::*;
#[path = "pr_metadata_base_legacy_v267.rs"]
mod current_legacy_v267;
#[path = "pr_metadata_base_original.rs"]
mod original;

const TIP: &str = "cccccccccccccccccccccccccccccccccccccccc";
const CHANGED_TIP: &str = "dddddddddddddddddddddddddddddddddddddddd";

fn original_parent() -> Value {
    let mut value = target("Native collaboration", "A fixture description", "main");
    value["id"] = json!(1007);
    value["merged"] = json!(false);
    value["labels"] = linked_issue(&value)["labels"].clone();
    value["assignees"] = linked_issue(&value)["assignees"].clone();
    value["milestone"] = linked_issue(&value)["milestone"].clone();
    value
}
fn linked_issue(parent: &Value) -> Value {
    let mut value = issue();
    value["id"] = json!(2007);
    value["node_id"] = json!("I_metadata_7");
    value["title"] = parent["title"].clone();
    value["body"] = parent["body"].clone();
    value["state"] = parent["state"].clone();
    value["user"] = parent["user"].clone();
    value["html_url"] = json!("https://github.invalid/example/repo/pull/7");
    value["pull_request"] = json!({
        "url":"https://github.invalid/api/v3/repos/example/repo/pulls/7",
        "html_url":"https://github.invalid/example/repo/pull/7"
    });
    assert_ne!(value["id"], parent["id"]);
    assert_ne!(value["node_id"], parent["node_id"]);
    value
}
fn branch(name: &str, tip: &str) -> Value {
    json!({"name":name,"commit":{"sha":tip},"protected":false})
}
#[test]
fn pr_metadata_base_keyboard_picker_cancel_query_preview_restart_and_exact_confirmed_write_preserve_git(
) {
    happy(false);
}
#[test]
fn pr_metadata_base_mouse_picker_cancel_query_preview_restart_and_exact_confirmed_write_preserve_git(
) {
    happy(true);
}
fn happy(mouse: bool) {
    current_legacy_v267::happy(mouse);
}
#[test]
fn pr_metadata_base_keyboard_capped_known_member_and_nullable_disabled_row_never_infer_unseen_values(
) {
    partial(false);
}
#[test]
fn pr_metadata_base_mouse_capped_known_member_and_nullable_disabled_row_never_infer_unseen_values()
{
    partial(true);
}
fn partial(mouse: bool) {
    current_legacy_v267::partial(mouse);
}
#[test]
fn pr_metadata_base_keyboard_complete_empty_and_forbidden_catalogue_are_not_the_same_result() {
    empty_and_forbidden(false);
}
#[test]
fn pr_metadata_base_mouse_complete_empty_and_forbidden_catalogue_are_not_the_same_result() {
    empty_and_forbidden(true);
}
fn empty_and_forbidden(mouse: bool) {
    current_legacy_v267::empty_and_forbidden(mouse);
}
#[test]
fn pr_metadata_base_keyboard_fresh_changed_tip_refuses_before_publication_marker_or_patch() {
    stale_tip(false);
}
#[test]
fn pr_metadata_base_mouse_fresh_changed_tip_refuses_before_publication_marker_or_patch() {
    stale_tip(true);
}
fn stale_tip(mouse: bool) {
    stale_tip_observed_input(mouse, false);
}
#[test]
fn owned_input_frames_v1_keyboard_held_confirm_one_actual_input_per_completed_focus() {
    stale_tip_observed_input(false, true);
}
#[test]
fn owned_input_frames_v1_mouse_held_confirm_one_actual_input_per_completed_focus() {
    stale_tip_observed_input(true, true);
}
fn stale_tip_observed_input(mouse: bool, controlled: bool) {
    current_legacy_v267::stale_tip_observed_input(mouse, controlled);
}

#[test]
fn pr_metadata_base_keyboard_final_numeric_account_change_hides_previous_private_choices_and_metadata(
) {
    changed_account(false);
}
#[test]
fn pr_metadata_base_mouse_final_numeric_account_change_hides_previous_private_choices_and_metadata()
{
    changed_account(true);
}
fn changed_account(mouse: bool) {
    current_legacy_v267::changed_account(mouse);
}

#[test]
fn pr_metadata_base_keyboard_control_only_stderr_over_budget_denies_choices_without_closing_source_calls(
) {
    raw_stderr(false);
}
#[test]
fn pr_metadata_base_mouse_control_only_stderr_over_budget_denies_choices_without_closing_source_calls(
) {
    raw_stderr(true);
}
fn raw_stderr(mouse: bool) {
    current_legacy_v267::raw_stderr(mouse);
}
