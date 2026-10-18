---
id: TASK-99.1
title: 'History: lane layout and graph rendering'
status: To Do
assignee: []
created_date: '2026-10-18 20:38'
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
- [ ] #1 Graph matches git log --graph parentage on fixture DAGs (unit tests)
- [ ] #2 Lane colors are stable while scrolling and paging
- [ ] #3 PTY journeys at 140x42 and 80x24 with keyboard and mouse on real Git fixtures (branches, merges, tags, remotes); working tree unchanged
<!-- AC:END -->
