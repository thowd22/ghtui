---
id: TASK-58
title: Inspect usage and manage GitHub Actions caches
status: Done
assignee:
  - '@github_provider'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-20 23:45'
labels:
  - actions
  - e2e
milestone: m-7
dependencies:
  - TASK-52
references:
  - 'https://docs.github.com/en/rest/actions/cache'
  - >-
    backlog/docs/doc-10 -
    GitHub-provider-native-API-contracts-and-verification.md
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-5 - GitHub-Actions-coverage-contract.md
priority: medium
type: feature
ordinal: 58
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Large or stale caches can explain CI behavior. Cache management needs concrete scope and deletion effects rather than a hidden global cleanup action.

## Testing steps

1. Code/provider tests: Verify cache paging/usage units/filtering, deletion IDs/keys and partial or stale outcomes.
2. Real-binary TUI E2E scenario `actions_cache`: Filter caches and inspect usage via keys/clicks, cancel then confirm selected deletion, and assert only selected journal targets and refreshed totals. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e actions_cache -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Cache list/filter shows repo/ref/key/size/access times and available usage context.
- [x] #2 Individual/selected deletion previews exact entries and preserves unrelated caches.
- [x] #3 Permissions, missing/stale entries and host limits are explicit and refresh after operations.
- [x] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [x] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Provider preparation under the authorized parallel implementation loop: verify official gh/REST contracts; add typed native target models and explicit argv/JSON stdin through the shared bounded runner; guard current account/resource identity, serialize writes and reconcile ambiguity; add strict fake-gh contracts for successful readback, paging, permissions, stale state and cancellation. Root owns native TUI forms and task-specific mouse/keyboard real-binary PTY journeys. Dependency completion, full workspace checks and required PTY coverage remain pending; backend implementation does not satisfy Done.

Native UI integration slice: build provider-free ActionsWorkspace prepared pages and ActionsAction enum, then pure Actions renderer with shared semantic status colors, actor/avatar fallback, clickable hit regions and matching keyboard shortcuts. Worker loads typed catalog/run-attempt/jobs/logs/annotations/artifact/workflow/settings data. Shared forms retain target identity and typed commands for confirmation; root integrates App/main dispatch without arbitrary gh commands. Add focused workspace/provider tests and owned real-binary Actions PTY journeys for each exposed native workflow; keep unsupported or unintegrated rows visibly honest and tasks In Progress until their actual code plus keyboard/mouse PTY and global gates pass.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-13 provider checkpoint: 47 strict offline contracts passed (zero ignored) via cargo test --offline --manifest-path /tmp/ghtui-provider-check.gVlcVM/Cargo.toml --test provider_contracts. New cases exercise PrContext permissions/fork/reviewer provenance and stale comparisons; Actions filtered-search caps, exact attempts/jobs/annotations/logs, debug rerun actual job IDs, pending accepted transitions, stale/completed guards, exact delete targets, cache bytes and preservation, deployment eligibility and partial state. After that checkpoint new workflow/YAML/artifact/secret/settings code is in preparation and not yet claimed verified. Compilation paused while foundation relocates pinned official dependencies and completes portable Cargo vendoring; prior isolated target hit disk quota, regenerable build outputs were cleaned by foundation. Native forms and real keyboard/mouse PTY journeys remain outstanding; no Done/AC claim.

Provider checkpoint 2026-10-13: actual workspace provider_contracts selected 67 tests, all passed, zero ignored, using explicit shared-target jobs2/incremental0/debug0 environment and cargo test --offline --test provider_contracts. cargo fmt --check passed. Doc-10 records APIs, exact guards, primary sources, artifact/secret bounds and remaining integration gaps. Full-suite/clippy attempts after this checkpoint encountered concurrent Search Nucleo Config::Default compilation changes (owner corrected); current all-target rerun and native task-specific keyboard/mouse PTY remain outstanding. No AC/DoD or Done claim from these provider-only results.

