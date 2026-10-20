---
id: TASK-59
title: Manage Actions secret metadata and variables across eligible scopes
status: In Progress
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
  - TASK-16
references:
  - 'https://docs.github.com/en/rest/actions/secrets'
  - 'https://docs.github.com/en/rest/actions/variables'
  - >-
    backlog/docs/doc-10 -
    GitHub-provider-native-API-contracts-and-verification.md
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-5 - GitHub-Actions-coverage-contract.md
priority: high
type: feature
ordinal: 59
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Actions configuration includes repository/environment/organization secrets and variables. Users must see permitted scopes and effects without the UI pretending that existing secret values can be read.

## Testing steps

1. Code/provider tests: Test scoped argv/stdin, variable round trips, policy membership, denied operations and secret redaction in every artifact/cache path.
2. Real-binary TUI E2E scenario `actions_secrets_vars`: Set a synthetic secret and edit a variable by keys/clicks, inspect metadata, cancel deletion, and assert exact scoped commands with no secret text on screen or persisted diagnostics. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e actions_secrets_vars -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Metadata lists and variables CRUD support verified eligible repo/environment/org scopes.
- [x] #2 Secret set/delete use transient masked input through gh with no cache/log/persisted value.
- [x] #3 Scope/selected-repo policies, permissions and read-back state are explicit and clickable.
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
Provider checkpoint 2026-10-13: actual workspace provider_contracts selected 67 tests, all passed, zero ignored, using explicit shared-target jobs2/incremental0/debug0 environment and cargo test --offline --test provider_contracts. cargo fmt --check passed. Doc-10 records APIs, exact guards, primary sources, artifact/secret bounds and remaining integration gaps. Full-suite/clippy attempts after this checkpoint encountered concurrent Search Nucleo Config::Default compilation changes (owner corrected); current all-target rerun and native task-specific keyboard/mouse PTY remain outstanding. No AC/DoD or Done claim from these provider-only results.

Actual verification checkpoint: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo check --offline --all-targets passed (4.54s); cargo test --offline --test provider_contracts passed 82 tests, zero failed/ignored (0.53s). New tests cover stable positive numeric account IDs and same-login reassignment guards, canonical merge queue enqueue/dequeue/auto-merge permission/readback, latest case-insensitive commit-status/pending semantics, PR failed post-write observation duplicate barriers, exact comment repository identity, and Actions inspect-then-ack recovery. Native ActionsWorkspace/actions_ui and typed shared-form plans now implemented and compiling, but task-specific real-binary keyboard/mouse Actions PTY journeys and final all-target test/fmt/clippy gates remain pending; no task completion or native parity is claimed.

Shared native Actions forms now use positive numeric account identity, typed resource keys and read-only ActionsPlan preview followed by separate Confirm. Secret fields are masked, excluded from durable drafts and Debug, never restored from draft values, blocked from external-editor files, and cleared after closing or confirmed submission. F5/F6 clickable/key field navigation makes narrow multi-field settings forms reachable. Four read-only Actions PTY journeys pass; synthetic-secret and variable mutation PTY remains next and no task completion is claimed.

Actual native PTY checkpoint from root: cargo test --offline --test tui_e2e actions_ -- --nocapture passed8 tests, zero failed/ignored (3.44s). This task contributed2 real-binary journeys: actions_secrets_vars keyboard/mouse. Evidence log target/build/root-actions-regression.log. Tests use physical terminal keyboard/mouse input, strict offline fake-gh command/effect journals, real temporary Git preservation, actors and resizing. Run controls prove job501/attempt2/debug→attempt3, watch/cancel and cancelled deletion; secret tests prove transient masked stdin, no external-editor files/value in draft/Debug/terminal/journal/product config, variable PATCH/readback and cancelled deletion. Earlier native button focus/paging and Escape synchronization failures were repaired without dropping assertions; artifacts retained. Provider checkpoint82 and native source6 were earlier passed separately; latest84provider/source8 and full global test/fmt/clippy gates still pending. This is scoped native coverage, not full task parity or Done: other accepted workflows/permission/platform/host/field-navigation cases remain open.

Checked task-specific command passed: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 node scripts/test-task.mjs actions_secrets_vars. Selected 6 meaningful code/provider and 2 real-binary PTY tests, all passed zero ignored; 1 real secret-form unit +1 secret-plan contract +4 provider scope/redaction/membership tests and2 native PTY3.35s. Runner independently verified successful actual keyboard=true and mouse=true journey records at /tmp/ghtui-test-coverage-PaiJ3R/journeys.jsonl. Latest global checkpoint also passed258 code+102PTY, cargo fmt --check and cargo clippy --all-targets -- -D warnings (root). Full task acceptance/dependencies/platform/remaining workflow scope remain open, so no Done claim.

2026-10-20 audit at 74360e48 (3/5 ACs met): Secret metadata and variable management work at repo and org scope, with masked transient secret input and keyboard/mouse PTY coverage. Remaining: PTY coverage for environment scope, variable delete/create, and denied-permission UI. Remaining: AC1: repo and org scope are PTY-verified; environment-scope secrets/variables are covered only by provider contracts. Variable create and delete are never completed in PTY (delete is only cancelled). AC3: selected-repository policy is shown and tested, but denied-permission states are not shown or tested in the TUI.

2026-10-20 final push (verified at 60167066, 6107/0): Variable scope journeys added (create/delete). See commit 6d8636b3.

2026-10-20 round: A secret or variable metadata read that fails with 403, 404 or unsupported now shows an explanation row ('Secret metadata unavailable: GitHub denied this account (HTTP 403)… repository admin…'). It never shows an empty list. Set/Delete secret and Set/Delete variable are disabled with the same reason: by key or click they open an 'Action unavailable' dialog and send nothing. Refresh clears the explanation once access is granted. New: 1 unit test, plus keyboard and mouse PTY journeys (actions_secrets_vars_*_denied_metadata_read_*). Same gates, 49/0 and 20/0. AC1 (environment-scope secrets in PTY) was not addressed. Merged to main by 11eb1ced; full gate 6206 passed, 2 load-only failures elsewhere (TASK-102).
<!-- SECTION:NOTES:END -->
