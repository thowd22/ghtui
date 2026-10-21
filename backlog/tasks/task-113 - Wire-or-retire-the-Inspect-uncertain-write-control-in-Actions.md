---
id: TASK-113
title: Wire or retire the Inspect uncertain write control in Actions
status: To Do
assignee: []
created_date: '2026-10-21 13:29'
labels:
  - bug
dependencies: []
priority: low
ordinal: 46069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-64 AC2 found ActionsWorkspace::mark_uncertain has no caller in the binary; uncertain Actions writes go through Publications, so the 'i Inspect uncertain write' control is unreachable in practice. Either wire it to the real uncertain-write path or remove the control.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The control is either reachable from a real uncertain Actions write (keyboard+mouse PTY) or removed with its tests and docs
<!-- AC:END -->
