---
id: TASK-60
title: Inspect and manage Actions policies environments and runners
status: In Progress
assignee:
  - '@github_provider'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-20 14:09'
labels:
  - actions
  - e2e
milestone: m-7
dependencies:
  - TASK-57
  - TASK-59
  - TASK-58
references:
  - 'https://docs.github.com/en/rest/actions/permissions'
  - 'https://docs.github.com/en/rest/actions/self-hosted-runners'
  - >-
    backlog/docs/doc-10 -
    GitHub-provider-native-API-contracts-and-verification.md
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-5 - GitHub-Actions-coverage-contract.md
priority: high
type: feature
ordinal: 60
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Full Actions support needs access to relevant operational settings when the account is allowed to change them. Changes to permissions, environments and runner associations affect many workflows.

## Testing steps

1. Code/provider tests: Contract-test setting diffs, permission scopes, runner paging/labels/associations, stale data and cancelled destructive actions.
2. Real-binary TUI E2E scenario `actions_settings`: Inspect and change a fixture policy/runner label/environment rule via keys/clicks, cancel removal, and assert exact scope, read-back and disabled unauthorized controls. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e actions_settings -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Read/edit supported Actions allowlists/token permissions and environment protection settings with explicit scope/effect preview.
- [ ] #2 Runner/group inventory shows labels/availability/associations and supports verified permitted label/association/removal operations.
- [ ] #3 Unsupported host/plan/org-admin capabilities explain limitations; no runner service provisioning or broad hidden change.
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
Provider preparation under the authorized parallel implementation loop: verify official gh/REST contracts; add typed native target models and explicit argv/JSON stdin through the shared bounded runner; guard current account/resource identity, serialize writes and reconcile ambiguity; add strict fake-gh contracts for successful readback, paging, permissions, stale state and cancellation. Root owns native TUI forms and task-specific mouse/keyboard real-binary PTY journeys. Dependency completion, full workspace checks and required PTY coverage remain pending; backend implementation does not satisfy Done.

Native UI integration slice: build provider-free ActionsWorkspace prepared pages and ActionsAction enum, then pure Actions renderer with shared semantic status colors, actor/avatar fallback, clickable hit regions and matching keyboard shortcuts. Worker loads typed catalog/run-attempt/jobs/logs/annotations/artifact/workflow/settings data. Shared forms retain target identity and typed commands for confirmation; root integrates App/main dispatch without arbitrary gh commands. Add focused workspace/provider tests and owned real-binary Actions PTY journeys for each exposed native workflow; keep unsupported or unintegrated rows visibly honest and tasks In Progress until their actual code plus keyboard/mouse PTY and global gates pass.

Next native verification slice after Task57 checked10-code/10-PTY: prepare unexported strict actions_settings keyboard and mouse journeys for the already-typed repository policy/workflow-token form, environment wait/reviewer/branch-mode changes, exact runner custom labels and cancelled removal. Every mutation must preview explicit scope/current-versus-requested values, reject stale snapshots and confirm readback while preserving temporary Git. Add meaningful workspace/provider selector contracts for canonical IDs, denied controls and exact scopes at80x24/120x40. Activate the tests only at the coordinated checkpoint; do not substitute JSON output or existing unrelated Actions tests for task-specific coverage. Advanced selected branch/tag patterns, installed-App custom protection, runner/group associations and complete host/admin capability explanation remain separate native integration work, not completion inferred from the initial settings journeys.

Root current whole regression reproduced a native recovery composer-close race: Original Attempt no longer contains ACTIONS INTENT, so the shared close fixture returned before its retained form/footer had actually closed and sent Refresh into the original overlay. Preserve the original environment9 versus current name99, exact one PUT/no resend and durable acknowledgement assertions. Strengthen shared Actions close proof to require all genuine composer-state labels and its Keep draft / Close footer absent before later navigation; rerun both-input settings, impacted Actions recovery and whole/fmt/Clippy. This is fixture synchronization, not a permission or publication guard relaxation.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Provider checkpoint 2026-10-13: actual workspace provider_contracts selected 67 tests, all passed, zero ignored, using explicit shared-target jobs2/incremental0/debug0 environment and cargo test --offline --test provider_contracts. cargo fmt --check passed. Doc-10 records APIs, exact guards, primary sources, artifact/secret bounds and remaining integration gaps. Full-suite/clippy attempts after this checkpoint encountered concurrent Search Nucleo Config::Default compilation changes (owner corrected); current all-target rerun and native task-specific keyboard/mouse PTY remain outstanding. No AC/DoD or Done claim from these provider-only results.

