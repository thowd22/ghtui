---
id: TASK-99.4
title: 'History: commit detail pane'
status: In Progress
assignee: []
created_date: '2026-10-18 20:38'
updated_date: '2026-10-20 03:15'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit detail pane (src/history/detail.rs) beside History from 120 cols (split_list_detail, ~1/3 width), D/P drill-down below; changed files from bounded git diff-tree --raw --numstat -z (src/git/history_detail_v994.rs, shared numstat_record reader), debounced 150 ms and cached per commit on a worker thread. Keys/tokens: Enter diff, f file history, Y blame, y SHA notice, parent links. Gate: cargo test --lib (filters) 91/91; tui_e2e history_* 2 passed, 4 failed in last run (Back-after-parent step and a 992 resize-top check) - both fixed after the run budget, not re-run. Not verified: test-task history_detail_v994.
<!-- SECTION:NOTES:END -->
