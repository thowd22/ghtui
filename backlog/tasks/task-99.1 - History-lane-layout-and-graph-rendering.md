---
id: TASK-99.1
title: 'History: lane layout and graph rendering'
status: In Progress
assignee: []
created_date: '2026-10-18 20:38'
updated_date: '2026-10-20 02:06'
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
Graph pages walk every branch, remote, tag, HEAD (incl. detached) and the opened revision via log --topo-order --stdin from the ref snapshot; pages share a walk fingerprint (replaces walk_tip/first-row checks); opening pages until the revision is loaded. Timeline rows use src/history/lanes.rs (layout from row 0, so paging keeps columns/colors), same-row joins replace the ↳ parent rows, graph column padded so messages align, 24 visible lanes then …, >128 lanes is an error. Legacy CommitGraph deleted. Gate: node scripts/test-task.mjs history_lanes_v991 — 14 lib + 2 PTY passed. Also passed: lib commit_graph/v1_repo_views/git_actor_roles (87), git_contracts (8), graph_compact_selection_contracts (5), tui_e2e commit_graph_v2/v3 native (14) and graph_paging_v1 (20 incl.). Full suite not yet run.
<!-- SECTION:NOTES:END -->
