---
id: TASK-25
title: Manage draft close reopen lock update and revert PR lifecycle
status: In Progress
assignee:
  - '@github_provider'
created_date: '2026-10-12 23:06'
updated_date: '2026-10-14 02:42'
labels:
  - review
  - e2e
milestone: m-2
dependencies:
  - TASK-24
  - TASK-18
  - TASK-11
  - TASK-34
  - TASK-35
references:
  - >-
    backlog/docs/doc-10 -
    GitHub-provider-native-API-contracts-and-verification.md
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-3 - PR-action-coverage-and-parity-contract.md
priority: high
type: feature
ordinal: 25
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Routine lifecycle operations should be reachable from the PR and explain their effect in plain language. Merged/draft/protected states differ and must not produce misleading actions.

## Testing steps

1. Code/provider tests: Verify lifecycle transition rules, commands, permissions, update-head guards, and revert provenance.
2. Real-binary TUI E2E scenario `pr_lifecycle`: Exercise ready/close/reopen/update/revert through keyboard and mouse fixtures, cancel destructive confirmations, and assert exact state transitions and no unauthorized command. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e pr_lifecycle -- --nocapture` and verify a nonzero selected test count; run the relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. The initial framework is delivered by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Draft/ready, close/reopen, lock/unlock, update branch and permitted branch deletion use explicit target and current state.
- [ ] #2 Revert of a merged PR previews and creates a traceable reverting PR using supported capability.
- [ ] #3 Restrictions, rules, unavailable operations, head changes and partial outcomes are visible and re-fetched.
- [ ] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [ ] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
- [ ] #6 All displayed actors use the shared username/avatar or honest fallback; Git and CI states use the shared semantic colors with redundant labels.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Prepare native typed provider requests and strict offline contracts in parallel: verify official endpoint fields, recheck host/account and expected PR head, serialize mutations through the shared runner, inspect resource identity/state, and read back effects. Keep completion dependent on the specified keyboard/mouse real-binary PTY journey and full application checks.

2026-10-14 source-only audit/handoff: wire draft/ready/close/reopen/lock/unlock/update/revert and separate remote-head deletion through the same typed PR actions, shared preview/confirm and durable journal. Read action-specific nullable capabilities/current exact state and genuine Issue/PR linkage; original-reference viewer precedes new value/head validation. Separate update202 acceptance from effect confirmation; verify revert returned node/URL/fields and remote-only branch target. Add central state/draft/merged/head metadata read-repair fences and real both-input pr_lifecycle journeys with strict command/SQL/Git/TTY evidence plus failure/restart/host/permission cases; full gates remain required. Review artifact target/build/foundation-task24-25-native-state-audit-20261014.md first.

Approved shared source-only model lane: bounded exact original state/operation plan and receipt/disposition contracts only, undeclared and unrun while Root owns core/Cargo. Native writers still require original/reference ordering, opaque fresh observation, accepted-pending journal semantics and mandatory state-filter repair. Deletion has no assumed CAS or gh delete-branch shortcut; trace actual remote-only lease semantics and local-effect preservation before choosing a transport.

Root approved exact retained-origin fresh lifecycle observation SOURCE-ONLY on 2026-10-14. New undeclared producer accepts only existing opaque PrMetadataCatalogueRead Arc, never legacy/raw authority constructor. One original capture/deadline/cancellation budget: user/repo/PR/independent linked issue/core lifecycle GraphQL/PR/SQL guard hook/repo/user (8 API children); private SQL errors retain final numeric identity-loss priority and close within the SAME budget, no hidden child after expiry/capture exhaustion/cancel. New private paging capture seam only for readonly GraphQL POST with no write resource. Positive actual original repo/PR DB/node and issue IDs/linkage, exact head/base/ref/lifecycle, nullable property provenance; generic viewerCanUpdate is not lock/draft/merge permission. Merge/auto/queue/rules explicitly NotLoaded. Stage strict contracts/handoff only; no includes/Cargo/schema/writer or native passing counts.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Provider implementation is actively in progress in parallel under the authorized team loop. Dependency completion, native TUI integration, and task-specific mouse plus keyboard real-binary PTY verification remain outstanding; no acceptance criterion is claimed complete from backend-only work. Focused provider checkpoint: cargo test --offline --manifest-path /tmp/ghtui-provider-check.gVlcVM/Cargo.toml --test provider_contracts -- --nocapture selected 38 tests, all passed, zero ignored (2026-10-13). Full workspace checks and dedicated journeys are still required before Done.

Provider checkpoint 2026-10-13: actual workspace provider_contracts selected 67 tests, all passed, zero ignored, using explicit shared-target jobs2/incremental0/debug0 environment and cargo test --offline --test provider_contracts. cargo fmt --check passed. Doc-10 records APIs, exact guards, primary sources, artifact/secret bounds and remaining integration gaps. Full-suite/clippy attempts after this checkpoint encountered concurrent Search Nucleo Config::Default compilation changes (owner corrected); current all-target rerun and native task-specific keyboard/mouse PTY remain outstanding. No AC/DoD or Done claim from these provider-only results.

Actual verification checkpoint: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo check --offline --all-targets passed (4.54s); cargo test --offline --test provider_contracts passed 82 tests, zero failed/ignored (0.53s). New tests cover stable positive numeric account IDs and same-login reassignment guards, canonical merge queue enqueue/dequeue/auto-merge permission/readback, latest case-insensitive commit-status/pending semantics, PR failed post-write observation duplicate barriers, exact comment repository identity, and Actions inspect-then-ack recovery. Native ActionsWorkspace/actions_ui and typed shared-form plans now implemented and compiling, but task-specific real-binary keyboard/mouse Actions PTY journeys and final all-target test/fmt/clippy gates remain pending; no task completion or native parity is claimed.

Source-only audit 2026-10-14 found all lifecycle native controls/journeys absent. Included lifecycle/lock/update/revert transports are prepared, with narrow provider evidence already in Root checkpoint only. Lock verifies locked boolean but lacks exact issue/reason receipt; update202 returns plain summary and loses accepted semantics; revert GET does not match actual mutation node/URL and chosen title/body/draft. No remote branch-delete implementation despite inventory row; gh merge --delete-branch also deletes local branch and cannot be used as remote-only native parity. Official REST/GraphQL delete inputs have no expected-old-OID delete guard; no atomic claim is made. Read dependencies/spec/primary docs and staged review artifact only; no included source/Cargo or new tests executed, no live mutation/remote/AC/Done.

Source-only model15 contracts staged/unrun with selected direct formatting PASS; see target/build/foundation-task24-25-state-model-handoff-20261014.md. No included source/Cargo/schema/writer/test registration changed, no task criteria marked. Remote deletion remains model-disabled. Actual read-only git version2.53.0/help plus official push reference and pinned remote.c support a candidate explicit expected-old-OID lease with empty-source exact remote refspec; no push/bare-remote test/live operation executed or atomic native claim. Need genuine head repo/ref/account/gh credential-helper scope, private transport config/hook/redirect policy, exact one-ref readback and real races before choosing remote-only Git vs disclosed unconditional API race. Never gh merge --delete-branch, no fallback bypass/checkout change; transport decision open. Accepted202 remains unconfirmed/unresolved, original viewing before fresh state/fields and durable state-filter repair remain native prerequisites.

2026-10-14 reviewed lifecycle-observation SOURCE-ONLY handoff is ready/frozen: NEW undeclared src/providers/gh/metadata_catalogue/lifecycle.rs and PRIVATE same-MetricsBudget child src/providers/gh/metrics/pr_lifecycle_capture.rs; tests/provider_contracts/pr_lifecycle_observation.rs contains15 meaningful UNREGISTERED/UNCOMPILED/UNRUN declarations. Actual direct rustfmt --edition2021 on those3 files and selected --check both exited0; no Cargo/native/live GitHub/core/schema/include/registration/README changes or passing test claim. API requires retained genuine opaque Arc<PrMetadataCatalogueRead>,8 proposed current-user/repo/PR/independent-issue/readonlyGraphQL/PR/SQLhook/repo/user reads under one original capture/cancellation/deadline. Fixed nullable viewer provenance and exact genuine IDs/full refs/lifecycle; caller SQL hook metadata-only, not Cache/auth/publication authority. Closing UID43/auth/missing-null-zeroUID outranks private local errors; hook cancellation/actual deadline prevents any next child.15 contracts include exact argv/stdin/counts, actual Pending journal/runner no-release, raw controls/failed-cap exhaustion and real Git preservation. These are staged assertions, NOT executed effects. Merge/queue/auto/rules and specific draft/lock eligibility explicitly NotLoaded; no publisher enabled. Exact API/declaration/limits/schema/evidence boundaries in target/build/foundation-pr-lifecycle-observation-handoff-20261014.md. Original viewer must retain separate tolerant current-access ordering; all native lifecycle/key+mouse/full/host/role/platform gates and dependencies remain open; no AC or Done change.
<!-- SECTION:NOTES:END -->
