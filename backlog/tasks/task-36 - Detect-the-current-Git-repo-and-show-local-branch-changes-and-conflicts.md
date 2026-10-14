---
id: TASK-36
title: Detect the current Git repo and show local branch changes and conflicts
status: In Progress
assignee:
  - '@codex'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-13 03:48'
labels:
  - localgit
  - e2e
milestone: m-6
dependencies:
  - TASK-11
  - TASK-31
  - TASK-34
  - TASK-35
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-6 - Local-Git-workspace-and-operation-contract.md
priority: high
type: feature
ordinal: 36
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Running ghtui inside a repository or any subdirectory should immediately reveal local work, even without gh login or a GitHub remote. Users need an understandable overview of what is saved, staged, changed, unpushed or conflicted.

## Testing steps

1. Code/provider tests: Test CWD detection, real Git status/branch relations and diff modes, conflicts, worktrees and absent remote/auth.
2. Real-binary TUI E2E scenario `local_workspace`: Launch from a nested directory, inspect local commits and all diff modes via keys/clicks, open a conflict indicator, and verify no implicit fetch/checkout and correct fixture state. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e local_workspace -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Startup detects repo/subdirectory/worktree and opens its workspace with branch/HEAD/upstream and local commits.
- [ ] #2 Working, staged, untracked and current-branch diffs plus conflict/sequencer indicators are distinct and clickable.
- [ ] #3 No-remote/no-auth, unborn/detached/shallow states remain useful and local operations remain available.
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
1. Preserve the installed-Git read-only discovery boundary already verified by local_workspace journeys, while ensuring explicit --repo only scopes GitHub and never removes access to the current local checkout. Discover in the background without stealing an explicit GitHub launch focus; Repository navigation retries missing local state through the same keyboard/mouse action and worker. 2. Explain an unborn checkout when Compare is requested rather than preparing nonexistent saved-version endpoints. 3. Add meaningful action-state unit tests and fresh real-binary keyboard/mouse launch journeys using explicit CLI --repo, nested local directories, dirty/staged state, narrow resize, no authentication and independently unchanged HEAD/index/working bytes/refs plus zero gh calls. 4. Run checked local_workspace coverage, full all-target tests, formatting and strict Clippy, record exact results. This is an eligible integrated slice based on existing compiled/tested shared Git/diff/identity providers; remaining broad dependency acceptance and platform/state coverage stay open, and no task Done is implied.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Backend preparation is available through shared src/git.rs, src/diff.rs and src/repository_view.rs. Real generated-Git fixture tests cover canonical status/diff/index/objects/refs, ordered DAG parents and immutable tree reads, staged patches, hook rejection, branch/tag guards, local-bare push/fetch, stash/worktrees, conflicts/abort, reflog rescue and bisect reset. Isolated offline source validation uses /tmp/ghtui-git-validation.AsAVCb/Cargo.toml: 8 code/model unit tests + 19 real Git integration tests passed (0 ignored), cargo clippy --offline --manifest-path /tmp/ghtui-git-validation.AsAVCb/Cargo.toml --all-targets -- -D warnings passed; rustfmt --check own modules/tests passed. Prepared backend only: dependencies, full UI, mouse/keyboard PTY and full-project test/fmt/clippy verification remain required. No task completion claimed.

Typed Repository startup/status/diff/graph/tree/conflict UI now has real-binary coverage. Checked CARGO_NET_OFFLINE=true node scripts/test-task.mjs local_workspace selected 5 code/provider tests +4 PTY tests, all passed with keyboard/mouse input and independent effect assertions; journal /tmp/ghtui-test-coverage-1Q9Fup. Both read-only journeys start from a nested checkout, show staged/unstaged changes, choose an old saved version in graph, lazily expand/open its saved file, and preserve HEAD, exact index bytes, working contents and status. Separate actual merge-conflict PTY journeys inspect stage1/2/3 object IDs without resolving or mutating them. No gh requests/fetch/checkout occur. Combined worktree-vs-HEAD mode, comparison picker and full mutation workflows remain separate outstanding work; no Done claim.

Root integrated startup/accessibility correction: explicit --repo scopes GitHub without suppressing local checkout discovery. Local state prepares in the background without stealing explicit Inbox focus, and keyboard/mouse Repository navigation retries empty state through the same worker action. Compare on an unborn checkout displays a read-only explanation instead of preparing invalid HEAD endpoints. Added two meaningful App contracts and two real-binary local_workspace journeys, each using fresh saved/dirty/staged and unborn nested checkouts with explicit CLI repository, narrow resize, overlay capture and independent HEAD/refs/index/working-byte preservation plus zero gh calls. Source formatted; focused execution is queued behind the Actions artifact owner shared Cargo gate. These new journeys are not yet verified; do not mark any acceptance criterion Done from prepared source.

Checked local_workspace gate now passed 7 code/provider contracts and 6 actual compiled-binary PTY journeys, zero failed/ignored, genuine keyboard/mouse input journals. Command: CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true node scripts/test-task.mjs local_workspace; log target/build/root-local-workspace-checked.log, artifacts /tmp/ghtui-test-coverage-8a5e8o. New explicit --repo journeys each prove nested dirty/staged checkout access with explicit Inbox focus retained, 80x24 resize, zero gh calls and unchanged HEAD/refs/exact index/working bytes, then a fresh unborn fixture proves useful staged diff plus Compare explanation/overlay capture without a saved ref or mutation. First run failed on an incorrect test expectation from Branch instead of Overview mode; corrected to actual saved version: none yet and fully reran. Repository renderer owner independently confirmed and corrected loading-shell glyph residue with full-area Clear and negative-control buffer contract; that separate latest change awaits full regression. Broader local states/dependencies/platform validation remain open; not Done.

Fresh full Linux checkpoint passed: CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test --offline --locked --all-targets: 288 meaningful code/provider contracts (including 90 strict provider contracts) and 114 actual compiled-binary PTY journeys, zero failures/ignored. cargo fmt --check passed; cargo clippy --offline --locked --all-targets -- -D warnings passed (8.07s). Logs target/build/root-all-targets-checkpoint.log, root-fmt-checkpoint.log, root-clippy-checkpoint.log. Node task-runner negative self-tests passed3/3; dependency integrity audit passed86 upstream roots/10300 files/108 manifest patches/117 registry packages; Backlog doctor clean72tasks. This includes new comparison2, remote-search2, raw-ref identity2, explicit-repo local/unborn2, Actions recovery2 and artifact2 PTY;18 native Actions journeys total. README reflects this actual green evidence only. Native macOS/live disposable-host/release and remaining product acceptance stay open; full product not Done.
<!-- SECTION:NOTES:END -->
