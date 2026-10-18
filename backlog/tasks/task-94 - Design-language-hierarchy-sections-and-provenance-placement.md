---
id: TASK-94
title: 'Design language: hierarchy, sections and provenance placement'
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
priority: medium
ordinal: 23069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Uppercase accent section headers, label/value blocks with aligned muted labels, one blank line between sections, provenance (Fetched ..., complete/partial) moved to the status line or border, and diagnostics behind i Details.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Partial and unknown states stay visible, never shown as zero
- [ ] #2 PTY test at 140x42 and 80x24 with keyboard and mouse, plus unit tests for the layout helpers
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
User decisions 2026-10-18: footer key bar replaces top button rows; list + preview split at >=120 cols (drill-down when narrow); rounded borders; Comfortable density by default with Compact as a setting. Mockups: https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3

2026-10-18 partial: ui_chrome::section_header (title left in view hue, provenance right, provenance wins when narrow, Warning role for partial) used for Inbox freshness, Actions freshness/watch and Dashboard as-of; coverage/PARTIAL rows unchanged. label_value helper added with unit tests but not adopted (detail strings are test literals; Phase 3 split). Tests: design_hierarchy_v94_* unit + PTY in tests/tui_e2e/design_inbox_v94_v95.rs.
<!-- SECTION:NOTES:END -->
