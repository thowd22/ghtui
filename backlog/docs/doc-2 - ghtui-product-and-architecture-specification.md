---
id: doc-2
title: ghtui product and architecture specification
type: specification
created_date: '2026-10-12 22:59'
updated_date: '2026-10-14 16:36'
---
# ghtui product and architecture specification

Status: proposed implementation baseline. Supported platforms: macOS and Linux only, including planned x86_64 and aarch64 release builds. Windows support is out of scope. User requirements: Rust, Ratatui, gh as an external dependency; full PR interactions; exceptionally clear Git exploration; fast search and dashboards across repositories the user chooses.

## Main views and connected navigation

| View | Starting point | Drilldown |
| --- | --- | --- |
| Inbox | Created by me, Assigned to me, Needs my review, selected-repo activity | PR summary, conversation, commits, files, checks, available actions |
| Repository | Auto-detected local repository or chosen repository and revision | Working/staged/branch changes, conflicts, Git operations, ancestry graph, comparisons, tree, history, blame |
| Actions | Selected repository set or PR/commit indicator | Workflows, runs, jobs, steps, logs, artifacts, dispatch, approvals, cache and settings |
| Search | Query plus visible repository set and source scope | Typed ranked results, context preview, exact revision or discussion |
| Dashboard | PR/review/check health for a selected repository set | Every metric opens its contributing records |

Global repository switcher, command palette, search, back navigation, contextual help, and a persistent host/repository/revision breadcrumb connect the views. Every visible button or action is clickable and keyboard-accessible, including dialogs, tabs, breadcrumbs, menus, palette results, and disabled-action explanations. Mouse and keyboard dispatch the same typed action and permission/confirmation logic. Support click-to-focus, wheel scrolling in the hovered panel, scrollbar interaction, tree expansion, file/hunk/graph selection, context actions, and drag selection for review ranges where relevant. Use layout-derived hit regions refreshed on every render/resize; overlays capture input so clicks never fall through. Support arrows, Enter, Escape, and Tab by default; offer Vim keys without requiring them. Narrow terminals show one focused panel with a clear back path. Action labels and loading states must remain understandable without memorizing hotkeys.

Show domain words with explanations: “commit — a saved version,” “branch — a name pointing to a commit,” “merge — joining histories,” and “common starting point” for merge base. Keep technical identifiers available for copying and expert use. Do not imply that graph lanes permanently identify branches: multiple refs can point to one commit.

## Runtime and boundaries

One application state owner consumes typed events and renders Ratatui frames. Workers perform gh/git calls, parsing, SQLite work, filesystem scans, highlighting, and indexing. Rendering uses prepared state and visible rows only. No network access, subprocess spawn, disk scan, or large parse runs in draw functions.

Use a single Rust package with clear modules initially: app/events, ui, domain, providers/gh, providers/git, storage, search, and config. Avoid a plugin platform or a large workspace until actual boundaries require it. Keep providers behind small interfaces so fixtures can drive the UI.

A process runner uses explicit executable and argv values, stdin for large bodies/JSON, deadlines, capped output or spill files, and cancellation with process cleanup. Reads have bounded concurrency and request coalescing. Interactive editor handoff restores the terminal before launch and re-enters it afterward. Repository content, titles, comments, and filenames are untrusted terminal text: sanitize escape sequences while preserving raw data for identifiers.

Writes serialize per resource, show pending/success/failure state, and re-fetch affected records. Never blindly retry a mutation after a timeout; reconcile its outcome first, then let the user resolve ambiguity. App launch and previews do not write to GitHub. Merge/close/delete and applying code changes display target and effect before confirmation in the product.

## GitHub provider

Prefer gh search/pr/repo/run/issue commands with --json and explicit host/repository/PR identity. For gaps, route REST/GraphQL requests through gh api. Pagination and field capability probes belong in this adapter. Avoid parsing colored CLI tables. Detect a missing or unsupported gh version and unauthenticated host with a useful setup instruction.

Resolve current login through gh and key durable records by host, current identity, repository ID/name, and resource ID; PR numbers alone are not globally unique. Recheck identity before any mutation after a host/account transition. Delegate gh auth login/switch to gh; do not extract or persist tokens.

Cross-repo queues have independent semantics: authored, assigned, and review-requested are separate queries. Deduplicate records while preserving queue membership. Selected repositories are an optional scope; default personal queues cover the current authenticated host rather than silently limiting to the current directory. Exclusions and saved repository sets are visible.

GitHub search caps cannot become invisible missing PRs. Split broad queries by repository/date when feasible, page results, and expose coverage and remaining work. Large organization inventories use paginated repository listing rather than assuming a single capped search finds everything visible to the account.

## Storage and refreshing

SQLite holds repository inventory, normalized PR/issue/thread/check records, local-repo mappings, preferences, saved views, drafts, and sync checkpoints. Store host/account provenance, source updated time, fetched time, completeness, and source URLs. Schema migrations must preserve saved views and drafts.

Render cache immediately, then refresh visible/high-priority data. Use explicit refresh, background polling with configurable intervals and jitter, and conditional requests where supported. GitHub does not provide a push feed through gh for arbitrary PR activity; do not claim realtime delivery.

