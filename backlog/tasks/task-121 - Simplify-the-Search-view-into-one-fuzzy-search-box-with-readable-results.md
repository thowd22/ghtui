---
id: TASK-121
title: Simplify the Search view into one fuzzy search box with readable results
status: Done
assignee: []
created_date: '2026-10-21 17:56'
updated_date: '2026-10-21 21:26'
labels:
  - usability
dependencies:
  - TASK-116
priority: high
ordinal: 54069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
User request (2026-10-21): all searches must be simple fuzzy searches with easily readable results. Search view today exposes modes (exact/regex/ranked), sources and saved versions up front. Default to one input: type and results update live, fuzzy-ranked across file names, code lines and PR/issue conversations in the current scope, using the shared finder from TASK-116. Results read like a list: file path (shortened) or PR title, line number, one highlighted snippet line, kind icon/label; grouped by file. Enter opens the result. Exact/regex/scope stay as optional toggles behind one Options control, not required to search. Keep honest PARTIAL/coverage notes but in one short line.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Opening Search and typing shows live fuzzy results with highlighted matches without choosing a mode or source
- [x] #2 Results are grouped and readable at 80 and 140 columns: short path or title, line, one highlighted snippet
- [x] #3 Exact/regex/scope remain available as optional toggles; existing search contracts still hold
- [x] #4 Keyboard+mouse PTY journeys type a query, pick a result and open it
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified in gate 24 (node scripts/test-all.mjs 6551/1; the 1 was a test-helper SIGCONT race, fixed and its 4 modules rerun 40/0), fmt and clippy clean. With loaded conversations in scope, Enter runs the search (account verification).
<!-- SECTION:NOTES:END -->
