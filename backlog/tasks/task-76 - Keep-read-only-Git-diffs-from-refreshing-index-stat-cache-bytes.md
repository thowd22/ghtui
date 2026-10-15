---
id: TASK-76
title: Keep read-only Git diffs from refreshing index stat-cache bytes
status: In Progress
assignee:
  - '@github_parallel'
created_date: '2026-10-15 01:54'
updated_date: '2026-10-15 03:50'
labels:
  - git
  - e2e
dependencies:
  - TASK-5
  - TASK-11
references:
  - >-
    https://git-scm.com/docs/git-config#Documentation/git-config.txt-diffautoRefreshIndex
documentation:
  - backlog/docs/doc-6 - Local-Git-workspace-and-operation-contract.md
priority: high
type: bug
ordinal: 5069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A genuine installed-Git control from the native selected-staging lane shows that ordinary worktree git diff may rewrite raw index stat-cache bytes even with GIT_OPTIONAL_LOCKS=0; the staged blob/mode/path remain semantically identical. Explicit diff.autoRefreshIndex=false preserves raw bytes in the same control. Read-only browsing promises to preserve the checkout/index, so implementation and independent test observers must not silently refresh it. Preserve the proof target/build/parallel-tree-native-diff-c2-independent-index-inspection-proof.json and do not misdiagnose an observer write as a repeated ghtui mutation.

## Testing steps

1. Code/provider tests: reproduce actual cached apply followed by a stat-cache-eligible unchanged worktree on real SHA1/SHA256 repositories. Independently compare raw index bytes and ls-files stage/blob/mode/path, HEAD/refs/config/worktree/untracked state before and after provider status/fingerprint/Working/Staged reads. Show the ordinary Git control really changes raw metadata without semantic change, and the application's read policy does not. Cover user config true, linked raw-byte worktrees, missing/stale/read-only failures and original deadline/cancellation/runner/capture accounting without new write authority.
2. Real-binary TUI E2E scenario readonly_index: fresh keyboard and mouse sessions browse Working/Staged/native views, search/select/resize/Source/Back on the exact stat-cache-eligible fixture. Assert completed visible content plus exact read argv and complete raw/semantic Git, SQLite/config/TTY preservation using independently read-only observers. Explicit confirmed staging remains the only authorized index write; canceled/stale inspection writes nothing.
3. Run cargo test --test tui_e2e readonly_index -- --nocapture and node scripts/test-task.mjs readonly_index with nonzero meaningful code and actual mouse/keyboard PTY counts. Then cargo test --all-targets, cargo fmt --check, cargo clippy --all-targets -- -D warnings, dependency/backlog audits and exact command/request-count regressions. Preserve failures and actual artifacts; no ignored tests or weakened raw-index equality.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Read-only installed-Git observations preserve exact raw and semantic index state even when Git would otherwise refresh unchanged worktree stat metadata; no global user config is modified.
- [ ] #2 The same original source, installed Git/retained runner, deadline/cancellation/capture limits and explicit mutation confirmations remain enforced; independent observers preserve raw state and strict argv checks are deliberately updated.
- [ ] #3 Meaningful genuine-Git provider contracts and task-specific keyboard/mouse real-binary PTY journeys pass with completed visible and independent command/storage/Git effects, wide/narrow/resize/failure coverage and positive nonzero counts.
- [ ] #4 Full all-target, fmt, strict-Clippy and audits pass with exact commands/counts/artifacts recorded; native macOS and release limits remain explicit.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run task-specific meaningful provider/code and keyboard/mouse real-binary PTY tests with nonzero actual selected counts and save exact results/artifacts.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check and cargo clippy --all-targets -- -D warnings before Done; preserve original raw-index and effect assertions.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Isolated 2026-10-15 implementation against immutable 9012a130: reproduce actual installed-Git cached-apply stat-cache refresh with an independent read-only raw/semantic observer; add only process-scoped read-policy diff.autoRefreshIndex=false while preserving write argv and existing runner/deadline/capture authority. Add genuine SHA-1/SHA-256 and linked/raw-path code contracts plus fresh keyboard/mouse readonly_index native journeys through Working/Staged, query/range/resize/Source/Back and cancellation/staleness. Run checked nonzero focused gates, whole all-targets, fmt, strict Clippy and dependency audit; export narrow baseline-relative hashes. Formal TASK-5/TASK-11 dependencies remain open, with native macOS/release evidence explicitly unrun; no acceptance/Done claim.

