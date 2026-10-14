---
id: TASK-56
title: Browse preview download and delete workflow artifacts
status: In Progress
assignee:
  - '@github_provider'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-13 16:29'
labels:
  - actions
  - e2e
milestone: m-7
dependencies:
  - TASK-55
references:
  - 'https://cli.github.com/manual/gh_run_download'
  - >-
    backlog/docs/doc-10 -
    GitHub-provider-native-API-contracts-and-verification.md
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-5 - GitHub-Actions-coverage-contract.md
priority: high
type: feature
ordinal: 56
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Actions artifacts are often the useful output of a run. Users should find them, inspect supported contents and download them without confusing expired or unsafe archives with valid data.

## Testing steps

1. Code/provider tests: Test paged/expired artifacts, traversal/symlink/archive bounds, safe extraction, download collisions and deletion contracts.
2. Real-binary TUI E2E scenario `actions_artifacts`: Open/preview/download an artifact via keys/clicks, inspect exported bytes, then test malicious/expired fixtures and cancelled deletion without unsafe files. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e actions_artifacts -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Artifacts show name/run/attempt/size/expiry/retention and supported integrity metadata.
- [ ] #2 Bounded text/image/archive previews and explicit download destination work without executing content.
- [ ] #3 Extraction validates paths/sizes; overwrite/delete requires explicit target confirmation and reports results.
- [ ] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
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

Next native slice: reuse the shared bounded PNG/JPEG display decoder (without widening avatar security), prepare text/image/binary archive-entry previews entirely in workers, explicitly label truncation/unsupported formats and integrity/expiry; expired and directory-only rows retain read-only reason actions. Real-binary actions_artifacts keyboard/mouse journeys will preview exact text/image content, export ZIP and safely extract into new explicit destinations, verify bytes/modes/collision preservation independently, cancel deletion then confirm exact artifact ID with preserved unrelated artifacts, and reject malicious/traversing archives without publishing files. Strict provider/source tests plus checked selector and global gates required; no archive content executed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Provider checkpoint 2026-10-13: actual workspace provider_contracts selected 67 tests, all passed, zero ignored, using explicit shared-target jobs2/incremental0/debug0 environment and cargo test --offline --test provider_contracts. cargo fmt --check passed. Doc-10 records APIs, exact guards, primary sources, artifact/secret bounds and remaining integration gaps. Full-suite/clippy attempts after this checkpoint encountered concurrent Search Nucleo Config::Default compilation changes (owner corrected); current all-target rerun and native task-specific keyboard/mouse PTY remain outstanding. No AC/DoD or Done claim from these provider-only results.

Actual verification checkpoint: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo check --offline --all-targets passed (4.54s); cargo test --offline --test provider_contracts passed 82 tests, zero failed/ignored (0.53s). New tests cover stable positive numeric account IDs and same-login reassignment guards, canonical merge queue enqueue/dequeue/auto-merge permission/readback, latest case-insensitive commit-status/pending semantics, PR failed post-write observation duplicate barriers, exact comment repository identity, and Actions inspect-then-ack recovery. Native ActionsWorkspace/actions_ui and typed shared-form plans now implemented and compiling, but task-specific real-binary keyboard/mouse Actions PTY journeys and final all-target test/fmt/clippy gates remain pending; no task completion or native parity is claimed.

Checked native artifact verification 2026-10-13: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 node scripts/test-task.mjs actions_artifacts exited 0: 5 meaningful code/provider contracts and 2 actual real-binary PTY journeys passed, zero ignored; keyboard=true and mouse=true. Coverage artifacts: /tmp/ghtui-test-coverage-sJtaMj. Source tests verify prepared bounded text/image/binary views, PNG decoding/truecolor and no-color fallback, exact-budget EOF/CRC checks, malicious paths/symlinks/duplicates/archive budgets, non-clobber export/extraction, and exact artifact deletion/readback. Each PTY journey verifies 15 exact matched gh commands, actual ZIP SHA256/export bytes, extracted text/PNG and 0600 file modes, unchanged existing export after rejected retry, cancelled deletion with no command, one confirmed Artifact41 DELETE plus run-specific readback preserving42/43, expired42 explanation without GET, and traversal rejection without outside files. Both resize and restore the terminal. Earlier full regression failure was a strict fixture endpoint mismatch (repository-wide expected versus correct run-specific readback); corrected fixture remains strict, with a single post-delete list. Next focused failure was an assertion crossing a wrapped notice word; assertion now reconstructs actual visible wrapped text. Retained artifacts1773540-1791862344574751430,1773540-1791862344578354081,819-1791862559089175628 document these. Latest full-suite/fmt/clippy and complete platform/live host checks remain outstanding; no task Done or full parity claim.

