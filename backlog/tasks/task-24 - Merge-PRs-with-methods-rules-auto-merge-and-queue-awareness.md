---
id: TASK-24
title: Merge PRs with methods rules auto-merge and queue awareness
status: In Progress
assignee:
  - '@github_provider'
created_date: '2026-10-12 23:06'
updated_date: '2026-10-16 23:19'
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
  - >-
    https://docs.github.com/en/rest/pulls/pulls#merge-a-pull-request-asynchronously
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

2026-10-16 V68 READ-ONLY Task24/25 current-native audit/proposal, exact frozen V67 fece0a68/full1320. Read current unchecked AC/deps, Doc3/Doc10 relevant native mutation contract and foundation audit/model/lifecycle handoffs. Audit actual current source and primary official docs/installed gh/git help; propose same current producer/Core/Form/queue/PrPlan/journal lifecycle with explicit accepted/merged/queued distinctions, durable filter repair and full19-operation requirement map. Implementation remains HOLD until Root reads/reviews exact material interfaces; no Cargo/rustc/PTY/build/live mutation/current source edits or completion claim. CLI append is individually locked for each task; prior multi-task append was refused without mutation.

Root approved full first Close/Reopen shared State foundation under Task25 plan after FULL c1768c01 material review; this is not merge completion. All19 operation inventory plus modern async UUID/queue/auto/stack, exact remote-only deletion and native origins remain required. Same producer/Form/owner/PrPlan/journal and durable query-fence, no legacy Metrics callback activation. Exact State-origin DDL/migration/accepted-vs-confirmed/genericACK release seam needs review before shared storage edits. Source-only until meaningful code/provider+real keyboard/mouse/current/full/fmt/strict gates; no task readiness/AC/Done changes.

V119 exact one-TEST-path corrective plan BEFORE edits, Root approved actual V304 final captured panics69/218/485/290 and proposal2f6999ae. NEW authenticated exact V118ef8d full1506 descendant, only src/pr_state_model_tests.rs: assert original Example/Repo spelling, make decoder repository fault genuinely different lowercase, reconstruct exact adjacent physical preview row bytes without injected spaces/newlines for Delete/Revert positive prose only. Preserve rawpayload/control/bidi/Debug/disabled19/unknownSource/wholefingerprint/decoder negatives/all15modelnames/C6native2/State35native6 and every other1505path. No productioncase normalization, wrap/globalhelper/source/cap/clock/Main changes; ZERO workerCargo/rustc/PTY/tests. Root latestV3061508 is explicit later disjoint composition, not copied here.

V120 BEFORE-edit plan: Root reviewed V1192351B actual-original-case edits and requested exact adjacent-physical-row positive prose, not whole-preview concat. Preserve immutable V1199693fbf2; NEW authenticated full1506 descendant only src/pr_state_model_tests.rs. Replace only the2 prose checks with windows(2) requiring exact independently fixed consecutive row literals, preserving leading boundary space and me/rged split; no trim/normalization/OR/logical-gap erasure. All other V119 bytes, all15modelnames/5480literals/C/State/native counts exact. No product/source/cap/clock/Main/runtime changes or workerbuilds.
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

2026-10-16 Root primary-source inventory refresh while reviewing native metadata: official https://docs.github.com/en/rest/pulls/pulls#merge-a-pull-request-asynchronously documents PUT merge-async,202+UUID acceptance,409 existing pending UUID/options,200 alreadymerged/enqueued, explicit direct_merge/merge_queue/default and separately retrieved result retained24h. It also says conventional merge endpoint does not support stackedPR/queue and recommends async. Current src/backlog search has ZERO merge-async/stackedPR entries; this is a tracked capability/native parity gap under existing merge task, NOT a claim of current gh/native support or authorization to silently replace working semantics. Before closure inventory installedgh/host capability and stacked/downstack effects, explicit exact head/bypass=false/target preview; add meaningful code/provider + paired realbinary tests for202/409/200queuedvsmerged/expired404/races/noresend/oldhost explanation. Preserve existing method/auto/queue/branch-deletion distinctions and no live mutations except explicitly configured disposable repo.

V68 read-only current-source proposal frozen: manifest target/build/provider-pr-state-v68-proposal-20261016.json SHAd9a6d8418f97c64287dc52d7e6ad977a013deaf00acf8968896d116f8e70958c; full material target/build/provider-pr-state-v68-proposal-20261016.proposal.json SHAc1768c019662a8c41e1440e5dbe038a0a8fa7effab36b2874c556be8c44694d7. Authenticated exact V67 all1320 before/after; static190native files have0merge_workflows/0pr_lifecycle declarations, foundations15+15 stillundeclared. Installed gh2.101.0/git2.53.0/help reverified read-only; current official merge-async/stack/UUID acceptance/queue/update/revert/ref docs primary sources refreshed. First proposed actual Close/Reopen10read/22Confirm/13manual purpose on same Core/queue/Form/PrPlan/journal plus durable state/query fence. Full19 operations, modern async+all open downstack effects, remote-head deletion/update/revert and all-source/host limitations stay REQUIRED. New closed State origin/disposition/recovery/CAS interfaces are material PROPOSAL ONLY awaiting full Root review; no third producer/oldcallback activation, no product edits/build/tests/live mutations or AC/Done.
<!-- SECTION:NOTES:END -->
