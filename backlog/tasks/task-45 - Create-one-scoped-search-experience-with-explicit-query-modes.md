---
id: TASK-45
title: Create one scoped search experience with explicit query modes
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
  - TASK-10
  - TASK-9
  - TASK-34
  - TASK-35
references:
  - >-
    backlog/docs/doc-10 -
    GitHub-provider-native-API-contracts-and-verification.md
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: feature
ordinal: 45
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The user needs rapid discovery across repositories, code and discussions. Fuzzy navigation, ranked text, exact/regex scans and remote search have different semantics and should be visible choices.

## Testing steps

1. Code/provider tests: Test query parsing, escaping, applicable filters, provider translations, result identities and incomplete remote coverage.
2. Real-binary TUI E2E scenario `query_modes`: Search a selected corpus via keys/clicks, change modes/filters, inspect a preview and open a result, then verify query contract and revision/thread destination. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e query_modes -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Search supports Ranked/Exact/Regex/GitHub modes and applicable repo/org/set/type/path/language/actor/state/date/revision filters.
- [x] #2 Query validation never silently weakens unsupported remote regex/revision/comment semantics.
- [x] #3 Results have type/source/scope/freshness, highlighted preview and correct destination context.
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
1. Preserve explicit Ranked/Exact/Regex/GitHub query semantics and typed host/account/repo/revision/resource destinations. 2. Implement cancellable selected Git/worktree corpus and bounded previews without draw I/O. 3. Expose typed SearchWorkspace/SearchAction integration API to root-owned input/runtime/rendering. 4. Add meaningful query/provider tests and actual keyboard/mouse PTY query_modes journeys, then full tests/fmt/clippy. Uncompleted dependencies remain acceptance gates.

Remote provider slice: translate shared Query only in explicit GitHub mode, reject unsupported regex/revision/comment/set filters, enforce endpoint-specific PR/issue types and selected repository scope; request text-match metadata with explicit paging/1000-result caps and incomplete reasons. Treat code item SHA as blob identity, not a commit. Exact file navigation requires separately resolving current default-branch commit and verifying the blob at that immutable path. Contract-test command argv, scope, malformed/stale identities, caps and provenance; integrate native mouse/keyboard PTY search through root/local_git before task completion.

Root native integration: build one prepared Search pane with grapheme-aware query/ref/name fields, shared typed mouse/key actions, explicit working/saved scope and corpus exclusions; worker-only scans and SQLite persistence; preview exact results and open immutable/mutable native file destinations. Add real-binary query_modes, exact_search and saved_searches journeys with independent HEAD/index/content/storage verification. Broader Ranked/GitHub/catalog dependencies remain open until implemented and verified.

Native GitHub integration uses fresh numeric-ID identity verification before and after scoped remote reads; map explicit CLI/saved repository sets into provider scope without using the local/current fallback as a GitHub repository. Preserve provider fragments as unlocated blob candidates; opening verifies the exact blob at an immutable commit before displaying bounded full-file content. Principal switches hide stale private Search/GH/Actions models without deleting stored data.

Add native discovered-PR opening as a separately scoped entry point, not personal-inbox membership bypass. A remote search result opens only its verified host and explicitly selected repository/positive PR number; canonical fetched details must match that key. Do not fabricate authored/assigned/review membership or mutate the personal inbox. Preserve exact selected PR on subsequent detail refresh, account checks before/after reads, and independent local Git state. Add meaningful pre-request invalid-scope/canonical-body contracts and actual query_modes keyboard/mouse PR-result journeys with complete strict gh detail fixtures, owner/author fallback, conversation content, narrow resize and returning to unchanged Search. Native issue/repository openings remain a separate unfinished slice.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Search backend preparation assigned local_git agent after actual identity/theme slice verification. Task dependencies remain integration gates; no task completion or dependency readiness claimed. Read task contract and architecture; official ripgrep15.1.0, nucleo-v0.4.0 and Tantivy0.26.2 source pins fetched for manifest-owner dependency audit. gh remains sole authenticated API transport; no mandatory rg executable or alternate ranked engine added.

Implemented preparatory canonical search model in src/search/model.rs: scoped raw-byte/SHA document keys, typed source/destinations, metadata filter application, exact result coordinates and versioned saved searches. Added tests/search_contracts.rs for account/revision/raw-path roundtrip, supported filter semantics, wrong destination/traversal rejection and saved-view SQLite restart/account boundaries. Actual focused compile/test is running; no source-corpus or UI completion claimed.

Provider checkpoint 2026-10-13: actual workspace provider_contracts selected 67 tests, all passed, zero ignored, using explicit shared-target jobs2/incremental0/debug0 environment and cargo test --offline --test provider_contracts. cargo fmt --check passed. Doc-10 records APIs, exact guards, primary sources, artifact/secret bounds and remaining integration gaps. Full-suite/clippy attempts after this checkpoint encountered concurrent Search Nucleo Config::Default compilation changes (owner corrected); current all-target rerun and native task-specific keyboard/mouse PTY remain outstanding. No AC/DoD or Done claim from these provider-only results.

