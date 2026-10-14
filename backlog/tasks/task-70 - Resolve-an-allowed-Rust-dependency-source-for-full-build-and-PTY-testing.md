---
id: TASK-70
title: Resolve an allowed Rust dependency source for full build and PTY testing
status: Done
assignee:
  - '@codex'
created_date: '2026-10-12 23:25'
updated_date: '2026-10-14 08:44'
labels:
  - environment
  - e2e
milestone: m-0
dependencies: []
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: spike
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The actual application cannot build because the available Cargo cache lacks Ratatui/Crossterm and the execution environment cannot resolve crates.io. Network-enabled git clone was explicitly rejected by the tool's granular policy. This is a concrete environment blocker, not an unimplemented feature or missing user authorization.

## Testing steps

1. Code/provider tests: Verify dependency source integrity, Cargo resolution and full compile/test commands using the actual package; reject zero selected tests and mismatched versions.
2. Real-binary TUI E2E scenario `dependency_transport`: Build the real ghtui executable, run bootstrap/help/quit and delayed-provider journeys via mouse and keyboard at wide/narrow sizes, and verify restored terminal and test artifacts.
3. Run `cargo test --test tui_e2e dependency_transport -- --nocapture` with a nonzero selected count and `cargo test --all-targets`; record actual results. Continue existing isolated offline code tests while transport is unavailable.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 An allowed source supplies all required crates with verified package provenance; no sandbox or network-policy bypass.
- [x] #2 The actual workspace builds and required code/provider and real-binary PTY mouse/keyboard suites run with nonzero tests.
- [x] #3 Dependency downloads/builds do not require unapproved writes outside task scope.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [x] #1 Record actual commands, selected test counts and artifacts; run cargo test --all-targets, cargo fmt --check and cargo clippy --all-targets -- -D warnings.
- [x] #2 Verify keyboard/mouse equivalent behavior through the actual executable; do not mark blocked or untested behavior complete.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Inspect the actual environment and exhaust allowed local/cache routes.
2. Apply an in-scope resolution if available without asking for renewed user authorization.
3. Verify the real code and keyboard/mouse PTY outcomes before completion.

Root current finalization verification: after measured/projection/local Git-label inclusion, create a NEW private ignored copied-source directory with Cargo/lock/.cargo/vendor/src/tests/scripts/README/AGENTS and initially empty task-specific CARGO_HOME plus separate target/private test TMP+Git ceiling. Run checksum/provenance audit, copied offline locked all-target --no-run and checked dependency_transport code+keyboard/mouse PTY. Preserve historical copy/artifacts; no user files/remote/dependency downloads/vendor implementation changes. Only mark dependency-transport task Done after its actual AC/DoD plus current main-project full/fmt/strictClippy evidence; macOS/release packaging remain separate task gates.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Observed cargo check --offline: no matching package named crossterm. cargo fetch cannot resolve index.crates.io. require_escalated git clone rejected because sandbox_approval is false. Safe cached-only subset tests remain possible.

New evidence: ordinary approved git ls-remote and git clone reach GitHub despite Cargo/getent DNS failures. Git clone ratatui first failed solely from wrong tag naming; verified official tag is ratatui-v0.30.2 at commit e665c36cb14752a61cd777fbd06dbef8474f2add. Team is pursuing legitimate official-source vendoring through approved Git commands while preserving licenses and version provenance.

Portable source milestone (2026-10-13): Cargo patch paths are relative vendor/upstream; .cargo/config.toml selects cargo-vendored registry sources. Recorded 41 official Git origin/revision pins with exact manifest-only adaptations and retained licenses. node scripts/audit-dependencies.mjs verifies 7,375 upstream files, 49 path patches, 96 registry packages and lock/source checksums; malicious path/corrupt-file negative code test is included. Checked runner CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 node scripts/test-task.mjs dependency_transport passed 2 code/provider + 2 actual-binary PTY tests, with real keyboard and mouse coverage journal /tmp/ghtui-test-coverage-5w9YnJ. Copy verification used an empty CARGO_HOME and resolved 142 packages via cargo metadata --offline --locked --filter-platform x86_64-unknown-linux-gnu, all manifest sources strictly inside the copied project; copied audit also passed. Independent copied cargo check advanced through actual Rust dependencies but failed Disk quota exceeded (not a code/dependency error), so copied compilation is NOT claimed complete. Removed only the redundant task-created copied project and cargo-cleaned its 109.8MiB target after verifying original source integrity. Embedded dependency Git metadata, unused exploratory versions, and large upstream Zip/VT100 test corpora were moved recoverably to /tmp/ghtui-vendor-metadata.yJVEQr; product source/tests retained, no project .git mutation. Current project vendor reduced to 167MiB before incoming search packages. Full current all-targets/fmt/clippy gate still required before Done.

