@AGENTS.md

# Lessons learned for agents

AGENTS.md is the project contract. This file records pitfalls that agents working on ghtui have actually hit, and the rule that avoids each one. Read it before starting. Add a short entry when you hit a new pitfall that the next agent should know about.

## 1. No testing loops

Endless test-and-retry loops are the most expensive failure here. Make one meaningful, targeted change per failure and then stop.

- **At most 3 focused test runs per task to fix failures.** After the third run, stop and report. Say what still fails, what you changed but did not re-run, and what you think the cause is. A clear report beats a fourth run.
- **Never run the full suite** (`node scripts/test-all.mjs`, about 30–40 minutes) unless the coordinator asked for it. Run only the tests your change affects: `cargo test --lib <filter>`, `cargo test --test tui_e2e -- <module>`, or `node scripts/test-task.mjs <key>`.
- **Read the failure before rerunning.** A rerun without a code change only tells you something if you're checking for flakiness, and that question has its own rule (section 4).
- **Don't wait on a build or test that has stopped making progress.** If it shows no output for about 20 minutes, check memory and swap and the processes it is running (`ps`, `free`), then stop it and report. Don't keep waiting. One merge agent waited 54 minutes on a `cargo test --no-run` that was thrashing at the memory cap.
- **Run the full suite in the background**, with `nohup … > log 2>&1 &`, and check its log every few minutes. Don't block on it.
- **Fix the cause, not the symptom.** Never make a test pass by loosening its Git, storage, command-count or privacy checks, or by adding sleeps.

## 2. Builds and host resources

- **Build and test only through `~/.cache/ghtui-agents/shared/agent-run.sh <agent-name> <cmd>`.** It allows at most 4 builds at once, each in a systemd scope with a 6 GiB cap and `CARGO_BUILD_JOBS=1`, and gives every agent its own target and temp directories. An unbounded `cargo` run can make the host unresponsive.
- **Never `cargo clean` and never delete `target/`.** A rebuild from nothing costs everyone a lot of time.
- **Keep `TMPDIR` outside this repository.** Otherwise Git-discovery tests walk up into ghtui's own `.git` and fail in confusing ways.
- **Write logs only under `~/.cache/ghtui-agents/<agent-name>/`.** A shared scratch directory let agents overwrite each other's logs.

## 3. Worktrees and merging

- **A new worktree can start on an old commit. Run `git merge --ff-only main` first.**
- **When cherry-picking an agent's branch onto main, take only the commits not on main yet** (`git log main..<branch>`). Skip any commit whose title is already on main; it is the same work under a different SHA.
- **Resolve conflicts so both sides' intent survives,** and build once afterwards.
- **Worktree-isolated agents: put multi-step build/test commands in a scratch script** and pass it to `agent-run.sh`. Compound shell commands or heredocs that mention `git` (even `git_contracts`) are refused by the isolation check, and a log redirect into a not-yet-created `~/.cache/ghtui-agents/<name>/` fails silently before the run starts.
- **Commit identity:** `git -c user.name=thowd22 -c user.email=thowd2@gmail.com commit`. Run `cargo fmt` first. Never add `.claude/` or `target-fullside-*.log`. Never push, and never tag or publish a release: those need the user's go-ahead.

## 4. "Flaky" usually means a real race

- **Before calling a test flaky, run its whole module, not just the test alone.** A test that passes alone and fails beside its siblings is load-sensitive, which usually means there's a race.
- **Example (TASK-87):** the Inbox palette test "flaked" on GitHub runners for weeks. The cause was in the product. A background load finishing just after `:` retired the open command palette, so pressing `:` silently did nothing.
- **To find a race, log events to a file.** A temporary debug log written behind an environment variable showed exactly where the key went. Remove the instrumentation before committing.

## 5. PTY test pitfalls

