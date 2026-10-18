---
id: TASK-89
title: 'Design language: rounded frames, breadcrumb titles and focus borders'
status: To Do
assignee: []
created_date: '2026-10-18 19:59'
updated_date: '2026-10-18 20:22'
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
User decisions 2026-10-18: footer key bar replaces top button rows; list + preview split at >=120 cols (drill-down when narrow); rounded borders; Comfortable density by default with Compact as a setting. Mockups: https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3
<!-- SECTION:NOTES:END -->
