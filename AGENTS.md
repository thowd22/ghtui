# ghtui development instructions

This project is a Rust/Ratatui Git and GitHub TUI for macOS and Linux only. Use `gh` for GitHub API transport and authentication, and installed `git` for local Git semantics. Public avatar assets may use an isolated unauthenticated image client. Do not add Windows scope or a separate GitHub authentication implementation.

The research and specifications linked from README.md describe the current product contract. Read relevant documents and task dependencies before implementation.

## Backlog workflow

Use Backlog.md CLI to create or edit tasks, documents, and milestones. Do not directly edit its generated Markdown. Begin with `backlog instructions overview` and the appropriate detailed workflow guide. Future tasks belong in this backlog with explicit dependencies, acceptance criteria, and testing steps.

The executable and shared real-binary PTY framework are implemented and under active verification. Read task status/evidence before starting: prepared backends do not imply their product workflows are complete. Respect dependency readiness. The user's 2026-10-16 reset replaces the parallel implementation loop: no sub-agents; one coordinating agent works on one feature at a time. See backlog doc-13 for the stopped-work inventory and sequential delivery policy. GitHub CI/CD is TASK-69 near the end; local verification is required from the start.

## Required verification for every task

### Development-host resource safety

After the host reboot/resource-pressure incident, serialize implementation and verification across all private copies. Do not restart sub-agents without a new user request. The coordinating Root alone launches Cargo, rustc and PTY verification, holding the shared `target/verification.lock`. Use explicit working directories, `CARGO_BUILD_JOBS=1`, `RUST_TEST_THREADS=2`, `RAYON_NUM_THREADS=2`, and disk-backed build/temporary paths.

On this Linux development host, put verification in a transient user scope with CPU quota 200%, memory high/max 4/6 GiB, swap max 512 MiB and task max 256, plus low CPU/I/O priority. Verify that limits are active; do not silently fall back to an unbounded run. These are local development safeguards, not runtime dependencies or native macOS evidence. Preserve interrupted logs and revalidate process ownership and working directories after session or host restarts.

Use `node scripts/test-all.mjs` for the unfiltered `cargo test --locked --all-targets` gate and `node scripts/test-task.mjs <key>` for checked task gates. Both compose a child-only Git environment that keeps automatic maintenance foreground without changing user/repository config or the installed application. Preserve this policy for direct diagnostic Cargo runs: detached maintenance escapes the PTY session and accumulates adopted zombies under the Linux test subreaper. Do not replace exact owned-session cleanup with indiscriminate orphan reaping or increase task limits to conceal cumulative leaks.

### Task gates

- Implement meaningful code/provider tests and a task-specific real-binary PTY E2E journey.
- Exercise both mouse and keyboard input and independently verify visible state plus command/storage/Git effects. Snapshot-only or direct-handler tests do not replace PTY coverage.
- Use temporary real Git repositories/local bare remotes and strict offline fake-gh fixtures. Live mutation tests require an explicitly configured disposable repository.
- Verify nonzero selected test counts; ignored tests, placeholder assertions, and accepted-by-default snapshots cannot satisfy required coverage.
- Finish the focused feature implementation and its test coverage, then run its code/provider and real-binary PTY gates; fix failures before moving to another feature. Run `cargo test --all-targets`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings` before completion. Targeted diagnostics remain allowed, but do not repeatedly launch full-suite gates after individual small edits.
- Record actual commands, test counts/results, and relevant artifacts in task notes. Do not mark planned or unverified work Done.

Every visible action must be clickable and keyboard accessible through the same action semantics. Test focus, resizing, scrolling, overlays, disabled controls, and relevant failure states. Usernames/avatar fallback and semantic colors/labels must be consistent across actor and Git/CI views.

Read-only browsing must preserve the user's working tree. Local and remote mutations require explicit target/effect handling, current state checks, and the task's relevant confirmation/recovery workflow. Do not create a remote as part of the initial planning work.
