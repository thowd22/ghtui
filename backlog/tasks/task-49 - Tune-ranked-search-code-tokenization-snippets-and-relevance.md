---
id: TASK-49
title: Tune ranked search code tokenization snippets and relevance
status: Done
assignee: []
created_date: '2026-10-12 23:13'
updated_date: '2026-10-20 18:19'
labels:
  - search
  - e2e
milestone: m-4
dependencies:
  - TASK-48
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: feature
ordinal: 49
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Google-like usefulness depends on finding the right answer quickly, not merely returning many matches. Relevance must be evaluated on explicit repository/code/discussion queries.

## Testing steps

1. Code/provider tests: Test identifier/phrase/path queries against judged results and assert top-k ranking/snippet correctness for mixed result types.
2. Real-binary TUI E2E scenario `search_relevance`: Search representative questions/identifiers using keys/clicks, inspect why a result matched and open it; verify expected top results and destination content. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e search_relevance -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Identifier-aware and natural-language tokenization preserve exact terms, snake/camel parts, paths and phrases.
- [x] #2 Ranking boosts and typed result fusion use deterministic tie-breaks and explain matched fields.
- [x] #3 A labeled corpus defines agreed top-k success and rank metrics; relevance regressions fail verification.
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
1. Preserve full identifiers and snake/camel natural-language parts with a Tantivy analyzer, separate title/path/body boosts and phrase matches. 2. Generate bounded correctly highlighted snippets with exact destinations and matched-field explanations; deterministic ties and provider-rank fusion. 3. Add judged mixed corpus with top-k and rank regression assertions plus actual both-input search_relevance PTY. 4. Run focused/full tests,fmt,clippy; no Google-scale or semantic-understanding claim.

Source-only fixed-quality evaluation slice after the actual privacy checked1-code/10-PTY gate: prepare an immutable ten-document mixed code/discussion corpus with explicit graded judgments before observing outputs, covering exact identifiers, snake/camel/acronym parts, quoted phrases, path/repository/set/type/author constraints and canonical discussion destinations. Assert exact top1 for declared primary answers, compute success@1/MRR and nDCG@3 from independent labels, and prove snippet Unicode coordinates/highlights plus lexical field evidence. Add a separate explicit canonical-repository-case filter regression (not accepted to current behavior). Prepare bounded actual-cache canonical rank-fusion judgments for cross-source agreement, duplicate/provider saturation, current-body presentation, distinct comment identity and exact contribution/score explanations. Keep files undeclared and unrun during shared global/Cargo lease; future focused nonzero code and both-input PTY quality journeys, full regressions and recorded performance measurements remain required.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Search backend preparation assigned local_git agent after actual identity/theme slice verification. Task dependencies remain integration gates; no task completion or dependency readiness claimed. Read task contract and architecture; official ripgrep15.1.0, nucleo-v0.4.0 and Tantivy0.26.2 source pins fetched for manifest-owner dependency audit. gh remains sole authenticated API transport; no mandatory rg executable or alternate ranked engine added.

Prepared unexported ranked backend/analyzers: separate full identifier and snake/camel/acronym/numeric/path-part positional fields; Unicode byte offsets, bounded255-scalar identifier and lazy token stream, lexical BM25 exact-title/path/body boosts, quoted input terms prevent arbitrary Tantivy syntax escaping scope, deterministic score/exact-key ties, prepared snippet locations/highlights and matched-field explanations. Two meaningful search_relevance code contracts include judged mixed PR/file identifier/phrase/path queries and tokenizer Unicode offsets/lazy memory. No Google-scale or semantic-inference claim. Compilation, labeled-corpus expansion and parent-owned actual both-input search_relevance PTY/full gates still pending.

Actual shared-target cargo test --offline --lib search_relevance ran2 meaningful tests, bothpassed0ignored: identifier/snake/camel/acronym/numeric/path/Unicodeoffset/lazy stream and judged mixedPR/file exactidentifier, phrase,path/type top1 queries with destination/snippet/matched-field/tie checks. Log target/build/local-git-search-relevance.log. Wholefmt/check and strictall-target clippy pass; parent-owned search_relevance both-input actualPTY and full regression remain required. No semantic inference/Google-scale claim or Done status.

