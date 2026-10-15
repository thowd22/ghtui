---
id: TASK-68
title: Provide one curl-to-Bash installer and macOS/Linux release binaries
status: To Do
assignee: []
created_date: '2026-10-12 23:13'
updated_date: '2026-10-15 09:53'
labels:
  - delivery
  - e2e
milestone: m-5
dependencies:
  - TASK-67
  - TASK-73
references:
  - 'https://code.claude.com/docs/en/setup'
  - 'https://axodotdev.github.io/cargo-dist/book/installers/shell.html'
  - 'https://axodotdev.github.io/cargo-dist/book/installers/usage.html'
  - 'https://axodotdev.github.io/cargo-dist/book/artifacts/checksums.html'
  - 'https://curl.se/docs/manpage.html'
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-12 - macOS-and-Linux-curl-bootstrap-release-contract.md
priority: high
type: feature
ordinal: 68
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Users want the same simple curl-to-Bash installation command on macOS and Linux. Replace native installer/package-format work with one project-owned bootstrap script that selects prebuilt release binaries. A shared script does not imply a shared executable: retain Intel/ARM macOS/Linux artifact verification. No .dmg/.pkg/.deb/.rpm, package-manager repositories, Homebrew tap, Windows or background updater is required. gh/git remain external dependencies and gh owns all GitHub authentication.

The researched contract is doc-12. Do not invent a release URL or create a remote. This task remains To Do behind TASK-67 and TASK-73; research and revised requirements are not implementation or release acceptance.

## Testing steps

