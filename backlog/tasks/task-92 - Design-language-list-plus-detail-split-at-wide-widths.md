---
id: TASK-92
title: 'Design language: list plus detail split at wide widths'
status: Done
assignee: []
created_date: '2026-10-18 19:59'
updated_date: '2026-10-20 01:37'
labels:
  - ui-design
dependencies:
  - TASK-89
  - TASK-91
references:
  - 'https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3'
priority: medium
ordinal: 21069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
At 120+ columns Inbox, Actions and Search show the list beside a live preview of the selected item; Tab moves focus between panes. Below 120 columns keep one pane with Enter to drill down. Gold standards: lazygit, gh-dash sidebar, k9s drill-down.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Moving the selection updates the preview without extra GitHub reads beyond what opening already does
- [ ] #2 Resizing across 120 columns keeps selection and focus
- [ ] #3 PTY test at 140x42 and 80x24 with keyboard and mouse, plus unit tests for the layout helpers
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
List plus preview split from 120 columns for Inbox, Actions Runs, Search; per-view list state, Tab/click pane focus. Gate: node scripts/test-all.mjs 5933 passed/0 failed, fmt and clippy clean (732b0709).
<!-- SECTION:NOTES:END -->
