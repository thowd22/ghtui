---
id: TASK-115
title: >-
  Show PR conversation as separated comment cards with obvious actions, like
  GitHub
status: Done
assignee: []
created_date: '2026-10-21 17:54'
updated_date: '2026-10-21 21:26'
labels:
  - usability
dependencies:
  - TASK-114
priority: high
ordinal: 48069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
User report: comments run together with no delineation and adding or acting on comments is not intuitive. Render each comment/review as a card: header with avatar chip, author, role badge (author/member/bot), relative and UTC time, edited marker; body rendered by the shared Markdown renderer; reactions row; visible per-comment buttons (Reply/Quote, React, Edit, Delete when allowed) by key and click. A persistent 'Add a comment' box at the bottom of Conversation opens the composer. A PR action bar shows the main actions plainly (Comment, Review: Approve/Request changes, Merge, Close, Mark ready) with disabled reasons.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Comments are visually separated cards with author, badge, time and reactions
- [x] #2 Each permitted comment action is a visible button that works by key and click with the existing write semantics and previews
- [x] #3 Add a comment box and PR action bar are always visible on Conversation; disabled actions say why
- [x] #4 Keyboard+mouse PTY journeys with strict fake gh cover reply, react, edit and add comment from the new controls
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified in gate 24 (node scripts/test-all.mjs 6551/1; the 1 was a test-helper SIGCONT race, fixed and its 4 modules rerun 40/0), fmt and clippy clean. Delete button has no PTY journey; Edit/Delete shown on every card, server decides eligibility.
<!-- SECTION:NOTES:END -->
