---
id: TASK-1
title: Bootstrap the Rust Ratatui application and first PTY smoke test
status: In Progress
assignee:
  - '@foundation'
created_date: '2026-10-12 23:06'
updated_date: '2026-10-16 23:33'
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

2026-10-16 Root reviewed actual project native failure artifacts/long-path header compaction and full original prepared Source identity document. Approve NEW exactV166 private task-local project.rs correction (separate commit from Metrics): preserve both named keyboard/mouse root+subdirectory journeys and full raw repository/HEAD/index/remotes/Cargo.toml/no-gh proof. Open EXISTING Source identity by actual key/click, prove complete exact actual path in SAME owned prepared scrollable document (including legitimate wrapping), close read-only, then unchanged Tree/Overview/resize flow. No ellipsis/prefix/substring substitute for complete path, no sharedhelper/timeouts/production behavior changes. Existing meaningful project code contract remains mandatory; strengthen long-path complete-source/no-effect code if needed. Freeze exactbefore/output vectors; Root sole capped checked project_git_init plus impacted/full/fmtstrict; no currentRoot/Main/liveverifier edit or acceptance claim.

Project fixture portability corrective within existing Task1/root-subdirectory read-only contract: NEW isolated exactRootV214 private successor, only project_contracts.rs and tui_e2e/project.rs (optional tiny testprivate fixture child) prepare temporary REAL normal Git repos populated with current Cargo.toml and actual source bytes, no clone/network/named remote/globalconfig. Retain original1code/2native names, meaningful explicit noRemote, actualroot+src discovery, full Source identity prepared document/wrapping, keymouseTree/Overview/resize/quit/TTY. Exercise both unborn and committed/dirty/index/source/remotes/config states in independently owned fixture; compare exact HEAD/allrefs/rawindex/status/Cargo/source/config/remotes and emptystrictGH. Preserve genuine current longpathSource helper and allothercurrent regressions. Do not weaken assertion to discoveredouterworkspace root, ignore missing.git, addcanonical.git, remove paths, or claimplannedcoverage. Rootalone actual code+pairedPTY/projectchecked/impacted/full/fmtstrict underexistingcaps.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented Rust/Ratatui/Crossterm shell, five views, CLI options, typed keyboard/mouse actions, terminal RAII/panic restoration and startup/help/resize/quit PTY journeys. Actual verification: rustc --edition=2021 --test src/cli.rs -o /tmp/ghtui-cli-tests && /tmp/ghtui-cli-tests --nocapture passed 2/2 bootstrap argument tests on Linux. cargo test --lib bootstrap could not resolve index.crates.io; cargo test --offline --test tui_e2e bootstrap -- --nocapture exits 101 before compilation because crossterm is absent from local registry. No PTY tests ran; required all-targets/fmt/clippy gates and macOS evidence remain outstanding. Task remains In Progress with unchecked AC/DoD.

Registry transport was recovered using pinned official Git source packages; actual full ghtui executable now compiles on Linux. Focused verification: cargo test --offline --lib bootstrap selected 8 tests, all passed (CLI, shell/action parity, sanitization/layout, injected raw/screen initialization faults and panic unwinding); cargo test --offline --test tui_e2e bootstrap -- --nocapture selected 3, all passed (keyboard/mouse help/palette/five views, 120x40→80x24, invalid startup configuration, alternate-screen/cursor/mouse cleanup and all termios flags/control characters). Initial failing keyboard journey correctly exposed adjacent ESC+key as Alt; harness now synchronizes on overlay closure. First full suite passed 103 tests; subsequent added tests require latest global rerun. Linux evidence is actual; native macOS build/PTY validation remains pending CI and is not claimed. Temporary absolute vendor paths must be made portable before distribution.

SOURCE-ONLY approved Project corrective frozen: target/build/graph-parallel-project-v43-current-corrective-sourceonly-v1-20261016.json SHA28e760089ac67f202becdf3ae87d86868513c69c4042bf00fc6a0cfb9e6ebc4a. Exact V1661294 baseline/output,1 test-only path; separate Metrics delta excluded. Existing keyboard/mouse root+src sessions now open genuine Source identity keyI/click and prove ENTIRE exact prepared Local repository path in one NEW completed owned wrapped document before pure Close and unchanged Tree/Overview/resize/zero-gh/Cargo.toml/index/remotes/TTY oracles. Negative actual compact-header sessions preserved. Fmt syntax/patch/hash checks only; all code/native UNRUN. Root alone executes gates; no task/platform completion claim.