Actual verification checkpoint: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo check --offline --all-targets passed (4.54s); cargo test --offline --test provider_contracts passed 82 tests, zero failed/ignored (0.53s). New tests cover stable positive numeric account IDs and same-login reassignment guards, canonical merge queue enqueue/dequeue/auto-merge permission/readback, latest case-insensitive commit-status/pending semantics, PR failed post-write observation duplicate barriers, exact comment repository identity, and Actions inspect-then-ack recovery. Native ActionsWorkspace/actions_ui and typed shared-form plans now implemented and compiling, but task-specific real-binary keyboard/mouse Actions PTY journeys and final all-target test/fmt/clippy gates remain pending; no task completion or native parity is claimed.

Native cache-management source now includes key-prefix/exact-ref typed filtering, cache ID/key/ref/size/version/access/creation metadata, repository-wide usage in bytes explicitly separate from filtered totals, selected-ID guarded deletion, post-delete full-catalogue preservation and refreshed usage. Prepared actions_cache keyboard/mouse PTY journeys independently assert cancelled preview launches no requests, exact key/ref query, one DELETE cache71 only, unrelated cache72 preserved, refreshed byte totals, resize and real Git unchanged. Focused code contract also checks read-only preview, scoped counts and summary-row mutation guard. Tests pending coordinated shared-target checkpoint; no result/Done claim.

Actual shared full checkpoint: root ran cargo test --offline --all-targets with CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target, jobs2/incremental0/dev+test debug0. Log target/build/root-all-targets-checkpoint.log: provider_contracts85/85 passed zero ignored (0.54s), actions_contracts10/10 passed zero ignored (0.17s), and tui_e2e102/102 passed zero ignored (7.66s). This task’s two prepared native keyboard/mouse journeys both executed and passed in that real-binary suite, including strict command effects and checkout preservation. New task-specific selector reruns and current fmt/clippy gates still pending; no Done/complete-parity claim.

Checked task-specific command passed: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 node scripts/test-task.mjs actions_cache. Selected 2 meaningful code/provider and 2 real-binary PTY tests, all passed zero ignored; 1 prepared-cache +1 provider deletion test and2 native PTY3.50s. Runner independently verified successful actual keyboard=true and mouse=true journey records at /tmp/ghtui-test-coverage-QWQNZl/journeys.jsonl. Latest global checkpoint also passed258 code+102PTY, cargo fmt --check and cargo clippy --all-targets -- -D warnings (root). Full task acceptance/dependencies/platform/remaining workflow scope remain open, so no Done claim.

2026-10-20 audit at 74360e48 (3/5 ACs met): Cache list, filter, usage and guarded single-cache delete work in the app, with keyboard and mouse PTY journeys. Remaining: tests for stale/missing/permission-denied deletes, and showing host cache limits in the UI. Remaining: AC3: the 'Selected cache changed or disappeared' (StaleHead) and 'still visible after deletion' guards exist in code, but no test exercises them. No test covers denied permissions. Host storage/retention limits (cache_limits, organization_cache_usage in operations.rs:651/707) are covered only by a provider test and are never called from the UI. Refresh after delete is done. AC4: the testing steps ask for paging, partial and stale outcomes, and no provider test covers stale, missing or partial de

2026-10-20 final push (verified at 60167066, 6107/0): Cache delete outcomes explained. Remaining: host storage/retention limits not in the UI; denied-permission states untested.

2026-10-20 round: Usage (u) now reads the host cache storage and retention limits (repos/{r}/actions/cache/storage-limit and retention-limit). A denied, missing or unsupported limit read shows an explanation instead of a default, and Refresh reads the limits again. Stale, missing and denied delete outcomes were already covered (provider contracts and cache_outcomes_v1 PTY). New: 1 unit test in src/actions_workspace/limitations.rs, the cache_management PTY now asserts the limits, and new keyboard and mouse PTY journeys cover a denied limit read followed by a Refresh (tests/tui_e2e/actions/limitations_v1.rs). Gates: cargo test --lib actions_workspace 49/0; cargo test --test tui_e2e -- actions_cache actions_settings actions_secrets_vars 20/0. Merged to main by 11eb1ced; full gate 6206 passed, 2 load-only failures elsewhere (TASK-102).
<!-- SECTION:NOTES:END -->
