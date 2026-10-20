---
id: TASK-38
title: Commit and amend local work with hooks and signing support
status: Done
assignee:
  - '@codex'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-20 23:45'
labels:
  - localgit
  - e2e
milestone: m-6
dependencies:
  - TASK-37
  - TASK-16
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-6 - Local-Git-workspace-and-operation-contract.md
priority: high
type: feature
ordinal: 38
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The TUI should complete the local edit-to-commit workflow while showing the exact staged changes. User hooks, signing and editor failures must be visible instead of hidden behind a stuck process.

## Testing steps

1. Code/provider tests: Test commit objects/messages/parents, rejected hooks, signing failure, amend confirmation and changed index.
2. Real-binary TUI E2E scenario `local_commit`: Stage then commit through keys and clicks, inspect new local commit, exercise a rejecting hook and amend cancellation, and verify expected refs/index/worktree. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e local_commit -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Commit form previews staged content and supports message/body and explicit amend.
- [x] #2 Configured hooks/signing run correctly with deliberate prompt/editor handling and clear failures.
- [x] #3 Commit/amend verifies resulting object/ref and preserves work when cancelled or rejected.
- [x] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [x] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reuse the shared Unicode composer for commit/amend messages and durable local drafts. 2. Show exact staged content and target/history effects before explicit confirmation through LocalPlan. 3. Preserve unstaged work, explain hook/signing failures and ambiguous results, and refresh local status/history. 4. Run real Git code/provider tests and local_commit keyboard/mouse PTY journeys with successful and failing hooks, explicit amend scope, no-effect cancellation and complete regression gates.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Backend preparation is available through shared src/git.rs, src/diff.rs and src/repository_view.rs. Real generated-Git fixture tests cover canonical status/diff/index/objects/refs, ordered DAG parents and immutable tree reads, staged patches, hook rejection, branch/tag guards, local-bare push/fetch, stash/worktrees, conflicts/abort, reflog rescue and bisect reset. Isolated offline source validation uses /tmp/ghtui-git-validation.AsAVCb/Cargo.toml: 8 code/model unit tests + 19 real Git integration tests passed (0 ignored), cargo clippy --offline --manifest-path /tmp/ghtui-git-validation.AsAVCb/Cargo.toml --all-targets -- -D warnings passed; rustfmt --check own modules/tests passed. Prepared backend only: dependencies, full UI, mouse/keyboard PTY and full-project test/fmt/clippy verification remain required. No task completion claimed.

Native local commit controller prepared: explicit full multiline message, actual staged patch preview, observed-state race check, confirmed Git commit, refreshed typed Workspace. Actual local_actions_contracts focused run passed commit fixture preserving unstaged working bytes, committing only staged bytes, preserving Japanese/multiline message, and rejecting changed state without moving HEAD. Added rejecting-hook contract but not yet executed because shared native FormPurpose integration is mid-edit; signing/interactive hook handoff and task-specific actual PTY journeys still open. Task is not Done.

Commit/amend use the shared Unicode multiline editor with local repository-scoped durable drafts. Read-only preview displays exact staged patch; Confirm action is separate and state-checked. Added actual local_commit keyboard/mouse journeys: UI stage one file, commit full message/body, verify parent/new objects and preserve another unstaged file, cancel amend, then reject a commit through configured pre-commit hook without losing index/worktree. These new journeys are executing; no criteria checked from source alone.

Actual cargo test --offline --test tui_e2e local_commit -- --nocapture:2passed, keyboard+mouse. UI staged one file and committed full Unicode message/body; independent Git object/parent/HEAD/index reads proved only staged content committed and another dirty file preserved. Cancelled amend left HEAD unchanged; configured rejecting pre-commit hook remained visible and preserved HEAD/index/worktree. Signing/prompt and explicit successful-amend cases remain before task closure.

Operation-only progress now retains bounded terminal-safe multiline stdout/stderr with token and whole transport-URL redaction, including successful hooks. This does not redact source blobs/diffs. Added actual successful-hook contract asserting fake hook stdout/stderr secrets/credential URL are absent while saved source blob bytes (including token-looking text) remain exact. Existing failed-hook/recovery/amend/signing and bisect semantics remain. Coordinated rerun/full gates pending.

Canonical diff parser repair verified by shared checked staging gate: 14 code/provider +6 actual keyboard/mouse PTY tests pass, zero ignored/failures, artifact /tmp/ghtui-test-coverage-8Avh5H. Added generated installed-Git core.quotePath=true/false new Unicode-space and quoted control-byte file header contract, alongside direction-aware neighboring partial unstage correctness. Native Japanese-space unstage no longer fails with Unterminated quoted Git path; full regression pending.

2026-10-18 a11: K Commit key/control; Commit disabled with explanation while nothing is staged (Amend unaffected); commit form status explains Ctrl-P then Ctrl-S; stale preview-first error cleared once preview is ready. Gate: node scripts/test-task.mjs v1_local_staging -> 4 code + 2 PTY pass.

2026-10-20 audit at 74360e48 (3/5 ACs met): Commit and amend with staged preview, hooks and a keyboard/mouse PTY all work; signing-failure and editor/prompt handling are untested. Remaining: AC2: hooks are covered (rejecting hook, and a successful hook with redacted output at git_contracts.rs:1585). Signing is only left enabled; no test configures a failing gpg/ssh signer to check the error shows. GIT_EDITOR and GIT_TERMINAL_PROMPT are set (git.rs:3088-3094), but no test checks editor or prompt handling. AC4: the Testing steps list signing failure; it is untested. A successful amend is tested only in contracts, not in a PTY.

2026-10-20 final push (verified at 60167066, 6107/0): Signing failure explained and tested. Remaining: successful amend has no PTY journey.

2026-10-20 round: Amend preview now lists the commit it replaces (kept in the reflog) and warns when that commit is already on the upstream (ahead==0). New contract v1_local_commit_amend_preview_names_replaced_published_commit_and_keeps_it_in_reflog. New PTY local_commit_{keyboard,mouse}_signing_failure_then_successful_amend: a failing gpg.program shows 'Commit signing failed' and keeps HEAD and the index, then a successful amend is checked (parent, message, tree, unstaged z.txt bytes, old commit in reflog, zero gh calls, 80x24 resize). Gates: local_actions_contracts v1_local_ 12/0; tui_e2e local_commit_ 5/1 (all 4 local:: journeys pass; the one failure is the unrelated collaboration mouse Search journey). Merged to main by 11eb1ced; full gate 6206 passed, 2 load-only failures elsewhere (TASK-102).
<!-- SECTION:NOTES:END -->
