---
id: TASK-74
title: Keep long local paths and raw refs readable before choosing or confirming
status: In Progress
assignee:
  - '@graph_parallel'
created_date: '2026-10-14 23:43'
updated_date: '2026-10-15 01:55'
labels: []
dependencies:
  - TASK-4
  - TASK-10
  - TASK-41
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-6 - Local-Git-workspace-and-operation-contract.md
priority: high
type: bug
ordinal: 3069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Six real-binary failures with genuinely long workspace roots show that local identity and mutation-target labels can disappear: the global context clips a non-UTF8 branch suffix, worktree inventory clips the raw basename, and the removal form only shows an editor tail. Users must be able to distinguish exact raw targets before choosing or confirming, without shortening their actual directory or guessing from a truncated label. Preserve the existing raw typed selection and current-source/mutation guards. This is a concrete layout/readability bug adjacent to Task35 evidence, not an expansion or completion of the broad color task or gated Task67 novice evaluation.

## Testing steps

1. Code/provider tests: use actual Git fixtures and TestBackend at80x24 and120x40, long Unicode and raw-byte paths/refs, multiple ambiguous basenames, escaped/literal-byte distinctions, no-color, missing/stale/disabled choices, resize, bounded detail text and disjoint shared key/mouse hits. Require exact raw targets and full40/64-character IDs remain unchanged; no Git/GH/SQL/I/O during display-only details.
2. Real-binary TUI E2E scenario long_local_identity: fresh keyboard and mouse fixtures with an explicitly long directory prefix, raw ref overrides and raw/Unicode/literal worktree siblings. Open discoverable full-source/full-target details, page/scroll/resize/close with both inputs; verify exact visible identity plus strict empty/expected GH journal, full original Git SHA/refs/HEAD/index/staged/dirty/untracked bytes, private SQL/config preservation and TTY/owned-child cleanup. For removal, Preview and explicit Confirm must affect only the exact chosen disposable worktree; cancel/stale/read-only inspection must affect nothing.
3. Run node scripts/test-task.mjs long_local_identity and impacted original immutable_overrides/local_picker/stash_worktrees and source_identity/scoped forms, retaining all original six negative artifacts. Require nonzero executed code/PTY counts and actual completed keyboard/mouse journals; run cargo test --offline --locked --all-targets, cargo fmt --check and cargo clippy --offline --locked --all-targets -- -D warnings. Record actual logs/counts/paths and nativeMac limits; no fixture shortening, ignored tests, increased timeout or title-only acceptance.
4. Equivalent required direct task commands: cargo test --test tui_e2e long_local_identity -- --nocapture and cargo test --all-targets. The count-checked wrapper must still verify nonzero actual executed code/provider and keyboard/mouse PTY coverage; these explicit commands do not waive offline/locked, format, Clippy, artifacts or native macOS limits.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 At80x24 and120x40, a long local workspace context preserves a useful distinct ref/revision label and an always-visible source-details action; full escaped source path and raw ref remain discoverable without extra Git/GH/SQL.
- [x] #2 Local inventory choices retain exact typed identities and expose an always-visible keyboard/mouse route to bounded, scrollable full selected target/identity details; clipping cannot hide the only detail action or turn display text into command authority.
- [x] #3 Worktree removal intent and preview expose the full escaped target before confirmation, including raw-byte versus Unicode/literal siblings; display/details/cancel never alter the source or chosen mutation target.
- [x] #4 Long-path Unicode/no-color/focus/resize/scroll/disabled/stale and both-input regressions execute with independent visible/command/storage/Git/config/TTY oracles and nonzero task-key counts.
- [ ] #5 Full local regression/format/strict-Clippy and source-integrity evidence are recorded; broad Task4/10/35/41/67 dependency acceptance and nativeMac status remain honest.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run meaningful task-specific code/provider and real-binary keyboard/mouse PTY tests with nonzero actual selected counts; record exact commands, results and retained artifacts.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify clickable mouse/keyboard action parity and applicable focus, resize, scrolling, overlay, loading, disabled and failure states with independent visible plus command/storage/Git effects; native macOS status remains explicit.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Read-only diagnosis is active in isolated copy only. First preserve exact six long-root raw artifacts and direct source/currentMain baseline hashes. Reuse the existing prepared Repository source-identity viewer; add a bounded compact context projection that keeps ref/revision suffix visible with an explicit ellipsis, never parsing it into a target. Add an always-visible F2 Details control to the existing local inventory, with page/scroll/Back using the same PickerAction semantics and exact selected typed value; escape and bound full-value display outside draw, with no commands or new authority. Investigate a narrow local removal-field full-value detail route or existing prepared Preview reuse without affecting PR/Actions form semantics. Send exact source paths and concrete UI choice to Root before edits; await material UI plan review, then implement in small tested slices. No Tree/Diff/Timeline, shared support, Main source or frozen SQLite/catalogue edits. Formal dependencies stay open.

