---
id: TASK-95
title: 'Design language: initials chips for people in lists'
status: Done
assignee: []
created_date: '2026-10-18 19:59'
updated_date: '2026-10-20 01:37'
labels:
  - ui-design
dependencies:
  - TASK-88
references:
  - 'https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3'
priority: low
ordinal: 24069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the 4-cell half-block avatar with a two-letter colored initials chip in list rows; keep image avatars in detail headers. Colors are stable per login and differ in lightness.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The same person has the same chip across Inbox, Actions and Repository
- [ ] #2 PTY test at 140x42 and 80x24 with keyboard and mouse, plus unit tests for the layout helpers
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Initials chips for actors, consistent in tables and headers. Landed in Phases 0-1 (pushed); verified in the full gates since, latest 5933 passed/0 failed with fmt and clippy clean (732b0709).
<!-- SECTION:NOTES:END -->
