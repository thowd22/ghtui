---
id: TASK-99.1
title: 'History: lane layout and graph rendering'
status: Done
assignee: []
created_date: '2026-10-18 20:38'
updated_date: '2026-10-20 09:53'
labels:
  - ui-design
  - history
dependencies: []
references:
  - 'https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3'
parent_task_id: TASK-99
priority: high
ordinal: 29069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Compute stable lanes for all refs with merge and fork connectors (●, │, ─╮, ─╯, ├), colored per lane from the theme; handle octopus merges, roots, shallow and detached histories; selection highlights the selected commit's ancestry.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Graph matches git log --graph parentage on fixture DAGs (unit tests)
- [x] #2 Lane colors are stable while scrolling and paging
- [x] #3 PTY journeys at 140x42 and 80x24 with keyboard and mouse on real Git fixtures (branches, merges, tags, remotes); working tree unchanged
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Multi-tip walk from one ref snapshot via src/history/lanes.rs; same-row joins, stable lane colors across pages; legacy CommitGraph removed. Focused: test-task history_lanes_v991. Gate: node scripts/test-all.mjs 5979 passed/0 failed, fmt and clippy clean (582be36f).
<!-- SECTION:NOTES:END -->
