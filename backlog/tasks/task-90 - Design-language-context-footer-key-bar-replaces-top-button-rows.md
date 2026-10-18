---
id: TASK-90
title: 'Design language: context footer key bar replaces top button rows'
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
ordinal: 19069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the rows of [x Button] controls above content with a footer of the 6-8 most relevant keys for the focused pane (key in accent, label muted), clickable with the mouse. The full action set stays in : Commands and ? Help. Disabled actions leave the footer; pressing their key shows the reason. View sub-modes (Runs/Workflows/..., Summary/Files/...) become one tab strip. Gold standards: k9s header hints, gitui command bar, lazygit options line.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every action remains reachable by key, click and the palette
- [ ] #2 Content gains at least 3 rows on every view at 140x42
- [ ] #3 PTY test at 140x42 and 80x24 with keyboard and mouse, plus unit tests for the layout helpers
<!-- AC:END -->
