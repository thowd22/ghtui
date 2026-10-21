---
id: TASK-102
title: >-
  Find why a SQLite WAL writer and a catalogue filter keystroke fail only under
  full-suite load
status: To Do
assignee: []
created_date: '2026-10-20 23:44'
updated_date: '2026-10-21 00:31'
labels:
  - bug
dependencies: []
priority: medium
ordinal: 35069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Gate 10 (11eb1ced, 6206 passed / 2 failed) failed two tests that pass when their whole modules run (30/0): sqlite_dependency_real_concurrent_wal_checkpoint_drafts_and_records_reopen_isolated got 'database is locked' from a cache put despite the 2s busy_timeout, and scoped_actions_workflow_v6_keyboard_real_restart_workflow_inspection_and_ack lost the typed catalogue filter (Filter focused: empty) after restart. Both passed in gates 8 and 9. Find the real race; never add sleeps or loosen checks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Root cause of the SQLITE_BUSY under load is identified and fixed in product or harness
- [ ] #2 Root cause of the lost filter keystroke after restart is identified and fixed
- [ ] #3 Both pass in two consecutive full gates
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-21: SQLite part reproduced (19/30 runs with 6 CPU hogs). Timing showed each failed write waited the full 2s busy_timeout before SQLITE_BUSY: lock starvation, not an immediate BUSY_SNAPSHOT. SQLite's polling busy handler is unfair, and the test's two writers loop tightly (put + put_draft, yield_now) against a continuous PASSIVE checkpointer, so one writer can lose every retry for >2s under load. The storage code path is unchanged this round. Fix options: a fairer test shape that keeps the concurrency contract, or a longer/explicit production busy policy for background writers. Also failed in gate 11 (6207 passed / 1 failed).
<!-- SECTION:NOTES:END -->