Actual verification checkpoint: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo check --offline --all-targets passed (4.54s); cargo test --offline --test provider_contracts passed 82 tests, zero failed/ignored (0.53s). New tests cover stable positive numeric account IDs and same-login reassignment guards, canonical merge queue enqueue/dequeue/auto-merge permission/readback, latest case-insensitive commit-status/pending semantics, PR failed post-write observation duplicate barriers, exact comment repository identity, and Actions inspect-then-ack recovery. Native ActionsWorkspace/actions_ui and typed shared-form plans now implemented and compiling, but task-specific real-binary keyboard/mouse Actions PTY journeys and final all-target test/fmt/clippy gates remain pending; no task completion or native parity is claimed.

Unexported source preparation only: settings helper now prepares canonical environment row-to-positive-ID mapping, readable branch/wait/reviewer/team rows with shared actor fallback, and policy/environment Before/After lines; no display string becomes an action target. Three helper unit contracts, three strict provider contracts, and two normal-scope actions_settings PTY source journeys are prepared but not included or run. Journeys expect22 exact calls for workflow-token policy, encoded prod/eu protection and runner custom-label changes, exercise actual field paging at80x24, cancel removal, inspect a busy-runner explanation, independently decode all three PUT bodies and preserve temporary Git. Provider contracts deliberately cover currently identified post-write gaps: selected environment ID must not change despite matching name/settings; read-only runner labels must actually remain preserved; stale/denied policy and retention maximum must block PUT. Integration and fixes will occur only after the current Corpus/PR native gate freeze. Official environment docs confirm explicit null permits all branches and protected/custom flags must be opposite, so absent/conflicting policy is never guessed as all branches: https://docs.github.com/en/rest/deployments/environments. Current native broad settings coverage, advanced controls, complete host/admin/platform behavior and all required actual gates remain open; no completion or tested-source claim.

Source activated after root global373-code/140-PTY/fmt/strict-Clippy checkpoint release: private settings helper and canonical environment row mapping are wired into native fields/draft target keys; user/team fallback and readable branch/wait metadata replace opaque one-line environment JSON. Policy/environment previews show separate Before/After values. Busy-runner removal opens a typed unavailable explanation, and in-place updates retain the selected environment/runner. Provider guards now require positive observed environment ID and unchanged post-write ID, and actually verify preservation of read-only runner labels. Three helper units, three strict provider contracts and two normal-scope actions_settings PTYs are included; formatting completed, but this new slice has not yet been compiled/tested. Coordinating the next shared Cargo window with root/foundation. No current task completion or advanced-control parity claim.

Additional coherent Task60 recovery slice, still unverified: ActionsTarget::EnvironmentProtection freezes the original positive environment ID while retaining name-resource write serialization. The inspector reads current inventory under verified positive principal before/after and exposes requested ID/name, original-by-ID and current-by-name separately; reused names never imply the old outcome or automatic retry. One strict producer/generation/read-only inspector contract plus two actual keyboard/mouse PTY source journeys now join the initial settings coverage, yielding expected seven code/provider tests and four PTY tests under actions_settings. The new journeys expect18 exact calls, one PUT only, ordinary Refresh preserving the barrier, explicit inspection/ack and unchanged temporary Git. Source formatted; checked execution, strict lint and fresh global gates pending the coordinated Cargo slot. Durable restart-safe Actions journals and advanced settings controls remain open.

Actual checked Task60 verification: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 node scripts/test-task.mjs actions_settings passed7 meaningful code/provider tests (3 helper units +4 strict contracts) and4 real-binary PTY tests, zero failed/ignored; keyboard and mouse coverage independently proved. Provider0.16s, PTY9.41s; log target/build/provider-actions-settings-verified.log and /tmp/ghtui-test-coverage-HNnUbq/journeys.jsonl. Basic paths verify22 exact calls, three decoded PUT bodies, preserved builtin labels, cancelled removal, busy-runner unavailable explanation, field paging80x24, resizing and unchanged Git HEAD/status. Exact-ID recovery paths verify18 calls, originalID9 versus reusednameID99, Refresh retaining barrier, explicit inspection/ack, onlyonePUT/no automaticretry. First two checked attempts retained7code/3PTY passes but found stale terminal-coordinate assumptions; helpers now wait for actual heading viewport movement and exact clicked-row selection, without altering expected API bodies or bypassing real input. Failed artifacts1199-1791873489259854212 and682-1791873607627143905 remain under target/tui-artifacts; historical failure logs retained. Focused strict Clippy currently blocked by a new unrelated root ui.rs needless-borrow diagnostic sent to owner; fresh lint/fmt/global gates remain pending. Advanced selected patterns/custom rules/group associations/complete capability explanation and durable Actions restart journals remain open. No AC/DoD or Done claim.

