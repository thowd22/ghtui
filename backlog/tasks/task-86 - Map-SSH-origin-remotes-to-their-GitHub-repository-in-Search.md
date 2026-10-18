---
id: TASK-86
title: Map SSH origin remotes to their GitHub repository in Search
status: Done
assignee: []
created_date: '2026-10-18 09:35'
updated_date: '2026-10-18 13:39'
labels: []
dependencies: []
ordinal: 15069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Live check 2026-10-18: Search says 'Current local checkout has no verified GitHub repository mapping' in a checkout whose origin is git@github.com:thowd22/ghtui.git. Confirm whether verification is required by design; if so, make the next step visible.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 SSH and HTTPS github.com origins resolve (or show an explicit verify action)
- [x] #2 PTY coverage for an SSH-origin fixture
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Unmapped local/current checkout stays unmapped by design (no inferred mapping); the Search warning now names the detected origin (SSH/HTTPS) and the explicit step: c Corpus -> Map existing, which already offers the origin repo and the checkout. Gate: tui_e2e remote_search 6/6 (SSH-origin keyboard+mouse assert the warning), lib search 115/115, corpus_ui_contracts 5/5, clippy clean.
<!-- SECTION:NOTES:END -->
