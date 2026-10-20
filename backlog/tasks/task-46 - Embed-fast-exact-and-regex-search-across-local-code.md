---
id: TASK-46
title: Embed fast exact and regex search across local code
status: Done
assignee:
  - '@codex'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-20 18:19'
labels:
  - search
  - e2e
milestone: m-4
dependencies:
  - TASK-45
  - TASK-11
references:
  - 'https://github.com/BurntSushi/ripgrep/tree/master/crates'
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: feature
ordinal: 46
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Exact matches and regex should work locally with predictable semantics and no additional required executable. Ignored/binary/large-file and worktree-versus-revision coverage must be clear.

## Testing steps

1. Code/provider tests: Compare regex/literal outputs to known corpus expectations including ignored paths, Unicode, newlines, blobs and cancellation.
2. Real-binary TUI E2E scenario `exact_search`: Find a phrase then regex via keys/clicks, change worktree/revision scope, cancel a slow scan, and assert exact result locations and open-file content. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e exact_search -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Ripgrep crates provide bounded cancellable scans with line/column previews and exclusion controls.
- [x] #2 Worktree and selected-commit searches are explicitly labelled and include correct uncommitted or committed content.
- [x] #3 Invalid/expensive regex, unusual paths and binary files show understandable bounded behavior.
- [x] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [x] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Embed pinned ripgrep matcher/searcher/ignore crates for bounded cancellable worktree scans and immutable Git-object scans. 2. Preserve raw paths plus visible source/revision/coverage and exact Unicode line/column anchors; explain ignore/binary/large/regex limits. 3. Test generated real repos, exclusion controls and cancellation; integrate exact_search keyboard/mouse PTY through parent-owned shell. 4. Run focused and full verification before completion.

Root native integration: build one prepared Search pane with grapheme-aware query/ref/name fields, shared typed mouse/key actions, explicit working/saved scope and corpus exclusions; worker-only scans and SQLite persistence; preview exact results and open immutable/mutable native file destinations. Add real-binary query_modes, exact_search and saved_searches journeys with independent HEAD/index/content/storage verification. Broader Ranked/GitHub/catalog dependencies remain open until implemented and verified.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Search backend preparation assigned local_git agent after actual identity/theme slice verification. Task dependencies remain integration gates; no task completion or dependency readiness claimed. Read task contract and architecture; official ripgrep15.1.0, nucleo-v0.4.0 and Tantivy0.26.2 source pins fetched for manifest-owner dependency audit. gh remains sole authenticated API transport; no mandatory rg executable or alternate ranked engine added.

Prepared explicit read-only Corpus collector for working files versus immutable saved Git objects, raw paths, ignore/hidden/binary/LFS/symlink/byte/document bounds and cancellation. Snapshot collection preflights immutable blob sizes using new GitRepository.object_sizes batched cat-file metadata and bounded content batches. corpus.rs remains unexported until manifest owner resolves official embedded ignore dependency; no mandatory rg process. Added actual generated-Git size/preflight/read-only test, execution pending current focused compile.

Manifest owner resolved embedded official ignore/grep crates; corpus+scan modules are exported and real generated-Git search contracts pass. Snapshot collection verifies actual immutable blob sizes before bounded batch reads, worktree retains uncommitted content and exact Unicode line/columns, unusual raw paths survive, .gitignore/.rgignore/hidden controls and binary/LFS/symlink/large limits are explicit. Added GIT_NO_LAZY_FETCH plus read-only empty GIT_ALLOW_PROTOCOL fallback for older Git so missing promisor objects never trigger silent transport; explicit fetch/write and lease ls-remote remain authorized. New negative-control promisor helper test is pending because current test archive hit hardquota, not counted as passed. Previous search9 tests passed; actual exact_search PTY remains required.

Checked exact_search gate passed10 code/provider and2 real-binary PTY tests, zero failures/ignored, actual keyboard and mouse input coverage. Artifact /tmp/ghtui-test-coverage-lYxJFT. Verifies explicit working/saved contents and line/column destinations at120x40/80x24, immutable original SHA file opening without checkout/index changes, traversal/symlink rejection, unavailable/promisor objects never silently fetching, regex/multiline/Unicode/exclusion/cancellation bounds, and one shared Arc source body for many matches rather than per-hit megabyte duplication. Full regression and broader Task45 Ranked/GitHub integration gates remain open.

2026-10-20 audit at 74360e48 (4/5 ACs met): Exact/regex local search is implemented and covered by PTY for both inputs. The only gap is a PTY journey that cancels a slow scan. Remaining: AC5: the PTY journeys cover phrase, regex, invalid regex, worktree/saved scope, exclusions and exact open, with keyboard and mouse at 120x40 and 80x24. No PTY journey cancels a slow scan. Cancellation is reachable (Action::Cancel and query edits set cancel_requested, src/search_ui.rs:490/558) but only the contracts test it.

2026-10-20 final push, verified at 60167066: node scripts/test-all.mjs 6107 passed 0 failed; fmt and clippy -D warnings clean. All audit gaps closed.
<!-- SECTION:NOTES:END -->
