---
id: TASK-110
title: Cache the comparison so reopening a large diff meets its budget
status: To Do
assignee: []
created_date: '2026-10-21 13:29'
labels:
  - performance
dependencies: []
priority: medium
ordinal: 43069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-66 release measurements: reopening a 5 MB commit diff takes 516-554 ms against 150 ms, the same as its first open, because the comparison is recomputed. Cache the prepared comparison by its exact inputs (commits, paths, options) with a bounded size.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reopening the same diff reuses the prepared comparison and meets the 150 ms release budget
- [ ] #2 Changed inputs or memory pressure recompute; tests cover both
<!-- AC:END -->
