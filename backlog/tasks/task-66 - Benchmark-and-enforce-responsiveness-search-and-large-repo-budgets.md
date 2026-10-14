---
id: TASK-66
title: Benchmark and enforce responsiveness search and large-repo budgets
status: To Do
assignee: []
created_date: '2026-10-12 23:13'
labels:
  - delivery
  - e2e
milestone: m-5
dependencies:
  - TASK-65
  - TASK-49
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: feature
ordinal: 66
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Super fast must be measured across the UI and its background workloads. Rust, async code or vendor benchmarks alone do not establish responsiveness.

## Testing steps

1. Code/provider tests: Benchmark fixture size/timing/statistics and guard bounds; verify relevance/correctness alongside optimization.
2. Real-binary TUI E2E scenario `performance`: Drive large-fixture journeys by keys/clicks in the PTY runner while delayed providers/indexing run, assert correctness and collect latency/resource results against documented budgets. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e performance -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Record cold/warm startup, input-to-render/render cost, cached diff, first search page, RSS/CPU/disk and subprocess counts.
- [ ] #2 Use documented release fixtures/hardware and p95 budgets from the architecture; indexing/network timing is separate.
- [ ] #3 Large histories/diffs/corpora/avatars/Actions logs remain bounded and responsive while work runs.
- [ ] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [ ] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->