Reviewed extension after actual first fixed-policy gate: autoRefreshIndex=false also disables Git stat-only name-status filtering. Actual nine-variant installed-Git diagnostic proves --name-status -z --exit-code returns only the genuine changed path and documented exit1, unlike unqualified name-status. Root approved mutable-only names child --exit-code, accepting exactly exit0/1 through the same policy/runner/window/cap/token and preserving original raw-path count/parser/fingerprint guards. Saved argv, explicit writes and all timeout/cap/cancel/non-Exit failures remain unchanged; no temporary index, parser replacement or global configuration. Add real changed/nochange and error-status checks. Sources: official git-config diff.autoRefreshIndex; git-diff --exit-code; Git v2.53.0 builtin/diff.c494 and diff.c6781. Preserve old red and nine-variant diagnostic.

Final reviewed implementation supersedes failed --exit-code alternative: mutable names child uses ONE --numstat -z exit0 read through unchanged capture/token/window/runner; saved names and writes unchanged. New bounded protocol adapter crosschecks existing canonical FileDiff paths/order/count, exact numeric or paired binary fields, complete unique NUL raw paths and rename/copy sentinel. Side absence comes only from exact single already-parsed new/deleted-file-mode six-octal metadata; conflicting, malformed or unclassifiable metadata refuses. No external/textconv flag relaxation, failed-stdout raw IDs, second patch parser, additional child or inferred change kinds. Protocol/code/native/full gates remain required; prior negatives retained.

Root admission 2026-10-15: validate frozen final eleven-source patch SHA1e40412f and output hashes, compose only narrow hunks onto current536+Stage22+Task77 candidate, preserving actual deadline_v2 runner body, write argv and Task41 guards. Adapt this native test ONLY to the now-real typed Stage/Unstage read-only preview and exact Esc return, not old disabled behavior or OR expectations. Keep raw index/semantic Git/storage/strict-gh/TTY assertions unchanged. Run checked readonly_index plus selected-plan regression, then coherent current full/fmt/strict and audits; record all prior/current failures and no native macOS or dependency completion.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Executed intermediate gates (not final): original policy four-code red2pass2fail; first false-policy eight-code3pass5fail; --exit-code attempted eight-code3pass5fail, initial2native0pass2fail. Preserved exact logs in target/parallel-readonly-index.Px2LHK. First application-red trace was inside worktree (causal attribution not exclusive); moved actual trace under .git and will re-run final original-policy negative. Crucial actual complete-argv diagnosis: --exit-code only content-filters name-status when external diffs are allowed; production --no-ext-diff must remain. Existing runner sanitizes failed Exit stdout, so raw NUL identifiers cannot be admitted from exit1. Revised numstat protocol proposal is under Root review, no old failure waived or acceptance checked. Actual installed Git2.53.0 and primary diff-format documents numeric/binary fields, NUL paths and old/new rename sentinel; independent raw-index observer itself uses no-optional-locks plus process config false.

Final-direction checked runner first terminal green: 21 actual code/provider contracts + 6 real-binary PTYs, zero failed/ignored, actual keyboard+mouse records in target/test-tmp/ghtui-test-coverage-qaikdh (copy log readonly-index-checked-final-first.log). Genuine same-retained-runner exact former READ policy independently refreshes raw index with unchanged semantic stages/HEAD-file/refs/config/work bytes+modes; no trace inside worktree, resolving causal-control ambiguity without rewriting earlier negative. Real readonly .git directory/foreign index.lock canary and pure/edited/raw-quoted actual copy producer pass. Root-reviewed unknown-mode evidence correction now in NEW final delta: no guessing both sides for pathless ordinary metadata, exact old/new-mode pair or actual binary/new/deleted evidence required; actual typechange duplicate-path refusal is explicitly not support. Final checked23/6, whole/fmt/strict and audits remain pending before final handoff. Review-only immutable production3 patch/JSON are target/build/provider-parallel-readonly-index-v1-review-20261015.{patch,json}; not a standalone admission because new test children are omitted. Formal deps/Mac/Done unchanged.

First complete gates are explicitly NEGATIVE and retained: readonly-index-whole-first.log stopped at lib1433pass/1fail0ignored65.92s because unchanged genuine combined-conflict factory lost its prior conflict/combined dedicated-viewer guidance (adapter still refused). Corrective V3 preserves that existing count-mismatch explanation without accepting any extra path/side. Strict readonly-index-clippy-first.log found4 ambiguous NUL literal spellings plus duplicate positive-PID module loading; NUL bytes stay identical with hex spelling. Root-approved TEST-only central registration in tests/tui_e2e.rs and imports in existing Related/newReadonly remove duplication; shared positive_pid_publication helper bytes remain SHA8c64197558fd8780c743ffecad5d39670f8f2c657f2295143d173b061c1ca6d1. No lint/assertion waiver. Frozen V2 nine-source focused artifact87d29a95... and manifestdaeb4f6c... remain unchanged and are not whole-green admission. V3 checked23/6 and rerun whole/fmt/strict pending; no task completion or borrowed Main green.

