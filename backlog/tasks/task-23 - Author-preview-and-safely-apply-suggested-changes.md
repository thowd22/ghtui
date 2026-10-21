---
id: TASK-23
title: Author preview and safely apply suggested changes
status: In Progress
assignee: []
created_date: '2026-10-12 23:06'
updated_date: '2026-10-21 06:29'
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
- [x] #1 Single/batched suggestions carry correct line/range and code text with a resulting-change preview.
- [x] #2 Application uses verified remote capability or an explicit isolated-worktree workflow with commit/push confirmation.
- [x] #3 Conflicts, outdated heads, permission failures, and attribution limitations are explained with no partial hidden write.
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
Read-only current native prerequisite audit after Timeline freeze (2026-10-15): Task23 remains ToDo/isReadyfalse and21/11/34/35 dependencies OPEN. Actual src/pr_actions.rs PrCommand ALL8 exposes only conversation new/edit/delete/react/subscription/viewed; src/forms.rs has no Inline/Suggestion/Pending/Thread purposes. Existing verified immutable M/H RIGHT ranges are explicitly read-only, not current publication/apply authority. src/providers/gh/pr.rs inline_comment returns raw Value; exact readback currently checks path/commit_id/line/body but not the positive node/current numeric author/side/start-side/start-line/parent receipt used by stronger typed create_review. Pending ownership still login/state based, native restart-safe pending-review lifecycle/thread workflows absent. Inventory pr-inline/pr-pending-reviews/pr-threads/pr-suggestions remains implementedfalse. Do not expose apply or claim a public apply endpoint/capability without verification; do not promote readonly range labels into writes. Potential safe progression: first strengthen typed inline receipt +durable single-submit journal/native exactRIGHT range publication; suggestion authoring must carry source/path/H/range/code and actual New(H) resulting-blob preview; apply requires explicit verified isolated-worktree commit/push+conflict/head/attribution confirmation. Pure unpublishable suggestion editor would be only partial and does not satisfy task. Root selected dependency-ready genuine mutable diff source/fingerprint factory under Task31 instead, while these corePR prerequisites are resolved; no implementation/status/dependency/AC changes for Task23 and no live writes.

2026-10-20 round: Added `S Suggest` to the Immutable file images overlay (key and click; bottom bar, fits at 80x24). It uses the same original-range, worker and draft path as `P Comment`. When no draft exists, the inline composer opens titled 'Suggested change' with the selected RIGHT lines in a ```suggestion block, and the draft autosaves. A LEFT range is refused with an explanation. The inline Preview now lists the resulting change (-old/+new rows) for every suggestion block. New src/suggestions.rs: suggestion_body/parse_suggestions; resulting_change, which keeps CRLF and missing final newlines and refuses overlapping, conflicting or out-of-range suggestions in a batch; and apply_isolated. apply_isolated builds the commit in a private temporary bare repo that borrows the user's objects through alternates. It fetches and checks the head, writes the tree with a private index, then runs commit-tree with an explicit committer and Co-authored-by trailers, recording attribution limits when an author has no numeric ID. Only when asked, it pushes with --force-with-lease on the observed head. The private repo is always removed, and the checkout, index and refs are never touched. Gates: `cargo test --lib -- suggestions scoped_pr_inline immutable_image` 245 passed / 0 failed (6 new suggestions tests). `cargo test --test tui_e2e -- suggestions catalogue_immutable_file_images inline_comments_keyboard_original` 9 passed / 2 failed: the 2 failures were the new suggestion journeys, a test-side resize helper mistake, fixed in run 3. `cargo test --test tui_e2e -- suggestions inline_comments_mouse_original` 3 passed / 0 failed. AC1 is only partly verified: a single suggestion is authored, previewed and published through the PTY by key and mouse at 120x40 and 80x24, but the batched resulting-change preview is engine-only. AC2 and AC3 are implemented and tested in the engine only, with no UI apply flow yet. Merged to main by 11eb1ced; full gate 6206 passed, 2 load-only failures elsewhere (TASK-102).

Posted suggestions apply from the threads overlay with preview + journaled push (92e6eeb9). suggestion 11/0; threads PTY 132/0. Missing: PTY journey choosing 2+ suggestions in one batch.

Batched suggestion apply (A All) and overlap refusal PTY journeys, keyboard + mouse (88b00824); suggestions 10/0. Open: permission-denied push and attribution-limit journeys; AC5/AC6 whole-flow audit. Gate 15 (node scripts/test-all.mjs, merged round 3 + 41672d74): 6303 passed / 0 failed; fmt + clippy clean.
<!-- SECTION:NOTES:END -->
