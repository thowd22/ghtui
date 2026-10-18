---
id: TASK-85
title: Open Actions logs at the selected job
status: Done
assignee: []
created_date: '2026-10-18 09:35'
updated_date: '2026-10-18 15:09'
labels: []
dependencies: []
ordinal: 14069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Live check 2026-10-18: pressing l on a run with the failed job selected opens logs at the first job; > Next failure works.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Logs open scrolled to the selected job's first line
- [x] #2 Keyboard and mouse PTY coverage
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Logs(None) on Run/Jobs falls back to the highlighted job row (scoped routes pass an exact job, unchanged). Gate: tui_e2e runs_focus_v1 4/4 incl. new keyboard+mouse journeys (strict fake gh requires --job 501); lib actions 814/814; tui_e2e actions/pr_checks 544/545 (1 unrelated flake passed 2/2 on rerun).
<!-- SECTION:NOTES:END -->