Focused actual cargo test --offline --test search_contracts9 passed: canonical query/destination/filter contracts, generated worktree/saved Git corpus, Unicode/raw paths, explicit exclusions, multiline scan/cancellation, saved SearchWorkspace preview/revision and Nucleo navigation. src/search/workspace.rs now provides typed prepared SearchAction/SearchWorkspace/SearchPreview API for root UI/runtime integration, no draw I/O; exact/regex are implemented, Ranked requires pending Tantivy and GitHub uses separate provider. Whole fmt and strict all-target clippy pass. Actual search PTY remains unimplemented; no completion.

Current typed local search backend/API and actual evidence documented in doc-11; remaining Ranked/GitHub/corpus controls and real search-specific PTY/full-regression gates are explicit.

Native Search pane implemented with shared typed keyboard/mouse actions for query/ref/name fields, Exact/Regex mode, selected existing corpus, hidden/ignored controls, prepared highlighted preview, read-only native file opening and saved SQLite views. Initial actual Search PTY run selected6: working/saved exact destinations and restart persistence4 passed; query-mode2 exposed a real clipped source-label defect, now corrected with a dedicated visible source label. Full rerun running; no acceptance completion from failed checkpoint.

Checked query_modes runner passed7 code/provider and2 actual PTY tests, zero failures/ignored, keyboard/mouse journal coverage. Artifact /tmp/ghtui-test-coverage-JVPsyG. Local Exact/Regex mode validation, visible source provenance, explicit selected corpus/exclusion controls, Unicode editing and native result opening are real. Ranked and GitHub native workers still need integration; do not interpret this local-mode checkpoint as full AC1 completion.

Prepared native GitHub SearchWorkspace adapter after green 258-code/102-PTY Linux checkpoint (which did not yet include remote-mode UI). Positive durable numeric account Scope::github must match HostIdentity; empty code/PR/issue selection and unsupported type/filter constraints fail before gh. RemoteCodeCandidate carries indexed blob/path, not commit identity or invented file coordinates; provider best-match order/caps and comment-fragment parent identity stay visible. ASCII fragment offsets are bounded for highlighting; Unicode offsets are retained without assuming undocumented units. Repository owners remain distinct from authors. Remote saved views retain selected repository list, explicit org query and account scope, with no local revision or implicit mapping requirement. Exact prepared viewer validates candidate-to-resolved commit/path/blob, keeps immutable native Search preview (never fabricated local Git workspace), bounds lines/bytes, and relies on provider-verified full content bytes. Four strict offline code contracts prepared; actual native GitHub mouse/keyboard journeys and full regression remain pending root integration. Primary fragment semantics: https://docs.github.com/en/rest/search/search#text-match-metadata .

Native GitHub mode worker now verifies positive numeric principal before/after provider reads; only explicit CLI/saved/mapped repos scope remote code/PR/issue search. Unknown code location is labelled provider fragment (no0:0); Open resolves/independently verifies immutable blob+commit and displays full bounded remote contents without fake LocalWorkspace/write actions. PR destinations route native GH viewer. Fresh principal changes hide old remote/GH/Actions views via runtime owner. New2actual query_modes_remote journeys prepared with strict22-read fixture, exact save scope, live identity loss and preserved HEAD/index/dirty bytes/storage; currently unrun. Issue/repository native opens and catalogue/corpus work remain explicit unfinished scope.

Remote results now render real author or repository-owner-with-not-author role and shared avatar/fallback; current active GH/Actions/Search surface drives bounded avatar prefetch. Prepared remote-file draw copies only visible lines rather than2MiB/10k-line previews every frame. Actual query_modes_remote2PTY running now. Updated native scope buttons explain provider-only indexed data vsunsupported remote revision/local exclusion semantics instead of silently changing modes.

First real query_modes_remote run reached verifiedfile opening and scroll but both tests failed on an uncleared default saved-search name (My search prefix). Fixture now sends actual Ctrl-A before entering its exact name. Actual rerunpassed2/2 zeroignored0.54s:4identity mode reads,5search reads,7exactfile-open reads,4local-save verifiedaccount reads,2liveauthfailure reads=22 strictGET/versioncalls, zero writes; owner@repo-owner isexplicitlynotfileauthor withsharedfallback, fragmentshowsno0:0,70-line verifiedfile scroll works, private results hidden onexpiry whileScope-ID savedquery/draft retained andHEAD/index/dirty unchanged. Log target/build/root-remote-search-pty.log. New failed remote-refresh handling clears prior Searchresults/preview/items thenrechecksprincipal; needs full regression and negative fixture extension.

Checked query_modes runner passed 11 code/provider contracts and 4 actual compiled-binary PTY journeys, zero failures/ignored, genuine keyboard and mouse input journals. Command: CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true node scripts/test-task.mjs query_modes. Artifacts /tmp/ghtui-test-coverage-pHNguU; log target/build/root-query-modes-checked.log. Includes remote fragment provenance, exact verified blob opening, owner-not-author rendering, explicit saved repository/account scope and authentication-expiry hiding while preserving local Git and private storage. Fresh all-target checkpoint currently has 110/112 PTY pass; two unrelated new Actions artifact fixture failures prevent claiming full green. Task remains In Progress because catalogue, native issue/repository opening and broad product dependencies remain unfinished.

