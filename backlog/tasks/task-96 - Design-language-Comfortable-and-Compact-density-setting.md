---
id: TASK-96
title: 'Design language: Comfortable and Compact density setting'
status: To Do
assignee: []
created_date: '2026-10-18 19:59'
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
