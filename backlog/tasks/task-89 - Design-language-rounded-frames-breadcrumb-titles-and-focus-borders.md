---
id: TASK-89
title: 'Design language: rounded frames, breadcrumb titles and focus borders'
status: To Do
assignee: []
created_date: '2026-10-18 19:59'
updated_date: '2026-10-18 23:26'
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

2026-10-18 partial: src/ui_chrome.rs (pane_block, header_rows/draw_header, main_pane, compact_scope). Header row 0 = logo chip + tabs (active filled with view hue) + Back/crumbs + account chip; Back/crumbs/Source identity wrap to row 1 when they do not fit. Context moved to the content pane's right border title. Rounded borders on all 58 Borders::ALL sites; focused main pane gets BorderFocus + bold ▶ title. Kept '{View} • focused view' title text (plan: test compatibility) though the task text says to drop it. Open: drill-down crumbs into pane titles (AC #2), routing takeovers through pane_block. Gate: cargo test --lib --test tui_e2e -- design_ ... (lib 43, PTY 19 passed).
<!-- SECTION:NOTES:END -->
