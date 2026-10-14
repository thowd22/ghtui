# ghtui development instructions

This project is a Rust/Ratatui Git and GitHub TUI for macOS and Linux only. Use `gh` for GitHub API transport and authentication, and installed `git` for local Git semantics. Public avatar assets may use an isolated unauthenticated image client. Do not add Windows scope or a separate GitHub authentication implementation.

The research and specifications linked from README.md describe the current product contract. Read relevant documents and task dependencies before implementation.

## Backlog workflow

Use Backlog.md CLI to create or edit tasks, documents, and milestones. Do not directly edit its generated Markdown. Begin with `backlog instructions overview` and the appropriate detailed workflow guide. Future tasks belong in this backlog with explicit dependencies, acceptance criteria, and testing steps.

The executable and shared real-binary PTY framework are implemented and under active verification. Read task status/evidence before starting: prepared backends do not imply their product workflows are complete. Respect dependency readiness and the user's authorized parallel implementation loop. GitHub CI/CD is TASK-69 near the end; local verification is required from the start.

## Required verification for every task

- Implement meaningful code/provider tests and a task-specific real-binary PTY E2E journey.
- Exercise both mouse and keyboard input and independently verify visible state plus command/storage/Git effects. Snapshot-only or direct-handler tests do not replace PTY coverage.
- Use temporary real Git repositories/local bare remotes and strict offline fake-gh fixtures. Live mutation tests require an explicitly configured disposable repository.
- Verify nonzero selected test counts; ignored tests, placeholder assertions, and accepted-by-default snapshots cannot satisfy required coverage.
- Run focused tests during implementation, then `cargo test --all-targets`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings` before completion.
- Record actual commands, test counts/results, and relevant artifacts in task notes. Do not mark planned or unverified work Done.

Every visible action must be clickable and keyboard accessible through the same action semantics. Test focus, resizing, scrolling, overlays, disabled controls, and relevant failure states. Usernames/avatar fallback and semantic colors/labels must be consistent across actor and Git/CI views.

Read-only browsing must preserve the user's working tree. Local and remote mutations require explicit target/effect handling, current state checks, and the task's relevant confirmation/recovery workflow. Do not create a remote as part of the initial planning work.
