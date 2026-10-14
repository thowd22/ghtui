---
id: TASK-64
title: Audit full PR Actions Git and interaction coverage
status: To Do
assignee: []
created_date: '2026-10-12 23:13'
labels:
  - delivery
  - e2e
milestone: m-5
dependencies:
  - TASK-60
  - TASK-56
  - TASK-44
  - TASK-27
  - TASK-63
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: feature
ordinal: 64
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Broad feature promises need concrete release evidence. Audit native actions, host/platform gaps, identity/color coverage, and mouse/keyboard access before claiming complete support.

## Testing steps

1. Code/provider tests: Validate manifest completeness and test-key coverage, reject an omitted action/input path, and verify capability/permission cases.
2. Real-binary TUI E2E scenario `parity_audit`: Enumerate and exercise enabled/disabled controls across PR/Actions/local Git views with keys/clicks and fixture effects; assert native coverage and explicit gaps. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e parity_audit -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 PR and Actions inventories map every row to implemented native behavior, tests and limitations.
- [ ] #2 Local Git operation coverage and all exposed action mouse/keyboard paths are enumerated and verified.
- [ ] #3 Every actor view uses shared identity/avatar fallback and every Git/CI state follows the semantic theme.
- [ ] #4 Opt-in disposable-repo verification records real host compatibility; browser links do not count as native support.
- [ ] #5 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [ ] #6 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->
