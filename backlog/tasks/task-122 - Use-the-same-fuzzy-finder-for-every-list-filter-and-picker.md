---
id: TASK-122
title: Use the same fuzzy finder for every list filter and picker
status: In Progress
assignee: []
created_date: '2026-10-21 17:56'
updated_date: '2026-10-21 21:26'
labels:
  - usability
dependencies:
  - TASK-116
priority: high
ordinal: 55069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
User request: every search should behave the same. Apply the TASK-116 finder (type-to-filter, fuzzy, highlighted matches, count, Up/Down/click, Enter, Esc) to: command palette, History filter, Tree/file pickers and repository_tree_palette, Actions runs filter, Inbox PR list filter, review-thread filter, saved-path and changed-files pickers, branch pickers in Local operations. Where a filter has structured syntax (e.g. author:), keep it working but make plain typing fuzzy by default.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each listed filter/picker uses the shared finder with live fuzzy results and highlights
- [ ] #2 Behaviour is identical by key and click in every one
- [ ] #3 PTY journeys cover at least palette, History, Actions runs, Inbox and a branch picker; existing journeys updated without loosening checks
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Palette, History filter, local pickers, saved-path and changed-files pickers on src/fuzzy_finder.rs; gate 24. Inbox/Actions runs/review threads moved to TASK-123.
<!-- SECTION:NOTES:END -->
