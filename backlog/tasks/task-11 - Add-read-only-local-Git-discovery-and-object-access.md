---
id: TASK-11
title: Add read-only local Git discovery and object access
status: In Progress
assignee: []
created_date: '2026-10-12 23:06'
updated_date: '2026-10-14 14:59'
labels:
  - foundation
  - e2e
milestone: m-0
dependencies:
  - TASK-5
references:
  - 'https://git-scm.com/docs/git-ls-tree'
  - 'https://git-scm.com/docs/git-cat-file'
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: feature
ordinal: 11
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
PR workflows and repository exploration need Git data without silently switching branches or changing the user's checkout. Object reads must be accurate for worktrees, unusual paths, and incomplete clones.

## Testing steps

1. Code/provider tests: Generate real Git fixtures and compare refs/status/objects to Git output, including unusual byte paths and missing history.
2. Real-binary TUI E2E scenario `git_provider`: Open each fixture repo through keyboard and mouse navigation, inspect status/revision and an object, and independently verify checkout/index hashes are unchanged. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e git_provider -- --nocapture` and verify a nonzero selected test count; run the relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. The initial framework is delivered by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Discover repositories, remotes, refs, HEAD, worktree/index status, and immutable trees/blobs with explicit identities.
- [ ] #2 Read operations preserve working files and index; batch/cache object access and parse safe machine output.
- [ ] #3 Unborn/detached/shallow/sparse/submodule/LFS/missing-object states are represented instead of crashing.
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
1. Add a bounded explicit-argv Git provider with repository discovery, immutable revision resolution, raw NUL-delimited paths, status, refs and object reads.
2. Exercise generated real repositories, unusual paths, unborn/detached/shallow states and checkout/index preservation.
3. Integrate local workspace views after dependency readiness, then run both real-binary keyboard and mouse PTY journeys.
4. Run full test/fmt/clippy verification before checking acceptance criteria or completing the task.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Local-Git agent is preparing the raw-path immutable-object/status provider and real generated Git contract fixtures in parallel with foundation. TASK-11 remains To Do until dependencies and required PTY verification are satisfied.

Refined dependencies: read-only Git adapter depends on the process boundary, not organization catalog/navigation. Final acceptance still requires actual-binary UI integration and tests.

Implemented src/git.rs and src/diff.rs with raw NUL-delimited repository/status/tree/conflict paths, immutable commit identities, ordered commit parents and refs, canonical diff rows and guarded local mutations. Isolated offline validation: cargo test --offline --manifest-path /tmp/ghtui-git-validation.AsAVCb/Cargo.toml --all-targets: 4 unit tests and 16 real generated-Git integration tests passed, 0 ignored. This minimal crate uses project source files plus cached libc/tempfile; it is not full project/PTY verification. Read preservation test independently checks unchanged index bytes, working bytes and HEAD. Mac/Linux raw paths include tab/newline/non-UTF-8 and literal pathspec metacharacters. Task remains uncompleted pending foundation and actual-binary keyboard/mouse PTY.

Expanded isolated offline validation: 9 parser/presentation unit tests + 25 actual-Git/model integration tests now pass (34 total, 0 failures/ignored). Added real hook descendant cancellation (1 focused test passed), stale remote URL/ref target rejection, simultaneous confirmed staging plans (one succeeds, second rejects stale state), and foundation Workspace graph to old revision tree/file plus current working diff model integration. cargo clippy --offline --manifest-path /tmp/ghtui-git-validation.AsAVCb/Cargo.toml --all-targets -- -D warnings passed; rustfmt --check owned modules/tests passed. Provider/presentation guide is doc-7. Main project dependency and PTY gates remain unclaimed.

Latest complete isolated source suite: CARGO_INCREMENTAL=0 cargo test --offline --manifest-path /tmp/ghtui-git-validation.AsAVCb/Cargo.toml --all-targets: 12 code/model/Workspace units +31 generated-Git integration contracts passed (43 total), 0 failures or ignored. Strict minimal-source clippy -D warnings and rustfmt --check owned files pass. Canonical parser additionally rejects truncated hunks and preserves non-UTF-8 headings. No full-project/actual-binary PTY result is inferred from the isolated crate. Documentation/evidence current in doc-7.

Portability audit (preparation, not macOS execution evidence): physical invalid-UTF8 filename fixtures are incompatible with ordinary APFS. Apple primary documentation says APFS creates valid UTF-8 filenames only: https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html . Preserve always-running raw Git-object path contracts on both supported OSes by creating blobs through installed Git hash-object stdin and injecting NUL-delimited index paths; use valid UTF-8 control-bearing physical paths on macOS while retaining Linux invalid-byte physical coverage. No required test is ignored/skipped and no raw identity assertion is removed. Native raw-tree mouse/keyboard journeys remain required and prepared; current actual validation is on Linux only.

Status housekeeping: this implemented provider scope is now In Progress, not an untouched To Do or a completed dependency. Superseding actual main-project Linux checkpoint passed288 code/provider plus114 real-binary PTY, zero ignored/failures, fmt and strict all-target Clippy. Actual Git contracts43 independently verify HEAD/index/working preservation, exact objects/parents/raw paths, worktree/bare/shallow state and guarded writes; named git_provider both-input journeys are present in the all-target checkpoint. Historical isolated source-only evidence above remains historical. Complete task-specific state/navigation/missing-object coverage and dependency/finalization audit still gate all acceptance checks/Done. See doc7 and target/build/root-all-targets-checkpoint.log; no macOS execution claimed.

Long-private-TMPDIR full immutable identity regression: repository rendering now provides a dedicated Saved full40/64-digit identity row independent of arbitrarily long directory breadcrumb, including80x24. History replacement/graft notice moves to its own row and content/hit offsets stay exact. cargo test --offline --locked --test identity_contracts git_provider_repository_full_saved_identity passed1/1,0ignored (local-long-path-version-code.log), across80/120 columns,40/64 identities,with/without overrides; verifies exact row hit coordinates and preserved real HEAD/index. Same longTMPDIR shared-target cargo test --test tui_e2e revision_tree passed2/2 actual keyboard/mouse journeys,0ignored,0.91s (local-version-long-path-pty.log), preserving staged/dirty files/index/HEAD and no gh calls. Current full global gates pending; no whole-task completion claim.

2026-10-14 Root original local-source navigation admission CLOSED GREEN as a slice, not full TASK30/31/32/34/10/11/2 acceptance. Current cargo test --offline --locked --all-targets passed1494 code/provider (977 library,289 strict GH,228 other) and434 compiled-binary PTY tests, zero failed/ignored; native38.93s, target/build/root-original-navigation-all-targets-current.log. cargo fmt --check exit0 root-original-navigation-fmt-current.log; cargo clippy --offline --locked --all-targets -- -D warnings exit0 root-original-navigation-clippy-current.log,1m00 includes build locks, not render benchmark. All current process handles terminal0.

Focused original_saved_navigation code31/31 and PTY6/6 passed, then checked node scripts/test-task.mjs original_saved_navigation passed31+6 with independently completed8/8 real sessions (4keyboard/4mouse, both SHA1/SHA256), target/test-tmp/ghtui-test-coverage-lYqBz6/journeys.jsonl; checked log root-original-navigation-checked-current.log, native10.43s. New original source and exact chosen/responsible SHA/raw path/line/role survive nested directory/file/history/blame/Graph/Diff/OriginalFile/global Back through the actual same retained Git producer, prequeue30s deadline/token, measured32MiB child/64MiB whole/128-child window and existing queue. Mutable Working restore checks current bytes; cached Actor/Commit stay pure. Source replacement fails beforechild0; pending real installed-Git history child is canceled/reaped, no late result. Exact private SQL/drafts/journals/config/Git refs/raw index/staged/dirty/untracked bytes, strictGH0/curl0 and restoredTTY independently asserted. No auth, rediscovery, HEAD fallback, fetched objects, reports or approvals revived.

Preserved actual red failures: fifth native4/6 exposed inactive cached History offset in an active Blame/Author restore key. Genuine SHA256 red contract failed0/1 (root-original-navigation-inactive-cache-red-current.log); key now normalizes only actual source return_view/history_offset/blame_start, retaining original provenance/member/role. Root whole first native430/434 also exposed real unborn standard project navigation regression; exact original HEAD+observed no head now retains honest unborn Tree/Graph/CommitChanges, no general HEAD fallback. Added genuine source contract and actual Project repo/subdir keyboard/mouse2/2 (4sessions) passed; log root-original-navigation-project-current.log. Failed whole/root prior native logs retained. Normal missing remotes and unset sparse config now use successful installed-Git enumeration/default queries instead of full-cap failed charges; measured actual failures still charge full reservation. Genuine unrelated-config exclusion/remote-change fingerprint/actualledger children contract passes. New genuine mixed-kind local/remote publisher refusal passes before all effects. Focused original31 includes these4 Root-added contracts beyond frozen27; no ignored/placeholder coverage.

Impacted checked gates passed navigation113code/20PTY (24actual complete sessions GUmger), file_history9/12 (14sessions LP806e), git_actor_association_native28/10 (16sessions mSgPiF), remote_tree_v129/8 (8sessions qhJ8YT), staging14/6 (6sessions 6lwPid), all0ignored, actual both inputs. Artifacts target/test-tmp/ghtui-test-coverage-<suffix>/journeys.jsonl; logs root-original-navigation-<key>-checked-current.log. Runner negative gate node --test tests/test_task_runner.test.mjs3/3 passed50.553ms; initial mistaken task_runner path selected no tests and was not evidence. Backlog auditor72valid0errors/doctor0duplicate,self,cycle; git diff --check0; git remote empty. macOS/live-host/full workflow/latency/release/CI not claimed. Three agent slots remain source-only Graph paging, immutable Tree jump and scoped Checks candidate/job selection. Boundary5-code/12-PTY, remote follow-up4/14, scoped bridge24/4 bundles remain unadmitted/unrun; dependencies and ACs stay open.

2026-10-14 original-source navigation boundary checkpoint passed 1,500 code/provider and 446 actual PTY tests, zero failures/ignored, whole formatting and strict all-target Clippy. TASK-32 and TASK-2 latest notes contain actual 5-code/12-new-PTY boundary coverage, checked gates, retained failures and exact logs/artifacts. Original chosen/responsible SHA/raw path/line/role, source identity, queued-before-start cancellation and disabled explanation parity are verified slices only. Graph/tree/diff/related-PR/full actor and platform/release/dependency requirements remain open; no AC/Done inferred.
<!-- SECTION:NOTES:END -->
