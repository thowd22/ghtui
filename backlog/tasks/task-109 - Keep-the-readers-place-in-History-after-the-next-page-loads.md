---
id: TASK-109
title: Keep the reader's place in History after the next page loads
status: Done
assignee: []
created_date: '2026-10-21 13:29'
updated_date: '2026-10-21 16:26'
labels:
  - bug
dependencies: []
priority: high
ordinal: 42069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Scrolling past the 200th commit loads 200 more ('400 commits loaded · showing 1–200') but the table returns to the newest commit with the bar on row 0. Found by TASK-66 (history_after_next_page in the perf report).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 After a continuation page loads, the selection and scroll stay on the row the user scrolled to, by key and by wheel
- [x] #2 Keyboard+mouse PTY on a 20k-commit fixture checks the selected SHA before and after the page loads
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified in gate 22 at 3c27e77f: node scripts/test-all.mjs 6468/0, fmt and clippy clean. performance_bounded_v66 History next-page journeys pass in the gate.
<!-- SECTION:NOTES:END -->
