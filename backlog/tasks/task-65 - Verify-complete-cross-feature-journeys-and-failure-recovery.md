---
id: TASK-65
title: Verify complete cross-feature journeys and failure recovery
status: To Do
assignee: []
created_date: '2026-10-12 23:13'
updated_date: '2026-10-15 02:13'
labels:
  - delivery
  - e2e
milestone: m-5
dependencies:
  - TASK-64
  - TASK-51
  - TASK-75
  - TASK-76
  - TASK-77
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: feature
ordinal: 65
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Individual features can pass tests while navigation, caching or identity falls apart at boundaries. End-to-end journeys must prove the whole product works with real process and terminal interactions.

## Testing steps

1. Code/provider tests: Test cross-feature state/identity reconciliation and verify all scenario keys select nonzero meaningful tests.
2. Real-binary TUI E2E scenario `complete_journeys`: Run the full real-binary fixture journeys through code/process/storage/terminal boundaries, inspect effects independently, and prove injected wrong results fail the suite. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e complete_journeys -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 PR review, local edit-to-commit/PR, history-to-tree/search, and Actions-to-log/artifact/approval journeys pass.
- [ ] #2 Each journey runs via keyboard and mouse on macOS/Linux with wide/narrow/resize scenarios.
- [ ] #3 Offline/rate-limit/permission/head-change/editor failures preserve drafts/state, and failures retain reproducible artifacts.
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
Explicit privacy audit item: the current legacy GithubAction::Compose worker restores a private saved PR draft using cached workspace identity before a fresh numeric-principal bracket. New typed metadata OPEN must require fresh positive principal before any private draft/original SQL; additionally audit and migrate generic legacy PR compose OPEN under TASK6/TASK16 with strict updated source-derived protocols and genuine keyboard/mouse reused-login UID switch/auth-expiry canary tests. Cache permission checks, unchanged login, or a later publish guard cannot substitute for this preprivate-read boundary. Preserve durable original references and no-resend barriers; document actual migrated test counts and full gates, not planned coverage.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Required future Task18 cross-feature journey: with strict offline gh and real Git, hydrate genuine PR body/comment/review/inline sources carrying an old label filter; add/remove an indexed genuine label through mandatory typed preview and exact current-set checks via keyboard and mouse; independently assert one ID-native mutation, original durable receipt, unchanged body/Git bytes, stale label-frame suppression and late generation refusal, then fresh metadata rehydration yields only new label matches. Inject actual structured cache-fence SQL failure after the confirmed receipt, navigate Search/Dashboard before opening original recovery, abruptly restart and repair bounded exact original label metadata before cached filtered reads; never infer invocation/effect from Pending or resend confirmed/uncertain writes. Reused UID/repository/PR node, renamed/reused actor nodes, readback/permission/rate/budget failures, both 120x40 and80x24 plus resize/focus/disabled actions must preserve private visibility and actual effect counts. Native journey is NOT implemented or run by the current source-only backend slice; dependencies/AC/status remain unchanged.

Additional SOURCE-ONLY TASK18 integration plan: target/build/provider-task18-native-integration-review-20261013.md specifies central original-parent label-filter repair at guard_discussion_reads, inherited by exact/ranked/saved queries, cached context/Open and genuinely discovered blended parents, with before/after journal/cache/fresh principal/current access proofs and all in-memory PR/Search/Source/Dashboard invalidation. Required keyboard/mouse cross-feature fixture independently asserts one ID-native mutation and original receipt, old label bindings hidden but body/Git bytes retained, prewrite late reservations refused, SQL fence failure navigating Search/Dashboard BEFORE original viewer and after SIGKILL, bounded original receipt repair without resend or invocation inferred from Pending, then only fresh rehydrated new-generation labels match. Unrelated canonical PR/repo/UID remains unchanged; malformed metadata UTF8/cap, current access loss, current epoch races, source unknown/partial and discovered parents cannot bypass. No test implementation/execution or completion is claimed by this planning artifact; Task65 remains To Do and dependencies/acceptance unchanged.

Generic legacy Compose privacy migration now has a source-only worker/accessor handoff at target/build/provider-legacy-pr-draft-open-v1-handoff-20261014.md and fourteen meaningful UNRUN identity/storage/Git declarations. Future actual key/mouse seven-kind journeys must add source-derived preprivate and final principal/resource checks to every actual OPEN/reopen/restart, independent private draft/receipt canaries, raw index/refs/staged/dirty bytes and TTY effects. Explicitly include sameUID repo403/replaced canonical DB/node and PR/fullhead/base changes BEFORE private SQL plus final UID/access drift hiding text and corruption diagnostics. Helper identity-only4 is separate from complete access-aware native arithmetic; no fake-ID conversion, later publish guard or unchanged login can substitute for current read access. Pending/Uncertain preserve original state/ref/no-resend, corruption/oversized UTF8 must not fall back fresh. No native implementation/run or completion from this handoff; Task65 remains To Do with dependencies and AC unchanged.

SOURCE-ONLY binding handoff ready: target/build/provider-legacy-pr-draft-binding-v1-handoff-20261014.md and reviewed candidate provider-legacy-pr-draft-binding-activation-v1-20261014.patch. Eleven undeclared Rust files contain45 declarations (14 identity/storage+13 access+10 combined+8 binding), all UNCOMPILED/UNRUN; existing exact-original7 must rerun after shared policy forwarding. Mandatory UnboundOriginal quarantine preserves bytes/no Form/no adoption/fallback; access-issued v4 compares actual repo/PR IDs and exact source, no auth/approval persisted. Original <=8KiB exact lookup on validated READ_ONLY connection precedes edited fields under same captured deadline; confirmed command/selected resource/kind-correct effect required, no resend. Explicit legacy serialized cap rederived1,724,722<2MiB; metadata/global caps unchanged. Zero-call cross-host preflight added. Selected edition2021 fmt/check exit0 and read-only patchcheck exit0 after retained initial context/format failures; neither is compile/test evidence. Normal planned native OPEN14, identity4 bytes separately outside group accounting; historical moved-head viewer/adoption UI remain unfinished. Root retains all included/Cargo lease.

