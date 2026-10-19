---
id: TASK-97
title: 'Design language: Help as grouped key tables and a plain palette'
status: Done
assignee: []
created_date: '2026-10-18 19:59'
updated_date: '2026-10-19 16:49'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
User decisions 2026-10-18: footer key bar replaces top button rows; list + preview split at >=120 cols (drill-down when narrow); rounded borders; Comfortable density by default with Compact as a setting. Mockups: https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3

2026-10-19: footer key bar on every view (src/footer.rs, per-view footer_keys/resolve_key), Help key tables and palette keys. Gate: test-all 5,875 passed after the final settings-wait fix (TASK-87 flake passed on rerun); fmt and clippy clean.
<!-- SECTION:NOTES:END -->
