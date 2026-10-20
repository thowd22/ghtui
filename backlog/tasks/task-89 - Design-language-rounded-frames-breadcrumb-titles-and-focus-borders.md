---
id: TASK-89
title: 'Design language: rounded frames, breadcrumb titles and focus borders'
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
priority: high
ordinal: 18069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Rounded borders with the title in the border as a breadcrumb (Actions › repo › Runs), accent border on the focused pane and dim borders elsewhere, active view tab highlighted, and no '• focused view' label. Gold standards: lazygit, btop.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Focused and unfocused panes are distinguishable without color
- [ ] #2 Breadcrumb follows drill-down and b Back
- [ ] #3 PTY test at 140x42 and 80x24 with keyboard and mouse, plus unit tests for the layout helpers
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Rounded pane frames, breadcrumb titles, focus borders via ui_chrome::pane_block. Landed in Phases 0-1 (pushed); verified in the full gates since, latest 5933 passed/0 failed with fmt and clippy clean (732b0709).
<!-- SECTION:NOTES:END -->
