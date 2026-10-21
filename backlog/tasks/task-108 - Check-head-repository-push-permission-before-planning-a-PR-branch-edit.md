---
id: TASK-108
title: Check head-repository push permission before planning a PR branch edit
status: To Do
assignee: []
created_date: '2026-10-21 09:56'
labels:
  - feature
dependencies: []
priority: medium
ordinal: 41069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-27 AC2 follow-up: the branch-edit plan says the remote decides with the user's Git credentials. Read the head repository's three-state permission flags first (unknown is not denied), migrating strict fixtures for the extra gh read, and add a PTY journey for a remote refusal (pre-receive hook) in this flow.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Plan shows push permission as granted/denied/unknown before confirm
- [ ] #2 Remote refusal journey (pre-receive hook) keeps the edited file and journals not written
<!-- AC:END -->
