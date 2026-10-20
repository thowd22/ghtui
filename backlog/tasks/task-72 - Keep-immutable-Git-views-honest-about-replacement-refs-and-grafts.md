---
id: TASK-72
title: Keep immutable Git views honest about replacement refs and grafts
status: Done
assignee:
  - '@local_git'
created_date: '2026-10-13 00:56'
updated_date: '2026-10-20 14:09'
labels:
  - git
  - e2e
dependencies:
  - TASK-5
  - TASK-2
references:
  - 'https://git-scm.com/docs/git-replace'
documentation:
  - backlog/docs/doc-6 - Local-Git-workspace-and-operation-contract.md
priority: high
type: bug
ordinal: 1069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replacement refs and legacy grafts can change effective object bytes or ordered parents without changing the requested SHA. Exact saved-version views and search need original-object provenance without silently changing normal Git mutation behavior.

## Testing steps

1. Code/provider tests: generate real repositories with replaced blob/commit objects and legacy grafts; independently compare original and Git-effective bytes/parents, flags, saved-search matches, exact destinations, guarded mutation effects and unchanged HEAD/index/working files.
2. Real-binary TUI E2E scenario `immutable_overrides`: keyboard and mouse fresh fixtures inspect notices, Graph/Tree/original file content and history guard at 120x40 and 80x24. Verify no gh/transport/override/ref/index/file mutations and restored terminal.
3. Run `cargo test --test tui_e2e immutable_overrides -- --nocapture` and `node scripts/test-task.mjs immutable_overrides` and verify nonzero code/provider and both-input PTY counts; then `cargo test --all-targets`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and record actual results/artifacts.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Exact-SHA saved graph/tree/blob/diff/search reads use original objects and ordered parents without replacement refs or grafts, retaining canonical identities
- [x] #2 Repository status and saved previews visibly distinguish replacement/graft overrides from original-object views in wide and narrow layouts
- [x] #3 Local mutation semantics are not silently changed; unsupported history operations expose a safe guard/explanation while ordinary staging retains documented installed-Git semantics
- [x] #4 Generated-Git/provider contracts and actual keyboard/mouse immutable_overrides PTY pass, preserving state and recording nonzero counts/artifacts
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [x] #1 Run meaningful immutable_overrides code/provider and actual keyboard/mouse PTY with nonzero counts and saved artifacts.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check and cargo clippy --all-targets -- -D warnings before completion.
- [ ] #3 Verify wide/narrow notices, guarded operations, original/effective object distinction and unchanged Git/transport effects; dependency gates remain required.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Coordinate RepoStatus replacement/graft flags with parent/foundation. 2. Separate original immutable reads from ordinary working/index/mutation calls and show a provenance notice. 3. Add real original-versus-effective Git contracts plus both-input PTY at120/80 columns. 4. Coordinate focused/full verification and document guarded limitations before completion.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Foundation integration prepared: Workspace saved revision resolution now uses GitRepository::resolve_original, preserving the parent-owned working/saved search file viewer method. LocalCommand disabled_reason visibly guards unsupported history writes when replacement/graft overrides are active, while Stage/Unstage/canonical line patches/explicit Fetch remain available with installed-Git semantics. Native conflict viewers retain exact RawPath context and display bounded immutable Base/Ours/Theirs bytes safely, not just object IDs. Actual immutable_overrides focused/full/PTY execution pending coordinated stable Search integration; not marked Done.

Original-object source split, replacement/graft RepoStatus flags, persistent native notice and history guard implemented. Dedicated actual keyboard/mouse Graph to original commit/parent to saved file to narrow resize to guarded Commit journeys prepared, with independent HEAD/index/refs/graft/working/Gh effects. Three meaningful immutable_overrides code contracts prepared (replacement blob/commit/tag, graft parents/diff, saved search original Unicode bytes), plus root/merge-parent original CommitChanges assertions. Coordinated focused/full execution pending; no criteria marked verified and no Done claim.

Verified actual current source: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true node scripts/test-task.mjs immutable_overrides selected3 meaningful code/provider tests and2 real-binary PTY journeys; allpassed,0ignored, keyboard=true,mouse=true. Artifacts /tmp/ghtui-test-coverage-cMwTjO; target/build/local-git-immutable-checked.log. Wide120x40/narrow80x24 original-parent/file/override notice and disabled history action verified with independent unchanged HEAD/index/refs/graft/working/Gh effects. First attempt used broad overlay title that also matched persistent toolbar; corrected exact overlay title and reran without weakening assertions. Whole cargo fmt --check and cargo clippy --offline --all-targets -- -D warnings passed; full cargo test --all-targets and dependency finalization remain pending, so task stays In Progress.

Superseding coordinated actual Linux checkpoint: cargo test --offline --locked --all-targets passed288 code/provider tests and114 real-binary PTY journeys, 0 failures/ignored, plus whole fmt and strict all-target Clippy. Actual immutable_overrides-named subset included5 code contracts (raw replacement prefix unit, three generated Git identity/graft/replacement contracts and original saved corpus coverage) and4 actual keyboard/mouse PTY journeys (original tree/graph guards plus raw-ref/non-UTF8 replacement-scope sibling safety). target/build/root-all-targets-checkpoint.log lines1360,1471/1475/1483,1683 and1756–1759 retain evidence. Non-UTF8 reference names are preserved raw, rendered aliases are never parsed as mutation argv, named picker operations are explicitly disabled while SHA browsing remains supported, and valid Unicode siblings are unchanged. var_os replacement-base bytes are honored rather than defaulted. Portable saved Git-object path fixtures always run on macOS/Linux; only physical filesystem fixtures differ according to APFS valid-UTF8 naming constraints. Native ref-write support for arbitrary bytes remains an explicitly guarded limitation, not a full Git parity claim. Status remains In Progress until full product task-finalization audit; no macOS execution claimed.

Root Graph admission exposed genuine original-object small HEAD capture overflow from graft deprecation advice. Existing original reads deliberately set GIT_GRAFT_FILE=/dev/null; now add only command-local -c advice.graftFileDeprecated=false before actual original-object argv. All stderr/error handling, raw parent semantics,32MiB child/64MiB measured pool/128children/30s capture limits remain unchanged. Graph lib previously5/34 now32/34 after fix; failures elsewhere retained. This is not fresh full immutable_overrides/whole/platform acceptance. Impacted raw replacement/graft and full suites remain required before delivery.

2026-10-20 audit at 74360e48 (full suite 6000/0, fmt+clippy clean): all ACs met, confirmed by an independent check. Original-object reads, override notice and history-write guard are in place with 4 Git/search contracts and 4 keyboard/mouse PTYs. All ACs checked. Status could move to Done.
<!-- SECTION:NOTES:END -->
