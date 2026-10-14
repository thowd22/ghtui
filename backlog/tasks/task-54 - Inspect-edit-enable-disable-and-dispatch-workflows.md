---
id: TASK-54
title: Inspect edit enable disable and dispatch workflows
status: In Progress
assignee:
  - '@github_provider'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-13 16:59'
labels:
  - actions
  - e2e
milestone: m-7
dependencies:
  - TASK-53
  - TASK-38
  - TASK-26
references:
  - 'https://cli.github.com/manual/gh_workflow_run'
  - >-
    backlog/docs/doc-10 -
    GitHub-provider-native-API-contracts-and-verification.md
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-5 - GitHub-Actions-coverage-contract.md
priority: high
type: feature
ordinal: 54
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Users need to run and understand workflows from the TUI. Dispatch must expose typed inputs and chosen ref; editing must use the repository's normal Git workflow.

## Testing steps

1. Code/provider tests: Verify input schemas/defaults, YAML/reference selection, dispatch payloads, state read-back and unsupported hosts.
2. Real-binary TUI E2E scenario `workflow_controls`: Inspect a workflow, fill inputs and dispatch by keys/clicks, cancel or confirm disable, and assert exact commands plus returned run correlation. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e workflow_controls -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Workflow YAML is browsable at the selected revision with trigger/input definitions and contextual help.
- [ ] #2 Enable/disable and dispatch use exact workflow/ref and validated required/boolean/choice/environment inputs.
- [ ] #3 YAML edits connect to local preview/commit/PR; unsupported triggers and denied writes are explained.
- [ ] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [ ] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
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

Native workflow_controls keyboard/mouse PTY source prepared: repository-reported default branch and immutable YAML SHA; boolean false plus explicit choice fast; no requests for cancelled disable preview; guarded dispatch with exact returned run ID/event/SHA; confirmed disable then enable readback; strict argv/stdin journal and unchanged real Git checkout at 120x40/80x24. Workflow enable/disable/dispatch disabled states now explain current state. Source prepared only: tests have not yet run in this build window. YAML local edit/commit/PR connection and complete dependency/global gates remain outstanding; no Done claim.

Actual shared full checkpoint: root ran cargo test --offline --all-targets with CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target, jobs2/incremental0/dev+test debug0. Log target/build/root-all-targets-checkpoint.log: provider_contracts85/85 passed zero ignored (0.54s), actions_contracts10/10 passed zero ignored (0.17s), and tui_e2e102/102 passed zero ignored (7.66s). This task’s two prepared native keyboard/mouse journeys both executed and passed in that real-binary suite, including strict command effects and checkout preservation. New task-specific selector reruns and current fmt/clippy gates still pending; no Done/complete-parity claim.

Checked task-specific command passed: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 node scripts/test-task.mjs workflow_controls. Selected 4 meaningful code/provider and 2 real-binary PTY tests, all passed zero ignored; 4 provider/schema tests0.15s and2 native PTY7.68s. Runner independently verified successful actual keyboard=true and mouse=true journey records at /tmp/ghtui-test-coverage-VVMzG5/journeys.jsonl. Latest global checkpoint also passed258 code+102PTY, cargo fmt --check and cargo clippy --all-targets -- -D warnings (root). Full task acceptance/dependencies/platform/remaining workflow scope remain open, so no Done claim.

Current durable workflow native migration focused rerun passed2/2 actual keyboard/mouse journeys, zero ignored8.31s, provider-native-durable-workflow2-rerun.log. Exact protocol includes fresh composer/account brackets, correlated dispatch200 receipt saved as accepted_pending, explicit original workflow/run catalogue inspection plus typed ACK recording TransitionObserved (not completion), shared immutable workflow draft OPEN6 restoring original Dispatch before fields, explicit F9 New intent before deliberate Disable, then separate confirmed Disable/Enable receipts. Exactly one Dispatch POST and two state PUTs; cancelled first Disable has no write/marker, actors/definition inputs/matrix/resize/Git preservation unchanged. First current Actions36 migration gate34pass/2workflowfails is retained because test incorrectly expected a new composer where the durable original shared draft must win. Corrected user workflow and strict request sequence, no weakened production assertion. New restart10 passed separately; whole current Actions46/full all-target/platform requirements remain open.
<!-- SECTION:NOTES:END -->
