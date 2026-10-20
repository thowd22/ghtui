---
id: TASK-99.2
title: 'History: ref chips and commit table'
status: In Progress
assignee: []
created_date: '2026-10-18 20:38'
updated_date: '2026-10-20 02:33'
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
- [x] #1 Every ref in git for-each-ref appears on its commit when loaded
- [ ] #2 PTY journeys at 140x42 and 80x24 with keyboard and mouse on real Git fixtures (branches, merges, tags, remotes); working tree unchanged
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Typed ref chips (Local/Remote/Tag/HEAD + current flag) from the graph walk's for-each-ref snapshot replace the 'HEAD at graph load' name; origin/HEAD symref and stash are not chips; local+same-named origin (or upstream) on one commit merge as 'name ⇅'; refs past 4096 are counted in the summary instead of failing (byte cap 8 MiB, snapshot kept as a digest). Rows are a ui_table History table (GRAPH|COMMIT|MESSAGE+chips|AUTHOR chip|AGE) under a 'History › all branches' rule with the ▌ bar and a 'Read-only · N commits loaded' status line; chips fit with an explicit +N. Gate: node scripts/test-task.mjs history_refs_v992 -> lib 15/15; PTY 140x42 parts pass (kbd+mouse, NO_COLOR, detached) but both journeys failed at the 80x24 column-order check from a fixture parser bug (AGE matched inside MESSAGE), fixed after the 3-run cap and not re-run. Lib graph/lanes filters 110/110, contracts 70/70, history_lanes_v991 PTY 2/2.
<!-- SECTION:NOTES:END -->