Fresh full Linux checkpoint passed: CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test --offline --locked --all-targets: 288 meaningful code/provider contracts (including 90 strict provider contracts) and 114 actual compiled-binary PTY journeys, zero failures/ignored. cargo fmt --check passed; cargo clippy --offline --locked --all-targets -- -D warnings passed (8.07s). Logs target/build/root-all-targets-checkpoint.log, root-fmt-checkpoint.log, root-clippy-checkpoint.log. Node task-runner negative self-tests passed3/3; dependency integrity audit passed86 upstream roots/10300 files/108 manifest patches/117 registry packages; Backlog doctor clean72tasks. This includes new comparison2, remote-search2, raw-ref identity2, explicit-repo local/unborn2, Actions recovery2 and artifact2 PTY;18 native Actions journeys total. README reflects this actual green evidence only. Native macOS/live disposable-host/release and remaining product acceptance stay open; full product not Done.

Native discovered-PR entry point and exact-comment prepared rows now integrated in source. open_search_result takes the typed remote result, rejects unverified/empty/wrong-host/wrong-kind/zero-number/outside/excluded repository scope before any gh request, and opens canonical fetched PR details without inventing personal-queue memberships. Existing exact active PR remains refreshable. PrCommentSelection carries server comment kind/ID/node/body/time on each header/body row; replace_pr_details accepts only the same canonical PR and invalidates stale metadata/diff/checks while rebuilding the current tab without I/O or acknowledgement. Two meaningful strict offline code contracts and two actual query_modes_remote PR-opening journeys are prepared, including full conversation, actor fallback, narrow resize, empty personal inbox after Back, retained Search query/results and independent unchanged local Git/zero writes. These new additions are not yet run; shared Cargo is currently with managed corpus owner. The prior288-code/114-PTY checkpoint remains the last full green evidence.

Latest checked query_modes passed13 meaningful code/provider contracts and6 actual compiled-binary PTY, zeroignored, genuine keyboard/mouse journal coverage. Artifacts /tmp/ghtui-test-coverage-ymBl8f, target/build/root-query-modes-checked.log. Includes new strict native discovered-PR opening from Search (20 exact read/version commands), full conversation/current actors, narrow resize, Back to an empty nonfabricated personal queue, and unchanged retained query/results plus HEAD/index/dirty bytes/zero writes. Exact comment header/body identities and same-node prepared replacement are code-verified; wrong/unverified/outside/excluded scopes fail before gh. First run reached all visible/command assertions and failed because independent fixture git status itself refreshed index stat cache; corrected test helper to GIT_OPTIONAL_LOCKS=0 and added meaningful framework contract, then reran the full key. Native issue/repository destinations and broader corpus/catalogue work remain open.

Fresh full Linux checkpoint passed after actual fixes: CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test --offline --locked --all-targets: 327 meaningful code/provider contracts, including provider100, journal9, PR-controller6, managed-corpus9, and 116 compiled-binary PTY journeys; zero failed/ignored. cargo fmt --check passed; cargo clippy --offline --locked --all-targets -- -D warnings passed9.02s. Logs target/build/root-all-targets-checkpoint.log, root-fmt-checkpoint.log, root-clippy-checkpoint.log. New query_modes discovered-PR2 actual journeys and13-code/6-PTY checked gate are included. Ordered synchronous publication-draft helper and canonical value/separate label mouse/key code contract are verified, but native PR mutations/inspection/ack and managed-corpus UI are not yet wired; no Task17/47 product-completion inference from backend tests. Whole checkpoint first caught a test-source rename race and a wide-emoji continuation-cell assertion, both corrected and fully rerun rather than excluding tests. README records this actual green checkpoint; remaining full-scope/native-platform/release/CI work stays open.

2026-10-18 a13: GitHub mode keeps checkout-origin default scope; typed repo:/org: qualifiers scope one query; issue/repository Open shows number/state/URL/full description in preview; rows show #N. Gate: node scripts/test-task.mjs v1_search (3 code + 2 PTY keyboard/mouse pass); filtered lib search 115, tui_e2e search 34, search_remote_contracts 8, search_contracts 15 pass.

2026-10-20 audit at 74360e48 (4/5 ACs met): Four explicit modes, full filter grammar with explicit (never silent) remote rejection, highlighted results and exact open destinations are PTY-tested both inputs; only per-result freshness display is missing. Remaining: AC3: result rows show a type/path/line/repository table with highlighted snippets (search_ui.rs:27) and exact destinations, and the source and scope are labelled ('including uncommitted content', corpus). Per-result freshness (indexed/fetched time) is not visibly rendered in Search results, and no PTY assertion checks freshness.

2026-10-20 final push, verified at 60167066: node scripts/test-all.mjs 6107 passed 0 failed; fmt and clippy -D warnings clean. All audit gaps closed.
<!-- SECTION:NOTES:END -->
