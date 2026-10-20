---
id: TASK-99.3
title: 'History: search, path filter and branch scope'
status: In Progress
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
Done: / highlight with dimmed non-matches, n/N, author:/path:/branch: chips, scope, f in Tree, H alias. Remaining: f from Search results and PR Files; file history following renames. Gate: node scripts/test-all.mjs 5979 passed/0 failed, fmt and clippy clean (582be36f).
<!-- SECTION:NOTES:END -->
