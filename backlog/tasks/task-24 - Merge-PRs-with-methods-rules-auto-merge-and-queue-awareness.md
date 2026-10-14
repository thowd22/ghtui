---
id: TASK-24
title: Merge PRs with methods rules auto-merge and queue awareness
status: In Progress
assignee:
  - '@github_provider'
created_date: '2026-10-12 23:06'
updated_date: '2026-10-14 02:42'
labels:
  - review
  - e2e
milestone: m-2
dependencies:
  - TASK-20
  - TASK-14
  - TASK-6
  - TASK-34
  - TASK-35
references:
  - 'https://cli.github.com/manual/gh_pr_merge'
  - >-
    backlog/docs/doc-10 -
    GitHub-provider-native-API-contracts-and-verification.md
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-3 - PR-action-coverage-and-parity-contract.md
priority: high
type: feature
ordinal: 24
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Merging is a major PR action and its outcome is not always an immediate merge. The TUI must respect current head/checks/rules and report queued or auto-merge states accurately.

## Testing steps

1. Code/provider tests: Test allowed methods, protected branches, head races, pending checks, queue/auto states, and timeout reconciliation.
2. Real-binary TUI E2E scenario `merge_workflows`: Choose and confirm a merge by keys and clicks, verify method/head payload and final state; simulate changed head, denied method, and queue behavior with no duplicate write. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e merge_workflows -- --nocapture` and verify a nonzero selected test count; run the relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. The initial framework is delivered by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Available merge/squash/rebase methods reflect repository settings and caller permissions.
- [ ] #2 Preview uses fresh head/check state and supported expected-head protection; branch deletion is a separate explicit choice.
- [ ] #3 Merge, queue, enable/disable auto-merge, and queue cancellation have verified distinct outcomes; admin bypass is never implicit.
- [ ] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [ ] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
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

Add typed merge-state/review-decision/viewer-permission and merge-queue provenance reads. Enqueue uses server expectedHeadOid; dequeue uses PR node ID after checking observed queue-entry ID; disabling auto-merge inspects current actor permission and request. Validate post-write queue/auto state with the shared ambiguity barrier. Native UI confirmations, merge method/rule previews and keyboard/mouse PTY still required; offline fixtures do not prove live host parity.

2026-10-14 source-only audit/handoff: prepare genuine original-bound merge/state observation with raw nullable repository method and viewer/rules provenance; extend existing shared PR action/form/journal/controller, not an alternate write stack. Distinguish merged versus queue admission versus auto-merge configuration versus accepted pending, preserve receipt-before-refresh/original no-resend. Mandatory durable state-filter repair before cached/index/context/Open and in-memory Inbox/PR/Dashboard invalidation. Review target/build/foundation-task24-25-native-state-audit-20261014.md before new source/activation; execute meaningful code/provider and both-input merge_workflows native gates with exact protocol/effects, then impacted/full/fmt/strictClippy. Dependencies remain open.

Root approved first source-only model slice on 2026-10-14: stage NEW undeclared pr_state_model.rs and meaningful contracts for bounded local original-bound intent previews and closed receipt/disposition decoding. No provider proof/authorization constructor, mutation method, core/schema change or second publisher. Draft pure plans remain not publishable until future genuine opaque current observation validates nullable settings/caller/rules and original source; AcceptedPending blocks and cannot become Confirmed or generic ACK. Exact queue/auto configuration is distinct from merge. Remote-only deletion strategy remains open; research actual installed-Git lease versus disclosed API race without checkout changes. Send source APIs/actual predicted target protocols before activation.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Provider implementation is actively in progress in parallel under the authorized team loop. Dependency completion, native TUI integration, and task-specific mouse plus keyboard real-binary PTY verification remain outstanding; no acceptance criterion is claimed complete from backend-only work. Focused provider checkpoint: cargo test --offline --manifest-path /tmp/ghtui-provider-check.gVlcVM/Cargo.toml --test provider_contracts -- --nocapture selected 38 tests, all passed, zero ignored (2026-10-13). Full workspace checks and dedicated journeys are still required before Done.

Provider checkpoint 2026-10-13: actual workspace provider_contracts selected 67 tests, all passed, zero ignored, using explicit shared-target jobs2/incremental0/debug0 environment and cargo test --offline --test provider_contracts. cargo fmt --check passed. Doc-10 records APIs, exact guards, primary sources, artifact/secret bounds and remaining integration gaps. Full-suite/clippy attempts after this checkpoint encountered concurrent Search Nucleo Config::Default compilation changes (owner corrected); current all-target rerun and native task-specific keyboard/mouse PTY remain outstanding. No AC/DoD or Done claim from these provider-only results.

Actual verification checkpoint: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo check --offline --all-targets passed (4.54s); cargo test --offline --test provider_contracts passed 82 tests, zero failed/ignored (0.53s). New tests cover stable positive numeric account IDs and same-login reassignment guards, canonical merge queue enqueue/dequeue/auto-merge permission/readback, latest case-insensitive commit-status/pending semantics, PR failed post-write observation duplicate barriers, exact comment repository identity, and Actions inspect-then-ack recovery. Native ActionsWorkspace/actions_ui and typed shared-form plans now implemented and compiling, but task-specific real-binary keyboard/mouse Actions PTY journeys and final all-target test/fmt/clippy gates remain pending; no task completion or native parity is claimed.

Read-only source audit 2026-10-14: no native merge/auto/queue PR controls or merge_workflows PTY declarations exist. Existing merge/queue/disable-auto transports and narrow strict provider contracts are not native parity; no new Cargo command/test result is claimed. Repository drops method settings; legacy merge returns any readable summary after CLI success, so requires typed distinct outcome. New async REST UUID support is separate official API, not behavior of included CLI wrapper. PrWriteJournal has no accepted-pending receipt state; future integration must extend same durable policy and reject generic ACK/refresh unlocking acceptance. API/protocol/coverage proposal artifact target/build/foundation-task24-25-native-state-audit-20261014.md awaits Root admission. Root owns included-source/Cargo; bridge17/native6 remains frozen; no AC/Done change or live mutation.

Approved first SOURCE-ONLY model handoff prepared 2026-10-14: NEW undeclared src/pr_state_model.rs +pr_state_model_tests.rs contain19 closed local operations and15 meaningful UNCOMPILED/UNRUN contracts. Local preview has no mutation/auth/proof/ACK constructor, bounded identity/payload/prepared rows, exact scope+comparison+state+fields fingerprint and shared actor fallback. Closed recorded-data decoder preflights8KiB UTF8, exact original command/principal/target/head/base/digest and explicit nullable/extra shape; queue/auto configuration differs from merged; accepted/unknown unresolved, no Confirmed head-change update or ACK. Direct selected rustfmt/check PASS only; no Cargo/core/schema/registration/README or coverage claim. Full API/source/native protocol/required safety boundaries in target/build/foundation-task24-25-state-model-handoff-20261014.md. Original-view admission must use tolerant genuine current-access before private SQL and close it BEFORE new-head validation; OPEN14 original-only/22 new-source and submit24/28 are predictions, not results. Mandatory state-filter repair and all native/host/role/platform/full dependencies remain open; bridge17/native6 frozen.

Next lifecycle observation read boundary is Root-approved source-only. Exact existing opaque catalogue origin,8 bounded API reads with SQL hook before final repo/user and shared raw capture/accounting/clock. No merged/queue/auto/rules completion or eligibility inferred: these remain explicitly NotLoaded/unavailable; no publisher enabled, accepted journal/model and bridge frozen, all tests staged until actual Root admission/gates.

2026-10-14 lifecycle source-read handoff staged/frozen under Task25, with15 undeclared UNCOMPILED/UNRUN provider contracts and selected rustfmt/check exit0 only. target/build/foundation-pr-lifecycle-observation-handoff-20261014.md specifies exact genuine opaque catalogue Arc origin, original-clock8-read observation and caller-owned metadata SQL hook/final numeric-principal priority. No merge/auto/queue/repository-method/rules eligibility or state is inferred: all remain explicitly NotLoaded; no publisher, accepted-journal change, core/registration/Cargo/live mutation or native coverage claim. Earlier15 state-model contracts remain frozen/unrun; Root owns earlier Task14 bridge activation and its independent verification. Required original access ordering, accepted-pending policy, state-filter repair and native merge_workflows both-input/full/host/role/platform/dependency evidence are still outstanding. No AC/Done change.
<!-- SECTION:NOTES:END -->
