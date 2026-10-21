---
id: TASK-118
title: Show worktrees as an expandable stack and open any of them in Repository
status: Done
assignee: []
created_date: '2026-10-21 17:54'
updated_date: '2026-10-21 21:26'
labels:
  - usability
dependencies: []
priority: high
ordinal: 51069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
User report: errors when worktrees are used; wants first-class worktree support. Repository shows a Worktrees stack: main checkout plus linked worktrees grouped and expandable, each with branch, clean/dirty counts, ahead/behind, short path (~ and common prefix trimmed) and lock/prunable state. Enter or click switches the Repository view to that worktree in place (no restart), with a visible 'Worktree: <branch>' context and a way back. When the current checkout's core.worktree points elsewhere (worklappy main checkout points to an Augment directory), do not dead-end: explain in plain words and offer to open the configured worktree, open the main checkout's linked worktrees, or (behind m Local operations with preview and confirmation) unset the stale core.worktree.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Repository lists all worktrees as an expandable stack with status, branch and short path, by key and click
- [x] #2 Selecting a worktree switches the Repository view to it in place; returning works
- [x] #3 A core.worktree mismatch shows choices instead of only an error; unsetting it requires preview and confirmation
- [x] #4 Keyboard+mouse PTY journeys on temp repos with 3+ linked worktrees and a mismatched core.worktree; read-only paths change no Git state
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified in gate 24 (node scripts/test-all.mjs 6551/1; the 1 was a test-helper SIGCONT race, fixed and its 4 modules rerun 40/0), fmt and clippy clean.
<!-- SECTION:NOTES:END -->
