---
id: TASK-103
title: >-
  Find why the Conversation held-post replacement test fails about half the time
  since 2026-10-21
status: To Do
assignee: []
created_date: '2026-10-21 01:44'
labels:
  - bug
dependencies: []
priority: medium
ordinal: 36069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
conversation_comments_v17_runtime_held_post_success_err_actual_panic_replacement (src/scoped_pr_collaboration_runtime_v17_tests.rs, modes 0..=2) panics at line 498: ResourceLocalGuard::validate_conversation_history_v17 returns IdentityChanged 'Original selected resource local fence unavailable', then the gh journal shows 43 calls instead of 46. Failed in gate 12 (6209 passed / 1 failed) and 3/6 module runs without the TASK-102 SQLite change (A/B), 2/5 single-threaded, 0/4 when run alone. It passed in gates 8-11 on 2026-10-20 with the same code, so look for a time or ordering dependence in the fixture or the local fence. This round only added struct fields to the involved files.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The failing fence step (cache open, source validate, journal open or history compare) is identified
- [ ] #2 The root cause is fixed without sleeps or loosened fence checks
- [ ] #3 The module passes 10 consecutive runs and a full gate
<!-- AC:END -->
