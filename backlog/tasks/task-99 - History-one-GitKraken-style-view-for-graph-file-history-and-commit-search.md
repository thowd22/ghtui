---
id: TASK-99
title: 'History: one GitKraken-style view for graph, file history and commit search'
status: In Progress
assignee: []
created_date: '2026-10-18 20:38'
updated_date: '2026-10-20 04:01'
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
Umbrella: mode named History (tab, palette Show History with graph/log tags, Help History section, pane title History › <repo> › <scope>, crumb Repository • History); ProvenanceView::History/Graph, FileAction::History/HistoryEntry and the old file-history page removed; blame G opens the commit in History; global Back from History's diff/drill-down returns to the table. Footer: / n N f Enter Y C y. Status 'Read-only · N commits loaded · scroll for more', boundary row kept. Helpers moved to the History layout. Gate: focused lib 402/402, contracts 59/59, PTY history_* 7/10 on run 3; history_view_v99 (2) and the detail mouse journey fixed after the last run, not re-run. Older file_history/original_saved_* PTY journeys still expect the retired views.
<!-- SECTION:NOTES:END -->