Fresh full Linux checkpoint passed: CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test --offline --locked --all-targets: 288 meaningful code/provider contracts (including 90 strict provider contracts) and 114 actual compiled-binary PTY journeys, zero failures/ignored. cargo fmt --check passed; cargo clippy --offline --locked --all-targets -- -D warnings passed (8.07s). Logs target/build/root-all-targets-checkpoint.log, root-fmt-checkpoint.log, root-clippy-checkpoint.log. Node task-runner negative self-tests passed3/3; dependency integrity audit passed86 upstream roots/10300 files/108 manifest patches/117 registry packages; Backlog doctor clean72tasks. This includes new comparison2, remote-search2, raw-ref identity2, explicit-repo local/unborn2, Actions recovery2 and artifact2 PTY;18 native Actions journeys total. README reflects this actual green evidence only. Native macOS/live disposable-host/release and remaining product acceptance stay open; full product not Done.

Root subsequent full checkpoint is now green: cargo test --all-targets passed 288 code/provider tests and 114 actual real-binary PTY journeys, zero ignored; cargo fmt --check and cargo clippy --all-targets -- -D warnings also passed. Both new artifact journeys are included. Source freeze was released afterward for the next PR/provider integration wave. These are actual verified checkpoints, not complete task/platform/live-host coverage.

Long-path artifact visibility correction after temporary-directory quota recovery: export/extraction result now puts the safety statement before the possibly very long destination path, so no-overwrite/no-execution remains visible. Same actual filesystem effect and existing exact assertions, no weakened tests. Checked shared-target jobs2/incremental0/debug0 node scripts/test-task.mjs actions_artifacts with TMPDIR=/home/admin2/CODE/ghtui/target/test-tmp and GIT_CEILING_DIRECTORIES same passed5 meaningful code/provider plus2 actual mouse/keyboard PTY,0failed/ignored, code0.11+0.14s/PTY6.49s; artifacts target/test-tmp/ghtui-test-coverage-FlG0wt, log target/build/provider-artifacts-long-path-checked.log. These actual journeys independently verify preview/image/binary bytes, exported ZIP/extracted content, cancelled overwrite/deletion, one exact artifact delete, unrelated artifact preservation and unsafe archive refusal. Prior long-path failures35487-1791884564379404233/35487-1791884564386615191 retained. Full current444-code global passes, remaining unowned2long-path PTY readiness/commit-ID issues and wholelint pending; no Done.

Durable integration native artifact keyboard+mouse pair now actually passed2/2, zero ignored,6.64s using strict offline gh and real Git checkout: cargo test --offline --locked --test tui_e2e actions_artifacts_ -- --nocapture, target/build/provider-native-durable-artifacts2-first.log. Exact ZIP bytes and extraction file bytes/0600 mode, non-overwrite refusal, expired/unsafe archive rejection, one exact ID41 DELETE plus independently confirmed typed SQL artifact_deleted receipt, and no remote marker for local archive effects or cancelled deletion remain asserted. New controller identity guards are explicit OPEN4 plus pre2/final2 around local effects/refusal and remote publication; code factory/metadata/canonical gates passed separately. Subsequent focused strict lib/Actions/provider/tui Clippy passed10.16s and wholefmt passed (provider-actions-retained-lease-focused-clippy.log and -fmt.log). This verifies this pair only; dependency readiness, native restart/receipt failure and current full all-target verification remain open. No Done/AC claim.
<!-- SECTION:NOTES:END -->
