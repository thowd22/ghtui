---
id: TASK-42
title: Guide merges rebases cherry-picks reverts and conflict resolution
status: In Progress
assignee:
  - '@foundation'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-18 00:54'
labels:
  - localgit
  - e2e
milestone: m-6
dependencies:
  - TASK-41
  - TASK-31
  - TASK-16
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-6 - Local-Git-workspace-and-operation-contract.md
priority: high
type: feature
ordinal: 42
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Git operations can pause midway and be confusing to new users. A coherent operation view should show what happened, what remains, and how to continue safely.

## Testing steps

1. Code/provider tests: Generate actual operation/conflict fixtures and verify resulting blobs/parents, continuation state and abort restoration.
2. Real-binary TUI E2E scenario `git_sequencers`: Start an operation, resolve a conflict by keys/clicks, continue and inspect history; repeat with abort/failure and assert fixture checkout/sequencer correctness. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e git_sequencers -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Merge/rebase/cherry-pick/revert preview affected commits and expose continue/skip/abort where valid.
- [ ] #2 Conflict viewer shows base/ours/theirs and operation context, supports selective resolution/editing and stages verified results.
- [ ] #3 Binary/directory/rename conflicts and dirty-state races are explicit; cancellation preserves recovery paths.
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

Added explicit ResolveConflict with exact raw path and index stage1/2/3 availability. It previews a destructive replacement of the working file, retains unmerged index state for inspection, and requires a separate Stage action. Actual conflict fixture selects theirs, independently inspects working bytes/conflict count, stages, continues using existing Git message, and verifies a clean two-parent merge with expected saved content. Abort path remains separately tested. Latest isolated suite: 11 units +31 Git contracts pass; PTY selective/editor/binary/directory workflows remain outstanding.

Native merge/rebase/cherry-pick/revert/continue/skip/abort/conflict-version intents are wired through shared explicit preview and confirmation. Conflict viewer shows bounded sanitized actual base/ours/theirs object bytes and retains the exact raw path on every row. Full-crate local_actions_contracts passed 10/10, including conflicting merge, explicit reconciliation, choose ours, stage, continue and independent two-parent verification. Initial keyboard/mouse PTY attempts found a real error-panel defect: only first line Git operation was rendered, hiding conflict/reconciliation details; artifacts target/tui-artifacts/56-1791853976699768600 and 56-1791853977066442836 retained. Parent is fixing shared form error rendering; no completion claimed.

Error defect fixed and reverified: failed Git operations retain bounded redacted stdout/stderr (actual CONFLICT), shared form error viewer has wrapped multi-line content plus F7/F8/mouse controls. Actual checked node scripts/test-task.mjs git_sequencers passed 4 code/provider + 2 real-binary PTY, zero ignored, shared offline target/jobs2/incremental0. Coverage /tmp/ghtui-test-coverage-pLDgmq. Both native journeys inspect CONFLICT, scroll to reconciliation instruction, explicitly refresh, select exact conflict bytes/stage, resolve ours, stage and continue; independent Git verifies two ordered parents and clean files. Batch log target/build/native-management-checked.log. Native rebase/cherry-pick/revert/skip/abort richer journeys/global gates remain open.

2026-10-18 a10: Overview shows paused merge/rebase/cherry-pick/revert banner with conflict count, next step and clickable Continue/Skip/Abort rows; menu guards conflicting actions while paused; conflict view shows working file markers; ours/theirs/base names; stage warns on leftover markers. Gate node scripts/test-task.mjs v1_local_recovery: 5 code + 6 PTY pass (keyboard+mouse); local_management:: 12/12, local:: 12/12.
<!-- SECTION:NOTES:END -->
