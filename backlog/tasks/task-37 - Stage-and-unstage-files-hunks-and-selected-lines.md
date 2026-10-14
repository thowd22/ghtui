---
id: TASK-37
title: Stage and unstage files hunks and selected lines
status: In Progress
assignee:
  - '@codex'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-14 02:26'
labels:
  - localgit
  - e2e
milestone: m-6
dependencies:
  - TASK-36
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-6 - Local-Git-workspace-and-operation-contract.md
priority: high
type: feature
ordinal: 37
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Preparing a commit should be obvious and precise. Staging must show what belongs in the next commit and avoid losing changes when files change during selection.

## Testing steps

1. Code/provider tests: Verify actual index blobs after file/hunk/range operations including whitespace, renames and stale selections.
2. Real-binary TUI E2E scenario `staging`: Select and stage/unstage the same changes by keys and mouse drag in fresh repos, inspect the staged diff, and compare index/working blobs independently. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e staging -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Files/hunks/line ranges can be staged or unstaged with preview and clear working-versus-index panels.
- [ ] #2 Mouse range selection and keyboard selection produce identical index content.
- [ ] #3 Stale file/index changes, binaries/renames and patch failures are handled without accidental discard.
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
1. Connect typed working-file/diff-anchor selections to the shared native Local operations menu and forms. 2. Generate read-only exact file/hunk/line staging previews off the terminal thread; require explicit state-checked confirmation. 3. Verify keyboard/mouse staged/unstaged and partial-line effects independently against real index/working bytes, including stale races and unusual paths. 4. Run focused local_staging code/provider/actualPTY tests and complete regression gates before completion.

Native canonical selection now uses exact DiffAnchor endpoints, one file/comparison boundary, Shift-arrow range extension and mouse drag without background I/O. Hunk selection shares the same action and range state; LocalContext receives actual selected hunk/line coordinates, not screen text. Test actual partial stage, reverse unstage, whole-hunk staging and preserved unrelated working change with separate fresh keyboard/mouse terminal journeys at wide/narrow sizes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Backend preparation is available through shared src/git.rs, src/diff.rs and src/repository_view.rs. Real generated-Git fixture tests cover canonical status/diff/index/objects/refs, ordered DAG parents and immutable tree reads, staged patches, hook rejection, branch/tag guards, local-bare push/fetch, stash/worktrees, conflicts/abort, reflog rescue and bisect reset. Isolated offline source validation uses /tmp/ghtui-git-validation.AsAVCb/Cargo.toml: 8 code/model unit tests + 19 real Git integration tests passed (0 ignored), cargo clippy --offline --manifest-path /tmp/ghtui-git-validation.AsAVCb/Cargo.toml --all-targets -- -D warnings passed; rustfmt --check own modules/tests passed. Prepared backend only: dependencies, full UI, mouse/keyboard PTY and full-project test/fmt/clippy verification remain required. No task completion claimed.

Added Diff::selected_patch using canonical file/hunk/line coordinates. It converts unselected removals to context, omits unselected additions and recomputes offsets independently of earlier unselected hunks. Quoted raw byte paths and no-newline markers are preserved; partial binary/add/delete/rename changes require whole-file staging. Focused actual-Git tests: cargo test --offline --manifest-path /tmp/ghtui-git-validation.AsAVCb/Cargo.toml --test git_contracts staging_selected -- --nocapture: 2 passed, 0 failed/ignored. Index contents and unchanged working contents were independently inspected, including reverse unstage. Full TUI selection/drag coverage remains outstanding.

Local ComparisonResult now retains original comparison mode and observed content state. GitRepository::plan_selected_patch accepts only Working-stage/Staged-unstage and rejects edits after a diff was shown, including before confirmation preview. Actual test staging_selected_diff_snapshot verifies stale rejection, refreshed selected stage, canonical reverse unstage and preserved working bytes. Latest offline suite: 11 units +27 Git contracts passed, 0 failed/ignored; strict clippy/fmt checks passed.

