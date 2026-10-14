---
id: TASK-3
title: Keep input and rendering responsive during background work
status: In Progress
assignee:
  - '@foundation'
created_date: '2026-10-12 23:06'
updated_date: '2026-10-14 18:46'
labels:
  - foundation
  - e2e
milestone: m-0
dependencies:
  - TASK-2
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: feature
ordinal: 3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
GitHub requests, Git reads, scanning, and indexing can take seconds. The terminal must continue responding while those operations run and ignore results from superseded selections.

## Testing steps

1. Code/provider tests: Drive reordered completions, queue saturation, cancellation, and concurrent failures through domain/event tests.
2. Real-binary TUI E2E scenario `async_events`: Delay a fixture request, switch views and cancel by keys and clicks, assert the shell stays responsive and the late result never overwrites the active screen. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e async_events -- --nocapture` and verify a nonzero selected test count; run the relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. The initial framework is delivered by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Workers use bounded queues and cancellation; stale completions cannot replace the current view.
- [x] #2 Rendering uses prepared state and visible rows without launching work.
- [x] #3 Loading, cancel, retry, and failure states are visible and usable while tasks are pending.
- [x] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [x] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [x] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [x] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Maintain bounded prepared-state workers, cancellable generation-isolated requests and visible loading/failure states; verify reordered/cancelled/hidden completions and context-sensitive identity privacy with checked real-binary keyboard/mouse journeys, then native platform/global gates before finalization.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Foundation runtime prototype uses two worker threads, queue capacity 4 and completion capacity 16; per-view request generations reject superseded results, cancellation uses provider CancellationToken, rendering consumes prepared state. Added reordered completion, hidden-view completion and queue-saturation unit tests; real-binary async_events keyboard/mouse journeys delay strict fake-gh, navigate, cancel, retry, inspect journals and require cancelled PID termination. All remain unrun because foundational dependencies cannot be fetched; task dependencies and AC/DoD remain open.

Actual checked framework execution: CARGO_NET_OFFLINE=true node scripts/test-task.mjs async_events selected 4 code/runtime tests and 2 real-binary PTY tests, all passed with actual keyboard+mouse paths; coverage journal /tmp/ghtui-test-coverage-Hel4vF. Tests prove reordered/superseded completions, hidden-view isolation, bounded queue saturation, worker-panic visible failure, delayed gh cancellation, navigation responsiveness and cancelled fixture PID cleanup. PTY now uses complete strict Inbox authentication/cache-refresh/three-queue request sequence rather than a version-only probe (8 matched commands including cancelled attempt). Worker shutdown cancels and joins before terminal cleanup. Native macOS execution and latest shared full-suite/fmt/clippy completion gate remain outstanding; task status unchanged pending dependencies.

Latest local full checkpoint258code/provider+102PTY, fmt and strictall-target Clippy passed. Two new async identity-transition tests additionally prove read-only Notice clearing on identity failure/same-login principal changes without clearing unrelated Settings state. Framework/platform prerequisites and dependency completion remain gates; actual implementation status is In Progress, not To Do or Done.

2026-10-14 NEW READ-ONLY continuation V2/current Graph-Tree runtime composition audit: target/build/provider-continuation-v2-current-runtime-composition-audit-v1-20261014.txt,181lines SHA2561b2b0fdeb4544b8bc4bc3d6d53bed44b7319f850fde9bad38b7a8f7737fac732. Actual frozen activation git apply --check exit0 is context ONLY. Definite admission corrections: add continuation veto to NEW current OriginalTree prepared-only (local-saved veto already in frozen delta; future unadmitted Diff needssame); specialized same-owner continuation-only publish+ownerless/mixed rejection BEFORE generic loading/status/private effects; current Err/panic/quota paths need redacted immediate cleanup rather than generic status/pending retain; typed Hidden needs actual retire_remote_views, since privacy onlyfences/source-drop and Main stoppedintent runs BEFORE poll, test finish's Close masks immediate retainedActions frame; prequeue take_work expiry clearsmatchingowner/no perpetualpending; EARLY global Action::Refresh guard must forward scoped semantics or retainlegacyonly. Normal Actions keyboardr alreadymapsdirectActionsActionRefresh, not falselyclaimedbroken. CurrentMain suppresses workerpanic payload, so no rawpanicTTYdisclosure claim. Inventory preserves actual original_tree_result/Request-Completion owner forwarding/Graphsource-focus/savedvisits, no new standalone Request field or parent overwrite, page=None completion with actual originalChecks retained. Meaningful genuineSource/Git/SQL/runtime+both-input native requirement matrix recorded, ZERO new declarations/tests/Cargo/includes/frozen edits/pass/AC/Done. Frozen1258line patch hash549bff0a8a3dfbcec911573b3adfc96f101abfc676bc0d71ec4a629fdd1cae71 unchanged; original48 staged declarations still UNRUN pending Root admission.

2026-10-14 FROZEN SOURCE-ONLY continuation corrective V3 addendum for Root review/admission: target/build/provider-continuation-corrective-v3-handoff-20261014.json SHA256 b82cb5322d6485d1df7e9ea06dc0c4126fb863ffe1c295a8cc68fb612ab7b532, readable .md SHA256 de34b54b22e9a6a99b021298126452a45b33982f8e017b9313f56a92563a2304. Four NEW undeclared files, TEN narrow UNAPPLIED hunks/SEVEN parents and separate THREE-hunk future-Diff conditional veto. Root confirmed actual GraphV3 admission is prerequisite; its absent retained facet is not called current. Strict same-generation/Source/Scope/Nav/route/capture/watch/request owner and closed continuation-only/mixed/no-owner result gate BEFORE every effect; immediate current Hidden/Err/caughtpanic/quota privacy plus retire_remote_views (typed compatibility install too), no synthetic Close, provider/SQL/Git/local Graph/Tree mutation or raw diagnostic. Matching prequeue expiry clears only matching ticket; global/footer/palette Refresh forwards SAME typed Actions semantic while legacy None stays disabled. Preserve Root newer queued local intent veto, Tree retained source/navigation, Request/Completion owner forwarding. Thirteen genuine Source/strict-gh/SQLite/Git/runtime declarations and EIGHT keyboard/mouse real-binary declarations,21 total, ALL UNREGISTERED/UNCOMPILED/UNRUN; original48+new21=69 declarations only. Native source-derived predictions prefix74/global refresh89/inner UID43 failure86/held closingSQL and Cancel87; actual PID T-stop/reap, exact original SQL change and unchanged drafts/journals/Git/config/TTY independently asserted. Actual selected rustfmt-check0, original frozen git-apply-check0, new patch numstat parse0 and31 in-memory old+delta context matches ONLY style/grammar/context, not tests. Reverified all13 original frozen source hashes and original1258-line patch unchanged. Root owns all included source/Cargo; no builds, admission, AC or Done claim. Full hash/source review, nonzero focused13+8/original32+16, shared regressions and whole all-target/fmt/strictClippy remain mandatory. Historical/head/step mapping(annotation step index UNVERIFIED)/canonical jumps/export/scoped writes/full parity remain required.
<!-- SECTION:NOTES:END -->