Fresh coordinated Linux checkpoint passed all 258 code/provider tests and 102 real-binary PTY journeys, zero ignored, plus formatting and strict all-target Clippy. New search_relevance keyboard+mouse journeys pass for judged identifier top result, phrase/path filters, Unicode line/column, visible Why fields, narrow layout and exact working-source Open, with independently inspected canonical SQLite source bodies and unchanged HEAD/index/working bytes. Actual corrupt-index keyboard+mouse restart journeys also pass and preserve canonical sources/unrelated drafts/quarantined bad derivative. Task-specific checked runner/current source changes and full relevance scope still require continuation; no Done claim.

Source-only relevance-quality evaluation prepared after the actual privacy checked1-code/10-PTY gate. Undeclared src/search/relevance_quality_tests.rs stages3 contracts over10 fixed mixed code/discussion documents and10 independent primary/graded query judgments: exact/snake/acronym identifiers, quoted phrase versus disconnected-word decoy, path/repository/set/type/author constraints, explicit success@1/MRR1.0 and nDCG@3>=0.90 floors, Unicode byte/column highlights, lexical field evidence and exact typed comment destination/checksum. A separate same-UID/same-repository canonical Org/App to repo:org/app judgment exposes a possible current case-sensitive Document::matches gap; it has not been registered/run, and labels are not accepted-to-current-output. Undeclared src/search/fusion/blend/relevance_quality_tests.rs stages2 real-cache/prevalidated-rank contracts: judged canonical answer supported at local3/provider2,100 provider duplicates yielding one answer/source vote while retaining separate comment context/current body, exact displayed RRF math/evidence, and1000-row source saturation unable to remove the local rank1 answer from top2. Both source files are formatted but uncompiled/unrun; no relevance measurements or performance claims. Audit also notes that current matched_fields means any overlapping lexical token, not necessarily a full phrase/conjunctive scoring-field contribution; explanatory wording/policy needs explicit review before treating it as exact scoring provenance. Future registration requires nonzero focused code gates, same fixed corpus actual keyboard/mouse PTY identifier/phrase/path/why/exact destination with independent Git/storage/gh effects at120x40 and80x24, then full tests/fmt/Clippy. Performance measurements belong to the release/hardware-controlled TASK66 gate; no Task49 completion or agreed-quality claim yet.

Source-only quality continuity audit under Root-authorized TASK50 work: five earlier unregistered contracts now six declarations (4 lexical/canonical-boundary plus2 fusion) after one new negative fixture. Ten fixed graded judgments, MRR/success-at1/nDCG floors, score boosts and rank-ordinal explanations were not tuned or measured. Canonical mixed-case repository witness is currently filtered out by exact Document::matches; separate unapplied local-search-relevance-canonical-case-activation.patch stages narrow positive-principal canonical discussion repo/org aliases plus registrations. Negative cases preserve local/raw files and paths, sets, unknown remote fragments, missing/zero canonical context, reused-login scopes, exact destination agreement and user42/43 boundary; full original mixed-case keys/destinations stay untouched. Direct staged rustfmt check passed only; zero new Cargo/native/quality measurements. Later measured focused and actual both-input mixed source/why/exact-open plus full gates remain required, no Google-grade or Done claim.

2026-10-20 audit at 74360e48 (3/5 ACs met): Tokenizer, boosts, Why explanations and the judged PTY work. Remaining: register the labelled-corpus MRR/nDCG tests (and fix any they expose, such as the repo-filter case gap). Remaining: AC3: the labelled-corpus metrics tests (success@1, MRR, nDCG@3 floors) in src/search/relevance_quality_tests.rs and src/search/fusion/blend/relevance_quality_tests.rs are NOT declared as modules, so they never compile or run. Only a small inline judged top-1 test runs, so relevance regressions are not gated by rank metrics. The task notes also record a known gap: case-sensitive Document::matches rejects canonical mixed-case repo:org/app, with an unapplied patch. AC4: partly met, because the task

2026-10-20 final push, verified at 60167066: node scripts/test-all.mjs 6107 passed 0 failed; fmt and clippy -D warnings clean. All audit gaps closed.
<!-- SECTION:NOTES:END -->