1. Code/provider tests: Use the stable key `release_install` to run actual Bash invocations with strict offline release transport and independent installed-byte/mode/version/target oracles. Cover Bash 3.2 and BSD/GNU behavior; four target mappings; unsupported OS/CPU; exact-version versus Latest consistency; missing Bash/curl/gh/git guidance; PATH shadowing and quoted spaces/Unicode; malformed/missing/wrong checksum or manifest; unsafe archive paths/types; symlink/unrelated destination refusal; permissions; interrupted/concurrent upgrades; same-version repeat; explicit rollback; owned-only uninstall preserving config/cache/drafts/repos. Exercise transport failures/timeouts/HTTPS-only redirects and common truncated streamed prefixes; failed integrity must precede any binary execution. Stubbed target mapping is not native host proof.
2. Real-binary TUI E2E scenario `release_install`: Pipe/download the script into Bash in an isolated child environment, install the actual release artifact, and launch that exact installed path through Session::launch_binary, with no development-binary fallback. Run fresh keyboard and mouse journeys through PR Inbox/checks, local Git graph/tree/diff, Actions logs and selected-scope Search; assert visible state and independent strict fake-gh journals, SQL/draft/config canaries, real Git HEAD/refs/index/dirty bytes, artifact identity and terminal restoration. Repeat representative journeys after upgrade/rollback and after a failed install leaves the old binary intact. Exercise 120x40/80x24, resize/scroll/focus, overlays and disabled dependency/error explanations.
3. Run `bash -n scripts/install.sh`, ShellCheck, `node scripts/test-task.mjs release_install`, `cargo test --test tui_e2e release_install -- --nocapture`, relevant code/provider tests, `cargo test --all-targets`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`. Verify nonzero meaningful selected tests and completed actual mouse/keyboard sessions; save commands, counts and failure artifacts. No scripts/tests are implied to exist by this plan.
4. Execute supported native target journeys on appropriate macOS/Linux hosts before claiming support, including planned Linux musl portability and minimum OS/CPU/runtime baselines. Cross-compilation or uname stubs alone do not satisfy native execution. Use the shared framework established by bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 One documented Bash 3.2-compatible curl-to-Bash entry point selects versioned x86_64/aarch64 macOS/Linux binaries; unsupported platforms fail clearly, with no native package installers or Windows scope.
- [ ] #2 Latest is resolved once to an exact version shared by all asset requests; explicit version pinning and install-directory override work without Rust, Node or jq on the user's machine.
- [ ] #3 Certificate-verified HTTPS/HTTPS-only redirects, bounded transport, exact SHA-256 and target/version checks occur before extraction or execution; malformed metadata, unsafe archive members and corrupt artifacts fail without installing.
- [ ] #4 Per-user no-sudo install and same-filesystem atomic upgrades preserve the prior executable on failure, handle interruption/concurrency, and refuse unrelated/symlinked targets; repeating the same verified version is safe.
- [ ] #5 gh/git remain external and auth belongs to gh; missing-dependency/PATH guidance is actionable, shell profiles are not silently edited, and local Git stays independent of GitHub login. Optional avatar/diff features retain clear fallbacks.
- [ ] #6 Docs cover trust limitations, download-inspect-run/manual archive alternatives, explicit rerun upgrades/version rollback and verified owned-only uninstall preserving configuration, cached data, drafts and repositories; no invented published URL or background updater.
- [ ] #7 The shared PTY runner launches the actual script-installed release binary, independently checks its bytes/version/target and matches keyboard/mouse product behavior on supported native hosts without source-build fallback.
- [ ] #8 Automated code/provider and shell tests pass for the task-specific success/failure scenarios with nonzero selected tests and recorded commands/counts/artifacts.
- [ ] #9 Real-binary PTY E2E tests pass for specified keyboard and mouse journeys, resize/focus/loading/error states and independent command/storage/Git effects; every visible action remains accessible by both inputs.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
2026-10-15 authorised PARALLEL SOURCE-ONLY slice ahead of formal TASK67/73 release acceptance: (1) NEW isolated authenticated current source copy; preserve all native/external leases and Main. (2) Bash3.2 scripts/install.sh: explicit HTTPS release-base until real hosting exists, one Latest version resolution, exact version/target manifest +SHA256SUMS, sole regular ghtui archive member streamed into private same-filesystem stage, raw ELF64/Mach-O CPU checks, bounded verified --version before commit. (3) Per-user quoted directory, owned regular target/0600 receipt, mkdir serialization, bounded current/pending old/new digest recovery ordering and atomic binary replace; explicit pin rollback/repeat, verified-only uninstall preserving application data; missing git/gh/PATH guidance without auth/package/profile changes. Conservative stale locks refuse automatic removal; document exact inspect/recover prerequisite. (4) Developer-only Node release-asset assembler for already built actual native binaries and manifests, no build/publish/CI automation. (5) Strict offline actual Bash/fake-curl code/provider declarations for target/integrity/archive/atomic/concurrency/interrupt/metadata/cleanup/missing tools, local real TLS curl fixture, common truncated prefixes; installed-path real-binary keyboard/mouse PTY declarations across local/PR/Actions/Search after success/update/rollback/failure. Archive tests package explicitly supplied actual executable and never fall back around installer; production release-profile/musl/nativeMac proof stays OPEN. (6) Root alone executes shellcheck/bash-n/focused code+PTY/full/fmt/strict/runtime/native gates under hard caps. Freeze precise current-relative paths/hashes; NO source/code/test execution or release completion claim in this lane.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-14 user-directed distribution revision: investigated official Claude Code setup, cargo-dist shell/usage/checksum contracts, curl options and GitHub artifact attestations. Adopt one project-owned Bash bootstrap plus four prebuilt binary archives as primary install; remove mandatory native package formats, keep gh/git external and no auto-updater. Exact research/trust/tests in doc-12. TASK-67 dependency, To Do status and unchecked DoD/AC retained. This is research/backlog documentation only: installer/native release tests not implemented or run, no remote/domain/release publication. Historical release_packaging key is superseded by release_install; future implementation must register meaningful code/provider and actual installed-artifact PTY tests, not a zero-count renamed filter.

Research/backlog verification 2026-10-14: node scripts/audit-backlog.mjs exit0,72tasks,validtrue,0errors; backlog doctor exit0,no duplicate/self/cyclic IDs; exact CLI JSON readback verified TASK-68/69 descriptions/criteria/doc12 links with unchanged dependencies/status/DoD and all AC unchecked; CLI readback verified full doc12/doc2 content. README planned installation has no invented URL; git remote -v empty. git diff --check exit0 only checks tracked changes (this worktree is predominantly untracked), not a full-file release check. No installer/code/PTY/native release tests run for these documentation-only changes; four previously registered scoped_check_pending_ lib contracts remain recorded red in TASK-14 and are unrelated to installer acceptance. No release/build/publication or project completion claim.

2026-10-14: Added TASK-73 as a direct release-artifact integrity prerequisite after verifying bundled SQLite3.50.2 and the official WAL-reset fix/backport advisory. Existing novice prerequisite TASK-67 retained; TASK-69 remains near-end and inherits TASK-73 through TASK-68. No installer/script/remote or advisory fix is implied implemented by this dependency change.

User supplied the explicit project remote on2026-10-14: git@github.com:thowd22/ghtui.git, for use when delivery checkpoint is ready. This replaces the missing-target planning context but does not imply a hosted pipeline/release exists or permission to push unfinished work now. Keep TASK69 near-end and gated by65/68; no workflow or release acceptance from this note. Root will configure/publish to this exact remote at the verified checkpoint, retain gh auth/installed git semantics, and record actual hosted/native-macOS/installed-archive evidence before completion. Primary installation/release URL remains unclaimed until artifacts really exist.

Root delegated this independent source implementation to keep agents progressing while guarded review/native cohorts undergo serial verification. Formal TASK67/73 dependencies and TASK69 near-end publication ordering remain unchanged; Task68 status remains To Do and every AC/DoD unchecked until actual release acceptance. No publishing/remote/hosted CI writes authorised or performed.

SOURCE-ONLY installer cohort now written in target/parallel-tree-release-install.BhbFOB (independent baseline tree 0faa0b9a22240d9c31b20b6ab6350166a8801d7b; copied Main860 firstparty hash 176f3869834ad4fdb468246bacba5cd3bbca4be7913a0bd75305499faf4db3b2). Seventeen intended paths: actual Bash3.2 installer, dev-only deterministic artifact assembler, strict offline curl fixture/provider contracts, real private loopback-TLS cases, explicit real-product QA descriptor helper, and eight installed-path keyboard/mouse PTY declarations via unchanged existing journey semantics. Node declarations:21 shell +1 realTLS top-level(5 actual transport cases)+6 assembly; Rust code wrappers3, actual installed PTYs8 spanning fresh/upgrade/rollback/refused-tamper and exact executable SHA/inode/receipt/user-data, PR source/check/log/SQL, Actions, local graph/tree/diff, scoped Search and restored TTY. FIRST/SECOND descriptors must be genuine distinct Cargo versions/native binary digests; missing inputs fail, no C probe/development-binary fallback. Source-only git diff --check0; NO Bash/Node/Cargo/rustc/PTY/fmt/Clippy execution by this lane, no pass claim, no public artifacts/remote/GitHub writes. Root is sole hard-capped verification lane and approved actual static musl QA builds; native macOS/RustMSRV/full/release/CI dependencies remain open, TASK68 stays ToDo. Primary references reconfirmed: https://curl.se/docs/manpage.html ; https://www.gnu.org/s/bash/manual/html_node/Bash-Builtins.html ; https://github.com/libarchive/libarchive/blob/master/tar/bsdtar.1 ; https://doc.rust-lang.org/stable/reference/linkage.html . These support protocol choices, not unrun Bash3.2/native-Mac compatibility.

FROZEN SOURCE-ONLY finalV2: target/build/parallel-tree-task68-installer-v2-sourceonly-20261015.patch SHA ce04b43ece4dab447c5ae0e15b6d203656d49991f54b9602d5d51fd3fceb3b08 (105436bytes/17paths) + parallel-tree-task68-installer-v2-sourceonly-handoff-20261015.json SHA d91a3a740df854fa14c947cb21df5b80f1dde03f09c4c9fc128104de0c7090fc. New immutable successor target/parallel-tree-release-install-v2.Wliw3q preserves original BhbFOB/earlyV1B. Added explicit .mjs exec shim for developer Node18+ portability and strict final binary-replacement failure case (real receipt renames, prior executable inode/digest preserved, next explicit retry), no executed defect claim. Read-only Main forward apply0/private reverseapply0/diffcheck0; all17 baseline/output SHA values recorded; only existing overlap newer tests/tui_e2e.rs registrations must compose. Cargo.toml/Cargo.lock/productionRust untouched. Declared3Rust provider wrappers/28Node(21shell+1TLS5modes+6asset)/8actual installed-path PTYs,48intended sessions; ALL UNCOMPILED/UNRUN. Root alone will build two actual different Cargo package-version static-musl QA releases and run capped verification; missing FIRST/SECOND descriptors fails, no substitute/skip. Earlier V1 transfer artifact corrupt atline1500(read-onlyapply128 due serialization removing Gitcontext blank) retained as negative; DO NOTAPPLY it. V1B valid earlypacket preserved, finalV2 supersedes it. No app/install/native-Mac/MSRV/full/release success, no publication/remote/auth/CI changes, no status/AC/Done promotion; formalTASK67/73 andTASK69delivery remain open.
<!-- SECTION:NOTES:END -->