Root approved2026-10-14 the explicit TASK74 view plan in fresh immutable a0e163e9 copy target/parallel-paths.9b85j1. Restore only eight previously frozen unchanged metadata prerequisites (six export-ignored tracked files and two checksum-declared ignored lockfiles); keep full source audit. First add bounded display-only context projection only when genuine prepared Source identity details exist, and prove full escaped original raw ref/path there. Then implement keyboard F2 plus mouse-clickable Details on genuine selected typed local inventory rows, original owner/ticket checks, bounded immutable display-only full-value preparation, paging/scroll/resize/Back and retirement; no I/O, chosen-value editing or mutation authority from labels. Prefer existing guarded RemoveWorktree Preview for effects plus narrowly scoped current-intent full details, without rewriting generic PR/Actions permission/form semantics. New code/provider and long-root both-input PTYs must preserve strict source/fullSHA/Git/SQL/config/TTY effects, stale/disabled/missing/cancel/ref/control safety. Send any newly necessary source hooks before edits; do not touch Tree/Diff/Timeline/shared support/frozenTask73/catalogue or Main. Keep Task67/formal prerequisites/Done/nativeMac gates open.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-15 isolated a0e163e9 implementation progress: target/parallel-paths.9b85j1 source audit verifies86upstream sources/10300files plus117registry packages; no Cargo/vendor changes. Actual header projection2/2 and new details/form/provider7/7 code tests green (paths-details-code-fifth.log), including exact raw selected choice/full64IDs, malicious terminal controls, bounded raw/escaped budgets, wrong slot/query/context/typed value, disabled current worktree, clone retirement, scroll/resize/edges and current-input source/purpose/edit/preview/secret retirement. First genuinely long330+byte-root keyboard/mouse PTYs2/2 green1.41s (paths-long-native-sixth.log, four sessions); full escaped raw source/ref/40HEAD and exact raw worktree+Unicode/literal siblings, no Git/SQL/config changes during display and explicit guarded removal effects. Added disabled/no-match and real external linked-HEAD change negatives, still running; do not claim latest focused/full acceptance yet. Old native stash_worktrees2/2 green4.91s after full current-input detail proof; original full predicates/effects preserved. Editable Form uses existing phase/footer/caret observer, cursorless completed frames only for read-only overlays; source/preview actual scroll revealed clipped offscreen rows, now full exact-scalar visible viewport checks. Original compilation/API, expected escaped-ESC, clone-title and readiness negatives retained, not ignored or counted positive. Formal Task4/10/41/67 and nativeMac remain open.

Frozen2026-10-15 V1 candidate: target/build/graph-parallel-task74-long-local-identity-v1-20261015.patch SHA256 dd9b1150214878102acd20010faea55e800dc83befcd9e047b7df53e47accaf6, exact12paths with byte-derived immutable a0e163e9 baseline/output hashes and16explicit producing/shared dependencies in paired -handoff.json. No Main source edits; no Cargo/vendor/new provider authority or Git guard changes. Final count-checked long_local_identity9code/2realPTY GREEN0failed/ignored1.59s, actual4sessions all exited/nonpanicking/owned cleanup complete, journal /var/tmp/sql73.sf49W4/ghtui-test-coverage-YdF4le/journeys.jsonl. Existing repository_source_identity checked19code/2PTY GREEN, bYFLJC journal. Original exact long TMP=/home/admin2/CODE/ghtui/target/parallel-sqlite.rEbslh/test-tmp +same Git ceiling reran all six original scenarios PLUS2original-overrides cases: immutable_overrides4/4, local_picker2/2, stash_worktrees2/2; actual journals4+2+2sessions all complete/both inputs, original raw effects unchanged. New actual330+byte Unicode/combining-root fixtures additionally prove exact raw/Unicode/literal sibling target, disabled current worktree, no-match, read-only Enter, real external HEAD change refusal, fresh explicit removal and unchanged main staged/working/untracked/index/refs. Five details actions Home/End/PgUp/PgDn/Back are clickable +keyboard with full wrapped hits also code-tested at40columns. Final fmt0/strictClippy0/sourceaudit86/10300/108/117 exit0. Isolated whole was honestly NEGATIVE: first1887code+621/624PTY3fail100.94s before final footer; final coherent1887code+623/624PTY1fail85.01s, older catalogue mouseQuit child failed exit1189. a0 copy lacks already-admitted frozen fe6 catalogue-close correction; no fix/whole-green inferred. First Actions control/cache-owned-file and blended explanation negatives retained and NOT diagnosed away by rerun. All logs/artifacts in target/parallel-paths.9b85j1/target; paired manifest records exact counts/commands/negative paths and original six artifacts. Root independent focused/Main combined whole still required. Formal Task4/10/41/67/deps/nativeMac and Task74 Done remain open.

