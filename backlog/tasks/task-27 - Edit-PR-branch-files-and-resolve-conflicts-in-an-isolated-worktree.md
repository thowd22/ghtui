---
id: TASK-27
title: Edit PR branch files and resolve conflicts in an isolated worktree
status: To Do
assignee: []
created_date: '2026-10-12 23:06'
updated_date: '2026-10-12 23:13'
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
- [ ] #1 Branch-file edit and conflict resolution open an isolated revision with a visible resulting diff.
- [ ] #2 Commit/push targets, effects, permission and head checks are explicit; no force push or overwrite is implicit.
- [ ] #3 Conflict, editor failure, changed-head, and cancellation preserve useful work and explain recovery.
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