Root FULL-read ProjectV43 SMALL13585B patch and actual prepared source identity raw-path construction/key/Close ownership. Independent fullreadonlyauth PASS manifest28e76008/patchabc19810, all1294baseline/output/current,1owned1293unowned/artifacts. Exact full escaped original root path remains mandatory in SAMEcompletedownedDoc via existingbounded adjacent literal; header compaction no longer mistaken for source identity. All oldnames/root+subdir/noGH/rawGit/config/index/remotes/Cargo/TTY effects retained. AllUNRUN/notstage whileW9early12409live, no production/sharedhelper or userMain edits.

2026-10-16 actual V170 checked node scripts/test-task.mjs project_git_init PASS1code+2real-binaryPTY/0ignored/key+mouse/exact2positiveunique/allcleanup. Same completed owned identity document proves ENTIRE escaped private root via bounded adjacent literal, then pure Close and original Tree/root/subdir80column/rawGit/noGH/TTY assertions. No compaction/assertion/production weakening. Report25f8e028/log9e7af28a/coverage6622060d; fmt/strictClippy/deps0. Full task acceptance unchanged/open.

Root V211 unfiltered actual gate stopped at project_contracts: discover CARGO_MANIFEST_DIR nested canonical no.git correctly finds outer Main root, contradicts test expected canonicalSource root. Terminal13:38:06.876Z/report863df1e00d7186588dd37ad4830a91c06aca03fc61d40ead2b6b638987f86ca2, original21154closed1/scopeinactive-dead. All2922library plus examined prior code/integration cohorts passed; project1FAIL means later native binary NOTRUN, whole-onlyjournal0/validatedfalse, notborrowed earlier focused sessions. Current project code/native fixtures incorrectly depend on developer checkout metadata; source archives/canonical source/CI must work without adding canonical.git. Root actual Main normalmetadata check remains separate, automatic discovery tests must own realtemporary normalGitproject. No productGitsemantic change or waiver.

Root V218 ACTUAL project_git_init checked gate PASS: 1 code contract and 2 real-binary native names, 8 nonpanicking exited cleanup-complete sessions with actual keyboard/mouse. Owned actual Cargo/src Git fixtures cover unborn and committed dirty index/worktree, root and src launch, entire prepared long path identity, exact source/index/HEAD/refs/status/config/no-remote preservation and no GH. Code additionally exercises unborn-staged/clean committed states. Current source V21779e92a55 includes cumulative V74 fixtures; report target/build/root-members-v218-private-20261016.json SHA3151e0d19e9f99d47e4df75f319ac000263ffd56fd0bf47fd2a55a9735ad0d3e; journal /var/tmp/ghtui-verification-20261016-kRLcXc/ghtui-test-coverage-ubv6dj/journeys.jsonl SHA5db0050241bc103dea57c63e02c19fe7b07ba18df2e67fcd5579f26c191826ba. Entire candidate remains NEGATIVE on separate Members code/native/Clippy; full unfiltered gate not run for V217, no TaskDone/Mac claim.

2026-10-16 USER SEQUENTIAL RESET: all three active sub-agents interrupted; exact owned ghtui-verify-coherent-feedback-v307-20261016.scope stopped with SIGTERM, original exec 1330 exit143 and scope inactive/dead. V307 stopped during test-inventory compilation: no completed application tests or PTY result; unfinished report/log/source preserved. Autonomous goal paused. See backlog doc-13 (Sequential delivery reset and current-state audit) for actual backlog counts, V304 failing evidence, private-source preservation inventory and one-worker/one-feature/testing-after-implementation policy. No task criteria/status changed and no product completion claimed; do not restart agents or discard target private source. First sequential workstream is consolidation/stabilization of one existing baseline, not another new feature/design branch.
<!-- SECTION:NOTES:END -->
