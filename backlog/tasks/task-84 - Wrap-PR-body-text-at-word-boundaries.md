---
id: TASK-84
title: Wrap PR body text at word boundaries
status: To Do
assignee: []
created_date: '2026-10-18 09:35'
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
- [ ] #1 Summary body wraps at whitespace when a word fits on the next line
- [ ] #2 Overlong tokens still hard-wrap
- [ ] #3 Focused PTY test covers a long PR body at narrow and wide widths
<!-- AC:END -->