Controller/UI foundation prepared in src/local_actions.rs and workspace.rs: exact status RawPath selections and canonical DiffAnchor row selections survive rendering, with no label-to-path guessing. LocalPlan prepare is read-only, displays target/Git effect and current selected file or canonical partial patch, rejects stale rendered line selections, and execute requires explicit confirmation plus fresh Git fingerprint. Actual focused cargo test --test local_actions_contracts passed 5/5 tests (2 staging, 1 commit, 1 branches/tags, 1 remote) before root native-form wiring. Stage/unstage selected unusual Japanese/tab path preserved unrelated edits and HEAD; selecting one addition staged only that line; stale diff was rejected before preview. Real-binary staging keyboard/mouse/drag integration is pending root form/menu wiring; not Done and no PTY completion claimed.

Connected shared Local menu/forms with typed LocalContext exact working-file/diff-line data, read-only LocalPlan preview and separate Confirm action; 39 native operations are discoverable with keyboard scrolling and mouse wheel/More controls. Fields retain selected raw identities; no operation chooses an arbitrary first file. Added actual staging keyboard/mouse cancel→preview→stage→unstage journeys with independent HEAD/index/worktree inspection, currently executing. Whole-project cargo check passed before new search matcher API typo, which its owner has fixed; no completion claimed during integration.

Actual cargo test --offline --test tui_e2e staging_ -- --nocapture:2passed, keyboard+mouse wide/narrow. Verified no index mutation before Preview/Confirm, cancellation unchanged, exact space-containing path stage/unstage, unchanged HEAD and working bytes, and zero gh calls. Partial hunk/multiline drag selection remains to complete the task; full-file success does not close those criteria.

New actual keyboard/mouse canonical-range PTY exposed a real neighboring-change reverse-unstage bug: selecting beta while alpha also staged produced a subset forward patch that could not match the complete index with --reverse. Both tests failed safely without losing work, artifacts target/tui-artifacts/123-1791855172942273579 and123-1791855172942457542. Backend owner fixed direction-aware canonical reversal and added real neighboring additions/removals/stale-index contract. Native partial-range/hunk rerun executing; no acceptance completion from failed checkpoint.

Actual both-input native range PTY revealed reverse-unstage neighboring additions failure; fixed direction-aware canonical selected_unstage_patch, which reverses the full selected file before partial selection and applies forward to the index shown. This retains unselected additions as context and omits unselected removals, while retaining stale-fingerprint checks. Added real generated-Git neighboring addition/removal contract asserting beta-only unstage, alpha remains staged appropriately, working alpha/beta/gamma bytes and HEAD preserved. Parent rerun and focused/full gates pending; earlier failure artifacts target/tui-artifacts/123-1791855172942273579 and target/tui-artifacts/123-1791855172942457542 were not accepted or weakened.

Actual native staging rerun selected4 real PTY journeys, all passed, zero ignored/failures: full-file preview/cancel/stage/unstage and canonical keyboard Shift-arrow/mouse drag partial alpha+beta staging, beta-only reverse unstage retaining alpha, then full-hunk staging at80x24. Independently verified exact index bytes after every confirmation, HEAD unchanged, working alpha/beta/gamma unchanged and zero gh calls. Context lines are highlighted within a selected hunk but excluded from mutation coordinates. Direction-aware partial unstage safety fix is now exercised by the actual binary. Checked code+PTY runner and full regression remain next gates.

Checked staging runner now passes9 actual code/provider tests and4 real PTY journeys, zero ignored/failures, keyboard+mouse coverage journal /tmp/ghtui-test-coverage-DydRoh. Same shared environment with node scripts/test-task.mjs staging. Includes correct neighboring additions/removals reverse-unstage, stale state guards, raw filenames/no-newline behavior and native partial range/hunk paths. Wider full-feature dependencies and current all-target regression gate remain open; no Done claim.

Coordinated checked staging gate passed after the unified-header correction: 14 code/provider tests and 6 real-binary PTY tests, zero failures/ignored; keyboard and mouse new-file Japanese-space preview/cancel/stage/unstage journeys now pass alongside range staging. Artifact /tmp/ghtui-test-coverage-8Avh5H (foundation-run shared checked runner). Parser splits a Git header tab delimiter only outside C-quoted exact filename bytes and preserves Unicode/spaces/control bytes; no filenames are normalised for Git operations. Fresh full-project regression is next; no Done claim yet.

