---
id: TASK-7
title: Persist host repository scopes keymaps and preferences
status: In Progress
assignee:
  - '@codex'
created_date: '2026-10-12 23:06'
updated_date: '2026-10-13 02:42'
labels:
  - foundation
  - e2e
milestone: m-0
dependencies:
  - TASK-2
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: feature
ordinal: 7
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Cross-repo work needs explicit saved repository sets and consistent input preferences. Users must understand the active host and scope without editing a large config before first use.

## Testing steps

1. Code/provider tests: Test defaults, round trips, invalid values, key collisions, and host/scope partitioning.
2. Real-binary TUI E2E scenario `configuration`: Change a repository set or keymap in the TUI using keys and clicks, restart, assert persistence, then load invalid config and verify recovery. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e configuration -- --nocapture` and verify a nonzero selected test count; run the relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. The initial framework is delivered by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Validated configuration persists repository sets, exclusions, refresh policy, themes, and keymaps.
- [ ] #2 Current host, identity, scope, and local mappings are visible; changes do not mix cached identities.
- [ ] #3 Invalid config has a recoverable diagnostic; defaults permit immediate use from any directory.
- [ ] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [ ] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Implement validated TOML preferences, repository sets/exclusions, host/theme/refresh/keymaps and explicit configuration paths.
2. Add atomic persistence and focused code tests.
3. Integrate settings/config-directory controls into the real app once foundation is available.
4. Run keyboard/mouse PTY configuration/restart/recovery scenarios and all required checks before completion.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Refined dependencies during authorized parallel implementation: configuration parsing and persistence depend on the executable/test framework, while gh identity resolution integrates through its adapter independently.

Authorized parallel preparation: src/config.rs and focused tests are written. Runtime/TUI integration and test execution await the shared framework and missing external crate downloads. No acceptance criteria are marked verified.

Actual cached-only source validation passed: cargo test --offline --manifest-path /tmp/ghtui-config-validation.Nihplk/Cargo.toml --all-targets: 3 tests passed, 0 ignored. Foundation integrated config CLI/settings and wrote actual-binary mouse/keyboard/restart PTY scenarios, still unrun because Crossterm/Ratatui are missing. No acceptance criteria marked complete.

Fresh checked node scripts/test-task.mjs configuration passed4 code+4 actual PTY, both input paths, zero ignored, artifact /tmp/ghtui-test-coverage-dc3kzq. Settings mouse journey now observes actual resized80x24 frame before clicking, saves host/scope/theme, independently reads persisted TOML, restarts with same storage and verifies restored settings. Invalid scope preserves saved bytes; custom keys drive actual visible controls. Log target/build/configuration-checked.log; full global gates remain coordinated root checkpoint.
<!-- SECTION:NOTES:END -->
