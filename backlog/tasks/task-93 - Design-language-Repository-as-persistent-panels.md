---
id: TASK-93
title: 'Design language: Repository as persistent panels'
status: Done
assignee: []
created_date: '2026-10-18 19:59'
updated_date: '2026-10-20 14:00'
labels:
  - ui-design
dependencies:
  - TASK-89
  - TASK-90
  - TASK-91
references:
  - 'https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3'
priority: medium
ordinal: 22069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Repository becomes numbered panels (1 Status, 2 Files, 3 Branches, 4 Commits) beside a diff/detail pane, focused with 1-4 and Tab, like lazygit. Overview text moves into Status. Gold standard: lazygit.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Stage, unstage and commit keep their preview and confirmation
- [ ] #2 Untracked noise (such as tool worktrees) collapses to a count with a toggle
- [ ] #3 PTY test at 140x42 and 80x24 with keyboard and mouse, plus unit tests for the layout helpers
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Overview is Files plus Recent commits panels with a preview from 130 columns. Remaining: native diff, saved-folder detail and pickers are still full-screen takeovers. Gate: node scripts/test-all.mjs 5933 passed/0 failed, fmt and clippy clean (732b0709).

Phase 5: verified at 74360e48. node scripts/test-all.mjs 6000 passed 0 failed; fmt and clippy -D warnings clean.
<!-- SECTION:NOTES:END -->
