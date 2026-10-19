---
id: TASK-90
title: 'Design language: context footer key bar replaces top button rows'
status: Done
assignee: []
created_date: '2026-10-18 19:59'
updated_date: '2026-10-19 16:49'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
User decisions 2026-10-18: footer key bar replaces top button rows; list + preview split at >=120 cols (drill-down when narrow); rounded borders; Comfortable density by default with Compact as a setting. Mockups: https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3

2026-10-19: footer key bar on every view (src/footer.rs, per-view footer_keys/resolve_key), Help key tables and palette keys. Gate: test-all 5,875 passed after the final settings-wait fix (TASK-87 flake passed on rerun); fmt and clippy clean.
<!-- SECTION:NOTES:END -->
