---
id: TASK-88
title: 'Design language: semantic theme tokens and text levels'
status: To Do
assignee: []
created_date: '2026-10-18 19:59'
labels:
  - ui-design
dependencies: []
references:
  - 'https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3'
priority: high
ordinal: 17069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Introduce role-based theme tokens (accent, selection, text, muted, dim, border, success, failure, warning, info, commit) and three text levels; every view styles via roles. Status color is always paired with a glyph (✓ ✗ ● ⊘ ○). Reference: design canvas Tokens board. Gold standards: lazygit theme config, btop themes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Theme exposes the roles, and --no-color maps them to text/glyph labels
- [ ] #2 No view uses raw colors
- [ ] #3 PTY test at 140x42 and 80x24 with keyboard and mouse, plus unit tests for the layout helpers
<!-- AC:END -->
