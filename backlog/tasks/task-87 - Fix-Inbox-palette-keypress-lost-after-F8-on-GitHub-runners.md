---
id: TASK-87
title: Fix Inbox palette keypress lost after F8 on GitHub runners
status: Done
assignee: []
created_date: '2026-10-18 18:03'
updated_date: '2026-10-20 01:37'
labels:
  - ci
  - flaky
dependencies: []
ordinal: 16069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
CI-only intermittent (3 of 7 hosted runs, incl. isolated retry): v1_navigation_inbox_keyboard_prompt_counts_help_and_palette_notifications presses ':' after the F8 'Refresh the Inbox (r) first' status and the palette never appears. Passes locally and in an Ubuntu 24.04 container. Ruled out: key gating, terminal queries, background launch reads. Covered by the CI retry-once for now.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Root cause identified via a pty_filter CI run with transcript output
- [ ] #2 Keypress is never dropped; test passes on hosted runners without retry
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Cause: a background completion retired the open palette right after ':'. Commands now reopens with fresh targets and the typed query. Gate: node scripts/test-all.mjs 5933 passed/0 failed, fmt and clippy clean (732b0709).
<!-- SECTION:NOTES:END -->
