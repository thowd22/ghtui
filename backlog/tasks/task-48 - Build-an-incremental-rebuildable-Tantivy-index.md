---
id: TASK-48
title: Build an incremental rebuildable Tantivy index
status: In Progress
assignee:
  - '@codex'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-20 14:09'
labels:
  - search
  - e2e
milestone: m-4
dependencies:
  - TASK-47
  - TASK-8
references:
  - 'https://github.com/quickwit-oss/tantivy'
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: feature
ordinal: 48
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Ranked search over large selected corpora needs a reusable index with trustworthy revision/freshness. Index failures must not corrupt source metadata or hide deleted records.

## Testing steps

1. Code/provider tests: Test incremental generations, deletes/renames, force-push changes, account boundaries and interrupted/corrupt rebuilds.
2. Real-binary TUI E2E scenario `search_index`: Index fixture repos through keys/clicks, search, change/delete a file and refresh, interrupt/rebuild, and assert visible results match the committed index generation. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e search_index -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Code and collaboration documents share one ranked index keyed by host/account/repo/revision/resource.
- [x] #2 Incremental generations handle changed/deleted/renamed files and rewritten refs with visible coverage.
- [ ] #3 Corruption/schema changes rebuild from SQLite/Git while the TUI remains usable; resource limits are configurable.
- [x] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
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
1. Build a single Tantivy derivative index with typed host/account/repo/revision/rawpath/resource keys and code-aware analyzer. 2. Persist source documents in SQLite and commit index generations with explicit coverage; replace changed/deleted/renamed scopes atomically. 3. Detect corruption/schema drift and rebuild safely from canonical source records/Git; bound files, documents and writer resources. 4. Verify generations/deletes/rewritten refs/account boundaries/cancellation in code and actual both-input search_index PTY before full gates.

Root independent parallel lane: investigate actual current whole-suite LockBusy on the second same-index publication (recorded failed whole third), inspect pinned Tantivy lock lifetime and concurrent real Git child inheritance, and create deterministic real-lock/child proof before any fix. Preserve nonblocking other-process refusal, canonical SQL generations, account/source boundaries, strict provider fixtures and current tests. Any fix must receive focused nonzero code plus keyboard/mouse search_index PTY and whole/fmt/strictClippy; no ignore/retry-based green claim.

Deterministic lock experiment: hold a real pre-exec fork child after a real Tantivy writer acquires its lock, finish/drop the writer and attempt a second real canonical-source publication while child remains alive. The child may use only async-signal-safe libc calls; bounded handshake and kill/wait guard required. If this fails as expected, introduce a narrow Directory adapter delegating all mmap I/O while explicitly unlocking its own flock guard before close (same existing SourceLock semantics), without vendor changes, sleeps or retry loops. Test real contention refusal, reacquisition under inherited FD, lock-path/symlink handling and canonical results. Run existing four actual search_index keyboard/mouse PTYs and focused checked gate; extend native repeated-refresh generation evidence as needed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Search backend preparation assigned local_git agent after actual identity/theme slice verification. Task dependencies remain integration gates; no task completion or dependency readiness claimed. Read task contract and architecture; official ripgrep15.1.0, nucleo-v0.4.0 and Tantivy0.26.2 source pins fetched for manifest-owner dependency audit. gh remains sole authenticated API transport; no mandatory rg executable or alternate ranked engine added.

Prepared src/search/index.rs (not yet exported or compiled): one account-scoped Tantivy derivative with exact source membership and canonical SQLite source frames; incremental ID/content fingerprints, complete delete/rename/rewrite handling, partial retention, generation replay rejection, nonblocking OS writer lock, bounded documents/bytes/writer/candidates/results, cancellation rollback and reopen recovery, owned-derivative corruption/schema quarantine preserving source data/drafts. Four meaningful code contracts prepared for restart/increments/partial/deletes, exact rewritten SHA/account/source boundaries, prepublication cancel and simulated crash/corrupt rebuild, budget/concurrency preservation. Parent-owned actual search_index keyboard/mouse integration and focused/full verification remain pending; dependency gates and no-Done status preserved.

