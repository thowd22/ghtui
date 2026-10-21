---
id: TASK-105
title: Let Refresh re-admit the Personal scope after a confirmed scoped review write
status: To Do
assignee: []
created_date: '2026-10-21 09:56'
labels:
  - bug
dependencies: []
priority: high
ordinal: 38069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After a confirmed pending-review write (review Add, review submit), retire_remote fences the Personal remote scope. The status line says to refresh, but Refresh sends nothing ('Original repository scope is fenced • no fallback or query sent'), so the user must restart ghtui. Found by TASK-65 review journeys. active_remote_scope_v1.rs forbids a fenced scope becoming a default personal query, so this needs an explicit re-admission path.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 After a confirmed scoped review write, Refresh (key and click) re-reads the same account's Personal scope without a restart
- [ ] #2 A changed account or repository identity still stays fenced and says why
- [ ] #3 Keyboard+mouse PTY journey performs two review writes in one session with strict fake gh
<!-- AC:END -->
