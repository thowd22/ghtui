---
id: TASK-97
title: 'Design language: Help as grouped key tables and a plain palette'
status: To Do
assignee: []
created_date: '2026-10-18 19:59'
labels:
  - ui-design
dependencies:
  - TASK-88
  - TASK-90
references:
  - 'https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3'
priority: medium
ordinal: 26069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Help shows grouped two-column key tables (which-key style) before prose; the command palette drops internal wording (catalogue, authority, IDs), right-aligns keys and dims descriptions. Gold standards: helix, lazygit ? menu.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every footer key of a view appears in its Help
- [ ] #2 PTY test at 140x42 and 80x24 with keyboard and mouse, plus unit tests for the layout helpers
<!-- AC:END -->
