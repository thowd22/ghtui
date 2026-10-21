//! ghtui keeps terminal interaction separate from Git and GitHub providers.
pub mod actions_controller;
pub mod actions_recovery;
pub mod actions_routes;
pub mod actions_ui;
pub mod actions_workspace;
pub mod actions_write_journal;
pub mod active_remote_scope_v1;
pub mod actor_avatar_projection_v1;
pub mod actor_avatar_states_v1;
pub mod app;
pub mod capability_inspector;
pub mod cli;
pub mod command_palette;
pub mod comparison;
pub mod config;
pub mod conflict_blocks;
pub mod context_help;
pub mod corpus_routes;
pub mod corpus_ui;
pub mod dashboard_profile_v1;
pub mod dashboard_threads_v1;
pub mod diff;
pub mod drafts;
pub mod editor;
pub mod footer;
pub mod forms;
pub mod git;
pub mod git_actor_labels_v1;
pub mod github_ui;
pub mod github_workspace;
pub mod history;
pub mod identity;
pub mod inbox_status_v1;
pub mod list_preview_v92;
pub mod local_actions;
pub mod local_availability;
#[cfg(test)]
pub(crate) mod local_commit_actor_origin_v1_test_support;
pub mod local_identity_readability_v1;
pub mod local_picker;
pub mod log_finder_v1;
pub mod markdown_v114;
pub mod metrics;
pub mod metrics_repository;
pub mod metrics_routes;
pub mod metrics_trend_v1;
pub mod metrics_ui;
pub mod metrics_workspace;
pub mod native_diff_operation_v3;
pub mod navigation_catalogue;
pub mod navigation_local_owner_v1;
pub mod navigation_local_source_v1;
pub mod navigation_state;
pub mod navigation_trail;
pub mod navigation_ui;
pub mod notification_pr_v63;
pub mod notifications_native_v63;
pub mod notifications_ui_v63;
pub mod notifications_v63;
pub mod origin_repository;
#[cfg(test)]
pub(crate) mod original_diff_sides_v1_test_support;
pub mod personal_pr_entry_v13;
pub mod personal_readmission_v105;
pub mod pr_actions;
pub mod pr_actor_profile_v1;
pub mod pr_branch_edit_v27;
pub mod pr_checks_actions;
pub mod pr_checks_native;
pub mod pr_checks_sql_guard;
pub(crate) mod pr_collaboration_origin_v17;
mod pr_comment_selection_shared_v1;
pub mod pr_conversation_cards_v115;
pub mod pr_create_v1;
pub mod pr_issue_links_v87;
pub mod pr_legacy;
pub mod pr_metadata_actions;
pub mod pr_metadata_base;
pub mod pr_metadata_catalogue;
pub mod pr_metadata_choices;
pub mod pr_metadata_native;
pub mod pr_metadata_picker;
pub mod pr_metadata_preview;
pub mod pr_metadata_quick_v1;
pub mod pr_progress;
pub mod pr_projection;
pub(crate) mod pr_read_origin_v13;
pub mod pr_state_model;
pub mod pr_timeline_links_v13;
pub mod pr_ui;
pub mod pr_write_journal;
pub mod prepared_form_text_v1;
pub mod providers;
mod read_only_pty_pane_v4;
pub mod remote_scope_gate_v1;
pub mod render_timings_v66;
pub mod repository_catalogue_avatars_v1;
pub mod repository_catalogue_dialogs;
pub mod repository_catalogue_intent_v1;
pub mod repository_catalogue_native;
pub mod repository_catalogue_save_v1;
pub mod repository_catalogue_ui_v1;
pub mod repository_catalogue_workspace_v1;
pub mod repository_commit_actor_avatars_v1;
pub mod repository_commit_actor_native_v1;
pub mod repository_commit_actor_profile_v1;
pub mod repository_commit_actor_ui_v1;
pub mod repository_diff_sides_native_v1;
pub mod repository_diff_sides_ui_v1;
pub mod repository_diff_sides_v1;
pub mod repository_diff_sides_worker_v1;
pub mod repository_graph_reread_native_v3;
pub mod repository_graph_reread_v3;
pub mod repository_graph_ui_v1;
pub mod repository_graph_v1;
pub mod repository_native_diff_content_v2;
pub mod repository_native_diff_native_v1;
pub mod repository_native_diff_projection_v1;
pub mod repository_native_diff_ui_v1;
pub mod repository_native_diff_v1;
mod repository_native_external_ui_v4;
#[doc(hidden)]
pub mod repository_native_external_v4;
pub mod repository_native_full_side_ui_v3;
pub mod repository_native_full_side_v3;
pub mod repository_native_mutable_plan_native_v2;
pub mod repository_native_mutable_plan_v2;
pub mod repository_panels_v93;
pub mod repository_related_pr_v1;
pub mod repository_source_identity;
pub mod repository_source_ui;
pub mod repository_tree_jump_ui_v1;
pub mod repository_tree_jump_v1;
pub mod repository_tree_native_v1;
pub mod repository_tree_palette_v2;
pub mod repository_tree_queue_notice_v2;
pub mod repository_tree_viewport_native_v2;
pub mod repository_tree_viewport_ui_v2;
pub mod repository_tree_viewport_v2;
#[cfg(test)]
mod repository_tree_viewport_v2_test_support;
pub mod repository_tree_worker_v1;
pub mod repository_ui;
pub mod repository_view;
pub mod runtime;
pub mod scoped_inbox_route_v1;
pub(crate) mod scoped_pr_collaboration_draft_v17;
mod scoped_pr_collaboration_history_v17;
pub mod scoped_pr_collaboration_native_v17;
pub(crate) mod scoped_pr_collaboration_plan_v17;
pub mod scoped_pr_collaboration_refresh_v19;
pub(crate) mod scoped_pr_collaboration_source_v17;
pub mod scoped_pr_collaboration_ui_v17;
pub(crate) mod scoped_pr_issue_links_v87;
pub mod scoped_pr_native_diff_native_v1;
pub mod scoped_pr_native_diff_source_v1;
pub mod scoped_pr_observed_tabs_v1;
pub mod scoped_pr_read_v1;
mod scrollbar;
pub mod search;
pub mod search_blend;
pub mod search_sources;
pub mod search_ui;
pub(crate) mod state_inbox_sync_v76;
pub mod storage;
mod storage_private;
pub mod suggestion_apply_v1;
pub mod suggestions;
pub mod terminal;
pub mod theme;
pub mod ui;
pub mod ui_chrome;
pub mod ui_table;
pub mod workspace;

