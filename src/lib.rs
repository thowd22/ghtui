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
pub mod context_help;
pub mod corpus_routes;
pub mod corpus_ui;
pub mod diff;
pub mod drafts;
pub mod editor;
pub mod forms;
pub mod git;
pub mod git_actor_labels_v1;
pub mod github_ui;
pub mod github_workspace;
pub mod identity;
pub mod local_actions;
pub mod local_availability;
#[cfg(test)]
pub(crate) mod local_commit_actor_origin_v1_test_support;
pub mod local_identity_readability_v1;
pub mod local_picker;
pub mod metrics;
pub mod metrics_repository;
pub mod metrics_routes;
pub mod metrics_ui;
pub mod metrics_workspace;
pub mod navigation_catalogue;
pub mod navigation_local_owner_v1;
pub mod navigation_local_source_v1;
pub mod navigation_state;
pub mod navigation_trail;
pub mod navigation_ui;
#[cfg(test)]
pub(crate) mod original_diff_sides_v1_test_support;
pub mod pr_actions;
pub mod pr_checks_actions;
pub mod pr_checks_native;
pub mod pr_checks_sql_guard;
mod pr_comment_selection_shared_v1;
pub mod pr_legacy;
pub mod pr_metadata_actions;
pub mod pr_metadata_base;
pub mod pr_metadata_catalogue;
pub mod pr_metadata_choices;
pub mod pr_metadata_native;
pub mod pr_metadata_picker;
pub mod pr_metadata_preview;
pub mod pr_progress;
pub mod pr_projection;
pub mod pr_ui;
pub mod pr_write_journal;
pub mod providers;
pub mod remote_scope_gate_v1;
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
pub mod repository_native_diff_native_v1;
pub mod repository_native_diff_projection_v1;
pub mod repository_native_diff_ui_v1;
pub mod repository_native_diff_v1;
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
pub mod scoped_pr_observed_tabs_v1;
pub mod scoped_pr_read_v1;
pub mod search;
pub mod search_blend;
pub mod search_sources;
pub mod search_ui;
pub mod storage;
mod storage_private;
pub mod terminal;
pub mod theme;
pub mod ui;
pub mod workspace;

#[cfg(test)]
#[path = "capability_app_tests.rs"]
mod capability_app_tests;
#[cfg(test)]
#[path = "../tests/support/immutable_git_v1.rs"]
pub(crate) mod immutable_git_v1_test_support;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/support/providers.rs"]
mod test_support;

pub mod repository_catalogue_read_limits_dialog_v1;
#[cfg(test)]
pub(crate) mod repository_catalogue_read_limits_test_support_v1;
pub mod repository_catalogue_read_limits_v1;
pub mod repository_catalogue_reload_v1;

#[cfg(test)]
mod remote_repository_v1_test_support;

pub mod remote_repository_native_v1;
pub mod remote_repository_v1;
