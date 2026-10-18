---
id: TASK-84
title: Wrap PR body text at word boundaries
status: Done
assignee: []
created_date: '2026-10-18 09:35'
updated_date: '2026-10-18 15:09'
labels: []
dependencies: []
ordinal: 13069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Live check 2026-10-18: PR Summary body wraps mid-word (e.g. 'obs|erve') at 140 columns. Shared wrapping is used by many PTY assertions; change carefully.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Summary body wraps at whitespace when a word fits on the next line
- [x] #2 Overlong tokens still hard-wrap
- [x] #3 Focused PTY test covers a long PR body at narrow and wide widths
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
wrap_cells breaks after the last whitespace; overlong tokens hard-wrap; rows still concatenate to the input. Gate: lib pr_body_wrap 1/1; tui_e2e pr_body_wrap 2/2 (asserts no mid-word fragments at 120x40 and 80x24); PR/review/conversation PTY 916/916.
<!-- SECTION:NOTES:END -->
