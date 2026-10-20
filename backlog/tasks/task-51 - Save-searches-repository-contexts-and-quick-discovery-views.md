---
id: TASK-51
title: Save searches repository contexts and quick discovery views
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
  - TASK-50
  - TASK-7
references:
  - 'https://github.com/helix-editor/nucleo'
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: medium
type: feature
ordinal: 51
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Frequent searches and meaningful repository groups should be one action away. Saved views need stable semantics and scope that remain understandable after repos change.

## Testing steps

1. Code/provider tests: Test saved-query round trips, context changes, fuzzy ordering, Unicode and missing-repo behavior.
2. Real-binary TUI E2E scenario `saved_searches`: Save and reopen a search with keys/clicks after restart, modify its repo set, and assert exact query/results and unavailable scope warnings. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e saved_searches -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Saved queries/contexts retain mode, filters, scope and sort and are editable/pinnable.
- [x] #2 Fuzzy pickers use nucleo for commands/repos/paths/refs while content uses its explicit search mode.
- [x] #3 Saved views survive restart and explain missing/inaccessible repos or unsupported filters.
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
1. Serialize stable SavedSearch mode/input/filters/account/repository/revision/sort/pin state in existing scoped SQLite saved_views. 2. Add nucleo-matcher for fuzzy navigation only, preserving explicit content modes. 3. Expose missing/inaccessible repository and unsupported-filter warnings on reopen without widening scope. 4. Verify restart/account boundaries in code then actual keyboard/mouse saved_searches PTY, followed by full gates. Backend preparation does not satisfy unfinished task dependencies.

Root native integration: build one prepared Search pane with grapheme-aware query/ref/name fields, shared typed mouse/key actions, explicit working/saved scope and corpus exclusions; worker-only scans and SQLite persistence; preview exact results and open immutable/mutable native file destinations. Add real-binary query_modes, exact_search and saved_searches journeys with independent HEAD/index/content/storage verification. Broader Ranked/GitHub/catalog dependencies remain open until implemented and verified.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Search backend preparation assigned local_git agent after actual identity/theme slice verification. Task dependencies remain integration gates; no task completion or dependency readiness claimed. Read task contract and architecture; official ripgrep15.1.0, nucleo-v0.4.0 and Tantivy0.26.2 source pins fetched for manifest-owner dependency audit. gh remains sole authenticated API transport; no mandatory rg executable or alternate ranked engine added.

Preparatory SavedSearch serde model retains version/mode/query/repositories/per-repository working-or-revision sources/sort/pin/account scope; SearchWorkspace save/load/restore uses existing SQLite namespace search:name and separate available mappings, refusing to broaden missing selections. Official Nucleo picker handles smart Unicode normalization/fuzzy indices/deterministic ties/limits/cancellation for navigation only. Actual search_contracts9 passed including saved SQLite restart/account isolation and Workspace/Nucleo cases. Actual saved_searches key/click restart journey remains required before completion.

Checked saved_searches runner passed2 meaningful code contracts and2 actual keyboard/mouse PTY journeys, zero ignored/failures. Artifact /tmp/ghtui-test-coverage-VzVSdw. Verified exact regex input/path filter, saved-version scope, selected repo, SQLite values, wide/narrow restart, then mapping changed to a different existing repo: restore reports unavailable original scope and refuses to broaden results. No gh calls or HEAD changes. Native pin/edit/list picker breadth and broader collaboration prerequisites remain open.

2026-10-20 audit at 74360e48 (3/5 ACs met): Saving, restoring after restart and the missing-scope warning work for keyboard and mouse. Remaining: pin/edit/delete controls in the UI, plus PTY coverage for them. Remaining: AC1: the model stores mode/input/filters/repositories/revision/sort/pinned (workspace.rs:1067-1148), but the UI always saves pinned=false (src/main.rs:2772). Users cannot pin, rename or delete a saved search; editing is only re-saving under the same name. AC5: the saved_searches PTY covers save, restore and the missing-mapping warning only, so pin/edit actions have no keyboard/mouse coverage and the 'modify its repo set' step is done by rewriting config, not through the UI.
<!-- SECTION:NOTES:END -->
