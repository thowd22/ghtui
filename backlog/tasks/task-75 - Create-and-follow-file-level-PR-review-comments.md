---
id: TASK-75
title: Create and follow file-level PR review comments
status: To Do
assignee: []
created_date: '2026-10-15 01:36'
updated_date: '2026-10-15 17:19'
labels:
  - review
  - e2e
milestone: m-2
dependencies:
  - TASK-19
  - TASK-21
references:
  - >-
    https://docs.github.com/en/rest/pulls/comments#create-a-review-comment-for-a-pull-request
documentation:
  - backlog/docs/doc-3 - PR-action-coverage-and-parity-contract.md
priority: high
type: feature
ordinal: 4069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
GitHub supports review comments on a whole changed file separately from line/range comments. The current InlineComment model requires positive line coordinates, so claiming full PR review parity from TASK-19 alone would omit that UI workflow. Preserve the original selected repository/PR/file identity without inventing line0, a hunk, or a comment ID.

## Testing steps

1. Code/provider tests: Meaningful provider and model tests with strict offline fake-gh: subject_type=file payload omits line/range coordinates; independently validate created and fetched author UID/node/parent/path/head/body and file-level subject. Test renames, absent/binary/large patches, capability refusal, changed head/file membership, cancellation, capture/deadline caps and uncertain outcomes.
2. Real-binary TUI E2E scenario file_comments: fresh keyboard and mouse journeys select a genuine changed file, compose/preview/cancel/confirm, resize/scroll/return, restart a saved draft, and inspect a reply. Independently assert completed visible state, exact single POST+GET effects, journal/draft/SQL state and unchanged local Git/config/TTY. Source loss must hide private content; ambiguous outcomes must never resend automatically. Live writes only to an explicitly configured disposable repository.
3. Run cargo test --test tui_e2e file_comments -- --nocapture and node scripts/test-task.mjs file_comments with nonzero executed code and PTY counts and positive actual keyboard/mouse journal entries. Then cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings. Record exact commands/counts and retained failure artifacts; no ignored tests or accepted-by-default snapshots.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Changed-file controls create an explicitly file-level comment through gh without fabricated line, range or thread identity; all actions have equivalent keyboard and clickable mouse semantics.
- [ ] #2 Preview and explicit Confirm retain original host/account/repository/PR/head/file provenance, revalidate current source and capability, persist drafts, and require exact independent creation/readback receipts before success.
- [ ] #3 File comments and their replies are visibly distinguished from line comments and PR conversation comments, with consistent actual usernames/avatar fallbacks, semantic labels and outdated/unavailable states.
- [ ] #4 Nonzero meaningful provider/code tests and task-specific real-binary mouse/keyboard PTY journeys pass with independent visible plus command/storage/Git effect assertions; full all-target/fmt/strict-Clippy gates and exact evidence are recorded.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run meaningful task-specific code/provider and real-binary keyboard/mouse PTY tests with nonzero actual selected counts; record exact commands, results and retained artifacts.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify clickable mouse/keyboard action parity and applicable focus, resize, scrolling, overlay, loading, disabled and failure states with independent visible plus command/storage/Git effects; native macOS status remains explicit.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
2026-10-15 graph_parallel Root-requested PROPOSAL ONLY before implementation: read Task75/deps19/21 +Docs3/4/10, inspected actual line-only InlineComment, genuine Task21 Files-member/Detail union, same Thread Work/result/owner, bounded ACK/atomic draft cleanup/journal/fence and common-origin B2 review API; checked current official REST review-comments and GraphQL Pulls schema. Proposal target/build/graph-parallel-task75-file-comments-proposal-v1-20261015.json requests Root review for closed genuine selected-file intent and no-line provider/payload/201+exactGET+FILE-node receipt, same existing Thread queue/owner/result plus shared Form, one original measured prewrite pool+fixed7 prepaid postwrite slots, bounded atomic versioned file-origin/history, and canonical Catalogue/Personal common-origin consumer composition. No viewerCanComment field exists in consulted schema; no push/viewerCanUpdate/login permission inference, no fabricated line1/range/thread. K/click file-level control proposed separately from line P. Meaningful file_comments code/provider and real paired key/mouse/TabEnter/native/draft/restart/heldcancel/partial/sourceUID/effects/TTY scenarios outlined; Root sole heavy verifier. ALL tests are PLANNED, no new declarations/source activation/builds/Main edits/AC/Done. Task75 remains To Do; dependencies19/21 OPEN until actual accepted verification.

