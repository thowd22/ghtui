---
id: TASK-69
title: Add GitHub CI and CD for testing building and releasing ghtui
status: To Do
assignee: []
created_date: '2026-10-12 23:13'
updated_date: '2026-10-17 04:31'
labels:
  - delivery
  - e2e
milestone: m-5
dependencies:
  - TASK-68
  - TASK-65
references:
  - 'https://docs.github.com/en/actions'
  - >-
    https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-12 - macOS-and-Linux-curl-bootstrap-release-contract.md
priority: high
type: feature
ordinal: 69
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
This project needs its own GitHub CI/CD near the end of delivery. Automate the existing local test gates and publish the single Bash bootstrap plus verified macOS/Linux prebuilt release archives from TASK-68, instead of native platform/package-manager installers. Hosted publication starts only after a remote is configured; this task does not create a remote.

## Testing steps

1. Code/provider tests: Validate workflow structure, least permissions, pinned actions/tools, native target matrix, cache keys and protected trigger rules. Validate exact tag/Cargo/binary/manifest version agreement, script/archives/checksums, attestation metadata where supported, failed-suite/version-mismatch/partial-matrix refusal, and draft-to-Latest promotion only after every required release gate. Fork PRs never receive write/signing credentials.
2. Real-binary TUI E2E scenario `project_cicd`: Use the real single-script release_install path on native macOS/Linux hosts for each supported x86_64/aarch64 target; independently prove the exact tested bytes are the uploaded bytes. Run packaged/test-built comparison and required keyboard/mouse product journeys through installed paths, including update/rollback/failure recovery at 120x40/80x24 with resize, fixture effects and terminal restoration. Verify failure artifacts and inspect a real pipeline run in the Actions viewer after remote configuration. Do not silently skip required PTYs or present cross-compilation/local YAML checks as a hosted run.
3. Run workflow/shell validation, `node scripts/test-task.mjs project_cicd`, `node scripts/test-task.mjs release_install`, `cargo test --test tui_e2e project_cicd -- --nocapture`, relevant code/provider tests, `cargo test --all-targets`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`. Verify nonzero meaningful counts, completed mouse/keyboard sessions and uploaded reproducible failure artifacts. These are future requirements, not executed test claims.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 PR/push CI runs formatting, clippy, code/provider tests, real-binary PTY E2E and coverage audits on macOS/Linux with failure artifacts; required tests are neither ignored nor silently skipped.
- [ ] #2 Protected release jobs build and natively verify supported x86_64/aarch64 macOS/Linux artifacts, the one Bash bootstrap, exact-version metadata and SHA-256 files, with tag/Cargo/binary versions agreeing.
- [ ] #3 CI invokes TASK-68's script to install the exact release artifacts and runs the shared keyboard/mouse PTY journeys against those installed paths before publication, including safe update/rollback/failure recovery.
- [ ] #4 Publish the script and tested archives/checksums to a draft GitHub release, promoting stable Latest only after all required gates pass; native package installers, Windows artifacts and extra updater services are not required.
- [ ] #5 Workflow permissions/caches/concurrency and pinned actions/tools are explicit; fork PRs receive no write/signing credentials. Publish build provenance attestations when the configured repository/plan supports them and document honest verification/trust boundaries without bootstrap login.
- [ ] #6 Pipeline verification distinguishes local validation, cross-compiled artifacts and actual native/hosted GitHub run evidence after remote setup; no remote or invented installation URL is created by local planning.
- [ ] #7 Automated code/provider workflow tests and genuine installed-release keyboard/mouse PTY journeys pass with nonzero selected counts, independent effects and actual commands/artifacts recorded.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
2026-10-16 READONLY V36 readiness proposal only: target/build/provider-parallel-task69-native-release-readiness-v36-proposal-20261016.md. Re-read69/65/68/73status/AC and Doc12 plus current checked task/full test-child scripts and installer/assembler/native QA adapters. Verified current official GitHub four native labels:ubuntu-24.04 x64,ubuntu-24.04-arm,macos-15-intel,macos-15 ARM; native Bash3.2/BSD on bothMacs, actual host/toolchain/image preflight required, no stub/crossbuild/Linuxscope-as-Mac evidence. Plan same tested final-tag binary/archive/script bytes through QA.1/QA.2 AND explicit QA.1/final-tag generated-descriptor pairings, read-only aggregate exact per-fileSHA/source/tag/bothinputs/cleanup gate, then protected draft-onlyupload and guarded stableLatest with no rebuild/rearchive. Action/tool fullSHA/hash audits and cache trust scopes explicit; initial cold/no-cache release, minimumpermissions and no fork secrets/livegh. Actual action pin lookup gaps remain holds, not inventedSHAs. Resource/time partition and failureartifact/nonzero/alltargets/fmtstrict/MSRV/native4target contracts specified. No workflowimplementation, remote/auth/push/CIrun/build/install/descriptor/publicURL/signing/attestation created. Formal65/68/67/73/69AC/deps remainopen; Root must review material final-tag installed pairing and aggregate/publication interfaces before anyimplementation.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-14 aligns near-end CI/CD with user-selected curl-to-Bash distribution: one script plus exact tested four-target binaries/metadata/checksums; install via the script and exercise installed executables before stable publication, preserve protected permissions/fork isolation and distinguish actual hosted/native evidence. TASK-68/TASK-65 dependencies, To Do and unchecked completion retained. Research/backlog changes only; no workflow implementation, remote, hosted run, signing credentials or publication claimed. See doc-12.

Research/backlog verification 2026-10-14: node scripts/audit-backlog.mjs exit0,72tasks,validtrue,0errors; backlog doctor exit0,no duplicate/self/cyclic IDs; exact CLI JSON readback verified TASK-68/69 descriptions/criteria/doc12 links with unchanged dependencies/status/DoD and all AC unchecked; CLI readback verified full doc12/doc2 content. README planned installation has no invented URL; git remote -v empty. git diff --check exit0 only checks tracked changes (this worktree is predominantly untracked), not a full-file release check. No installer/code/PTY/native release tests run for these documentation-only changes; four previously registered scoped_check_pending_ lib contracts remain recorded red in TASK-14 and are unrelated to installer acceptance. No release/build/publication or project completion claim.

User supplied the explicit project remote on2026-10-14: git@github.com:thowd22/ghtui.git, for use when delivery checkpoint is ready. This replaces the missing-target planning context but does not imply a hosted pipeline/release exists or permission to push unfinished work now. Keep TASK69 near-end and gated by65/68; no workflow or release acceptance from this note. Root will configure/publish to this exact remote at the verified checkpoint, retain gh auth/installed git semantics, and record actual hosted/native-macOS/installed-archive evidence before completion. Primary installation/release URL remains unclaimed until artifacts really exist.

Explicit user project remote read-only SSH probe completed2026-10-14: GIT_TERMINAL_PROMPT=0 and ssh BatchMode/ConnectTimeout/StrictHostKeyChecking=yes git ls-remote --heads git@github.com:thowd22/ghtui.git exited0 and advertised no heads. This is observed access at that time, not proof of all refs/permissions or a hosted pipeline. Main git remote -v remainsempty; no origin configured, push, release or external mutation occurred. User target reserved for verified delivery checkpoint; TASK69 remainsnear-end ToDo.

2026-10-16 Task68 READONLY coordination: provider-parallel-task68-installer-inventory-proposal-v26-20261016.json SHA9aee0f37688d98baf85f2fcef2b1dcb013a896ef8b0082bd1ea809380a626bb9 authenticates existing unrunV2 installer17. No CI/CD implementation or hosted claim. Future pipeline must use same tested4native archives+script bytes, real installed-path PTY and native-host evidence, exact tag/Cargo/binary/manifest agreement, licenses/provenance and all-target partial-failure refusal before draft->Latest. V2 assembler currently lacks distributed bootstrap-script/checksum asset; proposed SEPARATE script-checksum/provenance preserves exact4 archive SHA256SUMS grammar, HOLD Root protocol review/no publication. Task68/65 prerequisites remain open; no remote/push/auth/signing/status/AC changes.

2026-10-16 Root-approved Task68 bootstrap source-only successor a4411ae1 is frozen, not admitted/executed: local assembler separate install.sh/install.sh.sha256/<=4KiBprovenance, same caller-supplied version/tree +exact bytes, no tests/signing/attestation claim. Original4archiveSHA256SUMS/v1manifest/Latest grammar unchanged. Task69 later must compare actual tested script bytes/real native artifact bytes and refuse incomplete/partial publication; source module explicitly documents local I/O may leave NEW partial assets only, never overwrite old. No publicURL/default/remote/CI/release/signing changes, all acceptance/dependency statuses open. Manifest target/build/provider-parallel-task68-bootstrap-assets-v27-sourceonly-v1-20261016.json SHAa4411ae1f6ee3b69d56e9206c159524d9f348228f1083de1cfbfdcef51b30223.

V36 source/host readiness facts: Main currently no.gitHubworkflows/noinstallscript/no configuredremote;15723trackedvendorpaths separate from firstpartyfreezevector. Prepared1303QAversions are immutable WIP and needV149compilerfixes; Root muslstd/Zig observation is not productlink proof. Official runner/image/security/artifact/attestation docs verified2026-10-16 in proposal with directlinks. Confirmed checkout7.0.1 official fullcommit3d3c42e5aac5ba805825da76410c181273ba90b1 and upload-artifact7.0.2 cf430e030ddbb5b0abf93d22962f4752f3646cd9 as referenceidentities only, NOT audited/approved workflowpins; download/cache/attest fullpin/source audits pending. No .github or productfile changes/test execution/remote write. Task69 stays near-end ToDo/notready.

Root FULL-read16068B/68line read-only readiness proposal77961c5c876f0ce81d5334fad748070032b9d16c46eeb717ec1d499d6d891440. Concrete gap confirmed: two QA versions alone do not prove final-tag executable bytes, eventual SAME harness pairing QA.1→actual final-tag plus unchanged assembled/archive bytes required before protected immutable aggregate/draft/Latest. Formal65/68/73/67 and actual Linux/Mac/ARM/MSRV/Bash3.2/BSD/runtime/action-pin/source audits remainopen. No CI workflow implementation/remote/push/hostedtests/publicURL/publication authorized or executed by this review.

User explicitly approved separate Linux development readiness and final native macOS release acceptance (17Oct). Keep CI/CD near the end. TASK1/2 original macOS acceptance remains open and release-blocking; hosted native macOS build + actual keyboard/mouse PTY evidence must close it before release promotion. Linux baseline Task79 and cross-compilation are not native macOS evidence. This is a sequencing clarification, not implementation, run evidence, status/AC completion or remote creation.
<!-- SECTION:NOTES:END -->
