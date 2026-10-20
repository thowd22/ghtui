---
id: TASK-101
title: Find why two mouse journeys lose a click under full-suite load
status: To Do
assignee: []
created_date: '2026-10-20 17:42'
labels:
  - bug
dependencies: []
priority: medium
ordinal: 34069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In the full gate at b0bb385f (6103 passed / 2 failed) two mouse journeys lost a click; both pass when their whole modules run alone (blended::, owned_pty_cleanup_v1::: 88 passed) and passed in the previous full gate. owned_pty_cleanup_v1 mouse: on a fresh fixture at 80x24 the click on '? Help' got no reaction; terminal.raw shows one complete startup frame, mouse modes enabled first, then only idle redraws. blended mouse: the click on '[B Blend]' after closing Blend controls did not open them; the status showed 'Saved query: My search'. Suspects: a stale key bar after an overlay closes, or a mouse escape sequence split across PTY reads under load.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Root cause identified with event-log instrumentation (CLAUDE.md section 4)
- [ ] #2 Fix in the product or the shared click helper, not by adding sleeps
- [ ] #3 Both journeys pass in two consecutive full gates
<!-- AC:END -->
