---
id: TASK-96
title: 'Design language: Comfortable and Compact density setting'
status: To Do
assignee: []
created_date: '2026-10-18 19:59'
updated_date: '2026-10-19 20:24'
labels:
  - ui-design
dependencies:
  - TASK-92
  - TASK-94
references:
  - 'https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3'
priority: medium
ordinal: 25069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Settings › Density: Comfortable (default; spacing and splits) and Compact (today's maximum rows, for agents and small terminals). Persisted in config.toml.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both modes pass the same PTY journeys
- [ ] #2 PTY test at 140x42 and 80x24 with keyboard and mouse, plus unit tests for the layout helpers
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
User decisions 2026-10-18: footer key bar replaces top button rows; list + preview split at >=120 cols (drill-down when narrow); rounded borders; Comfortable density by default with Compact as a setting. Mockups: https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3

2026-10-19: density comfortable|compact in config.rs (default comfortable), ui_chrome::density() per frame; Compact = no list+preview split, queue summary carries freshness (no PULL REQUESTS row), 1-cell column gap, 1-line Search results. Settings Density control (focus order Host, Set, Theme, Density, Save, Close). Gate: node scripts/test-task.mjs design_density_v96 -> code 4/4, PTY 4/4 (keyboard+mouse, 140x42 and 80x24). Not yet: running all other journeys under Compact; Comfortable 2-row Inbox items.
<!-- SECTION:NOTES:END -->
