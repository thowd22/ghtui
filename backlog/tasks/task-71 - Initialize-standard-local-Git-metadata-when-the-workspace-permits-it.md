---
id: TASK-71
title: Initialize standard local Git metadata when the workspace permits it
status: Done
assignee:
  - '@codex'
created_date: '2026-10-12 23:25'
updated_date: '2026-10-13 02:47'
labels:
  - environment
  - e2e
milestone: m-0
dependencies: []
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: chore
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The user authorized a local ghtui Git repository with no remote for now. git init --initial-branch=main failed because the workspace .git is a read-only mount. A nonstandard relocated metadata workaround should not silently replace the requested repository.

## Testing steps

1. Code/provider tests: Verify standard Git discovery, branch and repository identity via git rev-parse/status without deleting existing metadata or creating a remote.
2. Real-binary TUI E2E scenario `project_git_init`: Launch ghtui in the project directory/subdirectory, open its local workspace using keyboard and mouse, and assert correct repo root/branch/status and unchanged tracked files.
3. Run `cargo test --test tui_e2e project_git_init -- --nocapture` with a nonzero selected count and `cargo test --all-targets`. Confirm git remote -v remains empty.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A normal git init --initial-branch=main succeeds with standard .git metadata in ghtui when permitted.
- [x] #2 Backlog Git integration can be enabled without automatic commits or remote operations.
- [x] #3 Code and real-binary mouse/keyboard PTY checks confirm local project discovery; no remote is created.
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
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Observed git init failure: /home/admin2/CODE/ghtui/.git/info/: Read-only file system. The directory is dr-xr-xr-x and contains no repository metadata. User authorization is present; filesystem policy is the blocking condition.

Updated unrestricted filesystem permissions revalidated authoritative .git directory as writable and empty; git init --initial-branch=main succeeded normally at /home/admin2/CODE/ghtui/.git. git rev-parse identifies exact project root; git remote -v remains empty. Backlog filesystemOnly disabled through CLI, with existing auto_commit:false and remote_operations:false preserved. No project commit or remote created. Code and actual mouse/keyboard project_git_init verification are being added; no premature Done.

Normal root/subdirectory project discovery code passes1/1; both actual PTY project journeys passed in the latest full-suite checkpoint. Focused runner exposed resize/event ordering flakiness and a prior assertion incorrectly requiring Recent commits above a large untracked-file list. Tests now assert actual branch/unborn saved-state, and framework resize-redraw synchronization is being fixed before the checked focused/global rerun. Standard main metadata remains normal and git remote -v empty; no Done.

Checked task-specific verification now passed: node scripts/test-task.mjs project_git_init with shared offline target ran1 code +2 actual keyboard/mouse PTY tests, zero ignored; coverage /tmp/ghtui-test-coverage-DDmSgj, log target/build/project_git_init-checked.log. Root/subdirectory discovery at120x40/80x24 preserves Cargo.toml/index/remotes and exposes actual main/unborn saved-state. Shared resize synchronization now requires the actual binary new-frame/bottom-toolbar output. Fresh full test/fmt/clippy gates are running before completion.

Fresh checked node scripts/test-task.mjs project_git_init passed1 actual-project metadata code contract +2 keyboard/mouse PTY, zero ignored, artifact /tmp/ghtui-test-coverage-DDmSgj. Both actual project root and src subdirectory activate local repository exploration, resize to80x24, return Overview, preserve Cargo.toml/index/remotes and send zero GH calls. Log target/build/project_git_init-checked.log; shared offline target/jobs2/incremental0. Full root checkpoint follows.

Final local environment verification: git init --initial-branch=main succeeded using standard project .git; git rev-parse and symbolic-ref confirm /home/admin2/CODE/ghtui/main; git remote -v empty. Backlog filesystem_only:false, auto_commit:false, remote_operations:false. Checked project_git_init1code+2PTY passed (/tmp/ghtui-test-coverage-DDmSgj). Fresh cargo test --offline --locked --all-targets passed258 code/provider+102PTY, zero ignored; cargo fmt --check and cargo clippy --offline --locked --all-targets -- -D warnings passed. Logs target/build/root-{all-targets,fmt,clippy}-checkpoint.log; task-specific log target/build/project_git_init-checked.log. Source/index/remotes unchanged by real project/subdirectory PTY browsing at both sizes.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Initialized normal local Git metadata on main with no remote, enabled Backlog Git integration without automatic commits/remote operations, and verified read-only root/subdirectory discovery through1code+2keyboard/mousePTY tests plus the complete local258code/102PTY test, formatting and strict Clippy gates.
<!-- SECTION:FINAL_SUMMARY:END -->