#[cfg(test)]
#[path = "capability_app_tests.rs"]
mod capability_app_tests;
#[cfg(test)]
#[path = "../tests/support/immutable_git_v1.rs"]
pub(crate) mod immutable_git_v1_test_support;
#[cfg(test)]
#[path = "../tests/support/owned_pty_processes_v1.rs"]
pub(crate) mod owned_pty_processes_test_support;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/support/providers.rs"]
mod test_support;
#[cfg(test)]
mod workflow_tls_contracts_v9;

pub mod repository_catalogue_read_limits_dialog_v1;
#[cfg(test)]
pub(crate) mod repository_catalogue_read_limits_test_support_v1;
pub mod repository_catalogue_read_limits_v1;
pub mod repository_catalogue_reload_v1;

#[cfg(test)]
mod remote_repository_v1_test_support;

pub mod remote_repository_native_v1;
pub mod remote_repository_v1;
pub mod repository_stash_recovery_v1;
pub mod scoped_actions_dispatch_history_v7;
pub mod scoped_actions_dispatch_publication_v7;
pub mod scoped_actions_dispatch_ui_v7;
pub mod scoped_actions_native_v1;
#[cfg(test)]
pub(crate) mod scoped_actions_publication_test_support_v3;
pub mod scoped_actions_publication_ui_v3;
pub mod scoped_actions_publication_v3;
pub mod scoped_actions_read_v2;
#[cfg(test)]
pub(crate) mod scoped_actions_rerun_test_support_v4;
#[cfg(test)]
pub(crate) mod scoped_actions_resources_test_support_v2;
pub mod scoped_actions_resources_v2;
pub mod scoped_actions_summary_history_v5;
#[cfg(test)]
pub(crate) mod scoped_actions_test_support_v1;
pub mod scoped_actions_v1;
pub(crate) mod scoped_actions_workflow_edit_ui_v9;
pub(crate) mod scoped_actions_workflow_edit_v9;
pub mod scoped_actions_workflow_publication_v6;
pub mod scoped_actions_workflow_ui_v6;
