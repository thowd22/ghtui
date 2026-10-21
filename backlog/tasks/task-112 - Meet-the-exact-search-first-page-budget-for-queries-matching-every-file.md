---
id: TASK-112
title: Meet the exact-search first-page budget for queries matching every file
status: Done
assignee: []
created_date: '2026-10-21 13:29'
updated_date: '2026-10-21 16:26'
labels:
  - performance
dependencies: []
priority: low
ordinal: 45069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-66 release: exact search first page took 123-125 ms against 100 ms for a query matching all 5,000 files; selective queries are well within budget. Likely first-page assembly does more than one page of work.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Exact search first page for an all-matching query meets 100 ms in the release perf journey
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified in gate 22 at 3c27e77f: node scripts/test-all.mjs 6468/0, fmt and clippy clean. No background total-count job; page stops at 1000-match budget with PARTIAL coverage.
<!-- SECTION:NOTES:END -->
