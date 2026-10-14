---
id: TASK-20
title: Manage pending reviews approvals change requests and dismissal
status: In Progress
assignee:
  - '@github_provider'
created_date: '2026-10-12 23:06'
updated_date: '2026-10-13 06:01'
labels:
  - review
  - e2e
milestone: m-2
dependencies:
  - TASK-19
  - TASK-17
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
ordinal: 20
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A review commonly contains multiple comments and a final decision. Drafting, submitting, discarding, and maintaining that review must be coherent across app restarts and permission changes.

## Testing steps

1. Code/provider tests: Verify review state transitions, combined payloads, duplicate-submit protection, dismissal permissions, and head races.
2. Real-binary TUI E2E scenario `pending_reviews`: Draft two comments, restart, edit one, submit a review using keys and clicks in separate fixtures, verify published state; exercise discard and denied dismissal. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e pending_reviews -- --nocapture` and verify a nonzero selected test count; run the relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. The initial framework is delivered by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Pending inline comments and summary can be added/edited/removed without premature publication.
- [ ] #2 Approve/request-changes/comment, submit/discard, reviewer re-request, and permitted dismissal map to verified capabilities.
- [ ] #3 Server pending-review state, local drafts, and changed heads reconcile with explicit outcomes.
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

Native receipt prerequisite for root/foundation journal migration: add typed create_review requiring positive numeric account and exact PrSummary host/node/base/head. Confirm POST positive review ID/node, requested decision/body/head, current numeric author and exact parent scope, then GET the exact review ID and revalidate; successful-but-unusable or failed readback is ambiguous and blocks retry. Keep legacy primitive review callers separate until the journal adapter migrates them. Add meaningful pending_reviews provider contracts for approve/comment/request-changes, empty approve summary, changed node/author/body/parent/head and failed readback, with current task-specific real keyboard/mouse pending-review workflows plus global verification still required.

Next verified-principal prerequisite before native pending-review exposure: preserve fresh raw REST user.id/node/parent/commit/body for pending review ownership and post-write confirmation. Same-login numeric reassignment must not authorize another account's pending draft. Guard positive review IDs before transport, compare selected observed draft state/body instead of overwriting a concurrently edited summary, and require exact review parent/node/current author/readback for update/submit/discard. Add a typed observed-review summary edit path for supported submitted summaries rather than treating every summary as PENDING. Verify current official REST review contracts (pending reviews omit submitted_at; submitted reviews cannot be deleted), strict wrong-account/node/parent/body/stale and failed readback provider cases, plus actual keyboard/mouse draft restart/submit/discard/dismiss journeys through the durable PR journal. Current typed new-review7-provider success and native conversation/composer gates do not prove full pending-review lifecycle support.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Provider implementation is actively in progress in parallel under the authorized team loop. Dependency completion, native TUI integration, and task-specific mouse plus keyboard real-binary PTY verification remain outstanding; no acceptance criterion is claimed complete from backend-only work. Focused provider checkpoint: cargo test --offline --manifest-path /tmp/ghtui-provider-check.gVlcVM/Cargo.toml --test provider_contracts -- --nocapture selected 38 tests, all passed, zero ignored (2026-10-13). Full workspace checks and dedicated journeys are still required before Done.

Provider checkpoint 2026-10-13: actual workspace provider_contracts selected 67 tests, all passed, zero ignored, using explicit shared-target jobs2/incremental0/debug0 environment and cargo test --offline --test provider_contracts. cargo fmt --check passed. Doc-10 records APIs, exact guards, primary sources, artifact/secret bounds and remaining integration gaps. Full-suite/clippy attempts after this checkpoint encountered concurrent Search Nucleo Config::Default compilation changes (owner corrected); current all-target rerun and native task-specific keyboard/mouse PTY remain outstanding. No AC/DoD or Done claim from these provider-only results.

Actual verification checkpoint: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo check --offline --all-targets passed (4.54s); cargo test --offline --test provider_contracts passed 82 tests, zero failed/ignored (0.53s). New tests cover stable positive numeric account IDs and same-login reassignment guards, canonical merge queue enqueue/dequeue/auto-merge permission/readback, latest case-insensitive commit-status/pending semantics, PR failed post-write observation duplicate barriers, exact comment repository identity, and Actions inspect-then-ack recovery. Native ActionsWorkspace/actions_ui and typed shared-form plans now implemented and compiling, but task-specific real-binary keyboard/mouse Actions PTY journeys and final all-target test/fmt/clippy gates remain pending; no task completion or native parity is claimed.

Typed create_review source now verified under the shared build environment: cargo test --offline --test provider_contracts pending_reviews -- --nocapture passed7/7, zero ignored (0.33 s;3new typed-publication contracts and4existing pending-review contracts). Positive numeric account required. Normal execution is5 exact gh calls: identity2, canonical PR summary1, POST1, exact review GET1. Receipt confirms positive review ID/node, requested decision/body/head, current numeric author and exact parent scope; malformed successful POST or failed/mismatched readback remains ambiguous and blocks duplicate write launch. Tests cover approve with empty summary, comment, request changes, reassigned username IDs, changed body/head/node/state/parent, failed readback and invalid POST identity. Legacy primitive review remains separate until root/foundation journal migration. Focused strict Clippy lib+Actions/provider/tui targets passed7.64 s. Native pending_reviews keyboard/mouse restart/submission/discard/dismissal journeys and current global gates remain required; no completion inferred from provider preparation.
<!-- SECTION:NOTES:END -->
