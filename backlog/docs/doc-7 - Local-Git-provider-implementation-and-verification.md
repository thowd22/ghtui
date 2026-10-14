---
id: doc-7
title: Local Git provider implementation and verification
type: guide
created_date: '2026-10-12 23:32'
updated_date: '2026-10-13 05:54'
---
# Local Git provider implementation and verification

Status: the installed-Git backend and native Repository integration have passing real-project Linux code and keyboard/mouse PTY evidence. Task-specific scope and full product parity remain incomplete; newly prepared changes require fresh verification.

## Provider boundary

`src/git.rs` uses the installed Git executable through the shared bounded `CommandRunner`. All command arguments are separate argv values; there is no shell evaluation. Reads disable optional index locking, external diff/textconv and terminal prompts. GitHub authentication and transport remain outside this provider.

`GitRepository::discover` supports subdirectories, linked worktrees and bare repositories. `discover_with_cancellation` and `with_cancellation` propagate a fresh background-request cancellation token into command reads and writes. Bare repositories explicitly lack working files/index. Unborn, detached, shallow, sparse and active operation state are represented in `RepoStatus`.

Tree, status, conflict and diff paths retain original bytes in `RawPath`. NUL-separated machine output is parsed; display labels escape terminal controls and unusual bytes separately. Commit parent order, author and committer identity remain explicit. Parent IDs are read from raw immutable commit objects in batches, because revision walking suppresses missing shallow parents; unloaded parents remain graph boundaries. No GitHub username is inferred from a local author name.

Saved revisions resolve to immutable commit object IDs. `tree_children` supports lazy directory reads, `objects` batches up to 1024 immutable blob IDs, and `blob` reads saved content without checkout. History and blame preserve the selected revision; refs, upstream relations, remotes, conflict stages, stashes, worktrees and reflogs have typed records.

## Comparison and staging semantics

`Comparison::Working` compares working files to the index. `Uncommitted` compares all tracked working files to HEAD, including staged root content in unborn repositories. `Staged` compares the index to HEAD. `Snapshots` compares saved A and B; `BranchChanges` compares their common starting point to B and is directional. Unrelated/incomplete histories produce an explanation.

`src/diff.rs` keeps canonical old/new paths, hunks, old/new line numbers and raw line bytes. The provider joins the patch with raw NUL-separated name-status identities so binary and mode-only changes retain accurate filenames. Unified/split presentation never rewrites canonical anchors. Local comparisons retain a content fingerprint, and plan_selected_patch rejects changes that occurred after rendering but before opening a confirmation preview.

`Diff::selected_patch` creates a partial staging patch for existing text files from file/hunk/line coordinates. Unselected removals become context; unselected additions are omitted. Hunk offsets are recomputed from selected changes, preserving quoted raw paths and no-newline markers. Partial add/delete/rename/binary changes currently require whole-file staging.

## Guarded writes

`plan(Operation)` records exact target/effect, destructive status and a content fingerprint. The fingerprint includes HEAD/status/index, working changes, untracked contents, refs, upstreams, named remote configuration and active operation. Remote previews include the resolved remote URL. `execute(plan, confirmed)` rejects cancellation and stale previews; writes serialize across the common repository to prevent app workers from racing.

Typed operations include staging/unstaging, cached patches, commit/amend with configured hooks and signing, branches/tags/upstreams, explicit named-remote fetch/pull strategies/push with exact expected-SHA force leases, stashes, linked worktrees, merge/rebase/cherry-pick/revert continuation, reflog rescue branches, explicit soft/mixed/hard reset, exact tracked-file discard and manual bisect/reset. Dirty history-changing operations are blocked. No implicit checkout, fetch, force push, hard reset or dirty-worktree deletion occurs. Explicit force/reset/discard operations are destructive previews requiring confirmation. Conflict index stage selection replaces only the chosen working file, leaves unmerged staging visible for inspection and requires a separate staging action. Continued operations accept their existing Git commit message.

A failed write may have effects. `reconcile_write` refreshes local state before acknowledging the previous outcome; callers must display the inspected state. Remote push outcomes additionally require remote-ref inspection. Hidden interactive editor or credential prompts fail visibly rather than hanging indefinitely; a complete terminal handoff still belongs to the UI workflow.

## Prepared presentation models

`src/repository_view.rs` has an incremental native parent graph with ordered edges, stable path palette slots, refs, unloaded boundaries, visible ancestry and plain-language explanations. Lanes are temporary tracing aids, not permanent branch identities.