Partition host/account caches and indexes. Clear or deactivate old identity results when identity changes, and provide explicit cache removal. Keep private data local. Cached access may outlive remote permission, so freshness/access errors are visible and a revoked repository stops syncing and is removed from active results. No offline mutations or queued auto-sends in the first release.

Remote-only repositories support PR review without cloning. Lightweight remote tree/content browsing is paginated and explicitly incomplete when the endpoint truncates. Full history and broad code search use an existing local checkout or an explicitly selected managed clone. Provide disk/progress estimates and clone/fetch controls; do not clone every repository in an organization on first launch.

## PR review correctness

Canonical diff rows retain old/new path, old/new line, hunk, side, and base/head SHAs. Inline comments anchor to that model and revalidate the PR head before submission. Highlighting, whitespace hiding, split view, and optional external renderers cannot redefine GitHub line coordinates.

Provide full conversation and event timeline, changed-file navigation, checks/logs, metadata edits, general comments, inline and multiline threads, pending review drafts, approve/request changes/comment, thread resolution, suggestions, and viewed-file state. Follow the separate PR action coverage document. Unavailable permissions or host features produce a reason, not an apparently successful action.

Fresh head SHA, checks, rules, and repository merge methods determine merge actions. Use gh's expected-head support for supported merges. Distinguish merged, auto-merge enabled, and queued results. Branch deletion is separate and explicit. GitHub merge-queue behavior is documented in [gh pr merge](https://cli.github.com/manual/gh_pr_merge).

Applying suggestions and remote edit operations use documented host capabilities where available. Otherwise use an explicit isolated-worktree commit/push workflow with equivalent observable behavior and permission checks. Browser access remains an escape hatch; it cannot count as native parity.

## Git exploration correctness

Use Git as the semantics authority. Resolve refs to commit IDs and preserve revision context. Read trees/blobs without checkout. Git NUL-delimited output handles paths containing spaces, tabs, newlines, or non-UTF-8 bytes; use lossy labels only for display while retaining raw paths.

The graph shows commit-parent relationships, refs, HEAD, merges, and loaded-history boundaries. Page or incrementally stream history while retaining graph continuity. Explain shallow clones, missing objects, detached HEAD, and unavailable refs.

Offer two explicitly labelled comparisons:
- “Compare saved versions”: compare the snapshots at A and B, equivalent to git diff A B.
- “Changes proposed by this branch”: compare merge-base(A,B) to B, equivalent to git diff A...B.

