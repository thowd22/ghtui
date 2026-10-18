---
id: TASK-99.2
title: 'History: ref chips and commit table'
status: To Do
assignee: []
created_date: '2026-10-18 20:38'
labels:
  - ui-design
  - history
dependencies: []
references:
  - 'https://claude.ai/artifact/63SzYQzQwKbCwYzGuAwqn3'
parent_task_id: TASK-99
priority: medium
ordinal: 30069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Branch (local), remote, tag and HEAD labels as colored chips on their commits; aligned columns graph | SHA | message | author chip | age with ellipsis truncation; selected row full-width bar.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every ref in git for-each-ref appears on its commit when loaded
- [ ] #2 PTY journeys at 140x42 and 80x24 with keyboard and mouse on real Git fixtures (branches, merges, tags, remotes); working tree unchanged
<!-- AC:END -->
