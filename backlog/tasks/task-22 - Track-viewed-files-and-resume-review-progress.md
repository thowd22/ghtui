---
id: TASK-22
title: Track viewed files and resume review progress
status: In Progress
assignee:
  - '@github_provider'
created_date: '2026-10-12 23:06'
updated_date: '2026-10-20 14:09'
labels:
  - review
  - e2e
milestone: m-2
dependencies:
  - TASK-15
  - TASK-8
  - TASK-34
  - TASK-35
references:
  - 'https://docs.github.com/en/graphql/reference/pulls'
  - >-
    backlog/docs/doc-10 -
    GitHub-provider-native-API-contracts-and-verification.md
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-3 - PR-action-coverage-and-parity-contract.md
priority: medium
type: feature
ordinal: 22
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Large PRs require a dependable notion of which files have been reviewed. Progress must synchronize with GitHub and become stale when the relevant file changes.

## Testing steps

1. Code/provider tests: Test viewed state normalization, local/server reconciliation, changed files, and unsupported host behavior.
2. Real-binary TUI E2E scenario `viewed_files`: Mark files using keys and clicks, restart, change a fixture head/file, refresh, and assert accurate progress and command identities. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e viewed_files -- --nocapture` and verify a nonzero selected test count; run the relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. The initial framework is delivered by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Mark viewed/unviewed maps to server capability and displays per-file progress.
- [x] #2 Changed-file and head refresh invalidates viewed state according to verified server semantics.
- [x] #3 Progress and the selected file resume after restart without masking unavailable server state.
- [x] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [x] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
- [x] #6 All displayed actors use the shared username/avatar or honest fallback; Git and CI states use the shared semantic colors with redundant labels.
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

Root native activation under authorized parallel implementation: add an exact positive-account PR/base/head-bound Review progress tab, preserve VIEWED/UNVIEWED/DISMISSED and honest unavailable states, and route selected canonical file mark/unmark through the existing durable PrPlan and shared confirmation form. Prepare metadata in workers with fresh identity and PR summary checks before/after reads; saved selection is account-scoped navigation intent, never restored server truth. Fresh head reads replace progress, unsupported/partial state cannot become zero or enable writes. Add meaningful pure/provider tests and actual viewed_files keyboard/mouse mark-unmark/restart/head-change/failure journeys, then focused and full local gates. Task dependencies and native journey evidence remain open; no Done or accepted criterion inferred.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Provider implementation is actively in progress in parallel under the authorized team loop. Dependency completion, native TUI integration, and task-specific mouse plus keyboard real-binary PTY verification remain outstanding; no acceptance criterion is claimed complete from backend-only work. Focused provider checkpoint: cargo test --offline --manifest-path /tmp/ghtui-provider-check.gVlcVM/Cargo.toml --test provider_contracts -- --nocapture selected 38 tests, all passed, zero ignored (2026-10-13). Full workspace checks and dedicated journeys are still required before Done.

Provider checkpoint 2026-10-13: actual workspace provider_contracts selected 67 tests, all passed, zero ignored, using explicit shared-target jobs2/incremental0/debug0 environment and cargo test --offline --test provider_contracts. cargo fmt --check passed. Doc-10 records APIs, exact guards, primary sources, artifact/secret bounds and remaining integration gaps. Full-suite/clippy attempts after this checkpoint encountered concurrent Search Nucleo Config::Default compilation changes (owner corrected); current all-target rerun and native task-specific keyboard/mouse PTY remain outstanding. No AC/DoD or Done claim from these provider-only results.

Actual verification checkpoint: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo check --offline --all-targets passed (4.54s); cargo test --offline --test provider_contracts passed 82 tests, zero failed/ignored (0.53s). New tests cover stable positive numeric account IDs and same-login reassignment guards, canonical merge queue enqueue/dequeue/auto-merge permission/readback, latest case-insensitive commit-status/pending semantics, PR failed post-write observation duplicate barriers, exact comment repository identity, and Actions inspect-then-ack recovery. Native ActionsWorkspace/actions_ui and typed shared-form plans now implemented and compiling, but task-specific real-binary keyboard/mouse Actions PTY journeys and final all-target test/fmt/clippy gates remain pending; no task completion or native parity is claimed.

Root native source activated: Review progress tab, positive numeric scope/PR node/repo/number/base/head prepared snapshot, exact selected-file Mark viewed/Mark unviewed through existing durable PrPlan/controller/shared forms, no code/checkout mutation; DIMISSED is labelled CHANGED SINCE VIEWED, unknown/null host state unavailable, partial scope never inferred zero. Source state matches official FileViewedState/mark/unmark schema at https://docs.github.com/en/graphql/reference/pulls#fileviewedstate (old generic enum/mutation docs URLs now redirect, checked primary category source). Navigation position stores only account-bound canonical PR/file/head, never a restored server viewed flag. Rendering borrows O(1) selected-file metadata; prepared actions share mouse/keyboard semantics. Read worker has fresh account/summary before/after; mutation receipt retains original head and changed-head optional refresh never installs old viewed flags on a new head. Actual private-TMPDIR/sharedtarget/offline locked cargo check --all-targets PASS8.52s root-pr-progress-first-check-rerun.log; initial missing internal fixture initializer failure retained root-pr-progress-first-check.log. Focused cargo test --lib viewed_files_progress PASS5/5 zero ignored0.02s after10.81s compile root-pr-progress-focused5-first.log. Four new strict executed-fake-gh read contracts and two actual-binary mark/unmark/restart/head-change journeys are formatted but UNRUN; PTY child still unregistered until coherent provider boundary. Current whole-suite/fmt/strict lint and all native task acceptance/dependencies remain required; no Done or full coverage claim.

ACTUAL Root strict-read contracts: sharedtarget/privateTMPDIR/offline locked cargo test --test pr_progress_contracts passed4/4 zero failed/ignored0.21s (root-pr-progress-provider4-first.log). Actual fake-gh checks pre/post numeric principal+exact PR summary, before/after head drift and final reused-loginUID43/final401 privacy; independently checks real temp Git baseline/one commit/staged and dirty status/bytes. First native viewed_files2 run failed0/2 from test readiness (keyboard sent Preview while fresh OPEN worker was finishing) and expecting a close button after shared runtime already closed a confirmed form; initial failure log/artifacts retained root-pr-progress-native2-first.log. Replaced assumptions with actual prepared-form header/status+exact command-count fences and automatic confirmed-overlay close, retaining every target/receipt/current-head/no-write assertion. Rerun2/2 PASS zeroignored1.97s (root-pr-progress-native2-readiness-rerun.log): keyboard/mouse120x40→80x24→restart80→120, VIEWED then fresh-head DISMISSED/CHANGED SINCE VIEWED then UNVIEWED; exact two deliberate GraphQL mutations, positive numeric account/path and independently confirmed SQL receipts retaining their distinct original heads, account-scoped saved navigation with no viewed flag, unchanged real localHEAD/index/tracked+untracked dirty bytes. Checked CARGO_NET_OFFLINE=true node scripts/test-task.mjs viewed_files passed10 meaningful code/provider+2 actual PTYs, keyboard=true/mouse=true, zerofailed/ignored, artifact target/test-tmp/ghtui-test-coverage-diPNko; log root-pr-progress-checked-gate.log. Official enum spelling is DISMISSED (corrects DIMISSED typo in earlier note). Full current alltarget/fmt/strictlint gate is running; remaining failure/permission/unknown-write/native task dependencies/macOS still not implied Done.

Fresh coherent Linux whole checkpoint GREEN after native progress activation: sharedtarget/privateTMPDIR/Gitceiling/jobs2/incremental0 offline locked cargo test --all-targets passed580 meaningful code/provider contracts and178 actual compiled-binary PTY journeys, zero failed/ignored; normal main0 is not coverage, PTY17.02s. Counts mechanically summed from every actual target result. Whole cargo fmt --check PASS0.87s, cargo clippy --offline --locked --all-targets -- -D warnings PASS1.70s. Logs target/build/root-actions-progress-all-targets.log, root-actions-progress-fmt.log, root-actions-progress-clippy.log. Checked viewed_files10+2 remains currenttask evidence; README updated to actual checkpoint. Failure/disabled/unknown-write additional matrix, related task dependencies, full product and native macOS/release gates remain open, no Task22 Done or live GitHub parity claim.

2026-10-20 audit at 74360e48 (6/6 ACs met): Viewed/unviewed is in the PR actions menu, and changed-head invalidation and restart resume are verified (viewed_files gate: 10 code tests plus 2 PTY tests, and pr_progress 4 PTY tests). It appears ready to mark Done. Remaining: No AC gap found. The backlog is still In Progress. The selected file is restored after restart from account-bound navigation, and the viewed flag is not restored from cache, as intended. Independent check: The happy path is real and reachable. The PR actions menu has MarkViewed/MarkUnviewed (src/pr_actions.rs:29-30,58-59), opened from 'v Review progress'. tests/tui_e2e/pr_progress.rs is included through pr_actions.rs:23. It runs paired keyboard and mouse at 120x40 and 80x24 and restarts. It checks exactly 2 mutations, then on the new head shows DISMISSED as CHANGED SINCE VIEWED with 0/2 viewed. Both
<!-- SECTION:NOTES:END -->