A...B is directional. Swapping sides changes the meaning. Commit-set ranges A..B are different from snapshot diffs and must not be conflated. This follows [Git's diff documentation](https://git-scm.com/docs/git-diff).

Show ahead/behind relative to the selected upstream, common ancestor, and merge parents. A merge commit can be compared to each parent. Distinguish working files, staged changes, and committed snapshots. Read-only exploration preserves the checkout; local Git management is also a required native workflow. Starting inside a repository or subdirectory opens a local workspace with commits, uncommitted/staged/branch diffs and conflicts, even without a GitHub remote or login. Staging, commits, branches, remotes, stashes, worktrees, sequencers, conflict resolution, recovery and bisect follow the Local Git workspace and operation contract.

Default diff rendering is native unified/split Ratatui with file tree, line numbers, syntax and within-line highlighting, context expansion, hunk movement, search, binary/LFS/submodule states, rename labels, and whitespace controls. Load files/hunks lazily. delta/difftastic integrations are optional viewers; structural output does not create review anchors.

## Search contract

One search bar has a visible mode: Ranked, Exact text, Regex, or GitHub. Fuzzy matching is for pickers. Search includes selected code/docs, repository names/descriptions, PR and issue titles/bodies, hydrated comments/review threads, and commit messages where opted in.

Ranked search uses Tantivy with a code-aware tokenizer that preserves identifiers and splits snake_case/camelCase alongside natural-language text. Boost exact identifiers, paths/titles, phrase matches, and scoped repository matches; tune boosts against a labeled corpus. Show why a result matched. Regex/exact scans use ripgrep crates for worktrees and a Git-object-backed path for chosen committed revisions.

A shared query model supports repo/org/set, type, path, language, author, label, state, dates, and revision where relevant. Parse/validate locally and translate only supported fields for each provider. Never silently weaken a regex, comment, or revision query into a different remote search.

Local indexes identify each document by host/account/repo/revision/path or collaboration resource ID. Default code indexing covers explicitly selected revisions, ordinarily default-branch HEAD. Worktree search sees uncommitted content and is labelled separately. History indexing is opt-in and bounded. Symbol-definition navigation is later work and is not implied by identifier matching.

Tantivy is derivative; rebuild after corruption/schema changes from selected Git objects and SQLite. Incremental checkpoints handle deleted/renamed files, rewritten refs, deleted comments, and identity changes. Index visibility advances with committed generations so UI coverage reflects what queries can actually return.

Remote search broadens discovery without cloning. Separate provider labels, group results by kind, deduplicate by identity, and combine rank lists without treating incomparable raw scores as equivalent. Show remote caps, incomplete results, loaded comments, indexed revision, last sync, and missing repositories. A search result always opens the corresponding revision/thread.

## Metrics and quality gates

Begin with actionable PR metrics: open count, waiting-on-my-review count, PR age, time since review requested, blocked checks, stale threads, first-review latency, and merge latency. State formulas, timezone, cohort, and missing-data behavior. Historical trends require retained events/backfill; current snapshots cannot reliably reconstruct all history. Empty data is not zero. Counts drill into records and distinguish partial scope.

Performance targets are budgets to validate, not measured results: first usable cached frame <=300 ms; p95 input-to-render <=50 ms; p95 render cost <=16 ms at 120x40; warmed ranked-search first page <=100 ms for a fixed 100k-document fixture; cached selected-file diff <=150 ms. Initial indexing and live GitHub latency have separate measurements. Record hardware, fixture shape, build profile, warm/cold cache, sample counts, percentiles, RSS, disk use, idle CPU, and subprocess count. Bound large files and history rather than promising fixed times for all repositories.

Every implementation task includes code/provider tests and real-binary PTY end-to-end scenarios for both mouse and keyboard paths, using the Code and TUI end-to-end testing contract. Bootstrap delivers the first PTY smoke test; the shared framework is the next prerequisite. Verification also uses real generated Git histories, captured/redacted gh fixtures, Ratatui buffer snapshots, subprocess failure tests, query correctness, and benchmark fixtures. Live mutation checks run only against an explicitly configured disposable repository. CI must never merge, post, or push into a user's normal repositories.

A novice evaluation asks a user to locate their PRs, explain a merge, compare two saved versions, browse an old file without checkout, and find a phrase in code/comments. Record success, time, wrong turns, and confusing labels; revise the UI before declaring it beginner-friendly.

## Delivery sequence

M0 establishes the runtime/providers/cache/interaction contracts. M1 delivers a useful personal inbox and read-only review. M2 completes PR collaboration and mutation workflows. M3 delivers repository graph/tree/compare/history. M4 delivers the full local Git workspace and operations. M5 delivers cross-repo search with indexing and quality evaluation. M6 delivers the full Actions workflow and settings coverage. M7 joins dashboards, notifications, parity audit, performance evidence, usability, and distribution.

The first usable release is M1. The full product target includes M2-M7; the early release does not redefine the requested scope. Implementation tasks remain To Do until actually built and verified. CI and release tests target macOS and Linux; no Windows-specific implementation or release work is required.

## Identity avatars and semantic color

Render a consistent actor identity everywhere: username/login, display name where useful, avatar, and bot/deleted/unknown status across PRs, comments, reviews, commits, history, blame, activity, metrics drilldowns, and Actions actors/approvers. Local Git author and committer are separate; show verified GitHub associations when available and otherwise preserve local name/email without inventing a username from a matching display name.

Avatar images load in bounded background work, cache by source/identity, and never delay text or input. Evaluate ratatui-image for supported terminal protocols; provide initials/placeholders in tmux/SSH/non-image terminals and deterministic test rendering. Fetch image assets with a dedicated unauthenticated asset client if gh cannot supply bytes; GitHub API and authentication still belong exclusively to gh. No additional image executable is a required dependency.

Use one semantic theme for changed files, conflicts, refs, HEAD, ancestors, diverged paths, selections and CI outcomes. Stable graph colors aid tracing without implying that a graph lane is a permanent branch identity. Dark/light, low-color and no-color modes retain symbols, names, and a graph legend. Syntax and within-line colors cannot obscure review selection or actor labels.

## GitHub Actions

Actions is a fifth main view. Full Actions operational and supported settings workflows follow the GitHub Actions coverage contract. PR/commit/repository indicators link to the exact SHA-related run. Use the same provider, actor, cache, search, mouse/keyboard, and PTY test infrastructure across Actions and PR checks; avoid two incompatible run viewers.

GitHub CI/CD for ghtui itself is a dedicated final delivery task. The code/provider/PTY framework is available locally from bootstrap, allowing every task to be tested while building before hosted automation is configured.

## Distribution and installation

The user chose one curl-to-Bash bootstrap script shared by macOS and Linux instead of native package installers. TASK-68 supplies verified versioned Intel/ARM release binaries plus a Bash 3.2-compatible, per-user, no-sudo script with target detection, SHA-256 verification before extraction/execution, safe atomic updates, explicit version pinning/rollback and non-destructive uninstall. gh and git remain external dependencies; installation does not implement authentication or auto-install dependencies. No Windows, .dmg/.pkg/.deb/.rpm, package-manager repository, background updater or new required service is in the delivery scope.

Follow [the curl bootstrap release contract](<doc-12 - macOS-and-Linux-curl-bootstrap-release-contract.md>) for research, trust boundaries, supported-target verification and task-specific code/provider plus actual script-installed binary keyboard/mouse PTY tests. TASK-69 remains near the end and publishes the script, exact tested binaries, checksums and supported build-provenance evidence only after release gates pass and a remote has been configured. Planning this change is not implementation, native macOS evidence, or a published install URL.