Root reviewed full553695ad Task75 proposal and APPROVED same Thread Work/result/Inbox owner/shared Form, distinct no-line genuine file payload/receipt, bounded atomic file-origin/history and canonical Personal common-source composition. Exact typed201/cap addendum graph-parallel-task75-file-comments-201-caps-addendum-v1-20261015.json records SAME prepaid ordinary runner fixed POST --include path/payload, one private borrowed existing HTTP envelope factor preserving all old204 byte fixtures/policies, actual201+bounded JSON THEN exactPOST/GET/FILE-node receipts; missing/wrongstatus afterinvoke uncertain/noresend. Root also APPROVED full NEW source-only coherent implementation after this addendum: authenticate current438/973+frozenTask21 coherent73+approvedPersonal common-origin seams, no Main/heavy/frozen edits. Postcaps262144/262144/262144/65536/131072/131072/65536 sum1179648; WHOLE8MiB carve measured pool after source/final/prepaid reserves, default2Filespages cap19totalchildren/2301952bytes, hard30pages cap47children/5971968bytes; each original lesser cap/deadline remains lesser. All12 native paired families and code groups remain required, Personal/follow/history not deferred as completion. Root sole tests; all declarations/runs currently0; formal19/21 readiness and AC/Done stayOPEN.

Required Personal restart inspection: distinct read-only actual Summary/PageTicket/PrParentIntent history origin, not Files/PersonalPrSource promotion. Proposed shared bounded presentation helper is Github-owned; existing B1 PersonalPrAccessSession reserves one measured2-child128KiB raw-parent pool and already closes actual repo->exact history SQL/body/ref->final numeric UID. Exact8child/663552-byte upper ledger. Same Thread Inbox queue/owner/result, purpose-specific inspect guard only, original body stays onApp, no ResourceLocalGuard relaxation/ACK/replay. Material API proposal target/build/graph-parallel-task75-personal-summary-history-proposal-v1-20261015.json awaits Root review before shared implementation. All12paired Task75 families remain required/unrun.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Source-only intermediate provider checkpoint frozen: target/build/graph-parallel-task75-file-comment-provider-v1-sourceonly-20261015.json SHA c0384293914ac0cc386b0a4b128efb96acc0349fe0179565506cde9b9d024267; patch aa22e6481e02f70fffa48969e5e40e0734130fdf11ac645f66135a40c7473c8f, archive435f7326c21b98c5157cf8c0eb741c5605ab77ea347f7a0e25a78568e3c7e941.11paths/5new, baseline43832+all73 coherentTask21 exact hashes.15code declarations (12genuine provider,3framing/compat),0PTY/activation so far; allUNCOMPILED/UNRUN, Root sole verifier. Existing204 test block remains byte-identical; same7prepaid cursor, original measured/token/runner/clock/cap and full raw-parent equality. Controller/sharedForm/origin/restart/history/Personal commonThreads and all12paired native families remain required next. No TaskDone/AC/dependency or strict-gate claim.

Source-only implementation continuation: authenticated cumulative Personal B2 V4 plus V14 Summary descriptor. Closed history origin Read | actual PersonalSummary now being composed; initial same-iteration J forwarding captures the actual database and descriptor token/deadline before the existing Inbox queue. Fresh history uses the unchanged original Core, exactly two repository observations plus opening/closing raw PR, original DB identity/marker/ref/body witness, bounded preparation before final SQL/numeric UID. No Personal source mint, Files blocker bypass, new queue, read-to-write/retry/ACK authority, build, native execution or dependency/Done claim.

Source-only coherent checkpoint being frozen: Task75 provider V1 plus authenticated cumulative Personal B1/B2V4/V14 prerequisites, common Files|Detail Thread migration and actual Summary-history phase V2. Consumer currently67 changed paths, expected33 code/provider declarations and26 real-binary keyboard/mouse wrappers, all UNCOMPILED/UNRUN; rustfmt syntax-only exit0 is not test evidence. Declared journeys include actual201/GET/FILE receipt, fresh FILE-root reply, readonly preview/resize/pureBack, changed member, live pre/post invocation cancellation, real cleanup SQL failure, external same-DB replacement, malformed numeric-author receipt, renamed/removed/binary/large/no-patch members, Personal queue entry and genuine Native-without-range return. Full changed-H/recreated-repo/account/newer-owner native matrix, File-specific caught-panic/mixed result tests, Personal protected restart native and complete FILE-thread edit/delete/reaction/resolve parity remain explicitly OPEN. Root remains sole heavy verifier; Task19/21 dependencies and all Task75 AC/Done remain OPEN. Frozen minimal Summary API V2 was independently source-authenticated/read/approved by Root, four declarations UNRUN and zero native coverage; it does not confer publication authority.
<!-- SECTION:NOTES:END -->
