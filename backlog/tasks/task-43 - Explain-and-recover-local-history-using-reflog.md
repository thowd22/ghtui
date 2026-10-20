---
id: TASK-43
title: Explain and recover local history using reflog
status: In Progress
assignee:
  - '@foundation'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-20 18:20'
labels:
  - localgit
  - e2e
milestone: m-6
dependencies:
  - TASK-42
  - TASK-28
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-6 - Local-Git-workspace-and-operation-contract.md
priority: high
type: feature
ordinal: 43
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Users need help finding work after a mistaken branch move or amend. Recovery should show the intended result and favor creating a rescue reference rather than silently discarding newer work.

## Testing steps

1. Code/provider tests: Verify reflog fixtures, rescue refs, reset modes, dirty guards and missing-object cases.
2. Real-binary TUI E2E scenario `reflog_recovery`: Find an amended-away commit via keys and clicks, preview and create a rescue branch, cancel destructive reset, and verify recovered ref plus unchanged dirty work. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e reflog_recovery -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Reflog explains local ref changes and links entries to commits/diffs and recovery choices.
- [ ] #2 Recovery can create a rescue branch; reset/discard choices expose exact scope and affected work before confirmation.
- [ ] #3 Expired/unreachable objects and reflog limitations are stated without claiming universal undo.
- [x] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [x] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
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

Added explicit Reset modes soft/mixed/hard and exact tracked DiscardFiles in addition to rescue branches. Every history/discard preview is flagged destructive and requires confirmation plus stale content checks. Actual fixture verifies cancelled soft reset preserves HEAD, soft reset preserves index/working bytes, mixed resets index only, confirmed exact discard changes only selected tracked contents, and explicit hard reset preserves unrelated untracked work. Reflog rescue test still preserves dirty files. Latest source suite: 11 units +31 Git contracts pass; task UI/PTY coverage remains incomplete.

Native recovery creates an exact rescue branch from a supplied immutable SHA and exposes soft/mixed/hard reset with explicit preview and cancellation. Full-crate local_actions_contracts passed 10/10 and both reflog_recovery real-binary PTY paths passed, independently verifying old rescue ref and that cancelling destructive hard reset preserves original HEAD and dirty file bytes at 120x40 and 80x24. LocalInventory includes bounded real reflog entries/expiry guidance, but UI reflog discovery/selection and expired-entry journeys remain open; this subset is not task completion.

Actual checked node scripts/test-task.mjs reflog_recovery passed 3 code/provider + 2 real-binary PTY, zero ignored, shared offline target/jobs2/incremental0. Keyboard/mouse coverage /tmp/ghtui-test-coverage-kScOu0; batch log target/build/native-management-checked.log. Native rescue ref from immutable old SHA and cancelled destructive reset are independently checked against actual refs/HEAD/dirty bytes at both sizes. Reflog inventory is typed/background-ready, but a native discoverable reflog picker and expired-entry UI journey are still required; not full task completion.

Typed local inventory picker gate passed: CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 node scripts/test-task.mjs local_picker selected 7 code tests (2 Form +5 actual Git/picker contracts) and2 actual-binary keyboard/mouse PTY tests, all pass/zero ignored. Artifact /tmp/ghtui-test-coverage-GMYYXM; log target/build/local-picker-checked.log. Native journeys verify cancel/no-match, exact stale branch rejection with unchanged HEAD/index, refreshed branch choice, immutable stash SHA disclosure/apply/retention,80x24 resized picker, and exact raw-byte worktree removal preserving a similar Unicode worktree and root dirty files. Reflog rescue immutable SHA picker currently has code coverage, not an additional native rescue-picker journey. APFS-valid newline path fixture replaces invalid filesystem UTF-8 only on macOS; Linux retains byte0xff, neither platform skips raw-path testing. Full product/global gates and task-specific remaining cases stay open; not Done.

2026-10-18 a10: Overview lists commits left behind by amend/reset/rebase from the raw HEAD reflog (no Git child, expiry stated); Enter opens the commit and Create rescue branch is prefilled. Gate node scripts/test-task.mjs v1_local_recovery: 5 code + 6 PTY pass.

2026-10-20 audit at 74360e48 (3/5 ACs met): Reflog left-behind discovery, open-commit and prefilled rescue branch are reachable and PTY-tested both inputs; remaining: reset preview enumerating affected work, missing/expired-object case tests. Remaining: AC2: a rescue branch works. Reset previews show only a generic 'can replace or remove existing history/files' warning (local_actions.rs:917); they do not list the exact commits or dirty files a hard or mixed reset would discard. AC3: the limitation text is present ('local reflog only, entries expire; not a universal undo', workspace.rs:717), but no test covers an expired, pruned or missing-object reflog entry, and only the HEAD reflog is used (branch reflogs are not).

2026-10-20 final push (verified at 60167066, 6107/0): AC2 closed: reset previews list commits and dirty files a hard/mixed reset discards. Remaining AC3: expired/pruned/missing reflog entries; recovery reads only the HEAD reflog.
<!-- SECTION:NOTES:END -->