V2 export metadata-only correction frozen: target/build/graph-parallel-task74-long-local-identity-v2-20261015.patch SHA2560094c16569cd7af0cbb13d18e80c8864c68d42bf14178417d8b3bae0e4d47108 (77660bytes), paired -handoff.json SHA256ff113bc5e1e869ae79d5ab3a41442cb23c64c924ed3c8da7abb7239c887fb404. All12baseline/output source hashes IDENTICAL V1; four required new-file Git mode lines added, V1 retained as export negative. Mechanical snapshot from actual immutable a0 Git blobs directly inside filesystem verified every baseline hash then git apply --check exit0; log paths-v2-immutable-byte-apply-check.log, byte baseline task74-byte-baseline.D8cTJZ. Read-only current Main Git apply-check excluding Root-owned README exit0; README only narrow paragraph needs Root newer-doc composition, no overwrite. No source changed after final focused/full/strict gates. Candidate now sent for independent Root review/admission; own inherited whole negative remains explicit and Task74 not Done.

Root 2026-10-15 Main admission: read complete production/header/details/forms changes, all9 domain tests and complete689-line long-root PTYs plus preserved original journey migrations. Frozen V2 SHA0094c165 and paired manifestff113bc5 verified;9 source/test output hashes exact, UI/lib composed only to preserve admitted C-native hooks, README narrow additive. No Cargo/vendor/Git guard/shared support writes. Current-Main long_local_identity checked running; frozen combined Timeline+C+Task74 all-target and strict-Clippy/audit running, not pass evidence yet. Agent honest623/624 older-a0 whole failure retained; newer fe6 catalogue-close prerequisites preserved, no inferred historical cause/fix or Done/Mac claim.

Actual current-Main long_local_identity checked terminal0:9code/provider and2real-binary PTY tests pass0failed/ignored1.61s; G7zrhd journal4actual complete sessions2keyboard/2mouse, each exited/cleanup completed/nonpanicking. All16 frozen agent positive/negative artifact digests independently verified. Combined current-Main strictClippy terminal0(1m17s including build locks; not a performance benchmark),fmt0,dependency audit86/10300/108/1170. Whole test handle71545 still running; no whole-green/Done/nativeMac claim.

Journal correction: direct Main G7zrhd records are 4 positive completed sessions, 3 with actual keyboard input and 2 with actual mouse input (mixed input overlaps). The preceding shorthand2keyboard/2mouse understated actual keyboard records; no sessions or assertions changed. Dependency audit reports117registry packages and exit0, not1170packages.

Root frozen combined Timeline + saved-native-diff + long-local-identity checkpoint, 2026-10-15: actual cargo test --offline --locked --all-targets terminal 0; 1,957 code/provider contracts (306 strict GitHub) and 646 actual PTY test functions pass, zero failed/ignored, PTY 85.66 s. 33 test targets include the zero-test main target, which is not counted as coverage. Root whole log target/build/root-diff-timeline-longpath-whole-first.log; strict all-target Clippy exit 0, fmt exit 0, dependency audit exit 0 (86 sources, 10,300 files, 108 adaptations, 117 registry packages). All 43 frozen production/test hashes unchanged through gates. Root preserved current SQLite/PID/Related guards and catalogue-close prerequisites. This green integrated checkpoint does not prove the causes of retained older negative artifacts, complete broad product coverage, native macOS verification, or release readiness.

Root objectively checked the four narrow product/readability criteria using reviewed native/domain evidence and current integrated gates. Task remains In Progress: broad dependency and native macOS/release acceptance are not closed by this Linux checkpoint.

Root integrated checkpoint2026-10-15 terminal GREEN: actual cargo test --all-targets with offline environment passes2012 code/provider (318 strictGH) and650 real-binary PTY functions,0failed/ignored,86.99s native;34targets includes zero-testmain NOTcoverage. All37 frozen source/test paths unchanged through gates (36code/tests+README); only README checkpoint prose updated afterward. strictalltargetClippy0(43.49s inclbuild),fmtcheck0,gitdiffcheck0,dependency audit86sources/10300files/108adaptations/117registry0. Logs target/build/root-inline-mutable-metadata-whole-first.log and -clippy-first.log; immutable source manifest root-inline-mutable-metadata-source-freeze-v1-20261015.json. Existing retained negative1975code/645of646PTY94.80s records Related env-sidecar publication race; separate complete metadata record +4contracttests and2 genuine held pre-env PTYs now prove correction, without weaker auth/proxy/argv/Git/SQL/TTY assertions. This does NOT diagnose/waive older agent Blended failure or prove full product/Mac/release coverage.
<!-- SECTION:NOTES:END -->