Task60 focused strict lint/fmt terminal: same shared target environment cargo clippy --offline --locked --lib --test actions_contracts --test provider_contracts --test tui_e2e -- -D warnings passed8.25s after root corrected its unrelated AvatarWidget needless borrow; log target/build/provider-actions-settings-clippy-final.log. cargo fmt --all -- --check passed. Checked settings7code+4actualPTY evidence remains green. Cargo released to foundation for Task17 native reading coverage and root fresh global checkpoint later; no full all-target completion claim from this focused gate.

Fresh Linux whole-project checkpoint GREEN: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test --offline --locked --all-targets passed398 meaningful code/provider contracts and150 actual binary PTY journeys,0failed/ignored (PTY12.59s). Includes lib106, provider114, PRsubscription6, PRUI5 and all previous normal targets; main0 is not counted as coverage. Whole cargo fmt --check passed and cargo clippy --offline --locked --all-targets -- -D warnings passed10.35s on the same coherent source. Logs target/build/root-all-targets-state-readers-checkpoint-rerun.log, root-fmt-state-readers-checkpoint.log, root-clippy-state-readers-checkpoint.log. First full attempt104/106lib exposed two remaining test-only0775 cache parent assumptions; private fixture corrections preserved assertions and the full rerun passed. Checked conversation_comments52+12 (/tmp/ghtui-test-coverage-El3Jo9), cache_storage20+6 (/tmp/ghtui-test-coverage-Lech0Z), actions_settings7+4 (/tmp/ghtui-test-coverage-HNnUbq) independently verify actual keys/mouse. Backlog audit72 valid, runner negative self-tests3/3, dependency audit86upstream roots/10300files/108patches/117registry packages passed. No remote/live GitHub mutation; Task50 hydration/durable Actions/advanced settings/remaining native PR/Git/dashboard/macOS/release/near-endCI remain open. No full-product or task Done inference from this checkpoint.

Durable native changed-environment-ID keyboard/mouse journeys are migrated and included in the fresh green Actions46 gate (provider-native-durable-actions46-rerun.log,46/46 zeroignored32.48s). Each verifies34 exact matched read/write commands, original requested environment ID9 remains distinct from current same-name owner99, one exact PUT only, normal Refresh retains uncertainty, explicit original inspection reports current_by_name99/original_by_id null/requested_environment_id9, and fresh explicit ACK records acknowledged with null receipt/original target9 rather than retargeting/retrying. New unavailable SQL-view keyboard/mouse journeys prove selecting other resources cannot remove fail-closed remote disabled reasons while cancelled local filtering remains accessible. Whole current all-target/provider/lint/platform coverage and broader org-only/runner/settings/dependency gaps remain open; no task Done inferred.

Root native regression fixes and retained evidence: source_transition checked first10code/9of10native failed because shared filter allowed old visible ID/name while actual async query frame was absent. Actual query+unique match/current row and subsequent list-focus barriers now pass10/10 and broad catalogue86/86 without changing target authority. Whole repeat401of402 failed only Actions keyboard environment9→name99 recovery close: Original Attempt lacks ACTIONS INTENT, so old fixture returned before the retained composer/footer was closed. Shared close now requires Preview/Intent/Original Attempt and Keep draft footer absent. Separate Actions46 initially45/46 failed mouse acknowledged-original F9: genuine artifact target/tui-artifacts/2814309-1791977685995376396 records44 matched reads, original acknowledged Cancel123/attempt2 and late F9 footer. Generic fixture attempted hidden control paging before that footer was painted. Added known F9 New intent to existing owned composer button handling, awaiting genuine visible click instead of paging behind overlay. New gates Actions46/46, settings9code+4PTY, runcontrol52code+6PTY and whole1383code+402PTY all green. One original uncertain write, explicit original inspection/acknowledgement and precisely one new confirmed write remain independently asserted; original environment ID9/current-by-name99, exact one PUT/no resend and durable recovery invariants retained. No weakened snapshots/effects or product guard relaxation.

2026-10-20 audit at 74360e48 (2/5 ACs met): Policy (workflow token), environment protection and runner label edits work, with keyboard/mouse PTY journeys and exact-ID recovery. Runner groups/associations and capability/permission explanations still have no PTY coverage. Remaining: AC2: the Runner groups view and its Create/Edit/Replace runners/Replace repositories/Delete commands are reachable (actions_workspace.rs ~1985), but they have only a provider contract (provider_contracts.rs:1650). No PTY journey covers runner-group association or a confirmed runner removal. AC3: limits are explained only for a busy runner and for 'Runner groups require an explicitly selected organization scope'. No tests cover host/plan/org-admin limits or disabled unauthorized controls. AC5: th
<!-- SECTION:NOTES:END -->
