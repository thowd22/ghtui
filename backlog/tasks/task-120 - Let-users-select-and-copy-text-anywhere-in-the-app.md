---
id: TASK-120
title: Let users select and copy text anywhere in the app
status: To Do
assignee: []
created_date: '2026-10-21 17:54'
updated_date: '2026-10-21 17:56'
labels:
  - usability
dependencies: []
priority: high
ordinal: 53069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
User report: cannot highlight text to copy and paste elsewhere; mouse capture blocks terminal selection. Add in-app selection: mouse drag selects text in any pane (highlighted), release or y copies via OSC 52 (with wl-copy/xclip/pbcopy fallback if present, no new deps for auth). Add y Copy on the focused item (SHA, URL, log line, comment body, path). Add a toggle (e.g. F12) that releases mouse capture so the terminal's native selection works, shown in the footer while active. Document Shift+drag in Help.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Dragging selects visible text in panes, viewers, logs and comments and copies it; a status message confirms
- [ ] #2 y copies the focused item's natural value in each view
- [ ] #3 A visible toggle releases mouse capture for native terminal selection and restores it
- [ ] #4 Lib tests for selection geometry and OSC 52 encoding; keyboard+mouse PTY journeys assert the OSC 52 sequence
- [ ] #5 Releasing the mouse after a click-and-drag puts exactly the highlighted text on the system clipboard with no extra key (OSC 52 plus wl-copy/xclip/xsel when present), and the status line confirms it
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
User clarified 2026-10-21: copy must happen on mouse release after click-drag highlight; no extra key required.
<!-- SECTION:NOTES:END -->
