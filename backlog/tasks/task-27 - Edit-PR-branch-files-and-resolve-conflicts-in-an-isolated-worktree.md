---
id: TASK-27
title: Edit PR branch files and resolve conflicts in an isolated worktree
status: Done
assignee: []
created_date: '2026-10-12 23:06'
updated_date: '2026-10-21 16:26'
labels:
  - review
  - e2e
milestone: m-2
dependencies:
  - TASK-26
  - TASK-23
  - TASK-25
  - TASK-34
  - TASK-35
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-3 - PR-action-coverage-and-parity-contract.md
priority: high
type: feature
ordinal: 27
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Some PR-page actions require modifying branch contents. Provide an understandable native edit/conflict flow without mixing those edits into an unrelated repository working tree.

## Testing steps

1. Code/provider tests: Generate divergent Git histories and assert conflict resolution blobs, commit parents, push targets and original checkout/index preservation.
2. Real-binary TUI E2E scenario `pr_branch_edits`: Open a conflicted PR, choose/edit a resolution and preview through keys and clicks, cancel or confirm, then assert journal and Git objects; changed head must block the push. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e pr_branch_edits -- --nocapture` and verify a nonzero selected test count; run the relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. The initial framework is delivered by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Branch-file edit and conflict resolution open an isolated revision with a visible resulting diff.
- [x] #2 Commit/push targets, effects, permission and head checks are explicit; no force push or overwrite is implicit.
- [x] #3 Conflict, editor failure, changed-head, and cancellation preserve useful work and explain recovery.
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
PR Files E Edit file / R Resolve conflicts in an isolated worktree with resulting diff, explicit commit and no-force push guarded by the observed head (2c659194). pr_branch_edits PTY 5/0. AC2 open: push permission pre-check and remote-refusal journey (TASK-108). Gate 19 (node scripts/test-all.mjs at d8f60411): 6419 passed / 0 failed; fmt + clippy clean.

Verified in gate 22 at 3c27e77f: node scripts/test-all.mjs 6468/0, fmt and clippy clean. AC2 push permission covered by TASK-108.
<!-- SECTION:NOTES:END -->
