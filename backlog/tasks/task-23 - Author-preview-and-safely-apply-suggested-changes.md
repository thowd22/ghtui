---
id: TASK-23
title: Author preview and safely apply suggested changes
status: To Do
assignee: []
created_date: '2026-10-12 23:06'
updated_date: '2026-10-15 00:37'
labels:
  - review
  - e2e
milestone: m-2
dependencies:
  - TASK-21
  - TASK-11
  - TASK-34
  - TASK-35
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-3 - PR-action-coverage-and-parity-contract.md
priority: high
type: feature
ordinal: 23
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Code suggestions are part of the review workflow. Applying them must show the exact resulting edit and commit without silently modifying an unrelated checkout.

## Testing steps

1. Code/provider tests: Use generated Git fixtures to verify suggestion ranges, resulting blobs/commits, batch conflicts, changed-head guards, and cleanup.
2. Real-binary TUI E2E scenario `suggestions`: Author and apply a suggestion via keyboard and mouse fixture journeys, assert preview/confirmation plus exact Git objects or remote commands; inject conflict and verify no push. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e suggestions -- --nocapture` and verify a nonzero selected test count; run the relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. The initial framework is delivered by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Single/batched suggestions carry correct line/range and code text with a resulting-change preview.
- [ ] #2 Application uses verified remote capability or an explicit isolated-worktree workflow with commit/push confirmation.
- [ ] #3 Conflicts, outdated heads, permission failures, and attribution limitations are explained with no partial hidden write.
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Read-only current native prerequisite audit after Timeline freeze (2026-10-15): Task23 remains ToDo/isReadyfalse and21/11/34/35 dependencies OPEN. Actual src/pr_actions.rs PrCommand ALL8 exposes only conversation new/edit/delete/react/subscription/viewed; src/forms.rs has no Inline/Suggestion/Pending/Thread purposes. Existing verified immutable M/H RIGHT ranges are explicitly read-only, not current publication/apply authority. src/providers/gh/pr.rs inline_comment returns raw Value; exact readback currently checks path/commit_id/line/body but not the positive node/current numeric author/side/start-side/start-line/parent receipt used by stronger typed create_review. Pending ownership still login/state based, native restart-safe pending-review lifecycle/thread workflows absent. Inventory pr-inline/pr-pending-reviews/pr-threads/pr-suggestions remains implementedfalse. Do not expose apply or claim a public apply endpoint/capability without verification; do not promote readonly range labels into writes. Potential safe progression: first strengthen typed inline receipt +durable single-submit journal/native exactRIGHT range publication; suggestion authoring must carry source/path/H/range/code and actual New(H) resulting-blob preview; apply requires explicit verified isolated-worktree commit/push+conflict/head/attribution confirmation. Pure unpublishable suggestion editor would be only partial and does not satisfy task. Root selected dependency-ready genuine mutable diff source/fingerprint factory under Task31 instead, while these corePR prerequisites are resolved; no implementation/status/dependency/AC changes for Task23 and no live writes.
<!-- SECTION:NOTES:END -->
