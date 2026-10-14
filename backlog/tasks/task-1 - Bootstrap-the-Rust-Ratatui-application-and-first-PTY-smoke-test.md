---
id: TASK-1
title: Bootstrap the Rust Ratatui application and first PTY smoke test
status: In Progress
assignee:
  - '@foundation'
created_date: '2026-10-12 23:06'
updated_date: '2026-10-12 23:56'
labels:
  - foundation
  - e2e
milestone: m-0
dependencies: []
references:
  - 'https://ratatui.rs/concepts/application-patterns/'
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: feature
ordinal: 1
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The project is currently a research backlog with no application. Establish a small binary/library package and usable terminal shell so all later work has a real executable to exercise. Target macOS and Linux only; gh and git remain external dependencies.

## Testing steps

1. Code/provider tests: Verify argument parsing, shell transitions, empty-state layout, and cleanup on injected initialization failure.
2. Real-binary TUI E2E scenario `bootstrap`: Launch the actual binary, open Help and a view using keys, quit, then repeat by clicking visible controls after resizing; assert clean exit and terminal restoration. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e bootstrap -- --nocapture` and verify a nonzero selected test count; run the relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. The initial framework is delivered by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The application builds on macOS and Linux and exposes version/help plus an Inbox, Repository, Search, Actions, and Dashboard shell.
- [x] #2 Startup, normal quit, startup failure, and panic restore terminal, cursor, alternate-screen, and mouse state.
- [x] #3 A first real-binary PTY test exists in this task; all exposed shell actions work by mouse and keyboard.
- [x] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [x] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [x] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [x] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Build a single Rust package with typed five-view shell and shared layout-derived mouse/keyboard actions. 2. Guard raw mode, alternate screen, mouse, cursor and panic cleanup. 3. Create a real portable-pty/vt100 startup/help/view/resize/quit suite with terminal-attribute checks. 4. Run focused and full checks and record observed results; keep macOS evidence explicit.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented Rust/Ratatui/Crossterm shell, five views, CLI options, typed keyboard/mouse actions, terminal RAII/panic restoration and startup/help/resize/quit PTY journeys. Actual verification: rustc --edition=2021 --test src/cli.rs -o /tmp/ghtui-cli-tests && /tmp/ghtui-cli-tests --nocapture passed 2/2 bootstrap argument tests on Linux. cargo test --lib bootstrap could not resolve index.crates.io; cargo test --offline --test tui_e2e bootstrap -- --nocapture exits 101 before compilation because crossterm is absent from local registry. No PTY tests ran; required all-targets/fmt/clippy gates and macOS evidence remain outstanding. Task remains In Progress with unchecked AC/DoD.

Registry transport was recovered using pinned official Git source packages; actual full ghtui executable now compiles on Linux. Focused verification: cargo test --offline --lib bootstrap selected 8 tests, all passed (CLI, shell/action parity, sanitization/layout, injected raw/screen initialization faults and panic unwinding); cargo test --offline --test tui_e2e bootstrap -- --nocapture selected 3, all passed (keyboard/mouse help/palette/five views, 120x40→80x24, invalid startup configuration, alternate-screen/cursor/mouse cleanup and all termios flags/control characters). Initial failing keyboard journey correctly exposed adjacent ESC+key as Alt; harness now synchronizes on overlay closure. First full suite passed 103 tests; subsequent added tests require latest global rerun. Linux evidence is actual; native macOS build/PTY validation remains pending CI and is not claimed. Temporary absolute vendor paths must be made portable before distribution.
<!-- SECTION:NOTES:END -->
