---
id: TASK-29
title: Compare commits and branches with plain-language semantics
status: Done
assignee:
  - '@foundation'
created_date: '2026-10-12 23:06'
updated_date: '2026-10-20 14:09'
labels:
  - repository
  - e2e
milestone: m-3
dependencies:
  - TASK-28
  - TASK-15
  - TASK-34
  - TASK-35
references:
  - 'https://git-scm.com/docs/git-diff'
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: feature
ordinal: 29
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Snapshot differences and a branch's proposed changes answer different questions. A novice should choose the right comparison without understanding two-dot versus three-dot syntax.

## Testing steps

1. Code/provider tests: Generate divergent/merge/root/shallow fixtures and compare all results to Git diff and merge-base, including directional swaps.
2. Real-binary TUI E2E scenario `compare_commits`: Select A and B on the graph or picker, switch comparison modes and swap via keys and clicks, assert changed files and explanations exactly match the fixture. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e compare_commits -- --nocapture` and verify a nonzero selected test count; run the relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. The initial framework is delivered by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Pick any two available refs/commits and clearly show base, target, direction, common ancestor and ahead/behind.
- [x] #2 Compare saved versions matches Git A-to-B snapshots; proposed-branch changes matches merge-base-to-B.
- [x] #3 Swap endpoints, compare merge parents and handle root/no-common-base/missing history explicitly.
- [x] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [x] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
- [x] #6 All displayed actors use the shared username/avatar or honest fallback; Git and CI states use the shared semantic colors with redundant labels.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a dedicated read-only CompareIntent/ComparisonPlan controller for two manual or typed immutable refs/commits, with worker-only original Git resolution and no write/checkout/confirmation semantics. 2. Reuse pure typed inventory chooser controls; expose labelled Snapshot versus Proposed changes modes, explicit direction/swap, common ancestors and ahead/behind, including multiple-base and shallow/unrelated explanations. 3. Root integrates FormPurpose::Compare and shared keyboard/click toolbar/form actions; keep prepared canonical diff anchors and head actor identity in Workspace. 4. Verify divergent/root/unrelated/shallow fixtures against independent Git diff/merge-base/rev-list, stale selections and endpoint swaps while preserving HEAD/index/files/refs. 5. Add actual compare_commits keyboard/mouse PTY journeys at wide/narrow sizes and run checked nonzero code+PTY gates, then full all-target/fmt/clippy before claiming completion.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Backend preparation is available through shared src/git.rs, src/diff.rs and src/repository_view.rs. Real generated-Git fixture tests cover canonical status/diff/index/objects/refs, ordered DAG parents and immutable tree reads, staged patches, hook rejection, branch/tag guards, local-bare push/fetch, stash/worktrees, conflicts/abort, reflog rescue and bisect reset. Isolated offline source validation uses /tmp/ghtui-git-validation.AsAVCb/Cargo.toml: 8 code/model unit tests + 19 real Git integration tests passed (0 ignored), cargo clippy --offline --manifest-path /tmp/ghtui-git-validation.AsAVCb/Cargo.toml --all-targets -- -D warnings passed; rustfmt --check own modules/tests passed. Prepared backend only: dependencies, full UI, mouse/keyboard PTY and full-project test/fmt/clippy verification remain required. No task completion claimed.

Added native immutable saved-commit Changes mode (keyboard d and same clickable toolbar action), labelled root-to-empty-tree semantics, and exact typed first/alternative ordered parent actions for merge commits. Canonical diff anchors preserve base/head/raw path identities, actor provenance and semantic roles; saved-history partial-staging controls now give explicit disabled explanations. New workspace code contract and two real-binary commit_changes keyboard/mouse journeys cover root and merge-parent content plus preserved refs/HEAD/index/dirty bytes. Focused verification is queued after the shared build-window handoff. Arbitrary A/B picker, endpoint swaps and proposed-vs-snapshot mode selection remain open; this is not full TASK-29 completion.

Actual subset gate node scripts/test-task.mjs commit_changes passed 2 code/provider + 2 actual-binary PTY, zero ignored, shared offline target/jobs2/incremental0. Both keyboard/mouse journeys at wide/narrow sizes verify selected immutable commit changes, distinct ordered first/second merge-parent diffs, root empty-tree introduced content, and unchanged refs/HEAD/index/dirty bytes; coverage /tmp/ghtui-test-coverage-69ICSt. Batch log target/build/native-management-checked.log. This proves saved commit inspection/merge parent subset, not the task-specified arbitrary A/B compare_commits picker/mode swap journey; remaining ACs not accepted by default.

Root read-only FormCompare source is connected to prepared controller/pickers and new C Compare toolbar: two typed/manual endpoints, explicit snapshot/proposed question, F4 chooser, F9/click swap, optional worker Preview and direct Open comparison with fresh typed-ref checks. Versioned draft pin persistence and separate read-only labels added. First all-target check compiled product source but caught missing Clone on new testfixture GhStep, now corrected by fixture owner. Actual compare_commits journeys are being prepared; no completion claimed.

Prepared original-object GitRepository::relationship(base,head) for native comparisons: resolved immutable base/head SHAs, ahead=head-only commits, behind=base-only commits, all best common_ancestors, and shallow loaded-history qualifier. Unrelated histories retain empty common-base list (snapshot comparison still valid); actual rev-list/merge-base errors are not recast as no ancestor. Multiple equally best bases are not silently described as unique. Real generated divergent/crisscross/unrelated, shallow clone, and replacement/graft contracts prepared; foundation/root own comparison forms/keyboard+mouse integration. No implicit fetch, checkout, index or history write. Focused execution pending coordinated build window.

Checked compare_commits runner now passed9 code/controller/Git/Form tests and2 actual keyboard/mouse PTY, zero ignored; /tmp/ghtui-test-coverage-uo3112, log target/build/compare-commits-checked.log. Native journeys choose exact A/B refs, Preview/cancel/versioned draft restoration, Snapshot, narrow resize, endpoint swap and Proposed mode, independently assert differing file counts/body/relationships and unchanged HEAD/index/refs/dirty/untracked/zero gh. Relevant root/shallow/unrelated/multiple-base code checks present; broader platform/full regression still required, not Done.

Implemented dedicated read-only ComparisonPlan/CompareIntent, exact typed ref/commit picker, Snapshot versus Proposed questions, endpoint swaps and native preview/open without Git write confirmation. Checked CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 node scripts/test-task.mjs compare_commits passed9 code tests (Form1/controller6/Git2) +2 real keyboard/mouse PTY, zero ignored. Artifact /tmp/ghtui-test-coverage-uo3112; log target/build/compare-commits-checked.log. Independent Git fixtures verify directional snapshot patches, merge-base-to-target proposed patches, canonical base/head anchors, ahead/behind, root/no-change, stale typed refs, unrelated roots, shallow available-history notices/no implicit fetch, and multiple best bases with exact chosen diff base. Native paths verify exactA/B choices, Preview/cancel/draft restore, Snapshot content,80x24 swap and distinct Proposed content/file count while HEAD/index/refs/dirty/untracked bytes and GH journal remain unchanged. All-target cargo check passed; current strict clippy needs4 newly added Git-contract clone-to-slice assertions fixed by owner. Whole-project final regression/platform and native multibase/shallow edge journeys remain open; not Done.

Fresh full Linux checkpoint passed: CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test --offline --locked --all-targets: 288 meaningful code/provider contracts (including 90 strict provider contracts) and 114 actual compiled-binary PTY journeys, zero failures/ignored. cargo fmt --check passed; cargo clippy --offline --locked --all-targets -- -D warnings passed (8.07s). Logs target/build/root-all-targets-checkpoint.log, root-fmt-checkpoint.log, root-clippy-checkpoint.log. Node task-runner negative self-tests passed3/3; dependency integrity audit passed86 upstream roots/10300 files/108 manifest patches/117 registry packages; Backlog doctor clean72tasks. This includes new comparison2, remote-search2, raw-ref identity2, explicit-repo local/unborn2, Actions recovery2 and artifact2 PTY;18 native Actions journeys total. README reflects this actual green evidence only. Native macOS/live disposable-host/release and remaining product acceptance stay open; full product not Done.

2026-10-18 a12: Compare form labels A/B as from/to (branch, tag or commit) and explains snapshot vs proposed. Existing comparison PTY + comparison_contracts pass.

2026-10-20 audit at 74360e48 (full suite 6000/0, fmt+clippy clean): all ACs met, confirmed by an independent check. Compare form (C) with snapshot and proposed modes, swap, ahead/behind and common base is implemented, with contract and keyboard/mouse PTY coverage. Edge cases (multi-base, shallow) are tested in contracts only.
<!-- SECTION:NOTES:END -->
