---
id: TASK-99
title: 'History: one GitKraken-style view for graph, file history and commit search'
status: In Progress
assignee: []
created_date: '2026-10-18 20:38'
updated_date: '2026-10-20 09:53'
labels:
  - ui-design
  - history
dependencies:
  - TASK-89
  - TASK-90
  - TASK-91
  - TASK-98
references:
  - 'https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3'
priority: high
ordinal: 28069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace Repository's separate Graph (g), file History (H) and commit searching with one History view, styled like GitKraken: colored lanes with commit dots and merge/fork curves across all branches; branch, remote and tag labels as chips on their commits; aligned columns (graph, short SHA, message, author chip, age); a search bar that filters or highlights by message, author, path and SHA while keeping non-matching commits visible for context; file history is this view with a path chip set (f on a file anywhere); a commit detail pane with message, parents, refs and changed files, Enter for the diff and Y for blame. Read-only by default; checkout/branch/reset stay behind m Local operations with preview and confirmation. Supersedes the graph part of TASK-28 and the history part of TASK-32 (blame stays in TASK-32). Code search across files stays in the Search view.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 History replaces Graph and file History; their keys and palette commands open History (g, H keep working as aliases)
- [ ] #2 Search, path filter and branch scope compose and are shown as removable chips
- [ ] #3 Large repositories stream and page without blocking; loaded-history boundaries stay visible
- [ ] #4 PTY journeys at 140x42 and 80x24 with keyboard and mouse on real Git fixtures (branches, merges, tags, remotes); working tree unchanged
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
History replaces Graph and file History (g/H aliases), paging verified on a 10k-commit fixture and a ~200-branch repo. Open: TASK-99.3 remainder and TASK-100 polish. Gate: node scripts/test-all.mjs 5979 passed/0 failed, fmt and clippy clean (582be36f).
<!-- SECTION:NOTES:END -->
