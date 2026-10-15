---
id: TASK-75
title: Create and follow file-level PR review comments
status: To Do
assignee: []
created_date: '2026-10-15 01:36'
updated_date: '2026-10-15 01:37'
labels:
  - review
  - e2e
milestone: m-2
dependencies:
  - TASK-19
  - TASK-21
references:
  - >-
    https://docs.github.com/en/rest/pulls/comments#create-a-review-comment-for-a-pull-request
documentation:
  - backlog/docs/doc-3 - PR-action-coverage-and-parity-contract.md
priority: high
type: feature
ordinal: 4069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
GitHub supports review comments on a whole changed file separately from line/range comments. The current InlineComment model requires positive line coordinates, so claiming full PR review parity from TASK-19 alone would omit that UI workflow. Preserve the original selected repository/PR/file identity without inventing line0, a hunk, or a comment ID.

## Testing steps

1. Code/provider tests: Meaningful provider and model tests with strict offline fake-gh: subject_type=file payload omits line/range coordinates; independently validate created and fetched author UID/node/parent/path/head/body and file-level subject. Test renames, absent/binary/large patches, capability refusal, changed head/file membership, cancellation, capture/deadline caps and uncertain outcomes.
2. Real-binary TUI E2E scenario file_comments: fresh keyboard and mouse journeys select a genuine changed file, compose/preview/cancel/confirm, resize/scroll/return, restart a saved draft, and inspect a reply. Independently assert completed visible state, exact single POST+GET effects, journal/draft/SQL state and unchanged local Git/config/TTY. Source loss must hide private content; ambiguous outcomes must never resend automatically. Live writes only to an explicitly configured disposable repository.
3. Run cargo test --test tui_e2e file_comments -- --nocapture and node scripts/test-task.mjs file_comments with nonzero executed code and PTY counts and positive actual keyboard/mouse journal entries. Then cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings. Record exact commands/counts and retained failure artifacts; no ignored tests or accepted-by-default snapshots.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Changed-file controls create an explicitly file-level comment through gh without fabricated line, range or thread identity; all actions have equivalent keyboard and clickable mouse semantics.
- [ ] #2 Preview and explicit Confirm retain original host/account/repository/PR/head/file provenance, revalidate current source and capability, persist drafts, and require exact independent creation/readback receipts before success.
- [ ] #3 File comments and their replies are visibly distinguished from line comments and PR conversation comments, with consistent actual usernames/avatar fallbacks, semantic labels and outdated/unavailable states.
- [ ] #4 Nonzero meaningful provider/code tests and task-specific real-binary mouse/keyboard PTY journeys pass with independent visible plus command/storage/Git effect assertions; full all-target/fmt/strict-Clippy gates and exact evidence are recorded.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run meaningful task-specific code/provider and real-binary keyboard/mouse PTY tests with nonzero actual selected counts; record exact commands, results and retained artifacts.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify clickable mouse/keyboard action parity and applicable focus, resize, scrolling, overlay, loading, disabled and failure states with independent visible plus command/storage/Git effects; native macOS status remains explicit.
<!-- DOD:END -->
