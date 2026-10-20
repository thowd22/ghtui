---
id: TASK-99.3
title: 'History: search, path filter and branch scope'
status: In Progress
assignee: []
created_date: '2026-10-18 20:38'
updated_date: '2026-10-20 03:20'
labels:
  - ui-design
  - history
dependencies: []
references:
  - 'https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3'
parent_task_id: TASK-99
priority: high
ordinal: 31069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
/ opens the search bar: plain text matches message, author, SHA prefix; path: and author: chips filter; n/N jump between matches; matches highlighted, non-matches kept for context with an option to hide them. f on a file (Repository, Search, PR Files) opens History with that path chip; renames followed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Results equal git log with the same pathspec/author/grep on fixtures
- [x] #2 Clearing chips restores the full graph with the same selection
- [x] #3 PTY journeys at 140x42 and 80x24 with keyboard and mouse on real Git fixtures (branches, merges, tags, remotes); working tree unchanged
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Filter row (/ query, path:/author:/branch: chips with ✕, scope chip), border match count, dimmed non-matches with bold SHA and underlined query, hide mode (h) with lanes joined through hidden commits, n/N match steps, B branch scope (one-branch walk via OriginalGraphReadV1::Only; clearing returns to the kept all-branches walk and selection), L/auto-load replaces N More, f/H on a saved file opens History with a path chip. Chips: git log --full-history -i -F --author + pathspec over the walk tips; text local (message/author/SHA prefix). Gate: node scripts/test-task.mjs history_filter_v993 — lib 16/16, PTY 2/2; lib history_/repository_graph/commit_graph_v1 242/242; PTY history_refs_v992 + history_lanes_v991 4/4. Open: f in Search (f is Save query there) and PR Files, rename following (--follow), filter kept across Back/Re-read; file_history/original_saved_navigation PTY journeys still press H for the old list.
<!-- SECTION:NOTES:END -->