- **The title repaints before the footer.** Right after switching views, the screen can show the new view's title above the old view's key bar. Wait for the target view's key bar, or for the exact row you will click, before reading click coordinates from the screen. Several mouse journeys clicked a blank cell because they used a stale bar.
- **Footer controls are `key Label` with no brackets.** `click_visible_action("[r Refresh]")` maps bracketed labels onto the key bar.
- **At 80×24 some controls overflow into the palette.** Use `invoke_via_palette` explicitly when an action may not be on the bar; never fall back silently.
- **The selection marker is the `▌` bar, not `> `.** Update the expected text, not the command or storage assertion.
- **Without colour, chips print differently.** Under `NO_COLOR`, an initials chip prints as `[AL] alice`, not `AL alice`. Match the login, not the chip styling.
- **Avoid vacuous negatives.** `!screen.contains("[Something")` passes trivially once a control has moved. Use `assert_control_absent`, or check the disabled reason.
- **A followed path chip puts "was <old path>" before the subject** in a History row's MESSAGE, so `HistoryRow.message` starts with it and `message_fg` is the label's muted ink there. Compare inks only on rows without a rename label.
- **`f` is File history in Search and PR Files; Search's Save query is `S`.** On PR Files, `f` is pushed before the tabs, so it shadows `f Files` (the tab already shown).
- **Match whole column headers.** A column parser found `AGE` inside `MESSAGE`. Parse header cells by word, not by substring.
- **Repository viewers no longer own row 0.** The native diff, saved-folder detail and the saved-path and changed-files pickers draw inside the content pane, under the shell header and above a key bar with `Esc Back`. Find their frame with `support::owned_pane_frame(screen, title)`, never by row-0 or last-row corners. Their state draws with the shell's `app.hits` split off into `app.takeover_v93`, so header hits never shift their focus indexes.
- **A wait that the old frame already satisfies proves nothing.** After clicking an option that starts a native diff projection, wait for a new frame that shows `Preparing native diff`, and then for a new settled frame. Do this before you click or resize again. Otherwise a later click can land on the resized layout, and the projection's result can overwrite a newer status such as `Stage disabled:`.
- **Viewer frames and actor rows match panes now.** An embedded viewer's top border reads `╭─ ▶ Title ─╮` (only colours change when that does not fit), so match the title text, never `╭Title`. Commit-actor rows draw the filled initials chip (`AB author …`); `[AB] author` appears only under `NO_COLOR`, and a cut label keeps the whole `role name` before its email.
- **Settings focus order is fixed:** Host, Repository set, Theme, Density, Save, Close. If you add a control, update the hard-coded indexes in `src/app.rs` and `src/ui.rs`, and update the keyboard journeys that tab to Save.
- **Paused-operation rows sit below the fold at 80×24.** A rebase or cherry-pick banner has Continue, Skip and Abort rows, so `[Abort rebase]` is not painted in the Files pane at 80×24. Click those rows at 120×40, or open the same command through `m`. In linked-worktree fixtures, find the index with `git rev-parse --git-path index`, not `.git/index`.

## 6. Product invariants that are easy to break

- **Every visible action works by key and by click,** with the same action semantics. A row stays a click target after a layout change. Footer priorities decide what is visible at 80 columns. Refresh and an armed Stop watch must not be pushed off the bar.
- **Read-only browsing never changes the working tree,** the index, refs or config. History is read-only; checkout, branch and reset stay behind `m` Local operations with preview and confirmation.
- **The full SHA and UTC times stay visible** in detail rows and panes even when tables show short SHAs and relative ages.
- **Keep the scope context private:** hiding a repository must also clear its head SHA from titles.
- **Use the shared building blocks** instead of drawing by hand:
  - theme roles in `src/theme.rs`, never hard-coded colours;
  - `ui_chrome::pane_block` and `split_list_preview`;
  - tables in `src/ui_table.rs`;
  - the footer key bar in `src/footer.rs`.
- **Every ghtui Git child runs with `--literal-pathspecs`,** so Git's own internal magic pathspecs (`:/`) become literal. `stash push --include-untracked` saved the untracked files but left them in place until an explicit `-- .` was added. When a Git subcommand behaves differently under ghtui than in a shell, try it with `git --literal-pathspecs` first.

## 7. Real repositories and GitHub

- **Live GitHub is read-only.** Never confirm a write on the user's real repositories. Use strict offline fake-gh fixtures, or a disposable repository the user configured for that purpose.
- **Conduit and the worklappy repo are read-only QA.** That means `/home/admin2/CODE/Conduit`, and `/home/admin2/II/insights-gcp-infrastructure` on `ssh worklappy`.
  - Browse only, and never push or edit.
  - Record `git status` and refs before and after, and confirm nothing changed.
- **On worklappy, never overwrite `~/.local/bin/ghtui`.** Copy the binary into a new `mktemp -d` directory under `/tmp`, use a temporary `--config-dir`, and `rm -rf` the directory afterwards.
- **The worklappy checkout's `core.worktree` points at an old directory,** so ghtui correctly shows its explanation screen there instead of the repository. To capture real History screens, work in a throwaway local clone inside that same `mktemp` directory:
  - `git clone --no-checkout <repo>/.git repo`, then `git fetch --update-head-ok origin '+refs/heads/*:refs/heads/*' '+refs/remotes/*:refs/remotes/*' '+refs/tags/*:refs/tags/*'` to copy every branch, remote branch and tag.
  - Compare `git for-each-ref` checksums of the original before and after. Don't run `git status` in the original, because it would touch the other worktree's index.
- **Do any writes in throwaway repositories,** like the temp fixtures, never in the user's working trees.

## 8. Reporting

- **Keep backlog notes short:** what changed, the gate command, and pass counts. Use the Backlog.md CLI. Don't mark work Done unless it has been verified.
- **End with a concise report:** changes, files, each test command with pass/fail counts, remaining failures, commit SHAs, and any product issues you noticed but didn't fix.
- **A test file no `mod` declares never compiles or runs.** TASK-49's relevance-quality tests sat unregistered for weeks. When you add `*_tests.rs` or `tests/tui_e2e/*.rs`, declare it (`#[cfg(test)] mod …;`, `#[path]`, or `tests/tui_e2e.rs`) and confirm its tests appear in the run output.
