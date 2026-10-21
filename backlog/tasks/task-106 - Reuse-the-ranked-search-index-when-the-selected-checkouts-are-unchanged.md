---
id: TASK-106
title: Reuse the ranked search index when the selected checkouts are unchanged
status: Done
assignee: []
created_date: '2026-10-21 09:56'
updated_date: '2026-10-21 16:26'
labels:
  - performance
dependencies: []
priority: medium
ordinal: 39069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-66 measured the warm Ranked search first page at 3.6-6.0 s (debug) against a 1 s budget: every explicit Ranked query collects the corpus again and reserves a new generation before reusing the index. The perf test reports it as known_gap with a 15 s ceiling.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A warm Ranked query on unchanged checkouts skips corpus collection and meets the documented first-page budget
- [x] #2 A changed checkout still re-collects; tests prove both paths
- [x] #3 tests/tui_e2e performance journey asserts the real budget instead of known_gap
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified in gate 22 at 3c27e77f: node scripts/test-all.mjs 6468/0, fmt and clippy clean. Release re-measure still pending in docs/performance.md.
<!-- SECTION:NOTES:END -->
