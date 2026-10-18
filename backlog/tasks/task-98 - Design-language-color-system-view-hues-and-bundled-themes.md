---
id: TASK-98
title: 'Design language: color system, view hues and bundled themes'
status: To Do
assignee: []
created_date: '2026-10-18 20:25'
updated_date: '2026-10-18 23:00'
labels:
  - ui-design
dependencies:
  - TASK-88
references:
  - 'https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3'
priority: high
ordinal: 27069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
More color, with meaning: each view owns a hue (Inbox mauve, Repository peach, Search teal, Actions blue, Dashboard green) used for its tab, focused border, section headers and footer keys. Data colors are fixed across views (branches teal, commit SHAs yellow, events pink, numbers peach) and status shows as filled badges (✓ PASSED, ✗ FAILURE, ● RUNNING) always with a glyph. Bundle themes Catppuccin Mocha, Tokyo Night and Gruvbox Dark plus a 16-color fallback; s Settings › Theme; --no-color unchanged. Mockups: canvas Theme row.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Truecolor themes degrade to 256 and 16 colors (COLORTERM detection) without losing meaning
- [ ] #2 Text and badges keep 4.5:1 contrast on each bundled theme
- [ ] #3 PTY test at 140x42 and 80x24 with keyboard and mouse, plus unit tests for theme mapping
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
User decisions 2026-10-18: default theme Catppuccin Mocha (Tokyo Night and Gruvbox Dark selectable in Settings, plus a 16-color fallback); status as filled badges in details and titles, colored glyphs in list rows. Mockups: https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3

2026-10-18: Five themes (mock hex), dark/light aliases, Indexed256 + hue-preserving 16-color downsample, COLORTERM detection, per-view hues, lane list, badge API, Settings swatch landed in b9e1dfb7. Gruvbox failure text tinted to #f95b43 and badge ink #1d2021 for 4.5:1. Still open: badges and data-color roles adopted in views. Gates: cargo test --lib design_ (8 passed); cargo test --test tui_e2e -- design_theme_v98 (2 passed).
<!-- SECTION:NOTES:END -->
