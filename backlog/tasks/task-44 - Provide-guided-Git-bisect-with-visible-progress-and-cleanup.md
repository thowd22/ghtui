---
id: TASK-44
title: Provide guided Git bisect with visible progress and cleanup
status: In Progress
assignee:
  - '@foundation'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-20 14:09'
labels:
  - localgit
  - e2e
milestone: m-6
dependencies:
  - TASK-43
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-6 - Local-Git-workspace-and-operation-contract.md
priority: medium
type: feature
ordinal: 44
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Finding when a regression appeared is a useful Git workflow that benefits from plain-language guidance. Automated shell execution should not be a hidden requirement.

## Testing steps

1. Code/provider tests: Test real bisect histories, skips/ambiguity, detached-state behavior and reset restoration.
2. Real-binary TUI E2E scenario `guided_bisect`: Start bisect and classify fixture commits using keyboard and mouse journeys, open culprit diff, reset, and verify original HEAD/worktree is restored. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e guided_bisect -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Select known good/bad commits and mark good/bad/skip with visible candidate and remaining scope.
- [x] #2 Bisect identifies a culprit when possible and exposes abort/reset with original-branch restoration.
- [ ] #3 Dirty work, no feasible candidate and interrupted bisect have clear recovery states.
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

Native bisect start/good/bad/skip/reset forms preserve explicit bounds and show paused/culprit/reset guidance plus bounded actual Git output. Full-crate local_actions_contracts passed 10/10, including monotonic real history classification and starting-branch restoration. Keyboard guided_bisect actual PTY passed; mouse journey initially mislocated an underlying guidance phrase instead of the overlay action and is being tightened to menu-row identity. Culprit diff, skipped/ambiguous/no-candidate and interrupted-session UI journeys remain open; no completion claimed.

Actual checked node scripts/test-task.mjs guided_bisect passed 2 code/provider + 2 real-binary PTY, zero ignored, shared offline target/jobs2/incremental0. Keyboard/mouse coverage /tmp/ghtui-test-coverage-dWzYes; batch log target/build/native-management-checked.log. Both native paths classify actual generated commits, independently verify the exact first-bad SHA in Git bisect log, inspect real culprit diff (-version2/+version3), then reset at 80x24 and prove original main/HEAD/clean worktree restored and BISECT_LOG removed. Explicit skips/ambiguity/interrupted/no-candidate richer cases and final global gates remain open.

2026-10-20 audit at 74360e48 (1/5 ACs met): Happy-path guided bisect (classify, culprit, diff, reset/restore) works with keyboard+mouse PTY; skip/ambiguity/dirty/no-candidate/interrupted states untested and remaining-scope only via raw git output. Remaining: AC1: the candidate and remaining scope appear only as raw Git output lines, with no structured candidate/remaining count. BisectSkip is wired but has no test. AC3: dirty work is refused through clean_required in git.rs:2008, but nothing tests that for bisect. There is no 'no feasible candidate' state, and the interrupted-bisect banner has no test. AC4: no tests for skip, ambiguity, detached start or no-candidate cases. AC5: the happy-path journey passes on both inputs, but the Skip action and th
<!-- SECTION:NOTES:END -->
