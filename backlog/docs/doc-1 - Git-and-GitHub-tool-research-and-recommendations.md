---
id: doc-1
title: Git and GitHub tool research and recommendations
type: guide
created_date: '2026-10-12 22:59'
updated_date: '2026-10-12 23:15'
---
# Git and GitHub tool research and recommendations

Research date: 2026-10-12. This is a fit assessment from primary documentation, not a head-to-head benchmark or a claim that one product is universally best.

## Product direction

Build ghtui in Rust and Ratatui around three strengths: a cross-repository PR inbox with complete review workflows; a clear map of repository history with commit, branch, tree, and diff navigation; fast discovery across selected repositories, code, PRs, and discussions; practical local Git management; and full GitHub Actions workflows. Dashboards summarize those same records and link back to them.

Supported product platforms: macOS and Linux only. Required external executables: gh and git. Authentication and GitHub transport belong to gh. Local Git semantics belong to git. The application owns presentation, orchestration, caching, and search.

## Interfaces worth learning from

| Tool | Documented strength | What ghtui should adopt | Fit and limitation |
| --- | --- | --- | --- |
| [gh-dash](https://github.com/dlvhdr/gh-dash) | Configurable PR/issue sections and actions | Separate Created by me, Assigned to me, and Needs my review queues; saved filters | Best reference for the inbox structure; its feature claims are not a verified parity contract |
| [ghui](https://github.com/kitlangton/ghui) | Cross-repo PR details, inline diff comments, review actions, Actions logs | A connected review flow with line selection and thread navigation | Closest focused PR reference; does not establish complete GitHub parity |
| [Lazygit](https://github.com/jesseduffield/lazygit) | Broad Git actions, hunks, history, comparison, custom renderers | Connected panels, file/hunk navigation, discoverable actions | Best broad Git workflow reference; ghtui should concentrate on understanding and review first |
| [GitUI](https://github.com/gitui-org/gitui) | Rust implementation, asynchronous Git work, contextual help | Responsive input while work runs; visible actions in every view | Strong performance architecture reference; its published benchmark is author-run and not evidence of current comparative speed |
| [Tig](https://github.com/jonas/tig) | Focused text interface for Git | Simple history-to-diff drilldown | Useful baseline for keeping repository browsing compact |
| [Serie](https://github.com/lusingander/serie) | Commit graph rendered with terminal image protocols | Clear ancestry lanes and graph-centered exploration | Graph inspiration; avoid requiring image-capable terminals |
| [GitKraken](https://gitkraken.com/features/commit-graph) | Interactive view of commits, refs, branches, and merges | Graph legend, relation highlighting, immediate commit details | Strong visual model reference; use native terminal cells rather than adopting a desktop runtime |
| [Fork](https://git-fork.com/) | Side-by-side diffs, file history/blame, tree at any commit | Revision-aware file browsing and persistent compare context | Strong repository-exploration reference; desktop application |
| [Sourcetree](https://www.sourcetreeapp.com/) | Beginner-oriented repository visualization | Plain-language labels and progressive disclosure | Useful usability reference; desktop application |
| [delta](https://dandavison.github.io/delta/features.html) | Syntax highlighting, split diffs, within-line changes, navigation | Native readable diffs and optional external viewing | Best diff presentation reference; ANSI output is not a reliable model for review line coordinates |
| [Difftastic](https://github.com/Wilfred/difftastic) | Syntax-based comparisons | Optional structural compare for refactoring | Viewing enhancement; documented memory/performance caveats and no applicable patch output |
| [Sourcegraph](https://sourcegraph.com/docs/code-search) | Cross-repository search, contexts, filters, saved searches, revision search | Search scope, previews, repeatable queries, result-type filters | Best search UX reference; its hosted/server model would add an account or service dependency |

These are recommendations inferred from feature fit. Borrow interaction patterns; check licenses before copying implementation code.

## Selected implementation tools

| Need | Choice | Reason |
| --- | --- | --- |
| Terminal interface | [Ratatui](https://ratatui.rs/concepts/application-patterns/) with Crossterm | Native Rust UI and supported application patterns |
| Background work | Tokio with bounded queues and cancellation | Network subprocesses and CPU-heavy work must not block input |
| GitHub access | [gh JSON commands](https://cli.github.com/manual/gh_search_prs) and [gh api](https://cli.github.com/manual/gh_api) | Delegate credentials and transport; structured output instead of terminal scraping |
| Git history/tree/diff | [git CLI](https://git-scm.com/docs/git-diff) | Established revision semantics and compatibility with installed Git |
| Exact and regex code search | [ripgrep's reusable Rust crates](https://github.com/BurntSushi/ripgrep/tree/master/crates): ignore, grep-regex, grep-searcher | Embed fast search without making rg another required executable |
| Fuzzy pickers | [nucleo](https://github.com/helix-editor/nucleo) | Appropriate for repository, file, ref, and command selection; not a substitute for content search |
| Durable metadata/cache | [SQLite through rusqlite](https://docs.rs/rusqlite/latest/rusqlite/) | Structured records, migrations, relationships, incremental checkpoints |
| Ranked global search | [Tantivy](https://github.com/quickwit-oss/tantivy) | Embedded Rust engine with BM25, phrase queries, facets, and incremental indexing |
| Avatar rendering | [ratatui-image](https://github.com/ratatui/ratatui-image) with initials fallback | Multiple terminal image protocols; keep username text visible and load asynchronously |
| Native syntax highlighting | [syntect](https://docs.rs/syntect/latest/syntect/) | A candidate for background highlighting; evaluate bounded cost during implementation |
| External diff enhancements | delta and difftastic, optional | Useful escape hatches; ordinary browsing/review must work without them |

SQLite is the source of truth for metadata. Tantivy is a rebuildable search index for both code and hydrated collaboration text. Use one ranked search engine, not competing SQLite FTS5 and Tantivy indexes. [SQLite FTS5](https://www.sqlite.org/fts5.html) is a viable smaller alternative, but the proposed choice is Tantivy for configurable code tokenization and a unified ranked-search product.

Start with a git subprocess adapter, batching object reads and caching immutable data. Consider git2 or gix only if measured process overhead becomes a bottleneck. Rust alone does not make subprocesses, network calls, or unbounded history loading fast.

## Two constraints that change the design

GitHub CLI explicitly says [gh search code uses the legacy search engine](https://cli.github.com/manual/gh_search_code); newer website features such as regex search are unavailable through that API. [GitHub's search documentation](https://docs.github.com/en/rest/search/search) also describes result caps, separate search rate limits, and incomplete results. The TUI must report partial coverage. A selected local corpus is the path to richer, predictable search.

The gh PR subcommands cover many actions but do not eliminate all endpoint knowledge. Inline review comments have a dedicated [REST interface](https://docs.github.com/en/rest/pulls/comments), and thread resolution, viewed-file state, pending reviews, and suggestions involve [GraphQL PR capabilities](https://docs.github.com/en/graphql/reference/pulls). All requests still execute through gh; no in-app OAuth flow, token store, direct GitHub API HTTP client, or GitHub SDK. Avatar bytes can use an isolated unauthenticated image-asset client without handling GitHub API authentication.

“Google grade” is an experience and evaluation target, not a promise of Google-scale indexing or semantic understanding. Deliver relevant ranked results, exact/regex modes, useful previews, clear scope, incremental updates, and repeatable quality tests. Natural-language/embedding search can be reconsidered after those work; it is not required for the first release.

## Expanded local Git and Actions requirements

Lazygit is the main reference for staging/unstaging, commit/amend, branch operations, stashes, worktrees and paused operations, with GitUI's responsiveness and contextual help as architectural guidance. Local mode detects a repository from the working directory and remains useful without a GitHub remote or login. It includes current-branch/working/staged comparisons, conflicts and recovery; see the Local Git workspace contract.

For Actions, use [gh workflow](https://cli.github.com/manual/gh_workflow_run), [gh run](https://cli.github.com/manual/gh_run_rerun), artifact downloads and gh api for supported approval/settings/cache/runner capabilities. A shared Actions viewer serves both PR checks and the dedicated Actions view; verify completeness against the separate Actions contract.

Identity and visual clarity are cross-cutting: verified usernames, async cached avatars with a visible text fallback, consistent semantic colors, and graph legends. Avatar images are enhancement data; core actions and usernames remain available in terminals without graphics.

GitHub CI/CD for ghtui itself is tracked as the final delivery task. Local code/provider/real-binary mouse-and-keyboard PTY tests are established at the beginning so implementation can be verified throughout.
