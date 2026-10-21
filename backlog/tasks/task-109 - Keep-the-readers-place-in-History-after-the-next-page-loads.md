---
id: TASK-109
title: Keep the reader's place in History after the next page loads
status: To Do
assignee: []
created_date: '2026-10-21 13:29'
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
- [ ] #1 After a continuation page loads, the selection and scroll stay on the row the user scrolled to, by key and by wheel
- [ ] #2 Keyboard+mouse PTY on a 20k-commit fixture checks the selected SHA before and after the page loads
<!-- AC:END -->
