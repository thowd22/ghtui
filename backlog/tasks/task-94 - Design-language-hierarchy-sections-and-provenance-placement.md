---
id: TASK-94
title: 'Design language: hierarchy, sections and provenance placement'
status: To Do
assignee: []
created_date: '2026-10-18 19:59'
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