RankedIndex/tokenizer now exported and SearchWorkspace::search_ranked(input,mutableCache,indexRoot,cancellation) integrated as worker-only public API. Same local corpus/revision rules as Exact/Regex; source context hashes sorted repo/raw directory/source/sets plus hidden/ignored/per-file-size policy. Coverage.deletion_authoritative distinguishes successful intended enumeration from searchable completeness: intentional large/binary/ignored exclusions permit deleted/out-of-policy key GC, while missing/I/O/cancel/total-budget scans retain prior records with explicit freshness warning. Real generated-Git workspace regression asserts disappeared/out-of-policy results, generation advancement, hidden/ignored opt-in contexts and preserved Git/files; sixth search_index code contract prepared. Parent native Ranked submit/restore+both-input PTY integration and coordinated verification remain pending.

Actual current source verification: shared-target cargo test --offline --lib search_index ran4 Tantivy contracts, allpassed0ignored; cargo test --offline --test search_contracts ran14, allpassed0ignored including2 search_index real-Git corpus/workspace tests for linked destinations and authoritative delete/policy GC. Tantivy tests exercise complete/partial generations, delete/rename, rewritten immutable SHA, exact source/account membership, restart, cancellation-beforepublication, simulated source/derivative crash, retained corrupt quarantine+rebuild and OSlock/accountbudget preservation. Logs target/build/local-git-search-contracts.log and current commands recorded by parent; fmt/check and strictall-target clippy pass. Actual search_index both-input Ranked UI PTY and full regression remain pending, no Done claim.

Root native Ranked submit/restore now routes worker-only SearchWorkspace::search_ranked into the shared selected-corpus/preview UI. Added actual keyboard/mouse search_index journeys for visible generation, restart, deleted-file GC and immutable saved destination opening with preserved HEAD/index/dirty bytes; currently executing. Existing Tantivy source and policy/account contracts green; no broader collaboration/managed-clone acceptance completion yet.

Actual native Ranked search_index keyboard/mouse journey command cargo test --offline --locked --test tui_e2e search_index -- --nocapture passed 2/2, zero ignored, during root session22935. Verified committed generation labels, process restart, deleted-file removal, immutable saved destination opening and unchanged HEAD/index/working bytes with zero gh calls. Full checked code/PTY gate plus interruption/corrupt rebuild journeys remain pending.

Prepared an additional derivative-recovery correctness contract: invalid Tantivy checkpoint payload or reader document count inconsistent with recorded fingerprints triggers a full delete/rebuild from canonical account-scoped SQLite records, rather than incrementally retaining unknown orphan postings. Recovery notices are prioritised and labelled REBUILT/RECOVERED for narrow native views. Full-rebuild state is derivative-only (not serialised in the compatibility checkpoint), and source checkpoints/unrelated drafts remain preserved. Fifth index code contract and keyboard/mouse corrupt-meta restart journeys are source-ready, awaiting the coordinated current checkpoint; not counted as passed yet.

First fresh global run reached70 passing library tests but exposed a real advisory-lock release race in search_index crash recovery: a concurrent fork-before-exec Git child could retain the shared open-file description after the parent closed its descriptor, making immediate reopen spuriously busy. SourceLock now explicitly unlocks before descriptor close; an actual inherited-fd child regression is being run before fresh global rerun. This failed checkpoint was not counted as success.

Fresh full-project regression exposed an actual concurrent-process lock lifetime failure: reopen reported index busy after index drop (70 library tests passed, 1 failed; target/build/root-all-targets-checkpoint.log retained). Fixed with SourceLock RAII explicitly LOCK_UN before closing the descriptor, including early open errors, because a concurrently forked Git child can briefly inherit the same open-file description before CLOEXEC. Added deterministic real fork/pre-exec inherited-descriptor contract with bounded readiness, active-writer negative control, immediate reopen while child remains alive, and child cleanup. Actual focused env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --offline --locked --lib search_index -- --nocapture passed 6 tests, zero failed/ignored, 0.09s. Full regression rerun is still required; no retry-as-success or ignored coverage.

Fixed source-lock race with explicit LOCK_UN-before-close and a deterministic actual pre-exec-child inherited-descriptor regression. Focused search_index code6/6 passed; fresh full suite is now green:258 code/provider+102 actual PTY, zero ignored, log target/build/root-all-targets-checkpoint.log. Both actual corrupt-index restart journeys show REBUILT notice, repaired results, retained canonical source records/drafts and quarantine; judged relevance journeys also passed. Whole fmt/audits passed; strict Clippy running. Managed corpus/collaboration and broader index interruption coverage remain open.

