---
id: doc-4
title: Code and TUI end-to-end testing contract
type: specification
created_date: '2026-10-12 23:01'
updated_date: '2026-10-13 00:32'
---
# Code and TUI end-to-end testing contract

User requirement: every task includes a testing step and uses a full code and TUI end-to-end framework, so verification happens during implementation. This applies to infrastructure, providers, indexing, metrics, distribution, and UX tasks as well as visible features.

The real Rust binary, shared provider fixtures, and portable-pty/vt100 framework are implemented and running. This contract remains the required verification baseline as feature coverage expands; task notes distinguish executed results from planned scenarios.

## Framework delivered first

The first task creates the Rust binary/library scaffold and a real-binary PTY startup/help/quit test. The next task extends it into the shared framework; subsequent tasks depend directly or transitively on that framework. Bootstrap carries its own PTY test, avoiding a circular dependency.

Use [portable-pty](https://docs.rs/portable-pty/latest/portable_pty/) to run the compiled ghtui binary with a real pseudo-terminal, and [vt100](https://docs.rs/vt100/latest/vt100/) to interpret the output stream into the displayed screen. Tests send actual key bytes, paste sequences, and terminal mouse reports (including SGR press/release, wheel, and drag), resize the PTY, assert visible text/cells/focus/state, and verify process exit and command effects.

[Ratatui TestBackend](https://docs.rs/ratatui/latest/ratatui/backend/struct.TestBackend.html) plus [insta snapshots](https://ratatui.rs/recipes/testing/snapshots/) cover deterministic widget layouts and styles. These complement PTY tests; snapshots alone do not prove event handling, subprocess execution, or terminal restoration.

## Test layers

1. Code tests cover domain transitions, parsers, comparison/query semantics, storage, ranking, permission/head checks, and metrics with meaningful expected outcomes. Generated Git fixtures exercise actual Git rather than mocked assumptions.
2. Provider contract tests drive the same process adapter used by production. A compiled fake-gh executable validates exact argv/stdin/host/repo, returns scripted JSON/status/delays, records commands, and rejects unexpected requests. This executable is on the test child's PATH only. Dedicated runner fixtures can simulate missing git or stuck children.
3. Real-binary PTY E2E tests exercise the user's navigation or action, assert the screen, and independently inspect command journals, SQLite/index state, Git objects, or exported files. Every task names at least one scenario here.
4. Complete journeys cover inbox -> PR -> diff -> draft review -> submit -> refreshed timeline; repository -> graph -> compare -> tree -> history; org -> selected corpus -> search -> correct revision/comment -> metric drilldown; local status -> stage -> commit -> PR; and Actions indicator -> run -> log/artifact/approval.
5. Opt-in live tests verify actual host contracts in an explicitly configured disposable repository. Offline E2E is required for every task; a live external service is not required for every test run. Fixture success cannot be presented as proof of live GitHub API compatibility.

Use temp directories for configuration, caches, fixture repos, and HOME-equivalent child directories through the process builder; do not repurpose the shell's HOME variable. Strip real credentials from the test child, set an inert host, disable external network paths, and use fake gh by default. A fixture mutation must never reach real GitHub.

## Required shared helpers

- Launch the actual test-built binary and packaged/release binary through the same journey runner.
- send_key, paste, click_visible_action, wheel, drag, resize, wait_for_screen(predicate, deadline), assert_cells, assert_focused_view, inspect_command_journal, restart_with_same_cache, and quit_and_assert_restored_terminal. Mouse helpers write actual terminal reports; they must not call application handlers directly. Locate clickable targets from the observed screen or independently specified fixture coordinates.
- Real Git fixture builders for linear/divergent/merge/root histories, renames, binary files, non-ASCII/unusual paths, shallow clones, worktrees, and rewritten refs.
- Scripted gh scenarios for pagination, partial search, permissions, missing fields, timeouts, rate limits, outdated heads, fork PRs, reviews, merge queues, and cancelled writes.
- Fixed test dates where assertions need them, seeded data, controllable provider delays, and isolated resources so parallel tests do not interfere.
- Screen synchronization based on expected state/command completion; no arbitrary sleep-based readiness checks.
- Bounded timeouts, cleanup of all child processes, and artifacts on failure: screen text/style snapshot, raw terminal transcript, sanitized subprocess journal, stderr, and fixture seed.
- Wide 120x40 and narrow 80x24 scenarios plus resize, Unicode, no-color, empty, loading, and failure coverage where applicable.

The terminal parser can assert emitted alternate-screen/cursor cleanup. OS terminal mode restoration requires checking PTY terminal attributes on supported systems; parser state alone cannot prove raw mode was restored. Keep platform-specific checks explicit.

## Every task's testing steps

Each task description names:
1. Code/provider test input and expected outcome.
2. A PTY scenario using the real binary, including key sequence or user journey and both screen/effect assertions.
3. The command to run the focused E2E scenario and the full regression suite.
4. Repeat the relevant journey with mouse input in a fresh fixture and assert equivalent screen/domain/command outcomes, including after resizing. Every exposed action must have both mouse and keyboard coverage. Check overlays, disabled buttons, and stale hit regions so one click cannot trigger an unrelated or duplicate action.

Test names contain the task's stable scenario key. During development run:
- cargo test --lib <scenario-key> for applicable code tests.
- cargo test --test provider_contracts <scenario-key> for applicable adapter tests.
- cargo test --test tui_e2e <scenario-key> -- --nocapture for its actual TUI journey.
- cargo test --all-targets before completing any task.
- cargo fmt --check and cargo clippy --all-targets -- -D warnings before handoff.

The framework must ensure filters really select tests: a checked runner or coverage audit fails if a task's key has zero code/provider tests or zero PTY tests or a missing mouse/keyboard interaction path for the task. A command that runs zero tests is not validation.

Every task also has test acceptance criteria and an unchecked testing Definition of Done. Add failure/edge scenarios pertinent to the feature instead of generic assertions that the app starts. Even configuration, benchmarking, and packaging changes must exercise their observable effect through the TUI.

Record the actual command, selected test count, result, and relevant artifact paths in task notes when implementing. Tests remain unrun and criteria unchecked while a task is merely planned. Do not mark a task complete with only a manual smoke test, an ignored E2E test, a placeholder assertion, or an automatically accepted snapshot.

## CI and evolving coverage

Run offline code/provider/PTY suites locally as each task is built. A dedicated near-end GitHub CI/CD task automates these suites and release publishing after a remote is configured. Run the required PTY suite on both Linux and macOS. Product support is macOS and Linux only; Windows and ConPTY are out of scope. Infrastructure limitations must fail or visibly gate the job rather than silently skip required E2E.

Upload failure artifacts and report task-key coverage. A deliberate wrong fixture response or missing expected screen text must make the test fail; validate the harness with a negative self-test. Re-run impacted journeys as each task is built and the complete suite before task completion.

Performance scenarios use release builds, representative fixed fixtures, and separate timing samples from deterministic correctness checks. Live GitHub latency is measured separately. Availability and correct state remain tested when a provider is slow.

## Mouse and keyboard contract

Full mouse support is a user requirement. A visible action cannot be decorative or keyboard-only. Keyboard navigation remains complete when mouse reporting is unavailable. Tests enumerate rendered actions and assert layout hit regions and keyboard dispatch agree with the same action identity. Exercise click-to-focus, hover-panel wheel routing, tabs, tree expansion, list/file/graph selection, scrollbars, contextual menus, dialogs, text-field caret positioning, and multiline review drag selection. Narrow layouts and overlays must have correct targets; no click-through. Text selection must preserve canonical diff coordinates. Quit/editor handoff restores mouse reporting along with other terminal state. See [Crossterm mouse events](https://docs.rs/crossterm/latest/crossterm/event/enum.MouseEventKind.html) for supported terminal event types.

## Identity image and color verification

Use known small avatar fixtures and test identity, async loading, cache, image decoding and pixel/protocol bounds in code tests. PTY tests verify username/initials fallback, image-protocol negotiation/emission where simulated, layout occupancy, mouse hit regions and cleanup; a text terminal parser cannot validate displayed pixels for Kitty/iTerm/Sixel. Add supported-terminal image rendering smoke evidence before claiming image-protocol compatibility. Keep fallback E2E deterministic on macOS and Linux.

Assert semantic cell styles with TestBackend and readable labels with the PTY parser. Verify no-color/light/dark modes, graph legends and selection overlays. Distinguish author from committer and never infer a verified GitHub identity from a local display-name match.