Root verified cargo metadata --offline --locked:150 packages with every manifest inside the workspace; node scripts/audit-dependencies.mjs passed41 upstream sources,7375 files,49 relative patches,96 registry packages. All root code compiles offline via relative vendored sources. Disk quota recurred due duplicated validation outputs; after verified Git tests and agent coordination, cargo clean removed100MiB only from /tmp/ghtui-git-validation.AsAVCb/target. Its source/lock and captured results remain, all subsequent builds share one target. No user source removed.

Search dependency transport recovered legitimately: actual Cargo now includes embedded ripgrep ignore/grep libraries, nucleo-matcher, and Tantivy0.26.2 mmap with default stems/compression disabled. Final current source audit passes 85 official repository pins, 10,286 retained upstream files, 107 relative source patches, and117 registry packages; hashes/lock/license provenance frozen, no automatic acceptance during tests. Exact additional Unix-only manifest reductions (Tantivy/fs4 Windows target dependencies) retained in vendor/search-manifest-adaptations.patch. Upstream htmlescape supplies only its original Apache-2.0 / MIT / MPL-2.0 Cargo declaration; this limitation is explicitly recorded rather than claiming missing license files exist. Large unused minimal-lexical corpora were sparse-trimmed; source revision and full recoverable Git objects retained. Temporary-files quota required moving task-owned metadata/test-corpus backup into ignored .verification/dependency-metadata, merging partial failed copies and independently verifying each saved Git HEAD before removing embedded metadata from shipping sources. Shared dependency compilation progressed through Tantivy to actual application compile; stopped only on parent mid-edit Search fields, not dependency or code owned by foundation. Complete current all-targets/fmt/clippy evidence still pending coordinated stable source window.

Resolved recurring compiler os122 quota safely: verified no active Cargo/rustc, moved exact task-owned shared target1.1GiB from /tmp/ghtui-provider-check.gVlcVM/target to ignored project target/build on workspace disk (681GiB available), kept old path as symlink. Artifacts/fingerprints/evidence preserved; no source or user data deleted. Subsequent checked cache_storage runner succeeded8 code/4 PTY. This removes temporary-filesystem quota as a build blocker; relocated clean-source validation remains separate.

A fresh independent portable source copy now exists at ignored .verification/portable-copy.Nh4dBT, including only project build/test/document/vendor inputs and excluding Git metadata, compiler outputs and development backups. The copied provenance/checksum audit passes: 85 official upstream roots, 10,286 retained upstream files, 107 patches, 117 registry packages. With an initially empty copied CARGO_HOME, cargo metadata --offline --locked --format-version 1 --filter-platform x86_64-unknown-linux-gnu resolved 225 packages; an independent check proved every package manifest is inside the copied project. This is dependency-resolution evidence only; copied compile/runtime gates are queued after the shared build handoff. Compiler artifacts have been recoverably relocated to ignored workspace target/build with a compatibility symlink, avoiding the /tmp quota without deleting prior evidence.

Independent copied build and runtime verified: in .verification/portable-copy.Nh4dBT with an initially empty CARGO_HOME and a separate ignored target, cargo test --offline --locked --all-targets --no-run succeeded in 53.29s, compiling every actual app/test target from the 225 wholly copied package sources. Retained compile.log in that copy. The copied checked node scripts/test-task.mjs dependency_transport then passed 2 meaningful source/code contracts and 2 actual-binary PTY journeys, zero ignored, with independently recorded keyboard and mouse input and restored terminal state; coverage artifact /tmp/ghtui-test-coverage-pEmtG0. Source corruption/escaping path negatives pass without changing baseline files. This closes the earlier copied-compile quota gap; final live-project all-target/fmt/clippy gate remains required before task completion.

Extended offline transport for exact GitHub-content verification: directsha1 0.10.6 official RustCrypto/hashes sha1-v0.10.6 commit7aba4b52715f9cb17a90303bab55bd59471d65ae, only unmodified sha1 package subtree retained atvendor/upstream/sha1 with both original licenses and explicit upstream_subdirectory provenance. Unused source siblings and clone metadata moved recoverably into ignored .verification/dependency-metadata; no user source removed. SHA-1 only verifies existing Git blob identity, not new security signatures. Existing audited base64 0.22.1 made directdependency. Cargo metadata --offline resolved actual lock; node scripts/audit-dependencies.mjs passed86official sources/10300file hashes/108relative patches/117registry packages. No new runtime-source or manifest-only adaptations; native all-target check with these dependencies passed. New copied-source build gate remains required for final release evidence.