Whole cargo fmt --check and cargo clippy --offline --locked --all-targets -- -D warnings passed after the fresh258code/102PTY full regression. Root logs target/build/root-fmt-checkpoint.log and root-clippy-checkpoint.log. Task still In Progress for managed selected clones, unified collaboration corpus and expanded interrupted native journeys; no source-only or partial-completion substitution.

Actual deterministic red now reproduced: cargo test --offline --locked --lib search_index_writer_drop_releases_actual_tantivy_lock_before_inherited_child_exec -- --nocapture selected1, passed0/failed1/ignored0,0.03s (build28.12s), target/build/root-index-inherited-lock-red.log. Real Tantivy writer is joined/dropped before new canonical-source publication; a bounded held pre-exec fork child still alive causes exact LockBusy. Child executes only libc after fork and has kill/wait cleanup. Pinned Tantivy ReleaseLockFile closes without explicit flock unlock, consistent with shared open-file-description semantics documented by https://man7.org/linux/man-pages/man2/flock.2.html . This proves this failure mode deterministically, not that every historical LockBusy had the same cause. Narrow delegated Directory fix next; no sleeps, ignore or speculative retry.

First targeted green actual1/1,0failed/ignored0.12s afterbuild27.79s, terminal47757exit0, target/build/root-index-inherited-lock-green-first.log. Real held pre-exec child remains alive after two distinct canonical Tantivy generations; old source disappears and new exact hits remain. New IndexDirectory delegates mmap storage/watch/atomic operations unchanged but owns same flock protocol through existing explicit-unlock SourceLock, preserving external writer exclusion; no vendor/dependency mutation or sleep/retry. Additional real contention/path/symlink/hardlink contracts and enhanced actual repeated-refresh both-input search_index journeys now staged; checked main nonzero gate executing. Prior full current run still failed31PTY; no full green or TaskDone claim.

Checked current first gate actually11code passed (9lib+2real corpus),0ignored, but four native ran2pass2fail0ignored0.83s. New same-corpus third-query assertion incorrectly required canonical source generation2; explicit query correctly reserves source3 even though derivative remains unchanged. Preserve failed log/artifacts target/build/root-parallel-tree-index-checked-first.log; correct expectation to exact source2+1 and independently assert actual Tantivy on-disk opstamp unchanged. This is not weakening lock/source/result assertions. Raw/staging keys were not reached after failure; own current gates rerun next.

Main current checked search_index now GREEN actual11code (9lib+2real corpus)/4compiled-binaryPTY,0failed/ignored,1.21s native, terminal79441exit0; target/build/root-parallel-tree-index-checked-second.log, completed key/mouse journals target/test-tmp/ghtui-test-coverage-cac6K7/journeys.jsonl. Real writer inheritance, real nonblocking writer contention and owned direct regular lock/symlink/hardlink/path rejection pass; no vendor rewrite/hidden retry. Native working-body replacement then explicit unchanged query proves exact source generation increments while independent on-disk Tantivy opstamp stays unchanged; actual deletion across restart and exact immutable saved destination still independently verified with unchangedGit/index/dirty and gh0/TTY. Main admitted raw42code/8PTY+staging15code/6PTY also GREEN Q1qKyK/wclluL. Whole/fmt/strictClippy subsequent integration gate remains required; full TASK48/deps/perf/Mac not Done.

2026-10-20 audit at 74360e48 (3/5 ACs met): The shared Tantivy index with generations, deletion GC and corrupt rebuild is implemented and PTY-verified. Remaining: configurable index limits, and PTY journeys for an interrupted build and a rewritten ref. Remaining: AC3: rebuild-on-corruption is met, but resource limits are not configurable. RankedLimits only uses defaults (workspace.rs:124); only max_file_bytes comes from config.toml. AC5: the 4 PTY journeys exist, but none covers interrupting an index build or a rewritten ref visibly changing coverage (both are tested only in contracts).
<!-- SECTION:NOTES:END -->
