---
id: TASK-63
title: Add a readable activity and notification inbox
status: To Do
assignee: []
created_date: '2026-10-12 23:13'
labels:
  - delivery
  - e2e
milestone: m-5
dependencies:
  - TASK-62
  - TASK-17
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: medium
type: feature
ordinal: 63
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Users need to know who changed a PR or workflow without repeatedly reopening each repository. Activity must retain scope and support useful notification state.

## Testing steps

1. Code/provider tests: Test notification paging/reasons, dedup, read/unread/subscription commands and permissions.
2. Real-binary TUI E2E scenario `notifications`: Open activity, filter and mark a notification through keys/clicks, follow it to its PR/run, then assert actor and server state across refresh/restart. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e notifications -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Activity and GitHub notifications show actor/login/avatar, reason, repo/resource and freshness.
- [ ] #2 Read/unread/subscription controls use verified gh/API capabilities and update related queues.
- [ ] #3 Polling is bounded and deduplicates changes without claiming push realtime.
- [ ] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [ ] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->
