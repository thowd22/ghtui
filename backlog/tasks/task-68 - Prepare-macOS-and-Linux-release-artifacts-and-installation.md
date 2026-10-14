---
id: TASK-68
title: Provide one curl-to-Bash installer and macOS/Linux release binaries
status: To Do
assignee: []
created_date: '2026-10-12 23:13'
updated_date: '2026-10-14 21:04'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-14 user-directed distribution revision: investigated official Claude Code setup, cargo-dist shell/usage/checksum contracts, curl options and GitHub artifact attestations. Adopt one project-owned Bash bootstrap plus four prebuilt binary archives as primary install; remove mandatory native package formats, keep gh/git external and no auto-updater. Exact research/trust/tests in doc-12. TASK-67 dependency, To Do status and unchecked DoD/AC retained. This is research/backlog documentation only: installer/native release tests not implemented or run, no remote/domain/release publication. Historical release_packaging key is superseded by release_install; future implementation must register meaningful code/provider and actual installed-artifact PTY tests, not a zero-count renamed filter.

Research/backlog verification 2026-10-14: node scripts/audit-backlog.mjs exit0,72tasks,validtrue,0errors; backlog doctor exit0,no duplicate/self/cyclic IDs; exact CLI JSON readback verified TASK-68/69 descriptions/criteria/doc12 links with unchanged dependencies/status/DoD and all AC unchecked; CLI readback verified full doc12/doc2 content. README planned installation has no invented URL; git remote -v empty. git diff --check exit0 only checks tracked changes (this worktree is predominantly untracked), not a full-file release check. No installer/code/PTY/native release tests run for these documentation-only changes; four previously registered scoped_check_pending_ lib contracts remain recorded red in TASK-14 and are unrelated to installer acceptance. No release/build/publication or project completion claim.

2026-10-14: Added TASK-73 as a direct release-artifact integrity prerequisite after verifying bundled SQLite3.50.2 and the official WAL-reset fix/backport advisory. Existing novice prerequisite TASK-67 retained; TASK-69 remains near-end and inherits TASK-73 through TASK-68. No installer/script/remote or advisory fix is implied implemented by this dependency change.
<!-- SECTION:NOTES:END -->
