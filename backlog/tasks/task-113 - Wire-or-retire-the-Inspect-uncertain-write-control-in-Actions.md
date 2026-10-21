---
id: TASK-113
title: Wire or retire the Inspect uncertain write control in Actions
status: Done
assignee: []
created_date: '2026-10-21 13:29'
updated_date: '2026-10-21 16:26'
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
- [x] #1 The control is either reachable from a real uncertain Actions write (keyboard+mouse PTY) or removed with its tests and docs
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified in gate 22 at 3c27e77f: node scripts/test-all.mjs 6468/0, fmt and clippy clean. Control retired (never reachable).
<!-- SECTION:NOTES:END -->
