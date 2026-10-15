---
id: TASK-76
title: Keep read-only Git diffs from refreshing index stat-cache bytes
status: To Do
assignee: []
created_date: '2026-10-15 01:54'
labels:
  - git
  - e2e
dependencies:
  - TASK-5
  - TASK-11
references:
  - >-
    https://git-scm.com/docs/git-config#Documentation/git-config.txt-diffautoRefreshIndex
documentation:
  - backlog/docs/doc-6 - Local-Git-workspace-and-operation-contract.md
priority: high
type: bug
ordinal: 5069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A genuine installed-Git control from the native selected-staging lane shows that ordinary worktree git diff may rewrite raw index stat-cache bytes even with GIT_OPTIONAL_LOCKS=0; the staged blob/mode/path remain semantically identical. Explicit diff.autoRefreshIndex=false preserves raw bytes in the same control. Read-only browsing promises to preserve the checkout/index, so implementation and independent test observers must not silently refresh it. Preserve the proof target/build/parallel-tree-native-diff-c2-independent-index-inspection-proof.json and do not misdiagnose an observer write as a repeated ghtui mutation.

## Testing steps

1. Code/provider tests: reproduce actual cached apply followed by a stat-cache-eligible unchanged worktree on real SHA1/SHA256 repositories. Independently compare raw index bytes and ls-files stage/blob/mode/path, HEAD/refs/config/worktree/untracked state before and after provider status/fingerprint/Working/Staged reads. Show the ordinary Git control really changes raw metadata without semantic change, and the application's read policy does not. Cover user config true, linked raw-byte worktrees, missing/stale/read-only failures and original deadline/cancellation/runner/capture accounting without new write authority.
2. Real-binary TUI E2E scenario readonly_index: fresh keyboard and mouse sessions browse Working/Staged/native views, search/select/resize/Source/Back on the exact stat-cache-eligible fixture. Assert completed visible content plus exact read argv and complete raw/semantic Git, SQLite/config/TTY preservation using independently read-only observers. Explicit confirmed staging remains the only authorized index write; canceled/stale inspection writes nothing.
3. Run cargo test --test tui_e2e readonly_index -- --nocapture and node scripts/test-task.mjs readonly_index with nonzero meaningful code and actual mouse/keyboard PTY counts. Then cargo test --all-targets, cargo fmt --check, cargo clippy --all-targets -- -D warnings, dependency/backlog audits and exact command/request-count regressions. Preserve failures and actual artifacts; no ignored tests or weakened raw-index equality.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Read-only installed-Git observations preserve exact raw and semantic index state even when Git would otherwise refresh unchanged worktree stat metadata; no global user config is modified.
- [ ] #2 The same original source, installed Git/retained runner, deadline/cancellation/capture limits and explicit mutation confirmations remain enforced; independent observers preserve raw state and strict argv checks are deliberately updated.
- [ ] #3 Meaningful genuine-Git provider contracts and task-specific keyboard/mouse real-binary PTY journeys pass with completed visible and independent command/storage/Git effects, wide/narrow/resize/failure coverage and positive nonzero counts.
- [ ] #4 Full all-target, fmt, strict-Clippy and audits pass with exact commands/counts/artifacts recorded; native macOS and release limits remain explicit.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run task-specific meaningful provider/code and keyboard/mouse real-binary PTY tests with nonzero actual selected counts and save exact results/artifacts.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check and cargo clippy --all-targets -- -D warnings before Done; preserve original raw-index and effect assertions.
<!-- DOD:END -->
