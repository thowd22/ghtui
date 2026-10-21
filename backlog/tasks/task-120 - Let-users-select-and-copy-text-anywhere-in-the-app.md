---
id: TASK-120
title: Let users select and copy text anywhere in the app
status: Done
assignee: []
created_date: '2026-10-21 17:54'
updated_date: '2026-10-21 21:26'
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
- [x] #1 Dragging selects visible text in panes, viewers, logs and comments and copies it; a status message confirms
- [x] #2 y copies the focused item's natural value in each view
- [x] #3 A visible toggle releases mouse capture for native terminal selection and restores it
- [x] #4 Lib tests for selection geometry and OSC 52 encoding; keyboard+mouse PTY journeys assert the OSC 52 sequence
- [x] #5 Releasing the mouse after a click-and-drag puts exactly the highlighted text on the system clipboard with no extra key (OSC 52 plus wl-copy/xclip/xsel when present), and the status line confirms it
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
User clarified 2026-10-21: copy must happen on mouse release after click-drag highlight; no extra key required.

Verified in gate 24 (node scripts/test-all.mjs 6551/1; the 1 was a test-helper SIGCONT race, fixed and its 4 modules rerun 40/0), fmt and clippy clean. Drag-release copy covered in History, PR Conversation and Actions log journeys.
<!-- SECTION:NOTES:END -->
