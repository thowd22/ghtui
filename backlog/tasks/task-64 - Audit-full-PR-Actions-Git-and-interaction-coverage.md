---
id: TASK-64
title: Audit full PR Actions Git and interaction coverage
status: To Do
assignee: []
created_date: '2026-10-12 23:13'
updated_date: '2026-10-17 15:17'
labels:
  - delivery
  - e2e
milestone: m-5
dependencies:
  - TASK-60
  - TASK-56
  - TASK-44
  - TASK-27
  - TASK-63
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: feature
ordinal: 64
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Broad feature promises need concrete release evidence. Audit native actions, host/platform gaps, identity/color coverage, and mouse/keyboard access before claiming complete support.

## Testing steps

1. Code/provider tests: Validate manifest completeness and test-key coverage, reject an omitted action/input path, and verify capability/permission cases.
2. Real-binary TUI E2E scenario `parity_audit`: Enumerate and exercise enabled/disabled controls across PR/Actions/local Git views with keys/clicks and fixture effects; assert native coverage and explicit gaps. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e parity_audit -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 PR and Actions inventories map every row to implemented native behavior, tests and limitations.
- [ ] #2 Local Git operation coverage and all exposed action mouse/keyboard paths are enumerated and verified.
- [ ] #3 Every actor view uses shared identity/avatar fallback and every Git/CI state follows the semantic theme.
- [ ] #4 Opt-in disposable-repo verification records real host compatibility; browser links do not count as native support.
- [ ] #5 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [ ] #6 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
2026-10-15 Root-authorized READ-ONLY Actions coverage audit: read doc5 and Tasks52-60 with dependencies/evidence; trace current executable scopedPR-to-Actions and direct Actions entrypoints, original source owners, provider/controller/UI/actions/forms and existing strict code+real-binary key/mouse tests. Separate reachable workflows, prepared-provider-only surfaces, unverified declarations and explicit host/platform limits. Produce concise evidence/path-based mandatory next implementation slices and testing needs; no Main source changes, builds, AC/DoD/status changes or broad implementation. Formal64 dependencies remain open; Root alone heavy verification.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Root authorized a parallel SOURCE-ONLY TASK3 shared Form visible-only rendering correction; approved proposal 4c37302c4348c634287b8a2806f092e800f86357f695d66964ea4d2d0b60b419. This fixes a known whole-body per-redraw gap, not the TASK64 broad PR/Actions/Git/native/live/actor/color parity audit. TASK64 staysToDo with direct60/56/44/27/63 dependencies open; no AC/DoD checked. TASK66 measured hardware/release/p95/RSS/cold-warm work read and remains separate. Intended22code/8PTY source declarations ALLUNWRITTEN/UNRUN at plan time; Root sole bounded verifier.

READ-ONLY Actions coverage audit frozen: target/build/parallel-tree-task64-actions-runtime-gap-audit-v1-20261015.json SHA2c5b517cdb4dbfd224cf274a314d0b0bb0b84794fcf7a880bb13909de6cb983c (35 current inspected source/test hashes; doc5 andTasks52-60 read). Real native legacy startup workflows/durable controller exist; do not confuse them with current tagged-source reachability. Catalogue OpenActions only initialcatalogue/pureselection; subsequentOpenRun/Refresh/View hits intentional legacy_entry(SomeScope) refusal. ScopedPRChecks permits exact member/log/annotations/loadedSearch/finiteWatch only, othercatalogues/drafts/recovery/publication disabled. Eight required nextslices remain: A1genuinecatalogueActionscontinuation; A2scopedread+durablewriteadapter; A3history/canonicalannotation/logexport; A4actualWorkflowYAMLlocalGitedit handoff; A5org-only settings scope seam (resource/planner/descriptor require repo), org/env secret andgroupnativeproof; A6provider-only deploymentpatterns/customAppnativecontrols; A7accepteddeploymentSIGKILL+unobservableLog/dispatch explicitACKnative matrix; A8commitCIindicators/crossrepoactivity+realcapability/platform/perfbudgets. Everyrow has actualpaths/evidence and meaningful code+bothinput GH/SQL/Git/config/TTY testing requirements, NOT newdeclarations/passcounts. Rootreviewed sourcegate/main andconfirmedA1gap. NoMain source,builds,runs,newtests,AC/DoD/status/dependencychanges; Task64 remainsToDo/formaldepsopen. Recommend firstboundedA1 source-bound originalowner/actualprovider/sharedActionsqueue plan forreview, no relaxedfallback or fullActionscompletionclaim.

A1 concrete source-bound continuation review proposal prepared after the frozen READ-ONLY audit: target/build/parallel-tree-scoped-actions-read-a1-proposal-v1-20261015.json; no source implementation or execution and no acceptance/status/dependency changes. It retains all eight mandatory Actions slices and distinguishes first catalogue read navigation from full native support. Optional-A helper activation gap is explicitly tracked rather than counted as executable coverage.

2026-10-16 Root current-scope READONLY refresh after V206: reread Doc5, Tasks64-69, immutable currentV65 provider inventory and actual compiled capability inspector/lib/app registration. Original A1/A2 no-continuation audit describes older source, not current: genuine Source-owned Actions dispatch84code40PTY and Workflow57code24PTY actualV206PASS, scoped read/publication/history adapters now included; cannot keep claiming initial Actions navigation absent. Static capability_inventory explicitly describes TRANSPORT presence only and inspector calls it neither native parity nor current permission; true pr-lifecycle/pr-merge rows do NOT prove native lifecycle/merge integration. Current lib still does not declare staged pr_state_model/lifecycle observation, so Tasks24/25 remain real native gaps. Broader org/settings/runner/deployment capability slices, full W9edit/PR history, cross-repoCI, exact row-level test mapping, nativeMac/live-host/usability/performance/installer/CI remain required; do not reduce full Actions/PR contract to passing dispatch/workflow cohorts. New sourceonly W9negativeV66 child12declarations is reviewed but uncompiled/unregistered/unrun. This is targeted read-only audit evidence, no source/AC/status/dependency changes or Task64Done.

2026-10-17 Linux foundation inventory checkpoint only: Task6 accepted its structured transport/auth/requirement/diagnostic inspector gates; Doc14 records all28public-operation/source/guard/task rows and executed fixture SUBSETS at source1530/039a, full5597/fmt/strict/audits. This does not satisfy Task64 AC1 or AC4 and none of its AC/status is changed. Actual native behavior/roles/host/API-version/plan/rule coverage remains required. Explicit current product gaps include suggestion application(Task23), isolated PR editing/conflicts(Task27), and complete native metadata/project/merge/Actions settings paths. Current public docs may default2026-03-10; existing application header remains unchanged, and real target compatibility cannot be inferred from current docs. Native macOS remains mandatory; live writes require an explicitly configured disposable repository. Do not count browser links, local-conflict tests, static flags or this ledger as native/live parity.
<!-- SECTION:NOTES:END -->
