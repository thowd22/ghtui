---
id: TASK-101
title: Find why two mouse journeys lose a click under full-suite load
status: In Progress
assignee: []
created_date: '2026-10-20 17:42'
updated_date: '2026-10-20 23:45'
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
- [x] #1 Root cause identified with event-log instrumentation (CLAUDE.md section 4)
- [x] #2 Fix in the product or the shared click helper, not by adding sleeps
- [ ] #3 Both journeys pass in two consecutive full gates
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-20 round: Root cause is in the shared click helper, not the product. A ghtui frame reaches the PTY over several reads, top rows first, and Session::wait_for_control located controls on the live, half-drawn vt100 screen.

Evidence comes from a temporary GHTUI_DEBUG_EVENT_LOG trace of app events and harness writes, captured under 8 CPU hogs. The instrumentation has been removed. In the owned_pty_cleanup_v1 mouse journeys the harness clicked column 70, row 5 at transcript byte 2110, before the key bar was painted. With no key bar on screen, tab_control (non-gapped and case-insensitive) matched the Inbox hint prose '• ? help' for [? Help]. The app received Down and Up there, no control was under them, and nothing changed.

The blended failure is the same race with a stale key bar. On the loading bar, B Blend sits at about column 66. On the settled bar, which adds 'o Open result', column 66 falls on 'S Save query', which explains the 'Saved query: My search' status.

Fix in tests/support/mod.rs: a new completed_control() locates the label only on cursorless_frames.last_cells, and only when no newer frame is arriving. The only bytes allowed after the last completed frame are a final cursor placement (ESC[r;cH). wait_for_control and try_wait_for_control use it. No sleeps were added and no checks were loosened.

Gates:
- Run 1, instrumented, 8 hogs: both owned_pty mouse journeys reproduced the failure.
- Run 2, fix, 3 hogs: all 4 owned_pty_cleanup_v1 journeys passed. The only failures were the two blended explicit_scopes journeys, keyboard and mouse alike, which timed out waiting 8s for the ranked search under the artificial load.
- Run 3, fix, no load: `repro.sh owned_pty_cleanup_v1 blended:: help_scrollbar mouse_hover design_footer_v90 navigation_` gave 82 passed, 0 failed.

AC#3 (two consecutive full gates) has not been run; it is for the coordinator. Merged to main by 11eb1ced; full gate 6206 passed, 2 load-only failures elsewhere (TASK-102).

2026-10-20: cause found (clicks located on half-drawn frames; [? Help] matched Inbox hint text). Session::wait_for_control now uses the latest completed frame (55b0516d). Both journeys passed in gates 8, 9 and 10; AC3 needs two consecutive clean gates.
<!-- SECTION:NOTES:END -->
