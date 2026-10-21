# ghtui v1 coverage: PR, Actions, local Git and every exposed action

This is the TASK-64 release evidence for the Linux v1. It maps every row of the PR and Actions
inventories (doc-3 and doc-5, as the closed requirement model
`providers::gh::CapabilityContractRowV1` names them) to the native behavior that implements it,
the code/provider tests and the real-binary PTY journeys (keyboard and mouse) that cover it, and
its known limitations. It then enumerates every local Git operation and every key-bar control
with its key and its click path.

`tests/coverage_inventory_v64.rs` keeps this document honest. It fails when:

- an inventory row or a Local operation is missing here, or listed twice;
- a test cited here is not a `#[test]`, is `#[ignore]`d, sits in a file no module declares, or
  (for a PTY journey) is not in the real-binary `tui_e2e` target;
- a native row cites no keyboard journey or no mouse journey (only rows marked **Gap** may);
- a key-bar control of a probed view state has no key, its key resolves to another control,
  it has no click target (key bar or tab strip when enabled; Commands always), or it is
  missing from [Key bar controls by view](#key-bar-controls-by-view);
- a Commands (`:`) entry cannot be chosen by typing its name and pressing Enter, or by
  clicking its row, or either path yields a different action;
- a PR actions menu row is missing from [PR actions menu](#pr-actions-menu);
- a bracketed control of a main overlay (Help, an explanation, Settings, Local operations,
  PR actions) has no click target, its key (or, without a key, Tab focus and `Enter`) does
  something other than its click, a menu row clicks another row than `j` selects, or the
  control is missing from [Overlay controls](#overlay-controls).

Run it with `cargo test --test coverage_inventory_v64`. The PTY journeys it names run with
`cargo test --test tui_e2e -- <name>`.

## How to read the paths

- **Keys** are the painted key tokens. View keys: `1` Inbox, `2` Repository, `3` Search,
  `4` Actions, `5` Dashboard. Global keys: `?` Help, `:` Commands, `r` Refresh, `s` Settings,
  `q` Quit, `Esc` Cancel/Back. Bindings are configurable; these are the defaults.
- **Click paths**: every enabled key-bar token is a click target with the same action as its
  key. Mode controls (PR tabs, Inbox queues, Repository modes, Actions catalogues) are tabs on
  the tab strip at the top of the pane. When a narrow bar drops a token by priority, the same
  action is a clickable row in Commands (`:`), with its key shown beside it. A disabled control
  leaves the bar; its key and its Commands row explain why.
- **Menus**: `p PR actions` (PR detail) and `m Local operations` (Repository) open a menu;
  `j`/`k` or arrows move, `Enter` runs the row, a click on the row runs the same row,
  `PgDn More`/`PgUp` page it.
- **Forms**: every write opens a form; `Ctrl-P` or `[Preview]` prepares a read-only preview,
  `Ctrl-S` or `[Confirm action]` publishes, `Esc` or `[Keep draft / Close]` closes it.
- A browser link never counts as native support. ghtui has no browser handoff and no
  clipboard integration; where a GitHub website feature has no public operation it is listed
  as a limitation or a **Gap**.

## PR inventory

| Row | Native behavior (key / click path) | Code/provider tests | PTY journeys (keyboard + mouse) | Known limitations |
| --- | --- | --- | --- | --- |
| `pr.queues` | Inbox (`1`): queue tabs `a` All, `c` Created by me, `m` Assigned to me, `v` Needs my review (tabs are clickable); `Enter` or a row click opens the PR; partial results show a warning. | `pr_inbox_preserves_cross_repo_membership_and_explicit_partial_coverage` | `personal_inbox_keyboard_overlap_scope_selection_and_partial_warning` `personal_inbox_mouse_overlap_scope_selection_and_partial_warning` `personal_pr_entry_v13_keyboard_actual_cross_queue_files_back_and_independent_effects` `personal_pr_entry_v13_mouse_actual_cross_queue_files_back_and_independent_effects` | Search caps of the GitHub search API are reported as incomplete, not hidden. Queues cover the configured host only. |
| `pr.details` | PR detail tabs `o` Summary, `c` Conversation, `h` Commits, `f` Files, `k` Checks, `t` Timeline, `u` Metadata, `v` Review progress (tab strip, clickable); `b`/`Esc` back. | `pr_details_loads_all_pages_and_keeps_canonical_files_and_actor_provenance` | `pr_details_pr_diff_pr_checks_keyboard_native_tabs` `pr_details_pr_diff_pr_checks_mouse_native_tabs` `pr_summary_context_v13_keyboard_loading_error_retry_and_narrow` `pr_summary_context_v13_mouse_loading_error_retry_and_narrow` | Very large PRs are paged; GitHub's own file-count limits apply and are stated. |
| `pr.checks` | Checks tab `k`: check rows correlated to the PR head SHA; `Enter`/click opens the exact run, job and log in Actions; rerun/cancel through the Actions run controls. | `pr_checks_bridge_provider_actual_distinct_check_suite_run_job_attempt_no_url_guess_preserves_git` | `pr_checks_bridge_keyboard_exact_source_job_logs_and_owned_back_test_framework` `pr_checks_bridge_mouse_exact_source_job_logs_and_owned_back_test_framework` `repository_catalogue_scoped_check_actions_keyboard_exact_bridge_and_matched_logs` `repository_catalogue_scoped_check_actions_mouse_exact_bridge_and_matched_logs` | Third-party (non-Actions) check runs show status and summary only; their logs live outside GitHub. |
| `pr.create` | Repository `m` Local operations, row Create pull request (or the Commands row): base/head/fork fields, draft flag, preview of the exact commits, explicit publish; the new PR appears in Created by me. | `create_pr_verifies_remote_fork_refs_prevents_duplicates_and_preserves_checkout` | `v1_pr_create_keyboard_new_pr_appears_in_authored_queue` `v1_pr_create_mouse_new_pr_appears_in_authored_queue` `v1_pr_create_keyboard_fork_parent_with_reviewers` `v1_pr_create_mouse_fork_parent_with_reviewers` | Creating a fork on GitHub is not offered; an existing fork remote is used. |
| `pr.metadata` | `p` PR actions: Edit PR title, Edit PR description, Change base branch, label/assignee/reviewer/milestone/project rows and issue links; each previews and reads back. | `pr_metadata_labels_assignees_and_explicit_milestone_removal_read_back` | `pr_metadata_actions_add_reviewer_keyboard` `pr_metadata_actions_add_reviewer_mouse` `v1_pr_metadata_quick_add_label_keyboard` `v1_pr_metadata_quick_add_label_mouse` | Project placement covers Projects (v2) items only; classic projects are retired by GitHub. |
| `pr.lifecycle` | `p` PR actions: Mark ready / Convert to draft, Close/Reopen (State on Summary and Conversation), Lock/Unlock, Update branch (merge or rebase), Delete head branch; each pinned to the observed head. | `pr_lifecycle_lock_and_update_branch_use_reason_and_atomic_expected_head` | `v1_pr_lifecycle_keyboard_marks_draft_ready_with_one_write` `v1_pr_lifecycle_mouse_marks_draft_ready_with_one_write` `pr_lifecycle_keyboard_locks_conversation_with_reason` `pr_lifecycle_mouse_locks_conversation_with_reason` | TASK-25 stays In Progress for its remaining AC; an accepted but unconfirmed close stays "uncertain" until a refresh reads it. |
| `pr.conversation` | Conversation tab `c`: new/edit/delete comment, add/remove reaction, browse reaction actors, subscribe/unsubscribe; composer drafts survive restarts. | `conversation_comments_shared_publication_guards_review_lifecycle_and_merge_exact_nodes_and_hosts` | `conversation_comments_keyboard_native_create_preview_cancel_confirm_and_receipt` `conversation_comments_mouse_native_create_preview_cancel_confirm_and_receipt` `conversation_comments_keyboard_native_edit_react_remove_delete_and_subscription` `conversation_comments_mouse_native_edit_react_remove_delete_and_subscription` | Minimizing (hiding) comments as a moderator is not offered. |
| `pr.inline` | Files tab `f`, a diff line: select a range, comment on the left or right side, reply to a thread; anchors use the canonical commit, path, side and lines. | `inline_comments_deleted_multiline_range_uses_canonical_patch_and_sha` | `inline_comments_keyboard_original_left_and_right_range_preview_explicit_confirm_retire_snapshot` `inline_comments_mouse_original_drag_left_and_right_range_preview_explicit_confirm_retire_snapshot` `v1_pr_threads_keyboard_diff_line_i_extend_p_posts_right_range_once` `v1_pr_threads_mouse_diff_line_click_drag_p_posts_left_range_once` | Comments on an outdated diff are shown read-only with their original anchor. |
| `pr.pending-review` | `p` PR actions: Approve review, Request changes, Submit comment review; Files `F10` Reviews: resume, submit or discard a pending review with its local draft batch. | `pending_reviews_create_without_event_and_submit_exact_owned_review` | `v1_pr_reviews_keyboard_approve_previews_publishes_and_refreshes` `v1_pr_reviews_mouse_approve_previews_publishes_and_refreshes` `pending_reviews_v2_native_keyboard_submit_comment` `pending_reviews_v2_native_mouse_submit_comment` | Your own PR cannot be approved; the row explains instead of opening a form. |
| `pr.dismissal` | Files `F10` Reviews: Dismiss review (with a message) and re-request a reviewer; reviewer rows in `p` PR actions. | `pending_reviews_discard_and_dismiss_inspect_current_review_state` `pending_reviews_v2_denied_server_enforced_dismissal_preserves_foreign_author_and_is_uncertain` | `pending_reviews_v2_native_keyboard_dismiss_unknown_privilege` `pending_reviews_v2_native_mouse_dismiss_unknown_privilege` `v1_pr_metadata_quick_request_review_keyboard` `v1_pr_metadata_quick_request_review_mouse` | Dismissal permission is server-enforced; a denied dismissal is reported as uncertain and never retried. |
| `pr.thread-state` | Files thread detail: Resolve / Reopen thread; Review progress tab `v` and Files: mark or unmark a file viewed; changed heads reset progress honestly. | `review_threads_resolve_exact_node_keep_outdated_provenance_and_partial_comments` | `review_threads_v1_publication_keyboard_resolve_exact_state_receipt_not_comment_body_mutation` `review_threads_v1_publication_mouse_resolve_exact_state_receipt_not_comment_body_mutation` `viewed_files_keyboard_marks_unmarks_restarts_and_keeps_changed_head_progress_honest` `viewed_files_mouse_marks_unmarks_restarts_and_keeps_changed_head_progress_honest` | None known beyond host feature availability. |
| `pr.suggestions` | Files: author a suggestion on a selected range; thread detail: apply one or a batch of suggestions as one commit in an isolated worktree, previewed, then pushed to the PR head. | `gh_provider_inventory_contracts_all_builtin_categories_cover_exact_pr_and_actions_rows` | `suggestions_keyboard_suggest_right_range_seeds_previews_and_publishes_once` `suggestions_mouse_suggest_right_range_seeds_previews_and_publishes_once` `suggestions_keyboard_plan_confirm_and_push_exact_commit_to_local_bare_remote` `suggestions_mouse_plan_confirm_and_push_exact_commit_to_local_bare_remote` | GitHub's "commit suggestion" web button has no public API; ghtui commits locally and pushes. Author credit needs the suggester's numeric id. |
| `pr.conflicts` | Files `E` Edit file / `R` Resolve conflicts: edit a PR branch file or resolve a merge with the base in an isolated worktree, see the resulting diff, commit explicitly and push to the PR head without force; a moved remote head refuses the push and keeps the edit. | `pr_branch_edit_resolution_needs_markers_gone_and_commits_both_parents` `pr_branch_edit_moved_head_writes_nothing_and_keeps_the_edit` | `pr_branch_edits_edit_file_keyboard` `pr_branch_edits_edit_file_mouse` `pr_branch_edits_resolve_conflicts_keyboard` `pr_branch_edits_resolve_conflicts_mouse` | Only edits to existing regular text files; push permission is not pre-checked (TASK-108). |
| `pr.merge` | `p` PR actions, Merge pull request: only the repository's allowed methods, pinned to the observed head; auto-merge enable/disable; merge queue enqueue/dequeue. | `merge_queue_enqueue_confirms_entry_id_without_confusing_synthetic_commit_with_pr_head` | `pr_merge_keyboard_previews_then_sends_one_pinned_squash_merge` `pr_merge_mouse_previews_then_sends_one_pinned_squash_merge` `merge_workflows_keyboard_adds_to_merge_queue_with_one_write` `merge_workflows_mouse_adds_to_merge_queue_with_one_write` | TASK-24 stays In Progress for its remaining AC. Queued is reported as queued, never as merged. |
| `pr.revert` | `p` PR actions, Revert on a merged PR: creates a new draft PR that names the original. | `pr_lifecycle_revert_creates_a_new_canonical_pr_with_original_provenance` | `pr_lifecycle_keyboard_reverts_merged_pr_into_a_traceable_draft` `pr_lifecycle_mouse_reverts_merged_pr_into_a_traceable_draft` | Uses the GraphQL revert mutation; hosts without it get an explanation. |
| `pr.navigation` | Commands `:` finds every view, PR tab and action; each view keeps its place; `b`/`Esc` back; timeline links open the exact comment, commit or checks. | `navigation_palette_choose_button_and_keyboard_enter_share_one_shot_activation` | `navigation_context_keyboard_five_views_keep_their_place_after_inbox_pr_files_and_resize` `navigation_context_mouse_five_views_keep_their_place_after_inbox_pr_files_and_resize` `pr_timeline_links_v13_keyboard_opens_comment_commit_and_head_checks_and_returns` `pr_timeline_links_v13_mouse_opens_comment_commit_and_head_checks_and_returns` | No browser handoff, no clipboard (the full SHA and URL are shown to select in the terminal), no PR export. |

## Actions inventory

| Row | Native behavior (key / click path) | Code/provider tests | PTY journeys (keyboard + mouse) | Known limitations |
| --- | --- | --- | --- | --- |
| `actions.discovery` | Actions (`4`): catalogue tabs `R` Runs, `W` Workflows and the settings catalogues; run filters, repository switch `{`/`}`; commit CI status in History opens the exact SHA's runs. | `actions_inventory_filter_encodes_refs_keeps_membership_and_exposes_search_caps` | `actions_inventory_keyboard_catalog_actor_resize_and_read_only_git` `actions_inventory_mouse_catalog_actor_resize_and_read_only_git` `actions_commit_runs_keyboard_history_commit_opens_exact_sha_runs` `actions_commit_runs_mouse_history_commit_opens_exact_sha_runs` | Organization-wide workflow lists need an organization scope. |
| `actions.details` | Runs: `Enter` opens the run (SHA, ref, event, actor, attempt, durations); jobs and steps with matrix names; failed job focused first. | `actions_attempt_jobs_steps_annotations_and_logs_preserve_exact_context` | `actions_run_view_keyboard_exact_attempt_job_logs_and_annotations` `actions_run_view_mouse_exact_attempt_job_logs_and_annotations` `v1_actions_runs_keyboard_open_focuses_failed_job_steps_and_error_lines` `v1_actions_runs_mouse_open_focuses_failed_job_steps_and_error_lines` | Job dependencies that cannot be read from the API are labelled as not inferred. |
| `actions.logs` | Run: `l` Logs, Annotations; log search, failed-step jump, bounded polling labelled with its time; export a loaded log to a new file. | `actions_attempt_rejects_cross_run_jobs_and_log_targets` | `v1_actions_runs_keyboard_logs_open_at_highlighted_job` `v1_actions_runs_mouse_logs_open_at_highlighted_job` `v1_actions_logs_keyboard_export_loaded_log_to_new_file` `v1_actions_logs_mouse_export_loaded_log_to_new_file` | No live log stream: gh gives logs of finished steps; watching polls. |
| `actions.dispatch` | Workflows `W`, then a workflow: `V` choose the revision to read its YAML, `D` Dispatch with typed inputs and a ref, `X` Disable / Enable, `E` edit locally; workflow edits go through an isolated worktree, commit and PR. | `workflow_controls_definition_fetches_immutable_yaml_and_dispatch_confirms_returned_run` | `actions_workflow_controls_keyboard_typed_dispatch_disable_enable_and_cancel` `actions_workflow_controls_mouse_typed_dispatch_disable_enable_and_cancel` `v1_actions_workflows_keyboard_d_x_keys_plain_preview_and_dispatch` `v1_actions_workflows_mouse_buttons_plain_preview_and_dispatch` | An old host that answers dispatch with an empty 204 is reported as accepted, not as a known run. |
| `actions.control` | Run: `w` watch, `A` rerun all, `F` rerun failed, `J` rerun the open job (debug logging), `C` cancel, `!` force-cancel, `-` delete run, `L` delete logs; each confirmed against the exact run and attempt. | `actions_run_control_rerun_debug_specific_job_uses_actual_job_id_and_reports_pending_acceptance` | `actions_run_control_keyboard_exact_job_debug_watch_cancel_and_cancelled_delete` `actions_run_control_mouse_exact_job_debug_watch_cancel_and_cancelled_delete` `v1_actions_controls_keyboard_watch_updates_live_and_stops_when_run_finishes` `v1_actions_controls_mouse_watch_updates_live_and_stops_when_run_finishes` `scoped_actions_control_v5_keyboard_force_explicit_choice_202_pending_no_false_ack` `scoped_actions_control_v5_mouse_force_explicit_choice_202_pending_no_false_ack` `scoped_actions_control_v5_keyboard_logs204_separate_f6_acceptance_no_deleted_flag` `scoped_actions_control_v5_mouse_logs204_separate_f6_acceptance_no_deleted_flag` | Accepted is shown separately from completed; an uncertain cancel is inspected, never resent. |
| `actions.artifacts` | Run: `a` Artifacts list with size and expiry, bounded preview of zip entries, `X` export or `U` safe extract to a new directory, `-` confirmed delete. | `actions_artifacts_preview_export_and_safe_extract_verify_bytes_without_overwriting` | `actions_artifacts_keyboard_previews_exports_extracts_deletes_and_rejects_unsafe_archive` `actions_artifacts_mouse_previews_exports_extracts_deletes_and_rejects_unsafe_archive` | Downloaded content is never executed; oversized archives are explained, not previewed. |
| `actions.approvals` | Approvals `G`: `Y` approve or `B` reject the selected pending deployment with a comment; `Y` approve a fork run from the run; history of decisions. | `actions_approvals_deployment_review_checks_eligibility_exact_environment_ids_and_partial_state` | `actions_approvals_keyboard_exact_environment_decisions_history_and_denied_gate` `actions_approvals_mouse_exact_environment_decisions_history_and_denied_gate` `actions_approvals_keyboard_fork_approval_cancel_preview_and_confirm_exact_run` `actions_approvals_mouse_fork_approval_cancel_preview_and_confirm_exact_run` | Custom deployment protection rule apps are shown, but their decisions belong to the app. |
| `actions.cache` | Caches `c`: `f` filter by key and ref; sizes, age, usage and quota context; `-` delete the selected cache by exact id. | `actions_cache_selected_delete_keeps_unrelated_entries_and_units` | `actions_cache_keyboard_filter_usage_cancel_and_exact_delete_preserves_other_cache` `actions_cache_mouse_filter_usage_cancel_and_exact_delete_preserves_other_cache` | Enterprise storage-limit administration is not offered. |
| `actions.settings` | Secrets `S` and Variables `v`: repository, environment and organization scopes; names and metadata; `=` set a secret with masked transient input or a variable; `-` delete it. | `actions_secrets_vars_secret_set_masks_stdin_journal_and_metadata_never_contains_value` | `actions_secrets_vars_keyboard_masked_transient_secret_variable_and_cancelled_deletions` `actions_secrets_vars_mouse_masked_transient_secret_variable_and_cancelled_deletions` `actions_secrets_vars_keyboard_environment_scope_secret_set_and_confirmed_delete` `actions_secrets_vars_mouse_environment_scope_secret_set_and_confirmed_delete` | Existing secret values are never retrievable. Empty organization selections need gh 2.75.0 or later. |
| `actions.environments` | Environments `e` (`M` edit the selected environment protection) and Policies `p` (`M` edit; sections `L` Allowlist, `T` Selected actions, `O` Workflow token, `H` Fork workflow, `Y` Fork approval, `U` Retention), each with an effect preview. | `actions_policy_edit_previews_typed_keys_checks_concurrency_and_reads_back` | `actions_settings_keyboard_policy_environment_runner_labels_and_cancelled_removal` `actions_settings_mouse_policy_environment_runner_labels_and_cancelled_removal` `actions_settings_keyboard_changed_environment_id_requires_exact_inspection_and_ack_no_retry` `actions_settings_mouse_changed_environment_id_requires_exact_inspection_and_ack_no_retry` | Some protection rules depend on the plan and repository visibility; unavailable ones are explained. |
| `actions.runners` | Runners `n` (`L` replace custom labels, `-` remove the runner) and Runner groups `g` (`+` create, `M` edit, `T` runners, `O` repositories, `-` delete): status, labels and associations. | `actions_runner_labels_keep_read_only_labels_and_validate_exact_runner` | `actions_settings_keyboard_organization_runner_group_associations_and_confirmed_runner_removal` `actions_settings_mouse_organization_runner_group_associations_and_confirmed_runner_removal` `actions_settings_keyboard_runner_groups_need_organization_and_denied_runners_disable_controls` `actions_settings_mouse_runner_groups_need_organization_and_denied_runners_disable_controls` | No runner installation or compute provisioning. Runner groups need an organization scope. |
| `actions.activity` | Dashboard (`5`): cross-repository review and Actions metrics with windows, saved dashboards and drilldowns to the exact PR or run; actor chips. | `metrics_provider_cancelled_and_offline_inventory_is_not_complete_empty_or_silently_retried` | `metrics_keyboard_repository_cohort_windows_all_cards_exact_native_open_and_return` `metrics_mouse_repository_cohort_windows_all_cards_exact_native_open_and_return` `dashboard_named_keyboard_create_switch_rename_delete_and_restart_restore` `dashboard_named_mouse_create_switch_rename_delete_and_restart_restore` | Metrics cover the selected repositories and window only; missing data is shown as incomplete. |

## Local Git operations

Every operation is a row of `m Local operations` in the Repository view (keys: `m`, then
`j`/`k` or arrows and `Enter`; mouse: click `m Local operations` on the key bar, then click the
row, `PgDn More` to page) and a row of Commands (`:`, by the same name). Each opens a form with a
read-only preview and an explicit confirmation; destructive operations show a warning.

| Operation | Keys | Click path | Code/provider tests | PTY journeys (keyboard + mouse) |
| --- | --- | --- | --- | --- |
| Stage selected file | `m` row 1; or `+` on a changed file | menu row; or `+ Stage` on the key bar | `staging_exact_raw_file_requires_confirmation_and_preserves_unrelated_edits` | `staging_keyboard_exact_file_preview_cancel_stage_and_unstage` `staging_mouse_exact_file_preview_cancel_stage_and_unstage` |
| Unstage selected file | `m` row 2; or `-` on a staged file | menu row; or `- Unstage` on the key bar | `staging_and_unstaging_preserve_files_and_reject_stale_content` | `staging_keyboard_exact_file_preview_cancel_stage_and_unstage` `staging_mouse_exact_file_preview_cancel_stage_and_unstage` |
| Stage selected lines | `m` row 3; or `+` with lines or a hunk selected | menu row; or `+ Stage` | `staging_selected_lines_ignore_earlier_unselected_hunks_and_reverse_exactly` | `staging_keyboard_canonical_range_and_hunk_produce_exact_index_content` `staging_mouse_canonical_drag_range_and_hunk_produce_exact_index_content` |
| Unstage selected lines | `m` row 4; or `-` with lines selected | menu row; or `- Unstage` | `staging_selected_unstage_preserves_neighboring_additions_and_removals_in_index` | `v1_local_staging_keyboard_hunk_line_file_and_commit_with_direct_keys` `v1_local_staging_mouse_hunk_line_file_and_commit_with_direct_controls` |
| Commit staged changes | `m` row 5; or `K` | menu row; or `K Commit` | `local_commit_exact_staged_preview_rejects_changed_state_then_commits_full_message` | `local_commit_keyboard_stage_message_hook_failure_and_amend_cancel` `local_commit_mouse_stage_message_hook_failure_and_amend_cancel` |
| Amend latest commit | `m` row 6 | menu row | `v1_local_commit_amend_preview_names_replaced_published_commit_and_keeps_it_in_reflog` | `local_commit_keyboard_signing_failure_then_successful_amend` `local_commit_mouse_signing_failure_then_successful_amend` |
| Create branch | `m` row 7 | menu row | `branches_tags_native_intents_retain_exact_targets_and_cancel_deletion` | `branches_tags_keyboard_create_switch_rename_tag_upstream_and_cancel` `branches_tags_mouse_create_switch_rename_tag_upstream_and_cancel` |
| Rename branch | `m` row 8 | menu row | `branches_tags_switch_rename_and_dirty_guard` | `branches_tags_keyboard_create_switch_rename_tag_upstream_and_cancel` `branches_tags_mouse_create_switch_rename_tag_upstream_and_cancel` |
| Switch branch | `m` row 9 | menu row | `v1_local_gaps_switch_untracked_collision_worktree_owned_branch_and_detached_head` | `branches_tags_keyboard_create_switch_rename_tag_upstream_and_cancel` `branches_tags_mouse_create_switch_rename_tag_upstream_and_cancel` |
| Delete branch | `m` row 10 | menu row | `v1_local_branches_first_push_defaults_tracking_and_unmerged_delete_shows_recovery` | `local_management_keyboard_worktree_owned_delete_tag_lease_and_conflicting_pull` `local_management_mouse_worktree_owned_delete_tag_lease_and_conflicting_pull` |
| Create tag | `m` row 11 | menu row | `branches_tags_native_intents_retain_exact_targets_and_cancel_deletion` | `branches_tags_keyboard_create_switch_rename_tag_upstream_and_cancel` `branches_tags_mouse_create_switch_rename_tag_upstream_and_cancel` |
| Delete tag | `m` row 12 | menu row | `branches_tags_native_intents_retain_exact_targets_and_cancel_deletion` | `local_management_keyboard_worktree_owned_delete_tag_lease_and_conflicting_pull` `local_management_mouse_worktree_owned_delete_tag_lease_and_conflicting_pull` |
| Set upstream | `m` row 13 | menu row | `remote_git_push_fetch_and_upstream_with_local_bare_remote` | `branches_tags_keyboard_create_switch_rename_tag_upstream_and_cancel` `branches_tags_mouse_create_switch_rename_tag_upstream_and_cancel` |
| Fetch remote | `m` row 14 | menu row | `remote_git_push_fetch_and_upstream_with_local_bare_remote` | `remote_git_keyboard_explicit_bare_push_fetch_and_effects` `remote_git_mouse_explicit_bare_push_fetch_and_effects` |
| Pull remote branch | `m` row 15 | menu row | `remote_git_divergent_pull_merge_and_rebase_preserve_both_histories` | `v1_local_branches_keyboard_diverged_sync_rejected_push_rebase_and_first_push` `v1_local_branches_mouse_diverged_sync_rejected_push_rebase_and_first_push` |
| Push branch | `m` row 16 | menu row | `remote_git_explicit_bare_push_and_url_preview_never_exposes_credentials` | `remote_git_keyboard_explicit_bare_push_fetch_and_effects` `remote_git_mouse_explicit_bare_push_fetch_and_effects` |
| Force push with exact lease | `m` row 17 | menu row | `remote_git_force_push_uses_exact_remote_lease_and_pull_strategy` | `local_management_keyboard_worktree_owned_delete_tag_lease_and_conflicting_pull` `local_management_mouse_worktree_owned_delete_tag_lease_and_conflicting_pull` |
| Create pull request | `m` row 18 | menu row | `create_pr_verifies_remote_fork_refs_prevents_duplicates_and_preserves_checkout` | `complete_journeys_v65_keyboard_edit_stage_commit_push_create_pr_offline_and_moved_head` `complete_journeys_v65_mouse_edit_stage_commit_push_create_pr_offline_and_moved_head` |
| Save stash | `m` row 19 | menu row | `stash_worktrees_save_apply_add_and_dirty_remove_guard` | `stash_worktrees_keyboard_immutable_stash_apply_and_dirty_removal` `stash_worktrees_mouse_immutable_stash_apply_and_dirty_removal` |
| Apply stash | `m` row 20 | menu row | `stash_worktrees_inventory_uses_immutable_stash_ids_and_blocks_dirty_removal` | `stash_worktrees_keyboard_immutable_stash_apply_and_dirty_removal` `stash_worktrees_mouse_immutable_stash_apply_and_dirty_removal` |
| Apply and remove stash | `m` row 21 | menu row | `stash_recovery_successful_pop_held_refresh_recreated_locator_refuses_new_workspace` | `stash_recovery_keyboard_actual_pop_inspect_return_and_conflict` `stash_recovery_mouse_actual_pop_inspect_return_and_conflict` |
| Delete stash | `m` row 22 | menu row | `stash_worktrees_view_contents_pop_drop_locked_and_cancelled_removal` | `stash_worktrees_keyboard_view_drop_stash_and_open_dirty_worktree` `stash_worktrees_mouse_view_drop_stash_and_open_dirty_worktree` |
| Add linked worktree | `m` row 23 | menu row | `stash_worktrees_save_apply_add_and_dirty_remove_guard` | `stash_worktrees_keyboard_immutable_stash_apply_and_dirty_removal` `stash_worktrees_mouse_immutable_stash_apply_and_dirty_removal` |
| Remove linked worktree | `m` row 24 | menu row | `stash_worktrees_ignored_local_bytes_are_guarded_before_installed_git_would_delete_them` | `stash_worktrees_keyboard_immutable_stash_apply_and_dirty_removal` `stash_worktrees_mouse_immutable_stash_apply_and_dirty_removal` |
| Merge saved history | `m` row 25 | menu row | `git_sequencers_merge_conflict_stages_and_abort_restore` | `v1_local_recovery_keyboard_merge_preview_refuses_change_made_while_open` `v1_local_recovery_mouse_merge_preview_refuses_change_made_while_open` |
| Rebase onto saved history | `m` row 26 | menu row | `v1_local_gaps_rebase_cherry_pick_and_revert_pauses_name_operation_and_recovery_rows` | `local_workspace_keyboard_linked_worktree_cherry_pick_branch_mode_and_rebase_abort` `local_workspace_mouse_linked_worktree_cherry_pick_branch_mode_and_rebase_abort` |
| Cherry-pick commit | `m` row 27 | menu row | `v1_local_gaps_rebase_cherry_pick_and_revert_pauses_name_operation_and_recovery_rows` | `history_previews_keyboard_list_commits_pick_abort_rebase_skip_revert_and_reset` `history_previews_mouse_list_commits_pick_abort_rebase_skip_revert_and_reset` |
| Revert commit | `m` row 28 | menu row | `v1_local_gaps_rebase_cherry_pick_and_revert_pauses_name_operation_and_recovery_rows` | `history_previews_keyboard_list_commits_pick_abort_rebase_skip_revert_and_reset` `history_previews_mouse_list_commits_pick_abort_rebase_skip_revert_and_reset` |
| Continue operation | `m` row 29; or the paused-operation banner row | menu row; or the banner's Continue row | `git_sequencers_conflict_stage_resolution_is_inspected_then_staged_and_continued` | `git_sequencers_keyboard_conflict_version_stage_continue_and_parent_links` `git_sequencers_mouse_conflict_version_stage_continue_and_parent_links` |
| Skip current commit | `m` row 30; or the banner's Skip row | menu row; or the banner's Skip row | `v1_local_recovery_sequencer_preview_refuses_a_change_made_while_it_is_open` | `history_previews_keyboard_list_commits_pick_abort_rebase_skip_revert_and_reset` `history_previews_mouse_list_commits_pick_abort_rebase_skip_revert_and_reset` |
| Abort operation | `m` row 31; or the banner's Abort row | menu row; or the banner's Abort row | `git_sequencers_merge_conflict_stages_and_abort_restore` | `history_previews_keyboard_list_commits_pick_abort_rebase_skip_revert_and_reset` `history_previews_mouse_list_commits_pick_abort_rebase_skip_revert_and_reset` |
| Choose conflict version | `m` row 32 on a conflicted file | menu row | `git_sequencers_native_conflict_choice_stage_continue_retains_two_parents` | `git_sequencers_keyboard_conflict_version_stage_continue_and_parent_links` `git_sequencers_mouse_conflict_version_stage_continue_and_parent_links` |
| Create rescue branch | `m` row 33; or a reflog row | menu row; or the reflog row | `reflog_recovery_creates_branch_without_discarding_dirty_work` | `reflog_recovery_keyboard_rescue_old_commit_and_cancel_reset` `reflog_recovery_mouse_rescue_old_commit_and_cancel_reset` |
| Reset current branch | `m` row 34 | menu row | `reflog_recovery_reset_modes_and_exact_discard_require_confirmation` | `history_previews_keyboard_list_commits_pick_abort_rebase_skip_revert_and_reset` `history_previews_mouse_list_commits_pick_abort_rebase_skip_revert_and_reset` |
| Discard selected file changes | `m` row 35 on a changed file | menu row | `reflog_recovery_reset_modes_and_exact_discard_require_confirmation` | `discard_keyboard_exact_file_preview_cancel_and_confirm_restores_saved_bytes` `discard_mouse_exact_file_preview_cancel_and_confirm_restores_saved_bytes` |
| Start bug search | `m` row 36 | menu row | `guided_bisect_finds_culprit_and_restores_original_head` | `guided_bisect_keyboard_classify_versions_find_culprit_and_restore` `guided_bisect_mouse_classify_versions_find_culprit_and_restore` |
| Mark version good | `m` row 37 | menu row | `guided_bisect_native_classification_progress_and_reset_restore_original_branch` | `guided_bisect_keyboard_classify_versions_find_culprit_and_restore` `guided_bisect_mouse_classify_versions_find_culprit_and_restore` |
| Mark version bad | `m` row 38 | menu row | `guided_bisect_native_classification_progress_and_reset_restore_original_branch` | `guided_bisect_keyboard_classify_versions_find_culprit_and_restore` `guided_bisect_mouse_classify_versions_find_culprit_and_restore` |
| Skip untestable version | `m` row 39 | menu row | `guided_bisect_dirty_refusal_detached_skip_interrupted_and_no_candidate` | `guided_bisect_keyboard_dirty_refusal_skip_interrupted_and_no_candidate` `guided_bisect_mouse_dirty_refusal_skip_interrupted_and_no_candidate` |
| End bug search | `m` row 40 | menu row | `guided_bisect_native_classification_progress_and_reset_restore_original_branch` | `guided_bisect_keyboard_classify_versions_find_culprit_and_restore` `guided_bisect_mouse_classify_versions_find_culprit_and_restore` |
| Resolve one conflict block | `m` row 41 on a conflicted file | menu row | `v42_conflict_blocks_resolve_one_block_at_a_time_refuse_stale_bytes_and_keep_the_index` | `v1_local_recovery_keyboard_resolve_conflict_blocks_one_at_a_time` `v1_local_recovery_mouse_resolve_conflict_blocks_one_at_a_time` |

Read-only browsing (Overview, History graph, Tree, Working, Staged, Branch, commit changes,
Uncommitted, file history, blame, compare) never writes the working tree, index, refs or
config; checkout, branch and reset stay behind `m` with preview and confirmation.

## PR actions menu

`p PR actions` on a PR detail opens this menu. Keys: `p`, then `j`/`k` or arrows and `Enter`;
mouse: click `p PR actions`, then click the row. A row that cannot run on this PR explains
why instead of opening a form.

| Row | Keys | Click path |
| --- | --- | --- |
| Acknowledge inspected result | `p`, then `j`/`k` and `Enter` | click the row |
| Add assignee | `p`, then `j`/`k` and `Enter` | click the row |
| Add conversation comment | `p`, then `j`/`k` and `Enter` | click the row |
| Add label | `p`, then `j`/`k` and `Enter` | click the row |
| Add to merge queue | `p`, then `j`/`k` and `Enter` | click the row |
| Add to project | `p`, then `j`/`k` and `Enter` | click the row |
| Approve review | `p`, then `j`/`k` and `Enter` | click the row |
| Browse selected comment reactions | `p`, then `j`/`k` and `Enter` | click the row |
| Change PR subscription | `p`, then `j`/`k` and `Enter` | click the row |
| Change base branch | `p`, then `j`/`k` and `Enter` | click the row |
| Clear milestone | `p`, then `j`/`k` and `Enter` | click the row |
| Close without merging | `p`, then `j`/`k` and `Enter` | click the row |
| Convert to draft | `p`, then `j`/`k` and `Enter` | click the row |
| Delete head branch | `p`, then `j`/`k` and `Enter` | click the row |
| Delete selected comment | `p`, then `j`/`k` and `Enter` | click the row |
| Edit PR description | `p`, then `j`/`k` and `Enter` | click the row |
| Edit PR title | `p`, then `j`/`k` and `Enter` | click the row |
| Edit selected comment | `p`, then `j`/`k` and `Enter` | click the row |
| Enable auto-merge | `p`, then `j`/`k` and `Enter` | click the row |
| Inspect Base history | `p`, then `j`/`k` and `Enter` | click the row |
| Inspect Close without merging history | `p`, then `j`/`k` and `Enter` | click the row |
| Inspect Description history | `p`, then `j`/`k` and `Enter` | click the row |
| Inspect Link Issue history | `p`, then `j`/`k` and `Enter` | click the row |
| Inspect Reopen unmerged PR history | `p`, then `j`/`k` and `Enter` | click the row |
| Inspect Title history | `p`, then `j`/`k` and `Enter` | click the row |
| Inspect Unlink manual Issue history | `p`, then `j`/`k` and `Enter` | click the row |
| Inspect member metadata history | `p`, then `j`/`k` and `Enter` | click the row |
| Inspect pending result | `p`, then `j`/`k` and `Enter` | click the row |
| Link Issue | `p`, then `j`/`k` and `Enter` | click the row |
| Lock conversation | `p`, then `j`/`k` and `Enter` | click the row |
| Mark selected file unviewed | `p`, then `j`/`k` and `Enter` | click the row |
| Mark selected file viewed | `p`, then `j`/`k` and `Enter` | click the row |
| Merge pull request | `p`, then `j`/`k` and `Enter` | click the row |
| New conversation comment | `p`, then `j`/`k` and `Enter` | click the row |
| Pending reviews, dismissal, re-request | `p`, then `j`/`k` and `Enter` | click the row |
| React to selected comment | `p`, then `j`/`k` and `Enter` | click the row |
| Remove assignee | `p`, then `j`/`k` and `Enter` | click the row |
| Remove from merge queue | `p`, then `j`/`k` and `Enter` | click the row |
| Remove from project | `p`, then `j`/`k` and `Enter` | click the row |
| Remove label | `p`, then `j`/`k` and `Enter` | click the row |
| Remove my selected reaction | `p`, then `j`/`k` and `Enter` | click the row |
| Remove team review request | `p`, then `j`/`k` and `Enter` | click the row |
| Remove user review request | `p`, then `j`/`k` and `Enter` | click the row |
| Reopen unmerged PR | `p`, then `j`/`k` and `Enter` | click the row |
| Request changes | `p`, then `j`/`k` and `Enter` | click the row |
| Request team review | `p`, then `j`/`k` and `Enter` | click the row |
| Request user review | `p`, then `j`/`k` and `Enter` | click the row |
| Revert merged PR | `p`, then `j`/`k` and `Enter` | click the row |
| Set milestone | `p`, then `j`/`k` and `Enter` | click the row |
| Submit comment review | `p`, then `j`/`k` and `Enter` | click the row |
| Unlink manual Issue | `p`, then `j`/`k` and `Enter` | click the row |
| Unlock conversation | `p`, then `j`/`k` and `Enter` | click the row |
| Update branch from base | `p`, then `j`/`k` and `Enter` | click the row |

## Key bar controls by view

Each row is one control of the key bar (or tab strip) in a probed view state, as
`footer::main_controls` builds it. The test probes these states: Inbox without an account,
with an empty queue and with three PRs; a PR on each of its eight tabs; Repository without a
repository and in Overview, Graph, Tree, Working, Staged and Uncommitted modes; Search and
Search corpus; Actions without an account, on each catalogue, on a finished run (Run), its
Logs, an Artifact, a Workflow definition and Publications; Dashboard before it loads. A disabled control that shares its key with an enabled one
on the same page (`X Enable workflow` beside `X Disable workflow`) lists that key; the key does
the enabled control and Commands explains the disabled one.
A loaded Dashboard's keys are covered by
`design_help_v97_dashboard_help_and_commands_cover_every_footer_key`.

| State | Key | Label | Click path |
| --- | --- | --- | --- |
| Inbox (no account) | `r` | Refresh | key bar, Commands |
| Inbox (no account) | `s` | Settings | key bar, Commands |
| Inbox (no account) | `?` | Help | key bar, Commands |
| Inbox (no account) | `:` | Commands | key bar, Commands |
| Inbox (no account) | `q` | Quit | key bar, Commands |
| Inbox (empty queue) | `Enter` | Open | Commands (explains) |
| Inbox (empty queue) | `r` | Refresh | key bar, Commands |
| Inbox (empty queue) | `F8` | Notifications | key bar, Commands |
| Inbox (empty queue) | `x` | Clear cache | key bar, Commands |
| Inbox (empty queue) | `a` | All | tab strip, Commands |
| Inbox (empty queue) | `c` | Created by me | tab strip, Commands |
| Inbox (empty queue) | `m` | Assigned to me | tab strip, Commands |
| Inbox (empty queue) | `v` | Needs my review | tab strip, Commands |
| Inbox (empty queue) | `?` | Help | key bar, Commands |
| Inbox (empty queue) | `:` | Commands | key bar, Commands |
| Inbox (empty queue) | `q` | Quit | key bar, Commands |
| Inbox (3 PRs) | `Enter` | Open | key bar, Commands |
| Inbox (3 PRs) | `r` | Refresh | key bar, Commands |
| Inbox (3 PRs) | `F8` | Notifications | key bar, Commands |
| Inbox (3 PRs) | `x` | Clear cache | key bar, Commands |
| Inbox (3 PRs) | `a` | All | tab strip, Commands |
| Inbox (3 PRs) | `c` | Created by me | tab strip, Commands |
| Inbox (3 PRs) | `m` | Assigned to me | tab strip, Commands |
| Inbox (3 PRs) | `v` | Needs my review | tab strip, Commands |
| Inbox (3 PRs) | `?` | Help | key bar, Commands |
| Inbox (3 PRs) | `:` | Commands | key bar, Commands |
| Inbox (3 PRs) | `q` | Quit | key bar, Commands |
| Inbox (3 PRs) | `y` | Copy | key bar, Commands |
| PR Summary | `b` | Back | key bar, Commands |
| PR Summary | `p` | PR actions | key bar, Commands |
| PR Summary | `J` | Thread history | key bar, Commands |
| PR Summary | `F9` | Review history | key bar, Commands |
| PR Summary | `r` | Refresh | key bar, Commands |
| PR Summary | `x` | Clear cache | key bar, Commands |
| PR Summary | `@` | Actor profile | key bar, Commands |
| PR Summary | `o` | Summary | tab strip, Commands |
| PR Summary | `c` | Conversation | tab strip, Commands |
| PR Summary | `h` | Commits | tab strip, Commands |
| PR Summary | `f` | Files | tab strip, Commands |
| PR Summary | `k` | Checks | tab strip, Commands |
| PR Summary | `t` | Timeline | tab strip, Commands |
| PR Summary | `u` | Metadata | tab strip, Commands |
| PR Summary | `v` | Review progress | tab strip, Commands |
| PR Summary | `?` | Help | key bar, Commands |
| PR Summary | `:` | Commands | key bar, Commands |
| PR Summary | `q` | Quit | key bar, Commands |
| PR Summary | `y` | Copy | key bar, Commands |
| PR Conversation | `b` | Back | key bar, Commands |
| PR Conversation | `p` | PR actions | key bar, Commands |
| PR Conversation | `r` | Refresh | key bar, Commands |
| PR Conversation | `x` | Clear cache | key bar, Commands |
| PR Conversation | `@` | Actor profile | key bar, Commands |
| PR Conversation | `o` | Summary | tab strip, Commands |
| PR Conversation | `c` | Conversation | tab strip, Commands |
| PR Conversation | `h` | Commits | tab strip, Commands |
| PR Conversation | `f` | Files | tab strip, Commands |
| PR Conversation | `k` | Checks | tab strip, Commands |
| PR Conversation | `t` | Timeline | tab strip, Commands |
| PR Conversation | `u` | Metadata | tab strip, Commands |
| PR Conversation | `v` | Review progress | tab strip, Commands |
| PR Conversation | `?` | Help | key bar, Commands |
| PR Conversation | `:` | Commands | key bar, Commands |
| PR Conversation | `q` | Quit | key bar, Commands |
| PR Conversation | `n` | Comment | key bar, Commands |
| PR Conversation | `A` | Approve | key bar, Commands |
| PR Conversation | `X` | Request changes | key bar, Commands |
| PR Conversation | `M` | Merge | key bar, Commands |
| PR Conversation | `Z` | Close | key bar, Commands |
| PR Conversation | `Y` | Ready | Commands (explains) |
| PR Conversation | `Q` | Quote reply | Commands (explains) |
| PR Conversation | `+` | React | Commands (explains) |
| PR Conversation | `E` | Edit | Commands (explains) |
| PR Conversation | `D` | Delete | Commands (explains) |
| PR Conversation | `y` | Copy | key bar, Commands |
| PR Commits | `b` | Back | key bar, Commands |
| PR Commits | `p` | PR actions | key bar, Commands |
| PR Commits | `r` | Refresh | key bar, Commands |
| PR Commits | `x` | Clear cache | key bar, Commands |
| PR Commits | `@` | Actor profile | key bar, Commands |
| PR Commits | `o` | Summary | tab strip, Commands |
| PR Commits | `c` | Conversation | tab strip, Commands |
| PR Commits | `h` | Commits | tab strip, Commands |
| PR Commits | `f` | Files | tab strip, Commands |
| PR Commits | `k` | Checks | tab strip, Commands |
| PR Commits | `t` | Timeline | tab strip, Commands |
| PR Commits | `u` | Metadata | tab strip, Commands |
| PR Commits | `v` | Review progress | tab strip, Commands |
| PR Commits | `?` | Help | key bar, Commands |
| PR Commits | `:` | Commands | key bar, Commands |
| PR Commits | `q` | Quit | key bar, Commands |
| PR Commits | `y` | Copy | key bar, Commands |
| PR Files | `b` | Back | key bar, Commands |
| PR Files | `p` | PR actions | key bar, Commands |
| PR Files | `f` | File history | Commands (explains) |
| PR Files | `E` | Edit file | Commands (explains) |
| PR Files | `R` | Resolve conflicts | Commands (explains) |
| PR Files | `r` | Refresh | key bar, Commands |
| PR Files | `x` | Clear cache | key bar, Commands |
| PR Files | `o` | Summary | tab strip, Commands |
| PR Files | `c` | Conversation | tab strip, Commands |
| PR Files | `h` | Commits | tab strip, Commands |
| PR Files | `f` | Files | tab strip, Commands |
| PR Files | `k` | Checks | tab strip, Commands |
| PR Files | `t` | Timeline | tab strip, Commands |
| PR Files | `u` | Metadata | tab strip, Commands |
| PR Files | `v` | Review progress | tab strip, Commands |
| PR Files | `?` | Help | key bar, Commands |
| PR Files | `:` | Commands | key bar, Commands |
| PR Files | `q` | Quit | key bar, Commands |
| PR Files | `y` | Copy | key bar, Commands |
| PR Checks | `b` | Back | key bar, Commands |
| PR Checks | `Enter` | Open check | Commands (explains) |
| PR Checks | `p` | PR actions | key bar, Commands |
| PR Checks | `r` | Refresh | key bar, Commands |
| PR Checks | `x` | Clear cache | key bar, Commands |
| PR Checks | `o` | Summary | tab strip, Commands |
| PR Checks | `c` | Conversation | tab strip, Commands |
| PR Checks | `h` | Commits | tab strip, Commands |
| PR Checks | `f` | Files | tab strip, Commands |
| PR Checks | `k` | Checks | tab strip, Commands |
| PR Checks | `t` | Timeline | tab strip, Commands |
| PR Checks | `u` | Metadata | tab strip, Commands |
| PR Checks | `v` | Review progress | tab strip, Commands |
| PR Checks | `?` | Help | key bar, Commands |
| PR Checks | `:` | Commands | key bar, Commands |
| PR Checks | `q` | Quit | key bar, Commands |
| PR Checks | `y` | Copy | key bar, Commands |
| PR Timeline | `b` | Back | key bar, Commands |
| PR Timeline | `Enter` | Go to item | Commands (explains) |
| PR Timeline | `p` | PR actions | key bar, Commands |
| PR Timeline | `r` | Refresh | key bar, Commands |
| PR Timeline | `x` | Clear cache | key bar, Commands |
| PR Timeline | `o` | Summary | tab strip, Commands |
| PR Timeline | `c` | Conversation | tab strip, Commands |
| PR Timeline | `h` | Commits | tab strip, Commands |
| PR Timeline | `f` | Files | tab strip, Commands |
| PR Timeline | `k` | Checks | tab strip, Commands |
| PR Timeline | `t` | Timeline | tab strip, Commands |
| PR Timeline | `u` | Metadata | tab strip, Commands |
| PR Timeline | `v` | Review progress | tab strip, Commands |
| PR Timeline | `?` | Help | key bar, Commands |
| PR Timeline | `:` | Commands | key bar, Commands |
| PR Timeline | `q` | Quit | key bar, Commands |
| PR Timeline | `y` | Copy | key bar, Commands |
| PR Metadata | `b` | Back | key bar, Commands |
| PR Metadata | `p` | PR actions | key bar, Commands |
| PR Metadata | `r` | Refresh | key bar, Commands |
| PR Metadata | `x` | Clear cache | key bar, Commands |
| PR Metadata | `@` | Actor profile | key bar, Commands |
| PR Metadata | `o` | Summary | tab strip, Commands |
| PR Metadata | `c` | Conversation | tab strip, Commands |
| PR Metadata | `h` | Commits | tab strip, Commands |
| PR Metadata | `f` | Files | tab strip, Commands |
| PR Metadata | `k` | Checks | tab strip, Commands |
| PR Metadata | `t` | Timeline | tab strip, Commands |
| PR Metadata | `u` | Metadata | tab strip, Commands |
| PR Metadata | `v` | Review progress | tab strip, Commands |
| PR Metadata | `?` | Help | key bar, Commands |
| PR Metadata | `:` | Commands | key bar, Commands |
| PR Metadata | `q` | Quit | key bar, Commands |
| PR Metadata | `y` | Copy | key bar, Commands |
| PR Review progress | `b` | Back | key bar, Commands |
| PR Review progress | `p` | PR actions | key bar, Commands |
| PR Review progress | `r` | Refresh | key bar, Commands |
| PR Review progress | `x` | Clear cache | key bar, Commands |
| PR Review progress | `o` | Summary | tab strip, Commands |
| PR Review progress | `c` | Conversation | tab strip, Commands |
| PR Review progress | `h` | Commits | tab strip, Commands |
| PR Review progress | `f` | Files | tab strip, Commands |
| PR Review progress | `k` | Checks | tab strip, Commands |
| PR Review progress | `t` | Timeline | tab strip, Commands |
| PR Review progress | `u` | Metadata | tab strip, Commands |
| PR Review progress | `v` | Review progress | tab strip, Commands |
| PR Review progress | `?` | Help | key bar, Commands |
| PR Review progress | `:` | Commands | key bar, Commands |
| PR Review progress | `q` | Quit | key bar, Commands |
| PR Review progress | `y` | Copy | key bar, Commands |
| Repository (none) | `r` | Refresh | key bar, Commands |
| Repository (none) | `s` | Settings | key bar, Commands |
| Repository (none) | `?` | Help | key bar, Commands |
| Repository (none) | `:` | Commands | key bar, Commands |
| Repository (none) | `q` | Quit | key bar, Commands |
| Repository Overview | `o` | Overview | tab strip, Commands |
| Repository Overview | `g` | History | tab strip, Commands |
| Repository Overview | `t` | Tree | tab strip, Commands |
| Repository Overview | `w` | Working | tab strip, Commands |
| Repository Overview | `i` | Staged | tab strip, Commands |
| Repository Overview | `b` | Branch | tab strip, Commands |
| Repository Overview | `d` | Changes | tab strip, Commands |
| Repository Overview | `e` | All edits | tab strip, Commands |
| Repository Overview | `u` | File back | Commands (explains) |
| Repository Overview | `+` | Stage | key bar, Commands |
| Repository Overview | `-` | Unstage | key bar, Commands |
| Repository Overview | `K` | Commit | key bar, Commands |
| Repository Overview | `f` | File history | Commands (explains) |
| Repository Overview | `Y` | Blame | Commands (explains) |
| Repository Overview | `P` | Previous | Commands (explains) |
| Repository Overview | `N` | Next | Commands (explains) |
| Repository Overview | `C` | Compare | key bar, Commands |
| Repository Overview | `m` | Local operations | key bar, Commands |
| Repository Overview | `J` | Jump | Commands (explains) |
| Repository Overview | `l` | Legend | key bar, Commands |
| Repository Overview | `a` | Author | key bar, Commands |
| Repository Overview | `c` | Committer | key bar, Commands |
| Repository Overview | `r` | Refresh | key bar, Commands |
| Repository Overview | `s` | Settings | key bar, Commands |
| Repository Overview | `?` | Help | key bar, Commands |
| Repository Overview | `:` | Commands | key bar, Commands |
| Repository Overview | `q` | Quit | key bar, Commands |
| Repository Overview | `p` | Push | key bar, Commands |
| Repository Overview | `y` | Copy | key bar, Commands |
| Repository Graph | `o` | Overview | tab strip, Commands |
| Repository Graph | `g` | History | tab strip, Commands |
| Repository Graph | `t` | Tree | tab strip, Commands |
| Repository Graph | `w` | Working | tab strip, Commands |
| Repository Graph | `i` | Staged | tab strip, Commands |
| Repository Graph | `b` | Branch | tab strip, Commands |
| Repository Graph | `d` | Changes | tab strip, Commands |
| Repository Graph | `e` | All edits | tab strip, Commands |
| Repository Graph | `u` | File back | Commands (explains) |
| Repository Graph | `/` | Search | key bar, Commands |
| Repository Graph | `n` | Next match | Commands (explains) |
| Repository Graph | `N` | Previous match | Commands (explains) |
| Repository Graph | `B` | Branch scope | key bar, Commands |
| Repository Graph | `h` | Hide non-matches | Commands (explains) |
| Repository Graph | `L` | Load more | Commands (explains) |
| Repository Graph | `D` | Details | key bar, Commands |
| Repository Graph | `P` | Parents | key bar, Commands |
| Repository Graph | `<` | Newer loaded | Commands (explains) |
| Repository Graph | `>` | Older loaded | Commands (explains) |
| Repository Graph | `Home` | First loaded | key bar, Commands |
| Repository Graph | `End` | Last loaded | key bar, Commands |
| Repository Graph | `x` | Discard graph Back | key bar, Commands |
| Repository Graph | `l` | Legend | key bar, Commands |
| Repository Graph | `←` | Pan left | key bar, Commands |
| Repository Graph | `→` | Pan right | key bar, Commands |
| Repository Graph | `R` | Re-read original | key bar, Commands |
| Repository Graph | `f` | File history | Commands (explains) |
| Repository Graph | `Y` | Blame | Commands (explains) |
| Repository Graph | `C` | Compare | key bar, Commands |
| Repository Graph | `m` | Local operations | key bar, Commands |
| Repository Graph | `J` | Jump | Commands (explains) |
| Repository Graph | `a` | Author | key bar, Commands |
| Repository Graph | `c` | Committer | key bar, Commands |
| Repository Graph | `r` | Refresh | key bar, Commands |
| Repository Graph | `s` | Settings | key bar, Commands |
| Repository Graph | `?` | Help | key bar, Commands |
| Repository Graph | `:` | Commands | key bar, Commands |
| Repository Graph | `q` | Quit | key bar, Commands |
| Repository Graph | `y` | Copy | key bar, Commands |
| Repository Tree | `o` | Overview | tab strip, Commands |
| Repository Tree | `g` | History | tab strip, Commands |
| Repository Tree | `t` | Tree | tab strip, Commands |
| Repository Tree | `w` | Working | tab strip, Commands |
| Repository Tree | `i` | Staged | tab strip, Commands |
| Repository Tree | `b` | Branch | tab strip, Commands |
| Repository Tree | `d` | Changes | tab strip, Commands |
| Repository Tree | `e` | All edits | tab strip, Commands |
| Repository Tree | `u` | File back | Commands (explains) |
| Repository Tree | `f` | File history | Commands (explains) |
| Repository Tree | `Y` | Blame | Commands (explains) |
| Repository Tree | `P` | Previous | Commands (explains) |
| Repository Tree | `N` | Next | Commands (explains) |
| Repository Tree | `C` | Compare | key bar, Commands |
| Repository Tree | `m` | Local operations | key bar, Commands |
| Repository Tree | `J` | Jump | key bar, Commands |
| Repository Tree | `l` | Legend | key bar, Commands |
| Repository Tree | `a` | Author | key bar, Commands |
| Repository Tree | `c` | Committer | key bar, Commands |
| Repository Tree | `r` | Refresh | key bar, Commands |
| Repository Tree | `s` | Settings | key bar, Commands |
| Repository Tree | `?` | Help | key bar, Commands |
| Repository Tree | `:` | Commands | key bar, Commands |
| Repository Tree | `q` | Quit | key bar, Commands |
| Repository Tree | `y` | Copy | key bar, Commands |
| Repository Working | `o` | Overview | tab strip, Commands |
| Repository Working | `g` | History | tab strip, Commands |
| Repository Working | `t` | Tree | tab strip, Commands |
| Repository Working | `w` | Working | tab strip, Commands |
| Repository Working | `i` | Staged | tab strip, Commands |
| Repository Working | `b` | Branch | tab strip, Commands |
| Repository Working | `d` | Changes | tab strip, Commands |
| Repository Working | `e` | All edits | tab strip, Commands |
| Repository Working | `u` | File back | Commands (explains) |
| Repository Working | `+` | Stage | key bar, Commands |
| Repository Working | `-` | Unstage | key bar, Commands |
| Repository Working | `K` | Commit | key bar, Commands |
| Repository Working | `h` | Hunk | key bar, Commands |
| Repository Working | `v` | Native diff | key bar, Commands |
| Repository Working | `f` | File history | Commands (explains) |
| Repository Working | `Y` | Blame | Commands (explains) |
| Repository Working | `P` | Previous | Commands (explains) |
| Repository Working | `N` | Next | Commands (explains) |
| Repository Working | `C` | Compare | key bar, Commands |
| Repository Working | `m` | Local operations | key bar, Commands |
| Repository Working | `J` | Jump | Commands (explains) |
| Repository Working | `l` | Legend | key bar, Commands |
| Repository Working | `a` | Author | key bar, Commands |
| Repository Working | `c` | Committer | key bar, Commands |
| Repository Working | `r` | Refresh | key bar, Commands |
| Repository Working | `s` | Settings | key bar, Commands |
| Repository Working | `?` | Help | key bar, Commands |
| Repository Working | `:` | Commands | key bar, Commands |
| Repository Working | `q` | Quit | key bar, Commands |
| Repository Working | `y` | Copy | key bar, Commands |
| Repository Staged | `o` | Overview | tab strip, Commands |
| Repository Staged | `g` | History | tab strip, Commands |
| Repository Staged | `t` | Tree | tab strip, Commands |
| Repository Staged | `w` | Working | tab strip, Commands |
| Repository Staged | `i` | Staged | tab strip, Commands |
| Repository Staged | `b` | Branch | tab strip, Commands |
| Repository Staged | `d` | Changes | tab strip, Commands |
| Repository Staged | `e` | All edits | tab strip, Commands |
| Repository Staged | `u` | File back | Commands (explains) |
| Repository Staged | `+` | Stage | key bar, Commands |
| Repository Staged | `-` | Unstage | key bar, Commands |
| Repository Staged | `K` | Commit | key bar, Commands |
| Repository Staged | `h` | Hunk | key bar, Commands |
| Repository Staged | `v` | Native diff | key bar, Commands |
| Repository Staged | `f` | File history | Commands (explains) |
| Repository Staged | `Y` | Blame | Commands (explains) |
| Repository Staged | `P` | Previous | Commands (explains) |
| Repository Staged | `N` | Next | Commands (explains) |
| Repository Staged | `C` | Compare | key bar, Commands |
| Repository Staged | `m` | Local operations | key bar, Commands |
| Repository Staged | `J` | Jump | Commands (explains) |
| Repository Staged | `l` | Legend | key bar, Commands |
| Repository Staged | `a` | Author | key bar, Commands |
| Repository Staged | `c` | Committer | key bar, Commands |
| Repository Staged | `r` | Refresh | key bar, Commands |
| Repository Staged | `s` | Settings | key bar, Commands |
| Repository Staged | `?` | Help | key bar, Commands |
| Repository Staged | `:` | Commands | key bar, Commands |
| Repository Staged | `q` | Quit | key bar, Commands |
| Repository Staged | `y` | Copy | key bar, Commands |
| Repository Uncommitted | `o` | Overview | tab strip, Commands |
| Repository Uncommitted | `g` | History | tab strip, Commands |
| Repository Uncommitted | `t` | Tree | tab strip, Commands |
| Repository Uncommitted | `w` | Working | tab strip, Commands |
| Repository Uncommitted | `i` | Staged | tab strip, Commands |
| Repository Uncommitted | `b` | Branch | tab strip, Commands |
| Repository Uncommitted | `d` | Changes | tab strip, Commands |
| Repository Uncommitted | `e` | All edits | tab strip, Commands |
| Repository Uncommitted | `u` | File back | Commands (explains) |
| Repository Uncommitted | `+` | Stage | key bar, Commands |
| Repository Uncommitted | `-` | Unstage | key bar, Commands |
| Repository Uncommitted | `K` | Commit | key bar, Commands |
| Repository Uncommitted | `v` | Native diff | key bar, Commands |
| Repository Uncommitted | `f` | File history | Commands (explains) |
| Repository Uncommitted | `Y` | Blame | Commands (explains) |
| Repository Uncommitted | `P` | Previous | Commands (explains) |
| Repository Uncommitted | `N` | Next | Commands (explains) |
| Repository Uncommitted | `C` | Compare | key bar, Commands |
| Repository Uncommitted | `m` | Local operations | key bar, Commands |
| Repository Uncommitted | `J` | Jump | Commands (explains) |
| Repository Uncommitted | `l` | Legend | key bar, Commands |
| Repository Uncommitted | `a` | Author | key bar, Commands |
| Repository Uncommitted | `c` | Committer | key bar, Commands |
| Repository Uncommitted | `r` | Refresh | key bar, Commands |
| Repository Uncommitted | `s` | Settings | key bar, Commands |
| Repository Uncommitted | `?` | Help | key bar, Commands |
| Repository Uncommitted | `:` | Commands | key bar, Commands |
| Repository Uncommitted | `q` | Quit | key bar, Commands |
| Search | `F` | Fuzzy | tab strip, Commands |
| Repository Uncommitted | `y` | Copy | key bar, Commands |
| Search | `e` | Exact | tab strip, Commands |
| Search | `x` | Regex | tab strip, Commands |
| Search | `n` | Ranked | tab strip, Commands |
| Search | `h` | GitHub | tab strip, Commands |
| Search | `w` | Working files | tab strip, Commands |
| Search | `v` | Saved version | tab strip, Commands |
| Search | `Enter` | Search | key bar, Commands |
| Search | `o` | Open result | Commands (explains) |
| Search | `c` | Corpus | key bar, Commands |
| Search | `f` | File history | Commands (explains) |
| Search | `M` | Repositories | key bar, Commands |
| Search | `O` | Options | key bar, Commands |
| Search | `S` | Save query | key bar, Commands |
| Search | `H` | Sources | key bar, Commands |
| Search | `B` | Blend | key bar, Commands |
| Search | `b` | Blend GitHub: off | Commands (explains) |
| Search | `r` | Refresh | key bar, Commands |
| Search | `s` | Settings | key bar, Commands |
| Search | `?` | Help | key bar, Commands |
| Search | `:` | Commands | key bar, Commands |
| Search | `q` | Quit | key bar, Commands |
| Search (corpus) | `F` | Fuzzy | tab strip, Commands |
| Search (corpus) | `e` | Exact | tab strip, Commands |
| Search (corpus) | `x` | Regex | tab strip, Commands |
| Search (corpus) | `n` | Ranked | tab strip, Commands |
| Search (corpus) | `h` | GitHub | tab strip, Commands |
| Search (corpus) | `w` | Working files | tab strip, Commands |
| Search (corpus) | `v` | Saved version | tab strip, Commands |
| Search (corpus) | `Enter` | Toggle | key bar, Commands |
| Search (corpus) | `b` | Hidden files: off | key bar, Commands |
| Search (corpus) | `i` | Ignored files: off | key bar, Commands |
| Search (corpus) | `o` | Open result | Commands (explains) |
| Search (corpus) | `c` | Corpus | key bar, Commands |
| Search (corpus) | `f` | File history | Commands (explains) |
| Search (corpus) | `M` | Repositories | key bar, Commands |
| Search (corpus) | `O` | Options | key bar, Commands |
| Search (corpus) | `S` | Save query | key bar, Commands |
| Search (corpus) | `H` | Sources | key bar, Commands |
| Search (corpus) | `B` | Blend | key bar, Commands |
| Search (corpus) | `r` | Refresh | key bar, Commands |
| Search (corpus) | `s` | Settings | key bar, Commands |
| Search (corpus) | `?` | Help | key bar, Commands |
| Search (corpus) | `:` | Commands | key bar, Commands |
| Search (corpus) | `q` | Quit | key bar, Commands |
| Actions (no account) | `r` | Refresh | key bar, Commands |
| Actions (no account) | `s` | Settings | key bar, Commands |
| Actions (no account) | `?` | Help | key bar, Commands |
| Actions (no account) | `:` | Commands | key bar, Commands |
| Actions (no account) | `q` | Quit | key bar, Commands |
| Actions Runs | `R` | Runs | tab strip, Commands |
| Actions Runs | `W` | Workflows | tab strip, Commands |
| Actions Runs | `c` | Caches | tab strip, Commands |
| Actions Runs | `S` | Secrets | tab strip, Commands |
| Actions Runs | `v` | Variables | tab strip, Commands |
| Actions Runs | `e` | Environments | tab strip, Commands |
| Actions Runs | `p` | Policies | tab strip, Commands |
| Actions Runs | `n` | Runners | tab strip, Commands |
| Actions Runs | `g` | Runner groups | tab strip, Commands |
| Actions Runs | `u` | Usage | tab strip, Commands |
| Actions Runs | `G` | Approvals | tab strip, Commands |
| Actions Runs | `r` | Refresh | key bar, Commands |
| Actions Runs | `P` | Publications | key bar, Commands |
| Actions Runs | `o` | Choose Actions scope | key bar, Commands |
| Actions Runs | `f` | Filter runs | key bar, Commands |
| Actions Runs | `w` | Watch | key bar, Commands |
| Actions Runs | `{` | Previous repo | key bar, Commands |
| Actions Runs | `}` | Next repo | key bar, Commands |
| Actions Runs | `?` | Help | key bar, Commands |
| Actions Runs | `:` | Commands | key bar, Commands |
| Actions Runs | `q` | Quit | key bar, Commands |
| Actions Runs | `y` | Copy | key bar, Commands |
| Actions Workflows | `R` | Runs | tab strip, Commands |
| Actions Workflows | `W` | Workflows | tab strip, Commands |
| Actions Workflows | `c` | Caches | tab strip, Commands |
| Actions Workflows | `S` | Secrets | tab strip, Commands |
| Actions Workflows | `v` | Variables | tab strip, Commands |
| Actions Workflows | `e` | Environments | tab strip, Commands |
| Actions Workflows | `p` | Policies | tab strip, Commands |
| Actions Workflows | `n` | Runners | tab strip, Commands |
| Actions Workflows | `g` | Runner groups | tab strip, Commands |
| Actions Workflows | `u` | Usage | tab strip, Commands |
| Actions Workflows | `G` | Approvals | tab strip, Commands |
| Actions Workflows | `r` | Refresh | key bar, Commands |
| Actions Workflows | `P` | Publications | key bar, Commands |
| Actions Workflows | `b` | Back | key bar, Commands |
| Actions Workflows | `o` | Choose Actions scope | key bar, Commands |
| Actions Workflows | `{` | Previous repo | key bar, Commands |
| Actions Workflows | `}` | Next repo | key bar, Commands |
| Actions Workflows | `?` | Help | key bar, Commands |
| Actions Workflows | `:` | Commands | key bar, Commands |
| Actions Workflows | `q` | Quit | key bar, Commands |
| Actions Workflows | `y` | Copy | key bar, Commands |
| Actions Caches | `R` | Runs | tab strip, Commands |
| Actions Caches | `W` | Workflows | tab strip, Commands |
| Actions Caches | `c` | Caches | tab strip, Commands |
| Actions Caches | `S` | Secrets | tab strip, Commands |
| Actions Caches | `v` | Variables | tab strip, Commands |
| Actions Caches | `e` | Environments | tab strip, Commands |
| Actions Caches | `p` | Policies | tab strip, Commands |
| Actions Caches | `n` | Runners | tab strip, Commands |
| Actions Caches | `g` | Runner groups | tab strip, Commands |
| Actions Caches | `u` | Usage | tab strip, Commands |
| Actions Caches | `G` | Approvals | tab strip, Commands |
| Actions Caches | `r` | Refresh | key bar, Commands |
| Actions Caches | `P` | Publications | key bar, Commands |
| Actions Caches | `b` | Back | key bar, Commands |
| Actions Caches | `o` | Choose Actions scope | key bar, Commands |
| Actions Caches | `f` | Filter caches | key bar, Commands |
| Actions Caches | `-` | Delete selected cache | Commands (explains) |
| Actions Caches | `{` | Previous repo | key bar, Commands |
| Actions Caches | `}` | Next repo | key bar, Commands |
| Actions Caches | `?` | Help | key bar, Commands |
| Actions Caches | `:` | Commands | key bar, Commands |
| Actions Caches | `q` | Quit | key bar, Commands |
| Actions Caches | `y` | Copy | key bar, Commands |
| Actions Secrets | `R` | Runs | tab strip, Commands |
| Actions Secrets | `W` | Workflows | tab strip, Commands |
| Actions Secrets | `c` | Caches | tab strip, Commands |
| Actions Secrets | `S` | Secrets | tab strip, Commands |
| Actions Secrets | `v` | Variables | tab strip, Commands |
| Actions Secrets | `e` | Environments | tab strip, Commands |
| Actions Secrets | `p` | Policies | tab strip, Commands |
| Actions Secrets | `n` | Runners | tab strip, Commands |
| Actions Secrets | `g` | Runner groups | tab strip, Commands |
| Actions Secrets | `u` | Usage | tab strip, Commands |
| Actions Secrets | `G` | Approvals | tab strip, Commands |
| Actions Secrets | `r` | Refresh | key bar, Commands |
| Actions Secrets | `P` | Publications | key bar, Commands |
| Actions Secrets | `b` | Back | key bar, Commands |
| Actions Secrets | `o` | Choose Actions scope | key bar, Commands |
| Actions Secrets | `=` | Set secret | key bar, Commands |
| Actions Secrets | `-` | Delete secret | Commands (explains) |
| Actions Secrets | `{` | Previous repo | key bar, Commands |
| Actions Secrets | `}` | Next repo | key bar, Commands |
| Actions Secrets | `?` | Help | key bar, Commands |
| Actions Secrets | `:` | Commands | key bar, Commands |
| Actions Secrets | `q` | Quit | key bar, Commands |
| Actions Secrets | `y` | Copy | key bar, Commands |
| Actions Variables | `R` | Runs | tab strip, Commands |
| Actions Variables | `W` | Workflows | tab strip, Commands |
| Actions Variables | `c` | Caches | tab strip, Commands |
| Actions Variables | `S` | Secrets | tab strip, Commands |
| Actions Variables | `v` | Variables | tab strip, Commands |
| Actions Variables | `e` | Environments | tab strip, Commands |
| Actions Variables | `p` | Policies | tab strip, Commands |
| Actions Variables | `n` | Runners | tab strip, Commands |
| Actions Variables | `g` | Runner groups | tab strip, Commands |
| Actions Variables | `u` | Usage | tab strip, Commands |
| Actions Variables | `G` | Approvals | tab strip, Commands |
| Actions Variables | `r` | Refresh | key bar, Commands |
| Actions Variables | `P` | Publications | key bar, Commands |
| Actions Variables | `b` | Back | key bar, Commands |
| Actions Variables | `o` | Choose Actions scope | key bar, Commands |
| Actions Variables | `=` | Set variable | key bar, Commands |
| Actions Variables | `-` | Delete variable | Commands (explains) |
| Actions Variables | `{` | Previous repo | key bar, Commands |
| Actions Variables | `}` | Next repo | key bar, Commands |
| Actions Variables | `?` | Help | key bar, Commands |
| Actions Variables | `:` | Commands | key bar, Commands |
| Actions Variables | `q` | Quit | key bar, Commands |
| Actions Variables | `y` | Copy | key bar, Commands |
| Actions Environments | `R` | Runs | tab strip, Commands |
| Actions Environments | `W` | Workflows | tab strip, Commands |
| Actions Environments | `c` | Caches | tab strip, Commands |
| Actions Environments | `S` | Secrets | tab strip, Commands |
| Actions Environments | `v` | Variables | tab strip, Commands |
| Actions Environments | `e` | Environments | tab strip, Commands |
| Actions Environments | `p` | Policies | tab strip, Commands |
| Actions Environments | `n` | Runners | tab strip, Commands |
| Actions Environments | `g` | Runner groups | tab strip, Commands |
| Actions Environments | `u` | Usage | tab strip, Commands |
| Actions Environments | `G` | Approvals | tab strip, Commands |
| Actions Environments | `r` | Refresh | key bar, Commands |
| Actions Environments | `P` | Publications | key bar, Commands |
| Actions Environments | `b` | Back | key bar, Commands |
| Actions Environments | `o` | Choose Actions scope | key bar, Commands |
| Actions Environments | `M` | Edit selected environment protection | Commands (explains) |
| Actions Environments | `{` | Previous repo | key bar, Commands |
| Actions Environments | `}` | Next repo | key bar, Commands |
| Actions Environments | `?` | Help | key bar, Commands |
| Actions Environments | `:` | Commands | key bar, Commands |
| Actions Environments | `q` | Quit | key bar, Commands |
| Actions Environments | `y` | Copy | key bar, Commands |
| Actions Policies | `R` | Runs | tab strip, Commands |
| Actions Policies | `W` | Workflows | tab strip, Commands |
| Actions Policies | `c` | Caches | tab strip, Commands |
| Actions Policies | `S` | Secrets | tab strip, Commands |
| Actions Policies | `v` | Variables | tab strip, Commands |
| Actions Policies | `e` | Environments | tab strip, Commands |
| Actions Policies | `p` | Policies | tab strip, Commands |
| Actions Policies | `n` | Runners | tab strip, Commands |
| Actions Policies | `g` | Runner groups | tab strip, Commands |
| Actions Policies | `u` | Usage | tab strip, Commands |
| Actions Policies | `G` | Approvals | tab strip, Commands |
| Actions Policies | `r` | Refresh | key bar, Commands |
| Actions Policies | `P` | Publications | key bar, Commands |
| Actions Policies | `b` | Back | key bar, Commands |
| Actions Policies | `o` | Choose Actions scope | key bar, Commands |
| Actions Policies | `M` | Edit selected Actions policy | Commands (explains) |
| Actions Policies | `L` | Allowlist | key bar, Commands |
| Actions Policies | `T` | Selected actions | key bar, Commands |
| Actions Policies | `O` | Workflow token | key bar, Commands |
| Actions Policies | `H` | Fork workflow | key bar, Commands |
| Actions Policies | `Y` | Fork approval | key bar, Commands |
| Actions Policies | `U` | Retention | key bar, Commands |
| Actions Policies | `{` | Previous repo | key bar, Commands |
| Actions Policies | `}` | Next repo | key bar, Commands |
| Actions Policies | `?` | Help | key bar, Commands |
| Actions Policies | `:` | Commands | key bar, Commands |
| Actions Policies | `q` | Quit | key bar, Commands |
| Actions Policies | `y` | Copy | key bar, Commands |
| Actions Runners | `R` | Runs | tab strip, Commands |
| Actions Runners | `W` | Workflows | tab strip, Commands |
| Actions Runners | `c` | Caches | tab strip, Commands |
| Actions Runners | `S` | Secrets | tab strip, Commands |
| Actions Runners | `v` | Variables | tab strip, Commands |
| Actions Runners | `e` | Environments | tab strip, Commands |
| Actions Runners | `p` | Policies | tab strip, Commands |
| Actions Runners | `n` | Runners | tab strip, Commands |
| Actions Runners | `g` | Runner groups | tab strip, Commands |
| Actions Runners | `u` | Usage | tab strip, Commands |
| Actions Runners | `G` | Approvals | tab strip, Commands |
| Actions Runners | `r` | Refresh | key bar, Commands |
| Actions Runners | `P` | Publications | key bar, Commands |
| Actions Runners | `b` | Back | key bar, Commands |
| Actions Runners | `o` | Choose Actions scope | key bar, Commands |
| Actions Runners | `L` | Replace custom runner labels | Commands (explains) |
| Actions Runners | `-` | Remove selected runner | Commands (explains) |
| Actions Runners | `{` | Previous repo | key bar, Commands |
| Actions Runners | `}` | Next repo | key bar, Commands |
| Actions Runners | `?` | Help | key bar, Commands |
| Actions Runners | `:` | Commands | key bar, Commands |
| Actions Runners | `q` | Quit | key bar, Commands |
| Actions Runners | `y` | Copy | key bar, Commands |
| Actions Runner groups | `R` | Runs | tab strip, Commands |
| Actions Runner groups | `W` | Workflows | tab strip, Commands |
| Actions Runner groups | `c` | Caches | tab strip, Commands |
| Actions Runner groups | `S` | Secrets | tab strip, Commands |
| Actions Runner groups | `v` | Variables | tab strip, Commands |
| Actions Runner groups | `e` | Environments | tab strip, Commands |
| Actions Runner groups | `p` | Policies | tab strip, Commands |
| Actions Runner groups | `n` | Runners | tab strip, Commands |
| Actions Runner groups | `g` | Runner groups | tab strip, Commands |
| Actions Runner groups | `u` | Usage | tab strip, Commands |
| Actions Runner groups | `G` | Approvals | tab strip, Commands |
| Actions Runner groups | `r` | Refresh | key bar, Commands |
| Actions Runner groups | `P` | Publications | key bar, Commands |
| Actions Runner groups | `b` | Back | key bar, Commands |
| Actions Runner groups | `o` | Choose Actions scope | key bar, Commands |
| Actions Runner groups | `+` | Create runner group | key bar, Commands |
| Actions Runner groups | `M` | Edit selected runner group | Commands (explains) |
| Actions Runner groups | `T` | Replace group runners | Commands (explains) |
| Actions Runner groups | `O` | Replace group repositories | Commands (explains) |
| Actions Runner groups | `-` | Delete runner group | Commands (explains) |
| Actions Runner groups | `{` | Previous repo | key bar, Commands |
| Actions Runner groups | `}` | Next repo | key bar, Commands |
| Actions Runner groups | `?` | Help | key bar, Commands |
| Actions Runner groups | `:` | Commands | key bar, Commands |
| Actions Runner groups | `q` | Quit | key bar, Commands |
| Actions Runner groups | `y` | Copy | key bar, Commands |
| Actions Usage | `R` | Runs | tab strip, Commands |
| Actions Usage | `W` | Workflows | tab strip, Commands |
| Actions Usage | `c` | Caches | tab strip, Commands |
| Actions Usage | `S` | Secrets | tab strip, Commands |
| Actions Usage | `v` | Variables | tab strip, Commands |
| Actions Usage | `e` | Environments | tab strip, Commands |
| Actions Usage | `p` | Policies | tab strip, Commands |
| Actions Usage | `n` | Runners | tab strip, Commands |
| Actions Usage | `g` | Runner groups | tab strip, Commands |
| Actions Usage | `u` | Usage | tab strip, Commands |
| Actions Usage | `G` | Approvals | tab strip, Commands |
| Actions Usage | `r` | Refresh | key bar, Commands |
| Actions Usage | `P` | Publications | key bar, Commands |
| Actions Usage | `b` | Back | key bar, Commands |
| Actions Usage | `o` | Choose Actions scope | key bar, Commands |
| Actions Usage | `{` | Previous repo | key bar, Commands |
| Actions Usage | `}` | Next repo | key bar, Commands |
| Actions Usage | `?` | Help | key bar, Commands |
| Actions Usage | `:` | Commands | key bar, Commands |
| Actions Usage | `q` | Quit | key bar, Commands |
| Actions Usage | `y` | Copy | key bar, Commands |
| Actions Approvals | `R` | Runs | tab strip, Commands |
| Actions Approvals | `W` | Workflows | tab strip, Commands |
| Actions Approvals | `c` | Caches | tab strip, Commands |
| Actions Approvals | `S` | Secrets | tab strip, Commands |
| Actions Approvals | `v` | Variables | tab strip, Commands |
| Actions Approvals | `e` | Environments | tab strip, Commands |
| Actions Approvals | `p` | Policies | tab strip, Commands |
| Actions Approvals | `n` | Runners | tab strip, Commands |
| Actions Approvals | `g` | Runner groups | tab strip, Commands |
| Actions Approvals | `u` | Usage | tab strip, Commands |
| Actions Approvals | `G` | Approvals | tab strip, Commands |
| Actions Approvals | `r` | Refresh | key bar, Commands |
| Actions Approvals | `P` | Publications | key bar, Commands |
| Actions Approvals | `b` | Back | key bar, Commands |
| Actions Approvals | `o` | Choose Actions scope | key bar, Commands |
| Actions Approvals | `Y` | Approve selected deployment | Commands (explains) |
| Actions Approvals | `B` | Reject selected deployment | Commands (explains) |
| Actions Approvals | `{` | Previous repo | key bar, Commands |
| Actions Approvals | `}` | Next repo | key bar, Commands |
| Actions Approvals | `?` | Help | key bar, Commands |
| Actions Approvals | `:` | Commands | key bar, Commands |
| Actions Approvals | `q` | Quit | key bar, Commands |
| Actions Approvals | `y` | Copy | key bar, Commands |
| Actions Run | `R` | Runs | tab strip, Commands |
| Actions Run | `W` | Workflows | tab strip, Commands |
| Actions Run | `c` | Caches | tab strip, Commands |
| Actions Run | `S` | Secrets | tab strip, Commands |
| Actions Run | `v` | Variables | tab strip, Commands |
| Actions Run | `e` | Environments | tab strip, Commands |
| Actions Run | `p` | Policies | tab strip, Commands |
| Actions Run | `n` | Runners | tab strip, Commands |
| Actions Run | `g` | Runner groups | tab strip, Commands |
| Actions Run | `u` | Usage | tab strip, Commands |
| Actions Run | `G` | Approvals | tab strip, Commands |
| Actions Run | `r` | Refresh | key bar, Commands |
| Actions Run | `P` | Publications | key bar, Commands |
| Actions Run | `b` | Back | key bar, Commands |
| Actions Run | `o` | Choose Actions scope | key bar, Commands |
| Actions Run | `j` | Jobs | key bar, Commands |
| Actions Run | `l` | Logs | key bar, Commands |
| Actions Run | `a` | Artifacts | key bar, Commands |
| Actions Run | `G` | Gates | key bar, Commands |
| Actions Run | `w` | Watch | key bar, Commands |
| Actions Run | `A` | Rerun all jobs | key bar, Commands |
| Actions Run | `F` | Rerun failed jobs | key bar, Commands |
| Actions Run | `J` | Rerun selected job | Commands (explains) |
| Actions Run | `C` | Cancel run | Commands (explains) |
| Actions Run | `!` | Force-cancel run | Commands (explains) |
| Actions Run | `-` | Delete run | key bar, Commands |
| Actions Run | `L` | Delete run logs | key bar, Commands |
| Actions Run | `Y` | Approve fork run | Commands (explains) |
| Actions Run | `@` | Actor profile | key bar, Commands |
| Actions Run | `{` | Previous repo | key bar, Commands |
| Actions Run | `}` | Next repo | key bar, Commands |
| Actions Run | `?` | Help | key bar, Commands |
| Actions Run | `:` | Commands | key bar, Commands |
| Actions Run | `q` | Quit | key bar, Commands |
| Actions Run | `y` | Copy | key bar, Commands |
| Actions Logs | `R` | Runs | tab strip, Commands |
| Actions Logs | `W` | Workflows | tab strip, Commands |
| Actions Logs | `c` | Caches | tab strip, Commands |
| Actions Logs | `S` | Secrets | tab strip, Commands |
| Actions Logs | `v` | Variables | tab strip, Commands |
| Actions Logs | `e` | Environments | tab strip, Commands |
| Actions Logs | `p` | Policies | tab strip, Commands |
| Actions Logs | `n` | Runners | tab strip, Commands |
| Actions Logs | `g` | Runner groups | tab strip, Commands |
| Actions Logs | `u` | Usage | tab strip, Commands |
| Actions Logs | `G` | Approvals | tab strip, Commands |
| Actions Logs | `r` | Refresh | key bar, Commands |
| Actions Logs | `P` | Publications | key bar, Commands |
| Actions Logs | `b` | Back | key bar, Commands |
| Actions Logs | `o` | Choose Actions scope | key bar, Commands |
| Actions Logs | `l` | Logs | key bar, Commands |
| Actions Logs | `/` | Search current logs | key bar, Commands |
| Actions Logs | `x` | Export log | key bar, Commands |
| Actions Logs | `{` | Previous repo | key bar, Commands |
| Actions Logs | `}` | Next repo | key bar, Commands |
| Actions Logs | `?` | Help | key bar, Commands |
| Actions Logs | `:` | Commands | key bar, Commands |
| Actions Logs | `q` | Quit | key bar, Commands |
| Actions Logs | `y` | Copy | key bar, Commands |
| Actions Artifact | `R` | Runs | tab strip, Commands |
| Actions Artifact | `W` | Workflows | tab strip, Commands |
| Actions Artifact | `c` | Caches | tab strip, Commands |
| Actions Artifact | `S` | Secrets | tab strip, Commands |
| Actions Artifact | `v` | Variables | tab strip, Commands |
| Actions Artifact | `e` | Environments | tab strip, Commands |
| Actions Artifact | `p` | Policies | tab strip, Commands |
| Actions Artifact | `n` | Runners | tab strip, Commands |
| Actions Artifact | `g` | Runner groups | tab strip, Commands |
| Actions Artifact | `u` | Usage | tab strip, Commands |
| Actions Artifact | `G` | Approvals | tab strip, Commands |
| Actions Artifact | `r` | Refresh | key bar, Commands |
| Actions Artifact | `P` | Publications | key bar, Commands |
| Actions Artifact | `b` | Back | key bar, Commands |
| Actions Artifact | `o` | Choose Actions scope | key bar, Commands |
| Actions Artifact | `X` | Export artifact ZIP | Commands (explains) |
| Actions Artifact | `U` | Extract artifact into a new directory | Commands (explains) |
| Actions Artifact | `-` | Delete artifact | key bar, Commands |
| Actions Artifact | `{` | Previous repo | key bar, Commands |
| Actions Artifact | `}` | Next repo | key bar, Commands |
| Actions Artifact | `?` | Help | key bar, Commands |
| Actions Artifact | `:` | Commands | key bar, Commands |
| Actions Artifact | `q` | Quit | key bar, Commands |
| Actions Artifact | `y` | Copy | key bar, Commands |
| Actions Workflow definition | `R` | Runs | tab strip, Commands |
| Actions Workflow definition | `W` | Workflows | tab strip, Commands |
| Actions Workflow definition | `c` | Caches | tab strip, Commands |
| Actions Workflow definition | `S` | Secrets | tab strip, Commands |
| Actions Workflow definition | `v` | Variables | tab strip, Commands |
| Actions Workflow definition | `e` | Environments | tab strip, Commands |
| Actions Workflow definition | `p` | Policies | tab strip, Commands |
| Actions Workflow definition | `n` | Runners | tab strip, Commands |
| Actions Workflow definition | `g` | Runner groups | tab strip, Commands |
| Actions Workflow definition | `u` | Usage | tab strip, Commands |
| Actions Workflow definition | `G` | Approvals | tab strip, Commands |
| Actions Workflow definition | `r` | Refresh | key bar, Commands |
| Actions Workflow definition | `P` | Publications | key bar, Commands |
| Actions Workflow definition | `b` | Back | key bar, Commands |
| Actions Workflow definition | `o` | Choose Actions scope | key bar, Commands |
| Actions Workflow definition | `V` | Choose workflow revision | key bar, Commands |
| Actions Workflow definition | `E` | Edit YAML locally | Commands (explains) |
| Actions Workflow definition | `X` | Enable workflow | Commands (explains) |
| Actions Workflow definition | `X` | Disable workflow | key bar, Commands |
| Actions Workflow definition | `D` | Dispatch workflow | key bar, Commands |
| Actions Workflow definition | `{` | Previous repo | key bar, Commands |
| Actions Workflow definition | `}` | Next repo | key bar, Commands |
| Actions Workflow definition | `?` | Help | key bar, Commands |
| Actions Workflow definition | `:` | Commands | key bar, Commands |
| Actions Workflow definition | `q` | Quit | key bar, Commands |
| Actions Workflow definition | `y` | Copy | key bar, Commands |
| Actions Publications | `R` | Runs | tab strip, Commands |
| Actions Publications | `W` | Workflows | tab strip, Commands |
| Actions Publications | `c` | Caches | tab strip, Commands |
| Actions Publications | `S` | Secrets | tab strip, Commands |
| Actions Publications | `v` | Variables | tab strip, Commands |
| Actions Publications | `e` | Environments | tab strip, Commands |
| Actions Publications | `p` | Policies | tab strip, Commands |
| Actions Publications | `n` | Runners | tab strip, Commands |
| Actions Publications | `g` | Runner groups | tab strip, Commands |
| Actions Publications | `u` | Usage | tab strip, Commands |
| Actions Publications | `G` | Approvals | tab strip, Commands |
| Actions Publications | `r` | Refresh | key bar, Commands |
| Actions Publications | `P` | Publications | key bar, Commands |
| Actions Publications | `b` | Back | key bar, Commands |
| Actions Publications | `o` | Choose Actions scope | key bar, Commands |
| Actions Publications | `{` | Previous repo | key bar, Commands |
| Actions Publications | `}` | Next repo | key bar, Commands |
| Actions Publications | `?` | Help | key bar, Commands |
| Actions Publications | `:` | Commands | key bar, Commands |
| Actions Publications | `q` | Quit | key bar, Commands |
| Actions Publications | `y` | Copy | key bar, Commands |
| Dashboard (not loaded) | `r` | Refresh | key bar, Commands |
| Dashboard (not loaded) | `s` | Settings | key bar, Commands |
| Dashboard (not loaded) | `?` | Help | key bar, Commands |
| Dashboard (not loaded) | `:` | Commands | key bar, Commands |
| Dashboard (not loaded) | `q` | Quit | key bar, Commands |

## Overlay controls

Each row is a control of a main overlay, probed at 120x40 and 80x24 by drawing the overlay,
clicking the control and pressing its key on fresh copies of the same state, and comparing the
effect. Commands (`:`) is probed entry by entry by
`coverage_v64_every_commands_entry_is_chosen_by_typing_enter_and_by_clicking_its_row`.

| Overlay | Control | Keys | Click path |
| --- | --- | --- | --- |
| Help | [PgUp Previous] | `PgUp` | click the button |
| Help | [PgDn More] | `PgDn` | click the button |
| Help | [Esc Close] | `Esc` | click the button |
| Explanation | [PgUp Previous] | `PgUp` | click the button |
| Explanation | [PgDn More] | `PgDn` | click the button |
| Explanation | [Esc Close] | `Esc` | click the button |
| Settings | [Theme: …] | `Tab` to it, `Enter` | click the row |
| Settings | [Density: …] | `Tab` to it, `Enter` | click the row |
| Settings | [Save settings] | `Tab` to it, `Enter` | click the button |
| Settings | [Esc Close] | `Esc` | click the button |
| Local operations | Menu rows | `j`/`k` or arrows, `Enter` | click the row |
| Local operations | [PgUp Previous] | `PgUp` | click the button |
| Local operations | [PgDn More] | `PgDn` | click the button |
| Local operations | [Esc Close] | `Esc` | click the button |
| PR actions | Menu rows | `j`/`k` or arrows, `Enter` | click the row |
| PR actions | [PgUp Previous] | `PgUp` | click the button |
| PR actions | [PgDn More] | `PgDn` | click the button |
| PR actions | [Esc Close] | `Esc` | click the button |

## Known gaps and limits (release notes)

- **PR branch editing (TASK-27)**: only existing regular text files; no new, deleted, binary, rename or mode-change edits; push permission is not pre-checked (TASK-108).
- **Live host compatibility (TASK-64 AC4)**: every journey above runs against strict offline
  fake-gh fixtures and local bare remotes. Opt-in verification against a disposable live
  repository has not been run; GitHub Enterprise minimum versions are unknown.
- **macOS**: native verification is deferred past the Linux v1.
- **No browser handoff, clipboard or PR export.** Full SHAs and URLs are shown in full to
  select in the terminal.
- **Merge and lifecycle (TASK-24, TASK-25)**: implemented and covered above; each task keeps
  one acceptance criterion open.
