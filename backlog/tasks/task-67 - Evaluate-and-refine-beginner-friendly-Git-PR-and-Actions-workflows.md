---
id: TASK-67
title: Evaluate and refine beginner-friendly Git PR and Actions workflows
status: To Do
assignee: []
created_date: '2026-10-12 23:13'
updated_date: '2026-10-14 16:38'
labels:
  - delivery
  - e2e
milestone: m-5
dependencies:
  - TASK-66
  - TASK-35
  - TASK-34
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: medium
type: feature
ordinal: 67
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The product must be usable by someone with little Git knowledge. A polished feature checklist does not prove that they understand the graph, comparison modes or next action.

## Testing steps

1. Code/provider tests: Verify revised labels/help/actions and regression snapshots, including no-color/initials fallback and narrow layouts.
2. Real-binary TUI E2E scenario `novice_usability`: Encode each evaluated journey as a real-binary keyboard and mouse regression, asserting correct outcome/selection after terminology or layout changes. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e novice_usability -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Evaluate locate-my-PR, explain-merge, compare-saved-versions, browse-old-file, resolve-conflict and find-failing-step tasks.
- [ ] #2 Record user success/time/wrong turns and confusing terms; revise interaction labels/layout from the evidence.
- [ ] #3 Required tasks work without memorizing shortcuts using mouse and discoverable keyboard navigation.
- [ ] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [ ] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-14 distribution now uses one macOS/Linux Bash bootstrap (doc-12). Include understandable gh/git prerequisites, first-run/PATH guidance and update terminology in this task's existing novice guidance criteria; actual shipped-script installation/usability and recovery are evaluated in TASK-68 after this task. Do not wait for a completed installer here: TASK-68 already depends on TASK-67. Dependencies/status/AC unchanged; no novice study or installer test executed by research.
<!-- SECTION:NOTES:END -->