SOURCE-ONLY native fixtures ready, UNREGISTERED/UNCOMPILED/UNRUN: tests/tui_e2e/legacy_pr_draft_admission_v1.rs child reuses existing pr_metadata ancestor/framework;6 meaningful keyboard/mouse declarations cover all7 witness save/restart (exact28 then56), raw/v1/v2 quarantine (28), repo403 (16)/final same-loginUID drift(28), source OPEN14. Requires future Root genuine admitted proof+witness slot/save; version4 and exact canonical IDs mandatory, no fake witness/default acceptance. Independent raw SQL/typed original journal generation/dirty Git/index/HEAD/full TTY canaries+terminal restoration,0mutations/claim/ACK. Detailed protocol/seams artifact target/build/provider-legacy-pr-draft-native-v1-proposal-20261014.md; selectedfmt/check exit0 only. Historical moved-head recovery/adoption remain unfinished; no includes/core/Cargo/status Done.

Root-approved SOURCE-ONLY historical same-parent handoff ready: target/build/provider-historical-pr-original-access-v1-handoff-20261014.md; new gh/metrics/historical_pr_original_access_v1.rs+tests12, native child tests/tui_e2e/historical_pr_original_access_v1.rs2 are undeclared/uncompiled/unrun. Distinct opaque access5 -> bounded exact original8KiB -> closing5; actualSQL/journal hook after PRGET3 BEFORE repo4/user5, same clock/capture, private callback diagnostics masked, noSQLafterfinal proof. Current c40 remains separate from original recordeda40; actual current IDs observed, no absent oldDB invented; no writer/adoption/ACK. Visibility/export activationcandidate gitapply--check exit0 after retained overlapping-hunk correction; selectededition2021fmt/check exits0 only. Explicit exact-original-only validatedREAD_ONLY constructor extraction/native routing/current-head labels remain Root-owned unimplemented, not inferred from missing draft. Planned native startup18+OPEN14+View10=42 subjectactualroute. Existing draft46/native6 still distinct frozen unrun. Official GetPR/GetRepo/global-node docs checked2026-10-14; no new gh/auth/version floor.

Root-approved SOURCE-ONLY wholeOPEN14 budget handoff FROZEN: target/build/provider-legacy-pr-workflow-budget-v1-handoff-20261014.md; six new undeclared files and separate unapplied provider-legacy-pr-workflow-budget-activation-v1-20261014.patch. Nine meaningful strict provider/SQLite/real-Git declarations are UNCOMPILED/UNRUN. One owned nonClone original Window + ONE common MetricsBudget now includes access5/identity2/private READ_ONLY restore/identity2/finalaccess5, upfront default capture allocations1,581,056 under2MiB. Successful raw stdout+stderr and full failed allocated reservations stay in the whole ledger; five-read deltas are not total. Same actual argv/version-prefix/lossy-firstline/empty-user semantics, fresh/noncoalescing gh transport preserved; no global identity/auth/CLI floor changes. New shared forwards use one restoration/five-parser/raw accounting policy, absolute SQL deadline, actual runner/program/env/numericScope binding and typed failures hiding private text. Selected edition2021 format/check exits0 and exact rg9 declarations; final patchcheck initially exited1 after Root newpr_checks_capture context, ONLY unappliedhunk revised to retain it, rerunexit0. Formatting/applicability are NOT tests. Frozen draft46/historical12/native6/2 unchanged; native witness slots/version4save/quarantine/operation-cache-journal-finalproof-age checks and deliberate old7/v2 migrations remain Root-owned prerequisites. No included source/export/registration/Cargo/native/remote/live mutations, no acceptance/status/Done claim.

2026-10-14 user-selected delivery change: retain this task's reusable code/provider and keyboard/mouse whole-product journeys, with an explicit binary-path seam and independent effects. TASK-68 replays them against the actual curl-script-installed release executable for install/update/rollback/interrupted-install recovery; TASK-69 automates that release gate. Do not make this pre-release task depend on a finished TASK-68 installer, which would reverse the current Task65->66->67->68 order. See doc-12; dependencies, status and AC unchanged, no new verification or implementation claimed.

2026-10-15 parity audit: TASK-75 explicitly tracks whole-file PR review comments (GitHub subject_type=file), separate from TASK-19 line/range publication and TASK-21 thread management. Whole-product review journeys must exercise genuine file-level create/readback/reply with both input modes, changed-head/source-loss and restart/uncertain no-resend recovery, exact remote and storage effects, actor fallback, unchanged Git and TTY. Added TASK-75 as direct prerequisite without changing the existing release/install ordering. New task remains To Do, no completion/testing claimed.

2026-10-15 concrete read-only regression TASK-76 added as prerequisite: independent installed-Git control proved default worktree diff can rewrite index stat-cache bytes even with optional locks disabled, while staged tree is unchanged. Whole-product browsing and independent observers must preserve raw and semantic index before release. Native selected-staging observer is corrected separately without weakening its byte equality. Evidence/provider policy fix and both-input read-only journeys remain To Do; no current full-suite green is claimed as proof of that new edge case.
<!-- SECTION:NOTES:END -->