`TreeBrowser` retains immutable revision/path/object identity through lazy expansion. `file_preview` labels text, binary, symlink, submodule and LFS-pointer content honestly, preserves raw bytes and bounds visible text. Workspace rows expose staged/unstaged/conflict/upstream/incomplete-history state with semantic roles and redundant labels. Diff display rows carry canonical side/path/SHA/line anchors through unified/split layouts and range selection.

## Historical isolated verification evidence and limits

Offline source validation uses `/tmp/ghtui-git-validation.AsAVCb/Cargo.toml`, which imports actual project modules by path and uses cached libc/tempfile dependencies. It is a workaround while the main Ratatui/Crossterm dependency downloads are restricted; it is not full-project or PTY validation.

The latest suite ran 12 meaningful parser/model/Workspace unit tests and 31 generated-real-Git integration tests (43 total) with 0 failures or ignored tests. The integration suite independently verifies working/index blobs, refs/HEAD, ordered parents and local bare remote outcomes. Fixtures cover divergent/merged/root/shallow/worktree histories, unusual byte paths, binary/mode-only changes, immutable browsing, selected staging, hooks, branches/tags, stash/worktree guards, merge conflicts/abort, reflog rescue and bisect restoration. A real-hook cancellation scenario passed: it awaits an actual 30-second hook child startup, cancels the process group and independently verifies unchanged HEAD/index and no hook completion. Additional tests cover changed remote URLs/ref targets, simultaneous confirmed staging plans, stale rendered diff selections, expected-SHA force leases, divergent pull with merge/rebase, explicit reset/discard scopes and conflict selection/staging/continuation.

Commands used with CARGO_INCREMENTAL=0 to control disk usage: `cargo test --offline --manifest-path /tmp/ghtui-git-validation.AsAVCb/Cargo.toml --all-targets`, focused `--test git_contracts staging_selected` (2 selected tests), `cargo clippy --offline --manifest-path /tmp/ghtui-git-validation.AsAVCb/Cargo.toml --all-targets -- -D warnings`, and `rustfmt --check` on the owned modules/tests. Required full-project test/fmt/clippy and real-binary keyboard/mouse PTY checks remain task acceptance gates. Backlog task status and criteria remain incomplete until those gates pass.

