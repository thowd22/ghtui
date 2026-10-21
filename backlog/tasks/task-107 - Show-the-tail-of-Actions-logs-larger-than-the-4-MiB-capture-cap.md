---
id: TASK-107
title: Show the tail of Actions logs larger than the 4 MiB capture cap
status: Done
assignee: []
created_date: '2026-10-21 09:56'
updated_date: '2026-10-21 16:26'
labels:
  - bug
dependencies: []
priority: medium
ordinal: 40069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A 20 MB Actions log is refused at the 4 MiB runner cap ('GitHub response exceeded its bounded capture'), so users see none of it. Found by TASK-66. Keep memory bounded but show the last part of the log with an honest truncation notice and the export path for the full log.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Logs over the cap show a bounded tail with a visible truncation notice, by key and click
- [x] #2 Memory stays bounded; the full log remains exportable
- [x] #3 Lib + keyboard/mouse PTY coverage with a large fixture log
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified in gate 22 at 3c27e77f: node scripts/test-all.mjs 6468/0, fmt and clippy clean.
<!-- SECTION:NOTES:END -->