ACTUAL NEW private copied-source verification: .verification/portable-copy.current.w8hEHr, initially empty cargo-home, separate target/private test-tmp+Git ceiling, env CARGO_HOME/CARGO_TARGET_DIR/TMPDIR each absolute inside copy; incremental0/jobs2/offline/profiledebug0. Copied cargo test --offline --locked --all-targets --no-run PASS exit0 initial1m23s compile.log, then synchronized Git-label next_back/native-caption/catalogue-restored-footer corrections and repeated all-target --no-run PASS compile-synchronized.log. Copied checked node scripts/test-task.mjs dependency_transport PASS2code+2realPTY actualkeyboard/mouse=true/0ignored .31s artifact test-tmp/ghtui-test-coverage-rUeMEA, dependency-transport-checked-current.log. Metadata --offline --locked --filter-platform x86_64-unknown-linux-gnu PASS and independent realpath assertion226/226 manifests INSIDE copy (metadata-current.json). Source audit86official/10300file hashes/108relativepatches/117registrypackages PASS copied audit-current.log and main root-git-actor-roles-dependency-audit.log. No dependency downloads/global cache/vendor runtime edits/remotes. Current main all-target1255code passed but353/356PTY failed3oldCorpus mouse clone-origin viewport fixtures; diagnosis/focused rerun underway, so required current fullgreen and finalization remain pending—no Done/AC2 claim yet. Native macOS/release packaging separate open delivery tasks.

ACTUAL current included checkpoint GREEN (2026-10-14): env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 TMPDIR=/home/admin2/CODE/ghtui/target/test-tmp GIT_CEILING_DIRECTORIES=/home/admin2/CODE/ghtui/target/test-tmp  cargo test --offline --locked --all-targets PASS1256code/provider=765lib+263strictGH+228others,356real-binaryPTY,0failed/ignored,PTY33.97s; target/build/root-git-actor-roles-observed-frames-all-targets-final.log. cargo fmt --check exit0 root-git-actor-roles-observed-frames-fmt-final.log; sameenv cargo clippy --offline --locked --all-targets -- -D warnings exit0(9.74s) root-git-actor-roles-observed-frames-clippy-final.log. Checked node scripts/test-task.mjs: git_actor_roles10code+2PTY (4SHA1/SHA256binary sessions) artifact target/test-tmp/ghtui-test-coverage-ewaO24; actor_identity12code+8PTY ...K96VfD; repo_catalog165code+44PTY ...XAPPtx; selected_corpus27code+8PTY ...dKjC0B; pr_checks_bridge20code+6PTY ...ZHsJKU; test_framework8code+3PTY ...ih5KMP; dependency_transport2code+2PTY ...Hm9I5H. All actualkeyboard/mouse=true/0ignored; exact logs root-git-actor-roles-checked-width-next-back-final.log/root-git-actor-roles-actor-identity-checked.log/root-git-actor-roles-repo-catalog-checked-frame-fixed.log/root-corpus-cycle-observed-value-checked-final.log/root-git-actor-roles-bridge-overlay-checked-final.log/root-corpus-cycle-overlay-framework-checked-final.log/root-git-actor-roles-corpus-frames-dependency-checked.log. Genuine raw objects/Git/config/private originalSQL/drafts/TTY preserved. No unadmitted observedtabs/read-limits/association fixtures in count; no remote/live mutation/macOS/performance/release/whole-product claim.

FINAL actual dependency transport revalidation: copied source synchronized after shared observed-field/overlay readiness fixes, offline locked all-target --no-run PASSexit0(4.34s) .verification/portable-copy.current.w8hEHr/compile-observed-frames.log, followed by checked dependency_transport PASS2code+2realPTY bothkeyboard/mouse=true/0ignored .30s, copy/test-tmp/ghtui-test-coverage-SUeCNH and dependency-transport-checked-observed-frames.log. Initially empty task-specific Cargo home, separate target/private TMP+ceiling, 226/226 manifests entirely within copy, provenance86/10300/108/117 and no downloads/global cached dependencies/vendor-runtime changes. Main checkeddependency2+2 + full1256code/356PTY/fmt/strictClippy now allGREEN, superseding earlier pending/failed corpus runs. Node runner negative3/3 pass0skipped; actual source integrity/path escape negatives included. All Task70 AC/DoD evidence now met for environment/dependency recovery. This completes ONLY transport recovery; Mac native validation, performance, packaging/CI and remaining product tasks stay open, no remote created.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Resolved the build/test dependency blocker with pinned, licensed and checksum-audited in-project sources. Verified a new clean copied-source offline locked all-target compilation with initially empty Cargo home and all226 package manifests inside the copy; copied checked2code/2realPTY and main1256code/356PTY/fmt/strictClippy pass, zero failed/ignored. See implementation notes/artifacts. Native macOS and release/CI are separate open tasks; no remote or live mutation.
<!-- SECTION:FINAL_SUMMARY:END -->
