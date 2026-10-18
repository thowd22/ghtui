---
id: TASK-91
title: 'Design language: table component for lists'
status: To Do
assignee: []
created_date: '2026-10-18 19:59'
labels:
  - ui-design
dependencies:
  - TASK-88
references:
  - 'https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3'
priority: high
ordinal: 20069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Shared table renderer: muted header row, aligned and truncating columns (ellipsis), status glyph column, short SHAs (7) and relative ages, full-width selection bar (▌ plus selection background). Apply to Inbox queues, Actions runs/jobs, Search results and Repository files/commits. Gold standards: k9s, gh-dash.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Columns adapt to width and drop low-priority columns when narrow
- [ ] #2 Full SHA and UTC time stay in detail panes and are copyable
- [ ] #3 PTY test at 140x42 and 80x24 with keyboard and mouse, plus unit tests for the layout helpers
<!-- AC:END -->
