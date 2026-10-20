---
id: TASK-100
title: 'History: polish follow-ups from real-repo QA'
status: Done
assignee: []
created_date: '2026-10-20 09:53'
updated_date: '2026-10-20 14:00'
labels:
  - ui-design
  - history
dependencies:
  - TASK-99
priority: medium
ordinal: 33069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
From the Phase 4 captures on the ~200-branch insights clone and the design critic.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 When HEAD is not on the first page, the window shows that newer commits exist above (a top boundary row or hint)
- [ ] #2 The GRAPH column at 80 columns takes less width so messages stay readable with many lanes
- [ ] #3 History shows one status line, not one in the table and one below it
- [ ] #4 PTY journeys at 80x24 and 140x42, keyboard and mouse, working tree unchanged
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Phase 5: verified at 74360e48. node scripts/test-all.mjs 6000 passed 0 failed; fmt and clippy -D warnings clean.
<!-- SECTION:NOTES:END -->
