---
id: TASK-41
title: Manage stashes and worktrees without losing local work
status: In Progress
assignee:
  - '@foundation'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-13 02:41'
labels:
  - localgit
  - e2e
milestone: m-6
dependencies:
  - TASK-40
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-6 - Local-Git-workspace-and-operation-contract.md
priority: high
type: feature
ordinal: 41
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Switching tasks requires preserving unfinished changes and isolating branches. Stash and worktree operations must explain what is saved, applied, removed or still dirty.

## Testing steps

1. Code/provider tests: Test stash contents, apply/pop conflicts, worktree ownership, lock/dirty guards and cancelled removal.
2. Real-binary TUI E2E scenario `stash_worktrees`: Save/open/apply a stash and add/open/remove a worktree through both input paths, verify objects and files, and assert dirty removal is blocked or explicitly confirmed. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e stash_worktrees -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Stash save/view/apply/pop/drop distinguishes tracked/untracked inclusion and resulting conflicts.
- [ ] #2 Worktree list/add/open/remove shows branch ownership and dirty/locked status.
- [ ] #3 Destructive removal/drop uses exact-target confirmation and never silently removes dirty work.
- [ ] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [ ] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Backend preparation is available through shared src/git.rs, src/diff.rs and src/repository_view.rs. Real generated-Git fixture tests cover canonical status/diff/index/objects/refs, ordered DAG parents and immutable tree reads, staged patches, hook rejection, branch/tag guards, local-bare push/fetch, stash/worktrees, conflicts/abort, reflog rescue and bisect reset. Isolated offline source validation uses /tmp/ghtui-git-validation.AsAVCb/Cargo.toml: 8 code/model unit tests + 19 real Git integration tests passed (0 ignored), cargo clippy --offline --manifest-path /tmp/ghtui-git-validation.AsAVCb/Cargo.toml --all-targets -- -D warnings passed; rustfmt --check own modules/tests passed. Prepared backend only: dependencies, full UI, mouse/keyboard PTY and full-project test/fmt/clippy verification remain required. No task completion claimed.

Native stash/worktree forms retain exact stash commit IDs and paths. Removal preview performs a bounded cancellable Git discovery/status check and refuses a dirty linked worktree before any confirmation. Actual full-crate local_actions_contracts passed 10/10 and both stash_worktrees real-binary PTY journeys passed, independently checking tracked/untracked bytes, immutable stash retention, linked worktree creation, dirty-removal refusal and safe clean removal while original working edits survive. Apply/pop conflicts, all picker/browsing UX and final all-target/fmt/clippy gates remain open; not Done.

Additional real-Git safety discovery: a linked worktree with only ignored.env reports empty porcelain status, but installed Git worktree remove succeeds and deletes those ignored bytes. Backend agent is adding a guarded refusal for ignored files, and both native stash_worktrees PTY journeys now exercise this refusal, independently checking ignored bytes and linked metadata remain before any known fixture cleanup. This closes an important cleanliness-vs-local-data distinction; the new regression is pending its focused run, not accepted by default.

Installed Git independently verified ignored-only worktree deletion risk in disposable /tmp/ghtui-ignored-worktree.c8s9yV: empty porcelain, ignored.env reported by ls-files --others --ignored, ordinary worktree remove exited0 and deleted only generated fixture bytes. Backend now refuses plan/execution if ignored local files exist in exact linked target, with message Worktree removal refused: ignored local files would be deleted. Move or remove those files explicitly first. No force/consent guessing added. Generated Git regression and foundation-owned both-input native ignored-file preview journeys prepared; actual focused/full verification pending.

Actual checked node scripts/test-task.mjs stash_worktrees passed 3 code/provider + 2 real-binary PTY tests, zero ignored, shared offline target/jobs2/incremental0. Keyboard/mouse coverage /tmp/ghtui-test-coverage-BGfrq0. Native paths independently verify immutable stash retention/apply, root tracked/untracked preservation, linked worktree creation, dirty refusal and ignored-only refusal with important ignored.env bytes preserved before safe clean removal. Backend negative-control proves ordinary Git would delete the ignored-only fixture; guarded code prevents it. Batch log target/build/native-management-checked.log. Picker and stash-conflict coverage/global gates remain open.

Typed local inventory picker gate passed: CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 node scripts/test-task.mjs local_picker selected 7 code tests (2 Form +5 actual Git/picker contracts) and2 actual-binary keyboard/mouse PTY tests, all pass/zero ignored. Artifact /tmp/ghtui-test-coverage-GMYYXM; log target/build/local-picker-checked.log. Native journeys verify cancel/no-match, exact stale branch rejection with unchanged HEAD/index, refreshed branch choice, immutable stash SHA disclosure/apply/retention,80x24 resized picker, and exact raw-byte worktree removal preserving a similar Unicode worktree and root dirty files. Reflog rescue immutable SHA picker currently has code coverage, not an additional native rescue-picker journey. APFS-valid newline path fixture replaces invalid filesystem UTF-8 only on macOS; Linux retains byte0xff, neither platform skips raw-path testing. Full product/global gates and task-specific remaining cases stay open; not Done.
<!-- SECTION:NOTES:END -->