V3 whole actual terminal101: all2035code/provider PASS (1434lib+318strictGH+283other),656PTY650pass6fail0ignored152.22s. Exact negatives retained readonly-index-v3-whole-first.log: existing Actions controls, two collaboration exact-source close observers, Tree full-queue retry, catalogue nested-close, and own new keyboard Options→body barrier. Own raw terminal artifact target/parallel-readonly-index.Px2LHK/target/tui-artifacts/1114136-1792033411615508898 shows legitimate Staged New selected row, original range2/query2/statusdisabled/currentSourcecontrol with Old row scrolled off. Corrective V4 test-only barrier requires strict NEW completed exact file/range/query/body/Source controls instead of invalid Old-visibility assumption; no extra seek keys/timeout/effect or snapshot waiver. V3 strictClippy terminal0(59.75s), fmt0; related_pr_avatars checked regression16code/16actualPTY0failedignored2.11s (coverageNkbMKY), sharedPIDbytes unchanged. Final V4 focus/full/strict pending; do not borrow newer Main cohort or claim TaskDone.

Final immutable Task76 V4 source11 handoff frozen: target/build/provider-parallel-readonly-index-v4-sources-20261015.patch SHA1e40412fc373e030dab4ad23c7586daf2666845cd22824454bf6d3f306a90354 and final manifest target/build/provider-parallel-readonly-index-v4-handoff-20261015.json. Exact9012 isolatedcopy target/parallel-readonly-index.Px2LHK; all11 finalsource hashes reverified. Counted readonly_index23code/6actualPTY GREEN0ignored (0.78s/2.26s,yVY0tS), sharedPID Related16/16GREEN,NkbMKY, unchanged conflict regression1/1GREEN; strictalltargetClippy0(8.36s),fmt0,dependency86/10300/108/117/backlog76valid0. Current whole remains RED:1637codepassed/1managed-corpus failed at alias retry671 (initialbudgetrefusal passed), strictprovider/PTynotreached. Independent entirePTYtarget actually652pass/4fail/6560ignored152.70s; all6newreadonly journeysPASS; retained negatives are blendedNotice, localcommit source-form65536, Graph09800batch, reactionCreated-unavailable; exactlogs/artifacts/hashes inmanifest. PriorV3whole2035codeallgreen/650of656PTYred and firstconflict/Clippy/otherintermediatenegatives explicitlyretained. No unrelatedtest/productfix/waiver, Task47actualhypothesis separatelyrecorded. Rootmustcompose narrowpolicy pair INSIDE newerStage deadlineDelegate and replace frozen9012Unavailable-preview expectation with exact currentMain typedStagePreview+Stage/Unstage,0Write/allindependentpreservation unchanged; no ORfallback. Regular→symlink2patch→1numstat remainsfailclosed Task31followup. NoMainwrites/Task76Done/AC/DoD/nativeMac/fullgreenclaim.

Root current Main admission first checked readonly_index passed: 23 meaningful code/provider +6 real-binary keyboard/mouse PTYs,0failed/ignored,2.62s native. Log target/build/root-readonly-index-v4-checked-first.log; coverage target/test-tmp/ghtui-test-coverage-A3KSzS. Native journey deliberately exercises actual ready Stage/Unstage selected preview and EscBack instead of obsolete disabled assertion; independently checks raw index, semantic stages, HEAD/refs/config/worktree/modes/untracked/storage, exact READ-policy argv and zero Git writers before/after preview. Three older parent files composed with536/Stage, new/test outputs hashvalidated. Full current integrated gate pending; all prior negative whole evidence preserved; Task76 remains In Progress.

2026-10-15 Root verified combined Main checkpoint: cargo test --offline --locked --all-targets terminal0 with2141code/provider (318strictGH)+690realbinaryPTY0failed/ignored, native98.71s; cargo fmt--all--check0, cargo clippy --offline --locked --all-targets -- -D warnings0(58.59s). Frozen76-path root-inline-normal-readonly-selection-freeze-v1-20261015.json source hashes held unchanged across gates; subsequent README-only documentation updates actual counts. Checked inline59/8, selectedplan21/6, readonly23/6, normal8/2, stash15/12, completedframe19/8, corrective18/8 and18continuationPTY all independently passed. Full/quality/focused exact logs and SHA are recorded in root-inline-normal-readonly-selection-final-gates-v1-20261015.json. Dependency audit86roots/10300files/108adaptations/117registry0, current backlog78valid0 after CLI correction. Cache-journal candidate and full-side WIP are NOT admitted. Earlier Main8negative and isolated candidate negatives remain explicit; TASK78 owns controlled generic focus/scroll/overlay diagnosis. This verified Linux feature checkpoint is not full task/dependency/native-macOS/release completion.
<!-- SECTION:NOTES:END -->
