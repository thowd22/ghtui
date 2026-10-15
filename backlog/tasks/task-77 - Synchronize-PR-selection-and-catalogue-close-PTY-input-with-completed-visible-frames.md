---
id: TASK-77
title: >-
  Synchronize PR-selection and catalogue-close PTY input with completed visible
  frames
status: In Progress
assignee:
  - '@root'
created_date: '2026-10-15 02:10'
updated_date: '2026-10-15 02:23'
labels: []
milestone: m-0
dependencies:
  - TASK-2
  - TASK-4
priority: high
type: bug
ordinal: 6069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The integrated stash-recovery whole gate retained two actual older journey failures: keyboard viewed-file selection ran past its target into the last row, and mouse original-catalogue recovery tried to close a vanished modal through stale geometry. Evidence is target/build/root-stash-recovery-integrated-whole-first.log and exact saved terminal artifacts. Inputs must be based on current completed visible state rather than timing assumptions; preserve all original product, command, storage, Git and cleanup assertions.

## Testing steps

1. Code/provider tests: completed_frame_actions observer contracts exercise already-selected/above/below/missing/ambiguous target rows, actual owned box coordinates, partial versus completed frames, focused cell markers across every byte split, unchanged cursorless proof and resize retirement.
2. Real-binary TUI E2E scenario completed_frame_actions: select an already-selected exact PR file and complete both publications across restart; close nested catalogue notices/warnings with missing, changed-node and empty current inventories. Exercise keyboard and mouse, 120x40/80x24 resize, strict offline gh and independently verified Git/index/refs/worktree/config, exact draft/journal/storage effects and terminal cleanup. Preserve original target/effect assertions.
3. Run node scripts/test-task.mjs completed_frame_actions and cargo test --test tui_e2e completed_frame_actions -- --nocapture with positive nonzero code/PTY counts, plus original viewed_files and repo_catalog_original_recovery journeys. Then cargo test --all-targets, cargo fmt --check and cargo clippy --all-targets -- -D warnings, dependency/backlog audits on one frozen source cohort. Record commands, actual counts and retained failure artifacts; no ignored tests, timeout increases, blanket retries or accepted-by-default snapshots.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Selection checks the actual current completed frame before one navigation input and waits for a new selection result before any next input; already-selected and overshot targets are handled without extra provider reads.
- [x] #2 Catalogue-close mouse actions use fresh current owned geometry after nested overlay closure; no retry-on-timeout or unconditional click can toggle a different visible action.
- [x] #3 Meaningful code tests and deterministic real-binary keyboard/mouse PTY regressions pass with independent strict gh, Git/storage and terminal cleanup assertions; no timeout increases or disabled assertions.
- [x] #4 Affected focused selectors and full all-target tests, fmt check and strict Clippy pass on one frozen source cohort and actual evidence is recorded.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [x] #1 Meaningful focused code/provider tests select a positive count and pass; exact commands/results retained.
- [x] #2 Actual-binary keyboard and mouse PTY regression journeys pass and independently assert Git/storage/transport effects and terminal cleanup.
- [x] #3 Coherent all-target tests, fmt check and strict Clippy pass; actual logs and retained negative evidence recorded.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Retain first full negative and inspect exact screen/command artifacts. 2. Reproduce already-selected keyboard overshoot and close-after-nested-overlay stale mouse geometry in deterministic native cases before changing their observer logic. 3. Introduce narrowly scoped complete-frame selection/current-close observers with pure contracts; keep product state and all effect assertions unchanged. 4. Run nonzero checked selectors, affected original journeys, coherent full/fmt/strict gates and preserve negatives; broad foundation dependencies and native macOS remain unfulfilled.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-15 retained Main whole-first negative: 2027 code/provider passed; 660/662 actual PTYs passed, 2 failed, 0 ignored, native117.05s; log target/build/root-stash-recovery-integrated-whole-first.log. Exact viewed-file failure saved target/tui-artifacts/3275078-1792030104847496147/screen.txt shows other.rs selected after repeated Down; genuine deterministic already-selected follow-up native red1pass/1fail retained in root-completed-frame-actions-native-red-first.log. Initial observer correction selected10 code (first iteration8) but actual4PTY2pass/2fail retained in native-green-first/checked-first logs: no visible marker on intermediate progress summary rows, and focused catalogue filter never renews cursorless-only proof. Read actual pinned ratatui-core terminal/render.rs apply_buffer_with_cursor: cell diff flush precedes Show or Hide; crossterm executes each visibility sequence. Extended existing single VT100/byte marker tracker with distinct last_cells/cells_end for focused cell observations, preserving old cursorless-only last/end exactly. Eight selection/owned-geometry plus two split/partial focused-marker code contracts now GREEN10/10; actual both-input preselected-file and nested-catalogue-close tests GREEN4/4,0ignored2.69s, checked log root-completed-frame-actions-checked-second.log and positive session journal target/test-tmp/ghtui-test-coverage-jIrbB3/journeys.jsonl. No product behavior, timeout, transport, raw Git/storage, target or cleanup assertions relaxed. Current full coherent source gates pending; Task77 remains In Progress.

Final Main verification2026-10-15: immutable36 code/test paths on9012 baseline, manifest target/build/root-stash-completed-frame-integrated-freeze-v2-20261015.json, hashes independently unchanged through all gates. Actual checked node scripts/test-task.mjs completed_frame_actions:10code/4PTY,0failed/ignored2.69s; journal jIrbB3 exactly10/10 exited nonpanicking cleanup_completed sessions,10keyboard/5mouse. Original viewed_files PTYs2/2 pass1.99s; original repo_catalog_original_recovery16/16 pass4.81s. Rechecked stash_recovery15code/12PTY, actual14 positive sessions BgMHkb; native_mutable_diff33code/2PTY, actual2 positive sessions BD5BYb. Actual cargo test --offline --locked --all-targets GREEN2037code/provider (including318strictGH) +666PTY,0failed/ignored90.92s native; cargo fmt --check0; cargo clippy --offline --locked --all-targets -- -D warnings0(37.27s); dependency audit86sources/10300files/108adaptations/117registry0. Logs target/build/root-stash-completed-frame-*-final-first.log plus original focused names. This verifies all narrow Task77 AC/DoD on Linux; formal foundation dependencies2/4 and native macOS remain open, so task stays In Progress without broader foundation or project completion claims. Earlier first-whole and deterministic/observer negatives retained, no retry-until-green/timeout waiver.
<!-- SECTION:NOTES:END -->
