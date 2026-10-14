---
id: TASK-52
title: Build Actions capability inventory workflow catalog and status indicators
status: In Progress
assignee:
  - '@github_provider'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-13 02:26'
labels:
  - actions
  - e2e
milestone: m-7
dependencies:
  - TASK-14
  - TASK-9
  - TASK-34
  - TASK-35
references:
  - 'https://cli.github.com/manual/gh_workflow_list'
  - 'https://docs.github.com/en/rest/actions/workflow-runs'
  - >-
    backlog/docs/doc-10 -
    GitHub-provider-native-API-contracts-and-verification.md
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-5 - GitHub-Actions-coverage-contract.md
priority: high
type: feature
ordinal: 52
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Actions needs full native operational support, not just a CI badge on a PR. Establish a shared model and verified coverage matrix so run viewers and actions agree across PRs, commits and repositories.

## Testing steps

1. Code/provider tests: Contract-test capability inventory, paginated workflow/run identity, SHA correlation and status aggregation.
2. Real-binary TUI E2E scenario `actions_inventory`: Open a PR or commit status indicator using keys/clicks, browse filtered workflow runs, and assert exact related run, actor and unavailable feature reasons. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e actions_inventory -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every Actions coverage row maps to a gh/API operation, permission/host/version constraint and implementation task.
- [ ] #2 Workflow/run lists paginate/filter across selected repos with actor usernames/avatars and explicit freshness.
- [ ] #3 PR/commit/repo indicators identify exact SHA/event/attempt and open the shared Actions view.
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
2026-10-13 provider checkpoint: 47 strict offline contracts passed (zero ignored) via cargo test --offline --manifest-path /tmp/ghtui-provider-check.gVlcVM/Cargo.toml --test provider_contracts. New cases exercise PrContext permissions/fork/reviewer provenance and stale comparisons; Actions filtered-search caps, exact attempts/jobs/annotations/logs, debug rerun actual job IDs, pending accepted transitions, stale/completed guards, exact delete targets, cache bytes and preservation, deployment eligibility and partial state. After that checkpoint new workflow/YAML/artifact/secret/settings code is in preparation and not yet claimed verified. Compilation paused while foundation relocates pinned official dependencies and completes portable Cargo vendoring; prior isolated target hit disk quota, regenerable build outputs were cleaned by foundation. Native forms and real keyboard/mouse PTY journeys remain outstanding; no Done/AC claim.

Provider checkpoint 2026-10-13: actual workspace provider_contracts selected 67 tests, all passed, zero ignored, using explicit shared-target jobs2/incremental0/debug0 environment and cargo test --offline --test provider_contracts. cargo fmt --check passed. Doc-10 records APIs, exact guards, primary sources, artifact/secret bounds and remaining integration gaps. Full-suite/clippy attempts after this checkpoint encountered concurrent Search Nucleo Config::Default compilation changes (owner corrected); current all-target rerun and native task-specific keyboard/mouse PTY remain outstanding. No AC/DoD or Done claim from these provider-only results.

Actual verification checkpoint: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo check --offline --all-targets passed (4.54s); cargo test --offline --test provider_contracts passed 82 tests, zero failed/ignored (0.53s). New tests cover stable positive numeric account IDs and same-login reassignment guards, canonical merge queue enqueue/dequeue/auto-merge permission/readback, latest case-insensitive commit-status/pending semantics, PR failed post-write observation duplicate barriers, exact comment repository identity, and Actions inspect-then-ack recovery. Native ActionsWorkspace/actions_ui and typed shared-form plans now implemented and compiling, but task-specific real-binary keyboard/mouse Actions PTY journeys and final all-target test/fmt/clippy gates remain pending; no task completion or native parity is claimed.

Native Actions source/render contracts passed six tests; two further contracts now assert every enabled paged control remains reachable at40x12,80x24,120x40 and empty scope/reserved global keys never launch gh. Native Actions shortcuts changed to R/W/S to preserve global1-5/s Settings. First real-binary PTY module tests/tui_e2e/actions.rs prepared for root inclusion: actions_inventory keyboard/mouse and actions_run_view keyboard/mouse exercise actors, exact attempt/job/log/annotation commands, resize and unchanged real Git state. Execution pending shared App/forms/main worker integration; no PTY pass is claimed yet.

Root native Actions App/runtime/UI integration now compiles with all-target cargo check (4.74s). Shared typed pages/commands, run/repo context, actor/avatar colors, keyboard and clickable controls, bounded watch refresh, explicit form preview/confirmation, account-ID draft partition and no-gh empty-scope navigation are wired. First four actual Actions inventory/run-view PTY journeys are now executing; compile/source coverage does not imply native E2E completion.

Actual native PTY checkpoint from root: cargo test --offline --test tui_e2e actions_ -- --nocapture passed8 tests, zero failed/ignored (3.44s). This task contributed2 real-binary journeys: actions_inventory keyboard/mouse. Evidence log target/build/root-actions-regression.log. Tests use physical terminal keyboard/mouse input, strict offline fake-gh command/effect journals, real temporary Git preservation, actors and resizing. Run controls prove job501/attempt2/debug→attempt3, watch/cancel and cancelled deletion; secret tests prove transient masked stdin, no external-editor files/value in draft/Debug/terminal/journal/product config, variable PATCH/readback and cancelled deletion. Earlier native button focus/paging and Escape synchronization failures were repaired without dropping assertions; artifacts retained. Provider checkpoint82 and native source6 were earlier passed separately; latest84provider/source8 and full global test/fmt/clippy gates still pending. This is scoped native coverage, not full task parity or Done: other accepted workflows/permission/platform/host/field-navigation cases remain open.

Current root native Actions regression cargo test --offline --locked --test tui_e2e actions_ -- --nocapture passed8/8 actual keyboard/mouse PTY journeys, zero ignored,3.44s: catalogue/actors/resize, exact run attempt/job/steps/logs/annotations, guarded exact-job debug rerun->attempt3, watch/cancel and cancelled run deletion, masked transient secret and variable controls. Log target/build/root-actions-regression.log. Pure ControlsPage/Watch now change prepared App state immediately to avoid lost worker-snapshot navigation; actual keyboard focus is the only underlined actionable control. Broader Actions catalog/settings and global regression remain open.
<!-- SECTION:NOTES:END -->
