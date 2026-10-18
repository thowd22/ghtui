---
id: TASK-99.4
title: 'History: commit detail pane'
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
priority: medium
ordinal: 32069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Right pane (or drill-down when narrow) shows the selected commit: subject, refs, author/committer chips, dates (relative + UTC), parents (clickable), full message and changed files with +/- counts; Enter opens the diff, Y opens blame at that revision, y copies the SHA.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Parent links and Back navigation return to the same graph position
- [ ] #2 PTY journeys at 140x42 and 80x24 with keyboard and mouse on real Git fixtures (branches, merges, tags, remotes); working tree unchanged
<!-- AC:END -->
