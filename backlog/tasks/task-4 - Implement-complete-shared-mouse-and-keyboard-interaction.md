---
id: TASK-4
title: Implement complete shared mouse and keyboard interaction
status: In Progress
assignee:
  - '@codex'
created_date: '2026-10-12 23:06'
updated_date: '2026-10-14 09:46'
labels:
  - foundation
  - e2e
milestone: m-0
dependencies:
  - TASK-3
references:
  - 'https://docs.rs/crossterm/latest/crossterm/event/enum.MouseEventKind.html'
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: feature
ordinal: 4
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Ratatui does not automatically make painted buttons interactive. A shared action and hit-region system must ensure every exposed action has both input paths and remains correct as panels move or resize.

## Testing steps

1. Code/provider tests: Check hit-region boundaries, overlay z-order, disabled actions, shared dispatch, and wide/narrow layout action coverage.
2. Real-binary TUI E2E scenario `mouse_keyboard`: Exercise clickable shell/menu/dialog controls, wheel and drag, repeat through keyboard navigation, resize between actions, and assert no click-through or duplicate dispatch. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e mouse_keyboard -- --nocapture` and verify a nonzero selected test count; run the relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. The initial framework is delivered by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All visible actions dispatch one shared typed action through both keyboard and mouse paths.
- [ ] #2 Click focus, tabs, lists, tree toggles, scrollbars, hover-panel wheel, dialogs, text fields, and context menus have defined behavior.
- [ ] #3 Overlays capture input; disabled actions explain their state; resize invalidates old hit regions.
- [ ] #4 Dragging a selectable range and keyboard range selection produce equivalent domain selections.
- [ ] #5 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [ ] #6 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Preserve one typed-action/hit-region semantics for every native surface, with prepared-state drawing and overlay capture. 2. Verify canonical diff keyboard ranges/mouse drag, local-picker dialogs, Actions paging/watch/focus and resizing without stale-hit dispatch. 3. Add task-specific real-binary mouse_keyboard journeys plus code tests for wide/narrow layouts, disabled controls, focus and z-order; retain missing global coverage items as open. 4. Run checked focused tests and complete global test/fmt/clippy gates before any Done claim.

Expose unavailable Actions controls as shared read-only notices rather than dropping their focus/hit regions; show the full wrapped reason in a scrollable overlay that captures input and never queues a write. Clear context-sensitive notice content when identity verification changes.

Investigate and fix actual terminal input suppression after a principal-refresh/resize boundary. New inspector identity PTYs record processed stale Enter/Down then Resize120x40 but subsequent real Help/Commands input is never delivered. Inspect pinned Crossterm Mio source and supported Unix file-descriptor polling feature; verify an unchanged-vendor use-dev-tty backend under the same strict native fixtures, without sleeps, retries or weakened privacy/count/Git assertions. Preserve failing evidence; add buffered-resize regression coverage and run checked input paths plus full test/fmt/strictClippy before recording a green checkpoint.

Source-only shared editor repair handoff: align horizontal viewport to an actual display-cell grapheme boundary and use the same visible boundary for mouse rows, without changing raw UTF8 caret/keyboard semantics. Stage an unapplied editor patch plus meaningful wide/combining/emoji/tab/multiline code contracts and actual Local CreateBranch draft-only keyboard/mouse PTY journeys (80 columns and an exact62-cell field,120 resize), independently checking caret output, scoped draft bytes, zeroGH and unchangedGit. Root owns included source/Cargo; all new tests remain UNRUN until coordinated activation.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Shared typed-action/hit-region prototype covers shell tabs/buttons, help/palette/settings overlay capture, click focus, scrolling, settings text caret/paste/Unicode editing and keyboard parity. Added mouse_keyboard settings Unicode/caret unit scenario. Full tree/range/scrollbar/context-menu interaction remains dependent on later views; no claim of full interaction coverage or task completion. Required PTY verification is blocked before compilation by absent Ratatui/Crossterm dependencies.

Native Actions focus and pure paging/watch fixes are currently integrated, with8 actual Actions journeys passing; canonical staging ranges and mouse drag already have4 passing actual journeys. New local-picker keyboard/mouse verification and shared resize-redraw synchronization are being verified. Existing transitive dependency readiness remains a completion gate under the user-authorized parallel implementation loop; this is actual In Progress work, not full action parity.

Native Actions disabled controls now retain shared clickable/Tab hits via ActionsAction::Unavailable, opening a read-only wrapped explanation instead of composing a forbidden write. Root added scrollable Notice overlay. Prepared task-specific mouse_keyboard_actions_disabled_ keyboard/mouse actual-binary PTY journeys for exact-run selected-job guard, resize, paging, close, zero mutations and unchanged real Git; focused contract now checks all enabled and unavailable paged controls remain reachable. Source pending execution in coordinated shared-target gate; no verification or Done claim.

Prepared shared resize synchronization now requires newly received terminal cursor output addressing the new bottom row plus independently decoded Help/Quit toolbar at that row, bounded to 8 seconds. This replaces the old immediate return that let subsequent clicks use stale frame coordinates. Focused configuration, semantic-colors and project PTY reruns remain pending a concurrent search/index.rs compile fix; no passing redraw or task completion is claimed yet. Runtime now clears private Notice text/scroll and dismisses only Notice overlays when identity becomes unavailable or host/account principal changes; 2 async tests are added, execution pending.

Focused verification after fixes passed local_picker7code+2PTY (GMYYXM), staging14code+6PTY (8Avh5H), configuration4+4 (dc3kzq), semantic_colors5+2 (p5O4ot), project_git_init1+2 (DDmSgj), all artifacts under /tmp/ghtui-test-coverage-<suffix>;2 actual runtime Notice identity privacy units also passed. New Actions disabled-control journeys and full current regression are running. No blanket action parity/Done claim.

Focused redraw/parity reruns passed after actual new-frame resize synchronization: checked configuration4 code+4 PTY (/tmp/ghtui-test-coverage-dc3kzq), semantic_colors5+2 (/tmp/ghtui-test-coverage-p5O4ot), project_git_init1+2 (/tmp/ghtui-test-coverage-DDmSgj), local_picker7+2 (GMYYXM) and staging14+6 (8Avh5H). Each checked runner independently verified actual keyboard+mouse coverage with zero ignored tests. Logs target/build/*-checked.log. Runtime Notice privacy2 async units passed, covering unavailable identity, Settings preservation, and same-login changed account IDs across all4 GH/Actions previous/new workspace combinations. New full-project checkpoint remains pending; this is not a full task completion claim.

Actual shared full checkpoint: root ran cargo test --offline --all-targets with CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target, jobs2/incremental0/dev+test debug0. Log target/build/root-all-targets-checkpoint.log: provider_contracts85/85 passed zero ignored (0.54s), actions_contracts10/10 passed zero ignored (0.17s), and tui_e2e102/102 passed zero ignored (7.66s). This task’s two prepared native keyboard/mouse journeys both executed and passed in that real-binary suite, including strict command effects and checkout preservation. New task-specific selector reruns and current fmt/clippy gates still pending; no Done/complete-parity claim.

Fresh complete local checkpoint now passed258 code/provider+102 actual PTY tests, zero ignored, plus whole cargo fmt --check and strict offline/locked all-target Clippy -D warnings. Native disabled Actions notices are mouse/Tab/Enter accessible, wrapped/scrollable, overlay-captured and read-only; actual2journeys and new notice/pure-page/identity-clear units are green. Exact Git selection/pickers/resize coverage green. Logs root-{all-targets,fmt,clippy}-checkpoint.log. Full action-parity audit and remaining product views still open, so task stays In Progress.

Checked task-specific command passed: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 node scripts/test-task.mjs mouse_keyboard. Selected 3 meaningful code/provider and 2 real-binary PTY tests, all passed zero ignored; 2 App notice/settings +1 Actions unavailable contract and2 native disabled-control PTY1.78s. Runner independently verified successful actual keyboard=true and mouse=true journey records at /tmp/ghtui-test-coverage-m2wn4J/journeys.jsonl. Latest global checkpoint also passed258 code+102PTY, cargo fmt --check and cargo clippy --all-targets -- -D warnings (root). Full task acceptance/dependencies/platform/remaining workflow scope remain open, so no Done claim.

Root ACTUAL buffered resize regression: new checked node scripts/test-task.mjs mouse_keyboard_resize passed2 runtime contracts and4 strict real-binary keyboard/mouse journeys, zero failed/ignored,1.60sPTY, coverage target/test-tmp/ghtui-test-coverage-QNbLwx; log root-unix-resize-input2-native4-checked.log. Exact source/private-scope transitions drop old overlay/hits before stale Enter/click packets and resize; genuine Help/Close then Commands prove event-order responsiveness without sleep/retry or direct handlers. Old Crossterm Mio backend processed Resize120x40 then never delivered later real input while app kept drawing; event diagnostic proves overlay=None and inspector=false. Same four native paths failed under old backend and pass using upstream use-dev-tty FD polling (macOS/Linux only), with no vendor runtime changes and already-vendored dependencies. Original6 inspector scenarios remain passing; original protocol/Git/storage/TTY assertions were preserved. First fresh full run passed815code but243/244PTY; Actions recovery mouse test waited on ACTIONS INTENT already visible before F9 then clicked old View-original geometry after it became Confirm-action (one-column Close shift). New joint actual new-local-intent/error/Confirm/no-original state barrier is added, with exact writes/SQL/Git assertions unchanged, focused10/full/fmt/strictClippy running. No blanket action-parity or Done claim.

Final integrated local gates now PASS: all-target test815code/244actualPTY zerofailed/ignored, wholefmt PASS, strict all-target Clippy -D warnings PASS15.19s, logs root-base-inspector-unix-poll-{all-targets,fmt,clippy}-final.log. Source-only alias lint correction preserved original journal assertions and final full rerun executed after it. Supported upstream Unix polling dependency feature/lock update is verified offline and source-integrity audit remains unchanged86/10300/108/117. Actions recovery10 actual passes6.94s after F9 genuine-frame barrier; file-history checked9code/6PTY passes3.24s after actual Notice-close synchronization, artifact target/test-tmp/ghtui-test-coverage-AekQ9b. Missing-host/private identity and source effects plus all existing native regression paths pass. Complete product action audit, scrollbars/contexts/remaining views and dependencies remain open; not Done.

SOURCE-ONLY editor viewport handoff prepared and frozen: target/build/foundation-editor-viewport-handoff-20261014.json plus unapplied production/registration patch artifacts. NEW editor_viewport_tests.rs declares5 meaningful code tests and NEW unregistered real-binary editor_viewport.rs declares2 key/mouse journeys; every declaration UNCOMPILED/UNRUN. Actual selected direct rustfmt/check and read-only git apply --check for both artifacts PASS; no included editor/App/lib/current-test edit or Cargo. Exact source-derived80/filter62 mismatch67-vs68 now has cell/caret/raw-byte contracts; active Local CreateBranch PTYs independently inspect real cursor escape output, exact scoped async SQLite draft, zeroGH and preserved realGit across70/80/120 layouts, whole combining deletion and actual close/reopen. Root owns coordinated activation/nonzero focused/full gates. Existing1020code/300PTY parent checkpoint is separate, not evidence for these new tests. AC/dependencies/Done remain open.

ACTUAL integrated discovery-limits checkpoint GREEN (2026-10-14): env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 TMPDIR=/home/admin2/CODE/ghtui/target/test-tmp GIT_CEILING_DIRECTORIES=/home/admin2/CODE/ghtui/target/test-tmp cargo test --offline --locked --all-targets PASS1306code/provider=815lib+263strictGH+228other;372real-binaryPTY,0failed/ignored,PTY34.09s, target/build/root-discovery-read-limits-all-targets-frame-final.log. cargo fmt --check exit0 root-discovery-read-limits-fmt-final.log; same env cargo clippy --offline --locked --all-targets -- -D warnings exit0(23.00s) root-discovery-read-limits-clippy-frame-final.log. Checked node scripts/test-task.mjs: repo_catalog_read_limits35code+14PTY actualkeyboard/mouse=true artifact target/test-tmp/ghtui-test-coverage-mJCDRl (root-discovery-read-limits-checked-current-final.log); broad repo_catalog200code+60PTY ...rwJ83h (root-discovery-read-limits-repo-catalog-checked-frame-final.log); measured22+2 ...oqfjC8 (root-discovery-read-limits-measured-checked-regression.log); scoped_pr_observed_tabs15+2 ...PHq2zA (root-discovery-read-limits-scoped-pr-checked-regression.log); cache_storage20+6 ...uz0zJF (root-discovery-read-limits-cache-checked-regression.log). All0ignored/actualbothinputs. Actual avatar8/8PASS1.03s root-discovery-read-limits-avatar-explicit-close-regression.log, partial detail2/2PASS0.66s root-discovery-read-limits-partial-detail-restored-regression.log. No remote/live mutation/macOS/performance/full-product evidence; broader dependency/AC/Done claims unchanged.
<!-- SECTION:NOTES:END -->