Git semantics were checked against primary documentation: [status porcelain](https://git-scm.com/docs/git-status), [tree objects](https://git-scm.com/docs/git-ls-tree), [diff comparison](https://git-scm.com/docs/git-diff), [commit history](https://git-scm.com/docs/git-log) and [revision resolution](https://git-scm.com/docs/git-rev-parse).



## Prepared immutable-override and operation-safety corrections

TASK-72 now explicitly separates original immutable reads from Git-effective working/index semantics. RepoStatus reports replacement_refs and grafted; exact saved graph/tree/blob/object-size/batch/history/blame/snapshot/merge-base/commit-change reads use --no-replace-objects and GIT_GRAFT_FILE=/dev/null. Ordinary working/index comparisons, staging and authorized fetch retain installed-Git behavior. Native Repository panes persistently show Original snapshots / replacement count / legacy-graft presence at wide and narrow layouts. Source search coverage/metadata carry the same notice. Unsupported history writes are guarded, not silently reinterpreted. Remove/disable repository overrides explicitly before history mutations; no replacement/graft mutation was added.

Comparison::CommitChanges retains exact commit and ordered parent index. Parent zero is the first parent; root commits compare against the empty tree and are labelled starting content; alternative merge parents are explicit. Invalid parents fail clearly. This does not replace snapshot or merge-base branch semantics. Native mode/parent selection is foundation-owned.

Selected unstage patches now reverse the full canonical selected file before selecting the subset, then apply forward to the actual index. Neighboring staged additions stay as context and unselected removals remain absent; reverse-applying a partial HEAD-to-index patch was incorrect. Stale snapshot rejection and raw paths remain intact.

Operation-only progress/error rendering now preserves bounded terminal-safe multiline stdout and stderr, with known tokens and whole transport URLs redacted. This includes successful hook/remote output; original user blobs/diffs are not redacted or rewritten. Failed merges can show stdout CONFLICT diagnostics and resolution guidance.

A real installed-Git negative control in disposable /tmp/ghtui-ignored-worktree.c8s9yV demonstrated that ordinary worktree remove can delete ignored-only local files despite clean porcelain status. Only generated fixture bytes were removed. Backend removal now checks ignored files in the exact linked target at preview/execution and refuses until the user explicitly moves/removes them. No force removal or guessed consent was added.

Meaningful generated-Git contracts and actual both-input native fixtures for these changes are prepared. Coordinated focused/full execution is still pending; these corrections are not new passing evidence or task-completion claims until recorded runs succeed.


## Current real-project Linux checkpoint

The coordinated actual workspace checkpoint passed 258 code/provider tests and 102 real-binary PTY journeys, zero failed or ignored, plus cargo fmt --check and strict cargo clippy --all-targets -- -D warnings. Git contracts selected 41, identity contracts 15, Search contracts 14, and library contracts 72. Logs are target/build/root-all-targets-checkpoint.log, root-fmt-checkpoint.log and root-clippy-checkpoint.log. Native override, range staging/neighboring unstaging, ignored-file worktree refusal, conflict/reconciliation, raw-path distinct saved-file opening, actor/legend and existing local management journeys are included. These are Linux measurements, not macOS execution claims. No remote was created and no live GitHub mutation was tested.

Canonical ---/+++ path headers now split an optional tab delimiter outside C quotes only. Installed-Git new Japanese-space filenames under core.quotePath true/false preserve exact bytes, and checked staging passed14 code +6 PTY paths (artifact /tmp/ghtui-test-coverage-8Avh5H).

### Latest source preparations after that checkpoint

GitRelationship resolves original base/head SHAs, reports head-only ahead/base-only behind commits, all equally best common starting points and shallow loaded-history context. Unrelated histories retain an empty base list, while actual command failures stay errors. Native comparison forms and both-input tests remain separately required.

Path display distinguishes invalid bytes (hex escapes), genuine Unicode replacement characters and literal backslashes; ordinary Unicode stays readable. Physical filename fixtures use valid UTF-8 control-bearing names on macOS and invalid bytes on Linux. Always-running raw Git tree/blob/Search/PTY fixtures create actual blobs and NUL-delimited index names without requiring an invalid-UTF8 filesystem file; this preserves raw saved-path coverage on APFS. Apple's primary filesystem restriction is documented at https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html . New portability preparations need rerunning; no ignored/skipped required tests are added.



## Superseding tested Git/identity checkpoint

The later coordinated actual project checkpoint passed288 code/provider tests and114 actual keyboard/mouse PTY journeys with0 ignored/failures; whole fmt and strict all-target Clippy also passed. Git contracts43 and identity contracts17 were included. This supersedes the earlier258/102 checkpoint and source-preparation pending wording above; it remains Linux evidence, not macOS execution or complete market/UI parity.

Original immutable relationship counts, all best common ancestors, shallow/unrelated histories, root/merge-parent changes, raw-path portable Git-object fixtures and saved Search destinations now have actual passing code/native integration evidence. Exact non-UTF8 ref names remain raw byte identities; rendered labels cannot become named Git mutation argv. Native named-operation pickers retain such refs visibly disabled while immutable-SHA browsing/comparison remains available. Invalid-UTF8 GIT_REPLACE_REF_BASE is preserved via var_os and raw prefix enumeration rather than silently reverting to Git's default prefix. Generated packed refs and actual both-input PTY journeys verify Unicode-label siblings are not mistaken for raw refs and replacement/graft notices/guards remain visible.

Repository rendering now explicitly clears its pane before drawing prepared controls/content; a meaningful TestBackend negative control prepaints old content and asserts no residual glyphs survive between toolbar buttons, while action hit areas/theme remain intact. Shared bounded PNG/JPEG ImageDisplay is separate from stricter avatar policy and supports honest truecolor half-block rendering with low/no-color text fallback; artifact/identity contracts and native previews passed in the shared checkpoint. Every immutable/raw reference or avatar fallback limitation remains labelled rather than invented.

Managed corpus Task47 is a separate in-progress Search workflow. Nine focused real-Git/strict-gh contracts now pass and preserve exact account/path/owner identity, but native management and full post-integration gates are not yet complete. See doc-11 and TASK-47 for evidence. No remote was created; live remote mutation remains restricted to an explicitly configured disposable repository.


## Verified native managed repository corpus

Search exposes [M Repositories] (uppercase M). This is an opt-in account-verified worker workflow; opening it never clones, fetches, checks out or deletes a repository. Its explicit Map, Clone, Fetch, Unmap, Quarantine, Restore, Acknowledge and Recover-owned-orphan operations share keyboard and mouse form actions. Space or [Space Next] cycles typed choices; F5/F6 and their clickable controls navigate fields in narrow terminals. Preview presents the current principal, exact target and complete effect; wrapped rows scroll with keys or wheel. Confirmation rechecks fields and observed ownership/source state. Busy forms expose the same Esc Cancel semantics through keyboard and mouse, await actual worker cleanup and invalidate stale plans.

The absolute private corpus base is configuration-root/corpus; account roots below it are hashes of canonical host and positive numeric GitHub account identity. Two accounts may safely use the same base without adopting each other's paths. Fresh verified operator usernames are rendered as @login; internal user-id cache keys are not usernames, and an unverified/mismatched numeric account is labelled account #ID. This does not infer a local Git file author.

Map references an existing exact directory and never changes its checkout/index/HEAD. Working content remains explicitly mutable; saved content resolves to immutable objects. Staged/index corpus search is unavailable and labelled, not simulated. Clone is explicit shallow/full-history policy, fetches saved objects only and does not populate a checkout. GitHub cloning uses gh repo clone HOST/OWNER/NAME with --no-upstream and Git flags, retaining gh authentication/protocol without a separate auth implementation. Explicit local-bare cloning is restricted to file transport so global URL rewrites cannot turn a fixture/local path into network access.

Owned storage uses UID/private-mode checks, no-symlink bounded ownership JSON, internal ordinary .git verification and atomic no-clobber publication/moves. Clone staging envelopes explicitly request 0700 at creation; tempfile defaults are not assumed private. Unknown network size is labelled unknown. Progress is a prepared snapshot of elapsed time and observed logical bytes/entries; the cancellation budget is not advertised as a filesystem quota or physical disk estimate. Original object reads remain subject to the immutable-override disclosures above.

Intentional mappings and retained recoveries live in account-scoped saved_views namespaces, separate from derived Cache.records. Legacy record intents migrate atomically before cache clearing, with newer durable intent winning; x Clear cache preserves mappings/recoveries. A restart restores eligible mappings without automatically cloning or fetching. Unmap preserves a user checkout and all Git objects; searching an empty scope gives an honest unavailable-source explanation rather than fabricated successful coverage. Quarantine moves only a proven owned directory to recovery and retains tracked, untracked and ignored bytes. Restore refuses existing destinations. Partial outcomes require explicit inspection/acknowledgement and never auto-delete or retry.

Normal cancellation kills/reaps the runner's actual child and retains partial bytes inactive. Sudden SIGKILL is different: it cannot restore terminal state, and a live transfer inherits the ownership lock so a restarted app cannot race its files. After that generated child exits, bounded read-only orphan inventory reports exact owner/scope/path evidence. Explicit Recover-owned-orphan quarantines those bytes without adoption, activation, resume, deletion or automatic retry. Lost publication metadata likewise does not silently adopt a directory or invent its lost remote association. Ownership markers guard app operations; they are not a cryptographic defense against the same OS user forging files.

Actual Linux gate: env CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 node scripts/test-task.mjs selected_corpus passed 27 meaningful code/provider tests (6 library storage/runtime/progress, 5 controller, 16 generated-real-Git/strict-gh contracts) and 8 actual compiled-binary PTY journeys, zero ignored, with both input paths independently verified. Artifact: /tmp/ghtui-test-coverage-K8E4Nj. Logs: target/build/corpus-native-final-verified.log, corpus-native-clippy.log and corpus-native-fmt.log. Focused strict Clippy for lib/managed/controller/tui passed 7.76 seconds, and cargo fmt --all -- --check passed.

The eight journeys cover dirty Map/preview cancellation/search/restart/native cache-clear/Unmap, real no-checkout shallow Clone/Fetch/Quarantine/Restore with retained ignored bytes, actual normal-cancel PID cleanup/partial acknowledgement, and abrupt app-death/live-lock/restart-blocked/explicit orphan recovery, each by keyboard and mouse at wide/narrow layouts. First failed gates remain documented in TASK-47: clipped preview disclosure, a nonprivate tempfile envelope, fixture authentication-refresh synchronization and explicit stale-draft choice handling were corrected, not ignored or accepted. The latest task gate is not a fresh whole-project all-target checkpoint, macOS execution, organization inventory completion or full-product Done claim. Parent is running the remaining coordinated PR and global regression gates; task dependencies remain open.
