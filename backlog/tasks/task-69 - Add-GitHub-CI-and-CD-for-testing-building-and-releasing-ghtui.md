---
id: TASK-69
title: Add GitHub CI and CD for testing building and releasing ghtui
status: To Do
assignee: []
created_date: '2026-10-12 23:13'
updated_date: '2026-10-14 16:39'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-14 aligns near-end CI/CD with user-selected curl-to-Bash distribution: one script plus exact tested four-target binaries/metadata/checksums; install via the script and exercise installed executables before stable publication, preserve protected permissions/fork isolation and distinguish actual hosted/native evidence. TASK-68/TASK-65 dependencies, To Do and unchecked completion retained. Research/backlog changes only; no workflow implementation, remote, hosted run, signing credentials or publication claimed. See doc-12.

Research/backlog verification 2026-10-14: node scripts/audit-backlog.mjs exit0,72tasks,validtrue,0errors; backlog doctor exit0,no duplicate/self/cyclic IDs; exact CLI JSON readback verified TASK-68/69 descriptions/criteria/doc12 links with unchanged dependencies/status/DoD and all AC unchecked; CLI readback verified full doc12/doc2 content. README planned installation has no invented URL; git remote -v empty. git diff --check exit0 only checks tracked changes (this worktree is predominantly untracked), not a full-file release check. No installer/code/PTY/native release tests run for these documentation-only changes; four previously registered scoped_check_pending_ lib contracts remain recorded red in TASK-14 and are unrelated to installer acceptance. No release/build/publication or project completion claim.
<!-- SECTION:NOTES:END -->
