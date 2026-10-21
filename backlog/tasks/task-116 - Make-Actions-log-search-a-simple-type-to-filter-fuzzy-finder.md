---
id: TASK-116
title: Make Actions log search a simple type-to-filter fuzzy finder
status: Done
assignee: []
created_date: '2026-10-21 17:54'
updated_date: '2026-10-21 21:26'
labels:
  - usability
dependencies: []
priority: high
ordinal: 49069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
User report: in Actions logs it is unclear how to search for the error line or any string. Pressing / (or clicking a visible Search button) opens one input; results update as you type with fuzzy matching (nucleo), matched characters highlighted, a match count, Up/Down or click to pick, Enter jumps to the line in the full log with context, Esc returns. n/N step through matches afterwards. Add a 'Jump to first error' control that finds ##[error]/error/failed lines. Remove mode choices from the default path (keep exact/regex as optional toggles).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Typing in the log search filters results live with highlighted fuzzy matches and a count
- [x] #2 Enter or click jumps to the matched line in context; n/N and Jump to first error work by key and click
- [x] #3 Lib tests for ranking and highlighting; keyboard+mouse PTY journey on a large fixture log
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified in gate 24 (node scripts/test-all.mjs 6551/1; the 1 was a test-helper SIGCONT race, fixed and its 4 modules rerun 40/0), fmt and clippy clean.
<!-- SECTION:NOTES:END -->
