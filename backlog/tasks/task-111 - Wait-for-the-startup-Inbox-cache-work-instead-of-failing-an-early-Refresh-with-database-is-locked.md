---
id: TASK-111
title: >-
  Wait for the startup Inbox cache work instead of failing an early Refresh with
  database is locked
status: Done
assignee: []
created_date: '2026-10-21 13:29'
updated_date: '2026-10-21 16:26'
labels:
  - bug
dependencies: []
priority: medium
ordinal: 44069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-66 release run: a Refresh clicked during startup failed with 'Error: database is locked • r Refresh to retry' after the 2 s busy budget while the startup Inbox work held the account cache. Also seen once on an Actions artifact export under load (TASK-65 round 5). Find which write holds the lock that long and queue the refresh behind it, or shorten the transaction.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A Refresh during startup waits for or queues behind the startup cache work and succeeds
- [x] #2 Lib test reproduces the contention deterministically; PTY journey refreshes during startup by key and click
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified in gate 22 at 3c27e77f: node scripts/test-all.mjs 6468/0, fmt and clippy clean.
<!-- SECTION:NOTES:END -->
