---
id: TASK-39
title: Manage local branches tags checkout and upstreams
status: In Progress
assignee:
  - '@foundation'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-13 03:22'
labels:
  - localgit
  - e2e
milestone: m-6
dependencies:
  - TASK-38
  - TASK-28
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-6 - Local-Git-workspace-and-operation-contract.md
priority: high
type: feature
ordinal: 39
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Users should navigate and organize local refs without accidentally replacing dirty files. Actions need plain-language effects and a visible current branch/upstream.

## Testing steps

1. Code/provider tests: Verify refs/upstreams and checkout outcomes against real Git, including dirty collisions and branches used by another worktree.
2. Real-binary TUI E2E scenario `branches_tags`: Create/switch/rename/tag via keys and clicks, cancel a deletion and attempt a dirty switch, then inspect actual refs and untouched conflicting work. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e branches_tags -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Create/rename/switch/delete branches and create/delete tags expose scope, current state and upstream.
- [ ] #2 Dirty/untracked collisions, worktree-owned branches and detached HEAD are handled explicitly.
- [ ] #3 Deletion or history-changing choices require exact-target confirmation and useful recovery context.
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
Connect the prepared typed local inventory picker through native forms and a worker-loaded overlay; persist exact ref/stash/worktree identities alongside versioned drafts, invalidate selections only on real text edits, and verify keyboard/mouse/stale-target/raw-path behavior through the executable.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Backend preparation is available through shared src/git.rs, src/diff.rs and src/repository_view.rs. Real generated-Git fixture tests cover canonical status/diff/index/objects/refs, ordered DAG parents and immutable tree reads, staged patches, hook rejection, branch/tag guards, local-bare push/fetch, stash/worktrees, conflicts/abort, reflog rescue and bisect reset. Isolated offline source validation uses /tmp/ghtui-git-validation.AsAVCb/Cargo.toml: 8 code/model unit tests + 19 real Git integration tests passed (0 ignored), cargo clippy --offline --manifest-path /tmp/ghtui-git-validation.AsAVCb/Cargo.toml --all-targets -- -D warnings passed; rustfmt --check own modules/tests passed. Prepared backend only: dependencies, full UI, mouse/keyboard PTY and full-project test/fmt/clippy verification remain required. No task completion claimed.

Native LocalCommand forms/controller and actual management PTY journeys are implemented in src/local_actions.rs and tests/tui_e2e/local_management.rs. Shared full-crate cargo check --offline --all-targets passed with the complete relative-path vendored search/runtime dependency graph. cargo test --offline --test local_actions_contracts passed 10/10, zero ignored, including branch/tag/upstream exact targets, local-bare remote effects, immutable stash IDs, dirty worktree removal refusal, conflicted merge resolution/stage/continue, reflog rescue cancellation, and bisect reset. Actual management PTY gate is running; no UI or task completion claimed until its results and full project gates are recorded.

Actual final focused command for this slice: with CARGO_NET_OFFLINE=true, CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target, CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=2, node scripts/test-task.mjs branches_tags passed 2 code/provider + 2 actual-binary PTY tests, zero ignored; keyboard/mouse coverage /tmp/ghtui-test-coverage-AOiUu0. Both paths now verify native create/switch/rename/tag/upstream, cancelled tag deletion and rejected dirty switch with exact refs/current branch/HEAD/dirty bytes preserved, at 120x40 and 80x24. Full batch log target/build/native-management-checked.log; complete global gates and richer picker/owned-branch cases remain open.

Native Form-owned local inventory picker connected: F4 and clickable Choose share exact typed selection semantics; workers gather refs/remotes/stashes/worktrees/reflog; display labels never parsed into raw targets. Version1 local drafts preserve raw worktree bytes and immutable identities, legacy maps restore compatibly, actual text edits remove typed selections while cursor/focus changes do not. Form/viewport/draft/secret unit gate passed4/4 before latest atomic-restore hardening; dedicated actual picker PTY preparation underway. No Done.

Typed local inventory picker gate passed: CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 node scripts/test-task.mjs local_picker selected 7 code tests (2 Form +5 actual Git/picker contracts) and2 actual-binary keyboard/mouse PTY tests, all pass/zero ignored. Artifact /tmp/ghtui-test-coverage-GMYYXM; log target/build/local-picker-checked.log. Native journeys verify cancel/no-match, exact stale branch rejection with unchanged HEAD/index, refreshed branch choice, immutable stash SHA disclosure/apply/retention,80x24 resized picker, and exact raw-byte worktree removal preserving a similar Unicode worktree and root dirty files. Reflog rescue immutable SHA picker currently has code coverage, not an additional native rescue-picker journey. APFS-valid newline path fixture replaces invalid filesystem UTF-8 only on macOS; Linux retains byte0xff, neither platform skips raw-path testing. Full product/global gates and task-specific remaining cases stay open; not Done.

Raw reference safety: native picker classifies canonical raw_name bytes, shows non-UTF8 named-write targets explicitly disabled with a reason, and never passes display escapes as Git argv. Immutable revision operations/read-only comparisons can still choose the exact saved object ID. Fresh Reference validation uses raw bytes rather than rendered labels. New real packed-refs code contract (APFS-valid storage) passed as part of checked local_picker8code+2PTY, zero ignored, artifact /tmp/ghtui-test-coverage-UMSKAC; source log target/build/local_picker-checked.log. Native non-UTF8 named writes remain explicitly unavailable rather than guessing a target.
<!-- SECTION:NOTES:END -->
