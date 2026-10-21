---
id: TASK-115
title: >-
  Show PR conversation as separated comment cards with obvious actions, like
  GitHub
status: To Do
assignee: []
created_date: '2026-10-21 17:54'
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
- [ ] #1 Comments are visually separated cards with author, badge, time and reactions
- [ ] #2 Each permitted comment action is a visible button that works by key and click with the existing write semantics and previews
- [ ] #3 Add a comment box and PR action bar are always visible on Conversation; disabled actions say why
- [ ] #4 Keyboard+mouse PTY journeys with strict fake gh cover reply, react, edit and add comment from the new controls
<!-- AC:END -->