Latest checked staging gate passed14 code/provider tests +6 actual-binary PTY tests, zero ignored: CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 node scripts/test-task.mjs staging. Coverage /tmp/ghtui-test-coverage-8Avh5H; log target/build/staging-checked.log. New untracked preview discloses actual text lines, binary byte count or symbolic-link target without reading referenced private bytes. Both inputs cancel then explicitly stage and unstage a Japanese+space filename at120x40/80x24, independently verifying index bytes, unchanged HEAD/working file, unrelated binary file and zero GH calls. A real quoted Git-path parser failure was reproduced and fixed by Git agent; current new-file/selected-file/range/hunk journeys all pass. Directory-stage refusal and aggregate untracked fingerprint budget also have real Git coverage. Global gates pending root checkpoint; not Done.

Aligned new untracked content classification with shared file viewer: invalid UTF-8 as well as NUL is honestly labelled binary; expanded real no-write preview contract verifies12-byte invalid-UTF8 content. Latest checked staging14code+6actualPTY all pass/zeroignored, artifact /tmp/ghtui-test-coverage-kMMGah; log target/build/staging-checked.log. Both keyboard/mouse existing file, canonical selected range/hunk and Japanese-space new-file staging/unstaging journeys remain passing.

Root current whole gate code956PASS, actualPTY293PASS1FAIL0ignored27.35s, root-repository-source-all-targets-layout-final.log. Existing canonical mouse drag panicked while locating beta immediately after alpha. Independent saved failure screen target/tui-artifacts/1864255-1791944165417565758/screen.txt proves actual120x40 body shows complete +alpha and partial next row +b (terminal byte stream mid-frame), not missing canonical content or viewport overflow. Keep exact canonical geometry and all independent before/after index/HEAD/dirty/gh assertions; add explicit actual wait_for_screen(+beta) for both inputs before deriving the two endpoints. Focused canonical_ run active; fresh whole/fmt/clippy/checked gates required. No task Done or coverage reduction.

Focused canonical_ actual terminal81506exit0 selects4native: both canonical staging keyboard/mouse plus2 genuine blended canonical-context journeys, all4PASS0ignored3.85s, root-repository-source-staging-canonical2-readiness-rerun.log. Genuine actual failure artifact partial +b proves wait needed, not assertion removal. Current fresh whole terminal16129exit0 passed956code/provider+294actualPTY0failed/ignored27.56s after readiness fix; root-repository-source-all-targets-readiness-final.log. Current final strictClippy/test-borrow correction and checked staging runner remainpending; no Task37 Done or broader acceptance inferred.

Fresh checked node scripts/test-task.mjs staging terminal25794exit0 passed14code/provider+6actualnative0failed/ignored2.02s, actualkeyboard/mouse completion target/test-tmp/ghtui-test-coverage-6WD4XM; root-repository-source-staging-checked-final.log. Original exact index bodies after alpha+beta stage, beta-only unstage with alpha retained, full hunk at80x24, unchangedHEAD/workingbytes/GH0 remain verified. New source labels/contextbar control preserve canonical targets and geometry. Whole strictClippy17.02s andfmt passed; final alltarget after comment/test-only borrow corrections nowrunning. BroadTask37/dependencies/notyetMac remainopen.

Final coherent full Linux checkpoint passed956code/provider+294actualPTY0failed/ignored27.48s (terminal18026exit0 root-repository-source-all-targets-borrows-final.log); wholefmt and strictalltargetClippy -Dwarnings PASS. Fresh checkedstaging14+6 actualkeyboard/mousePASS with no weakening index/dirty/HEAD/gh effects; original streamed endpoint readiness defect and exact failure artifact documented above. Task37 fullscope/dependencies/platform gates remainopen; no Done inferred from this layout regression fix.
<!-- SECTION:NOTES:END -->
