---
id: TASK-102
title: >-
  Find why a SQLite WAL writer and a catalogue filter keystroke fail only under
  full-suite load
status: To Do
assignee: []
created_date: '2026-10-20 23:44'
updated_date: '2026-10-21 06:29'
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
- [x] #1 Root cause of the SQLITE_BUSY under load is identified and fixed in product or harness
- [x] #2 Root cause of the lost filter keystroke after restart is identified and fixed
- [ ] #3 Both pass in two consecutive full gates
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-21: SQLite part reproduced (19/30 runs with 6 CPU hogs). Timing showed each failed write waited the full 2s busy_timeout before SQLITE_BUSY: lock starvation, not an immediate BUSY_SNAPSHOT. SQLite's polling busy handler is unfair, and the test's two writers loop tightly (put + put_draft, yield_now) against a continuous PASSIVE checkpointer, so one writer can lose every retry for >2s under load. The storage code path is unchanged this round. Fix options: a fairer test shape that keeps the concurrency contract, or a longer/explicit production busy policy for background writers. Also failed in gate 11 (6207 passed / 1 failed).

2026-10-21: SQLite starvation fixed in 16941191 (storage::busy_v1 fair 1 ms retry within the 2 s budget; scoped BusyDeadline guards replace lasting shorter busy_timeouts; WAL stress contract paces writers per round). Stress with 6 CPU hogs: 19/30 failures before, 0/40 after. Gate 12: 6209 passed / 1 failed, the failure being an unrelated pre-existing race (TASK-103). Lost-keystroke part still open.

Keystroke half: harness typed before the discovered workspace was installed (busy modal drops keys by contract). filter() now waits for the painted [/ Filter] (97acd232). repo_catalog_dialogs + workflow_v6 27/0. AC3 pending clean gates.

Lock follow-up: no transaction spans a child; slow commits under ionice -c3 exceed the 2 s budget. d8cc3c2a gives journal validation a fair bounded wait; 41672d74 lets a current cache open skip BEGIN IMMEDIATE. Gate 15 (node scripts/test-all.mjs, merged round 3 + 41672d74): 6303 passed / 0 failed; fmt + clippy clean. (clean gate 1 of 2 for AC3).
<!-- SECTION:NOTES:END -->
