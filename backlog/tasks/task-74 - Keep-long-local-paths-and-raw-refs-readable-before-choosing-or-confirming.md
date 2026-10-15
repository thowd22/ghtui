---
id: TASK-74
title: Keep long local paths and raw refs readable before choosing or confirming
status: In Progress
assignee:
  - '@graph_parallel'
created_date: '2026-10-14 23:43'
updated_date: '2026-10-14 23:45'
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
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 At80x24 and120x40, a long local workspace context preserves a useful distinct ref/revision label and an always-visible source-details action; full escaped source path and raw ref remain discoverable without extra Git/GH/SQL.
- [ ] #2 Local inventory choices retain exact typed identities and expose an always-visible keyboard/mouse route to bounded, scrollable full selected target/identity details; clipping cannot hide the only detail action or turn display text into command authority.
- [ ] #3 Worktree removal intent and preview expose the full escaped target before confirmation, including raw-byte versus Unicode/literal siblings; display/details/cancel never alter the source or chosen mutation target.
- [ ] #4 Long-path Unicode/no-color/focus/resize/scroll/disabled/stale and both-input regressions execute with independent visible/command/storage/Git/config/TTY oracles and nonzero task-key counts.
- [ ] #5 Full local regression/format/strict-Clippy and source-integrity evidence are recorded; broad Task4/10/35/41/67 dependency acceptance and nativeMac status remain honest.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Read-only diagnosis is active in isolated copy only. First preserve exact six long-root raw artifacts and direct source/currentMain baseline hashes. Reuse the existing prepared Repository source-identity viewer; add a bounded compact context projection that keeps ref/revision suffix visible with an explicit ellipsis, never parsing it into a target. Add an always-visible F2 Details control to the existing local inventory, with page/scroll/Back using the same PickerAction semantics and exact selected typed value; escape and bound full-value display outside draw, with no commands or new authority. Investigate a narrow local removal-field full-value detail route or existing prepared Preview reuse without affecting PR/Actions form semantics. Send exact source paths and concrete UI choice to Root before edits; await material UI plan review, then implement in small tested slices. No Tree/Diff/Timeline, shared support, Main source or frozen SQLite/catalogue edits. Formal dependencies stay open.

Root approved2026-10-14 the explicit TASK74 view plan in fresh immutable a0e163e9 copy target/parallel-paths.9b85j1. Restore only eight previously frozen unchanged metadata prerequisites (six export-ignored tracked files and two checksum-declared ignored lockfiles); keep full source audit. First add bounded display-only context projection only when genuine prepared Source identity details exist, and prove full escaped original raw ref/path there. Then implement keyboard F2 plus mouse-clickable Details on genuine selected typed local inventory rows, original owner/ticket checks, bounded immutable display-only full-value preparation, paging/scroll/resize/Back and retirement; no I/O, chosen-value editing or mutation authority from labels. Prefer existing guarded RemoveWorktree Preview for effects plus narrowly scoped current-intent full details, without rewriting generic PR/Actions permission/form semantics. New code/provider and long-root both-input PTYs must preserve strict source/fullSHA/Git/SQL/config/TTY effects, stale/disabled/missing/cancel/ref/control safety. Send any newly necessary source hooks before edits; do not touch Tree/Diff/Timeline/shared support/frozenTask73/catalogue or Main. Keep Task67/formal prerequisites/Done/nativeMac gates open.
<!-- SECTION:PLAN:END -->
