---
id: TASK-123
title: >-
  Add type-to-filter to Inbox, Actions runs and review threads and unify the
  fuzzy rankers
status: To Do
assignee: []
created_date: '2026-10-21 20:30'
labels:
  - usability
dependencies:
  - TASK-122
priority: high
ordinal: 56069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-122 follow-up. Inbox PR list, Actions runs list and the review-thread list have no text filter (InboxFilter is a queue enum, RunFilter a server query, threads a scope toggle). Add a visible Filter control on each (key decision: / is global Search, so use f or a clickable Filter box) built on src/fuzzy_finder.rs. Move repository catalogue and PR metadata pickers off direct nucleo onto it. Fold src/fuzzy_rank_v1.rs (Search streaming scan, TASK-121) into src/fuzzy_finder.rs so one ranking exists.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Inbox, Actions runs and review threads have a type-to-filter box with highlighted fuzzy matches, by key and click
- [ ] #2 Repository catalogue and metadata pickers use the shared finder
- [ ] #3 Only one fuzzy ranking module remains; Search, log finder and pickers rank identically
- [ ] #4 Keyboard+mouse PTY journeys for the three new filters
<!-- AC:END -->
