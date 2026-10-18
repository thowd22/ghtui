---
id: TASK-40
title: Fetch pull and push with explicit remotes and conflict handling
status: In Progress
assignee:
  - '@foundation'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-18 00:34'
labels:
  - localgit
  - e2e
milestone: m-6
dependencies:
  - TASK-39
  - TASK-5
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-6 - Local-Git-workspace-and-operation-contract.md
priority: high
type: feature
ordinal: 40
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Useful Git management includes synchronization, but users need to know which remote/ref will change and what divergence means. Networking and credential prompts cannot block the UI.

## Testing steps

1. Code/provider tests: Use local bare remotes to verify fetch/pull/push refs, divergent histories, rejected pushes and no implicit force.
2. Real-binary TUI E2E scenario `remote_git`: Sync a divergent fixture through keys and clicks, resolve or abort the resulting operation, and verify local/bare-remote refs and visible ahead/behind. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e remote_git -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Remote/url/ref/upstream and ahead/behind are visible before fetch/pull/push.
- [ ] #2 Pull strategy and conflict/sequencer outcomes are explained; any force operation requires explicit guarded choice.
- [ ] #3 Failures/prompts/cancellation preserve work and report partial remote/local effects accurately.
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

Added explicit PullWithStrategy with fast-forward-only, merge and rebase, plus ForcePushLease requiring exact inspected remote SHA. Force preview checks push URL branch identity; execute rechecks and Git receives --force-with-lease=ref:expected. Regular push remains non-forced. Actual local-bare fixture tests pass for lease rejection/confirmation and divergent pull under merge and rebase, preserving both local and remote commits/files. Latest isolated source validation: 11 units +31 Git contracts passed, 0 ignored; clippy -D warnings passes. No full UI/PTY completion claimed.

Native LocalCommand Fetch/Pull/Push/ForcePushLease forms use explicit configured remotes and branches, typed ff-only/merge/rebase strategy, current state gates and an exact expected remote SHA for force-with-lease. Actual full-crate local_actions_contracts passed 10/10; two real-binary PTY remote_git journeys passed for keyboard and mouse at wide/narrow sizes, independently verifying local bare remote main SHA, upstream/tracking refs, clean working tree and zero fake-gh commands. Full pull/divergence/auth/ambiguous-push UI coverage and full-project final gates remain open; not Done.

Actual checked node scripts/test-task.mjs remote_git passed 5 code/provider + 2 real-binary PTY tests, zero ignored, with shared offline target/jobs2/incremental0. Keyboard/mouse coverage artifact /tmp/ghtui-test-coverage-jRaPCM. Code includes exact lease, changed-target rejection and divergent pull merge/rebase against real local bare remotes; native PTY verifies explicit push/upstream/fetch and independent remote/tracking SHA effects at both sizes. Batch log target/build/native-management-checked.log. Broader native divergent/auth/ambiguous-push recovery remains open; no full task completion claimed.

2026-10-18 a09: sync previews show upstream/ahead/behind and predict diverged/remote-ahead/first-push outcomes; plain-language failure summaries; push defaults tracking for unpublished branches. Gate: node scripts/test-task.mjs v1_local_branches passed 2 code + 2 PTY (keyboard+mouse), 0 ignored; local_management PTY group 14/14 and local_actions_contracts 13/13.
<!-- SECTION:NOTES:END -->
