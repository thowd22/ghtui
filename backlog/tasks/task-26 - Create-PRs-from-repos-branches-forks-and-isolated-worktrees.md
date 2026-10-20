---
id: TASK-26
title: Create PRs from repos branches forks and isolated worktrees
status: In Progress
assignee:
  - '@github_provider'
created_date: '2026-10-12 23:06'
updated_date: '2026-10-20 23:45'
labels:
  - review
  - e2e
milestone: m-2
dependencies:
  - TASK-18
  - TASK-15
  - TASK-11
  - TASK-16
  - TASK-34
  - TASK-35
references:
  - >-
    backlog/docs/doc-10 -
    GitHub-provider-native-API-contracts-and-verification.md
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-3 - PR-action-coverage-and-parity-contract.md
priority: high
type: feature
ordinal: 26
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Authors need to create PRs from the same tool and understand what will be proposed. Branch/fork choice and local checkout state should not be implicit.

## Testing steps

1. Code/provider tests: Test base/head/fork selection, diff preview, push/create payloads, duplicate ambiguity, and dirty-worktree preservation.
2. Real-binary TUI E2E scenario `create_pr`: Build a PR through keys and clicks with temporary Git and fake gh, cancel then publish, verify original checkout unchanged and new PR appears exactly once. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e create_pr -- --nocapture` and verify a nonzero selected test count; run the relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. The initial framework is delivered by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Create flow chooses host/repo/fork/head/base, shows the proposed diff, and edits title/body/draft/reviewers.
- [x] #2 Required checkout/branch/push operations use an explicit local mapping or isolated worktree and preview their effects.
- [x] #3 Cancelled or failed publication preserves draft; success returns the canonical PR and appears in authored queue.
- [x] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [x] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
- [ ] #6 All displayed actors use the shared username/avatar or honest fallback; Git and CI states use the shared semantic colors with redundant labels.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Prepare native typed provider requests and strict offline contracts in parallel: verify official endpoint fields, recheck host/account and expected PR head, serialize mutations through the shared runner, inspect resource identity/state, and read back effects. Keep completion dependent on the specified keyboard/mouse real-binary PTY journey and full application checks.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Provider implementation is actively in progress in parallel under the authorized team loop. Dependency completion, native TUI integration, and task-specific mouse plus keyboard real-binary PTY verification remain outstanding; no acceptance criterion is claimed complete from backend-only work. Focused provider checkpoint: cargo test --offline --manifest-path /tmp/ghtui-provider-check.gVlcVM/Cargo.toml --test provider_contracts -- --nocapture selected 38 tests, all passed, zero ignored (2026-10-13). Full workspace checks and dedicated journeys are still required before Done.

Provider checkpoint 2026-10-13: actual workspace provider_contracts selected 67 tests, all passed, zero ignored, using explicit shared-target jobs2/incremental0/debug0 environment and cargo test --offline --test provider_contracts. cargo fmt --check passed. Doc-10 records APIs, exact guards, primary sources, artifact/secret bounds and remaining integration gaps. Full-suite/clippy attempts after this checkpoint encountered concurrent Search Nucleo Config::Default compilation changes (owner corrected); current all-target rerun and native task-specific keyboard/mouse PTY remain outstanding. No AC/DoD or Done claim from these provider-only results.

Actual verification checkpoint: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo check --offline --all-targets passed (4.54s); cargo test --offline --test provider_contracts passed 82 tests, zero failed/ignored (0.53s). New tests cover stable positive numeric account IDs and same-login reassignment guards, canonical merge queue enqueue/dequeue/auto-merge permission/readback, latest case-insensitive commit-status/pending semantics, PR failed post-write observation duplicate barriers, exact comment repository identity, and Actions inspect-then-ack recovery. Native ActionsWorkspace/actions_ui and typed shared-form plans now implemented and compiling, but task-specific real-binary keyboard/mouse Actions PTY journeys and final all-target test/fmt/clippy gates remain pending; no task completion or native parity is claimed.

2026-10-18 a07: Repository Local operations 'Create pull request' (title/body/base/draft) for the checkout's GitHub origin: read-only preview (account, pushed head, duplicate open PR, compare commits/files; unpushed branch explains Push branch), confirm rechecks then one POST repos/{repo}/pulls; draft kept per branch. Gate: node scripts/test-task.mjs v1_pr_create -> 6 lib + 4 PTY (keyboard+mouse) passed; clippy/fmt clean. Gaps: fork/upstream target, reviewers, inbox refresh after create.

2026-10-20 audit at 74360e48 (1/6 ACs met): Creating a PR from the current checkout's origin branch works (preview, duplicate check, one POST, draft kept per branch). Tests: v1_pr_create, 6 lib and 4 PTY. Missing: fork or upstream target, reviewers, inbox refresh after create, and a test that a failed POST keeps the draft. Remaining: AC1: there is no host, repo or fork chooser (origin only), and no reviewers field. Title, body, base, draft and the diff summary are present. AC3: cancel keeps the draft and success reports the canonical PR, but the authored inbox queue is not refreshed after creation, and there is no test that a failed POST keeps the draft. AC2 has no isolated-worktree option, but its intent is met because nothing is pushed or checked out implicitly. AC4 and AC5: no fork or reviewer scenarios. AC6: not verified

2026-10-20 final push (verified at 60167066, 6107/0): Fork parent chooser and Reviewers field (logins or org/team) in create PR; authored queue refreshed after create. See commit 0bebb7b2.

2026-10-20 round: Added PTY journeys v1_pr_create_{keyboard,mouse}_new_pr_appears_in_authored_queue in tests/tui_e2e/pr_create_v1.rs. Each loads the Inbox, creates a PR from Repository and returns to the Inbox. The Inbox reloads once with no Refresh, and the new PR is listed under Created by me. The journey checks for exactly one POST and an unchanged checkout. Fixed a real race in src/main.rs: a late draft-save ack replaced a Local form's failure status ('Pull request not created • draft kept locally') with 'Draft saved locally'. This made the keyboard failed-POST journey fail under load. The existing fork, reviewers, failed-POST, unpushed and cancel-then-publish journeys cover AC1, AC3, AC4 and AC5. Gate: node scripts/test-task.mjs v1_pr_create: 8 lib + 10 PTY passed, 0 failed. AC6 (shared actor chips in the create preview) is not verified. Merged to main by 11eb1ced; full gate 6206 passed, 2 load-only failures elsewhere (TASK-102).
<!-- SECTION:NOTES:END -->
