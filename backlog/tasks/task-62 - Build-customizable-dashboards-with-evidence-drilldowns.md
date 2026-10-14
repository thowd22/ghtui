---
id: TASK-62
title: Build customizable dashboards with evidence drilldowns
status: To Do
assignee: []
created_date: '2026-10-12 23:13'
labels:
  - delivery
  - e2e
milestone: m-5
dependencies:
  - TASK-61
  - TASK-51
  - TASK-35
  - TASK-34
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: medium
type: feature
ordinal: 62
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The user wants an easy overview across the repos they care about. Dashboard widgets should surface review/CI bottlenecks and lead to the exact search, PR or run rather than disconnected numbers.

## Testing steps

1. Code/provider tests: Test widget aggregation/sorting, filters, drilldown identities, saved layouts and empty/partial states.
2. Real-binary TUI E2E scenario `dashboards`: Build/save/reopen a dashboard through keys/clicks, inspect a failing-run or review-age widget, and assert its contributing view matches the displayed metric. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e dashboards -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Saved dashboards contain review queues, stale PRs, unresolved threads, failing Actions and defined metric trends.
- [ ] #2 Scope/window/layout preferences persist and every displayed action/metric is clickable and keyboard reachable.
- [ ] #3 Actor widgets, colors and partial-data indicators agree with underlying records.
- [ ] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [ ] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->
