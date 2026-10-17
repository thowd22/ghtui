---
id: TASK-82
title: Fix Linux v1 usability blockers found in a real-account walkthrough
status: In Progress
assignee:
  - '@claude'
created_date: '2026-10-17 20:32'
updated_date: '2026-10-17 21:58'
labels: []
dependencies:
  - TASK-9
priority: high
type: bug
ordinal: 11069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A real-account walkthrough of the debug build (2026-10-17, Conduit checkout, gh logged in) found blockers that fixture-only PTY tests missed: (1) repository views never infer the checkout's GitHub origin, so Actions/Search/Dashboard demand a scope inside a GitHub checkout; (2) the legacy Actions run list paginated the whole run history (451 runs ≈5 MB) into a 4 MiB capture and failed on any busy repository; (3) opening a PR from the personal Inbox set a DefaultPersonal scope that permanently blocked Actions ('ID-bound repository access required'); (4) freshness labels showed raw Unix seconds. Deferred, tracked here: Inbox auto-load on first visit, PR body wrapping, Files-tab k shortcut, jargon in status lines.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Launching inside a checkout whose origin is on the configured host scopes Actions, Search and Dashboard to that repository without narrowing the Inbox or stealing local focus
- [x] #2 Actions run lists read only the newest page and show Loaded N of TOTAL instead of failing on large histories
- [x] #3 Opening an Inbox PR no longer blocks Actions, Search or Dashboard
- [x] #4 Freshness labels show UTC calendar time
- [x] #5 Focused unit and PTY gates pass, followed by one full test-all, fmt and strict clippy
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented (2026-10-17, Claude):
- src/origin_repository.rs: detect owner/name from origin (or the only remote) on the configured host; main.rs uses it after --repo and the active set for Actions, Dashboard and Search remote scope. Inbox is never narrowed, and local focus is kept.
- Provider run_catalog() and runs() read the newest page only (no --paginate); coverage shows 'Loaded N of TOTAL'. 28 fake-gh fixtures updated to single-page form.
- remote_scope_gate_v1: a DefaultPersonal scope no longer blocks Actions/Search/Dashboard legacy routes; Inbox refresh is still guarded.
- Freshness labels use UTC calendar time.
- Forms: Ctrl-S = Submit/Confirm (same action as the button; Ctrl-Enter is unreliable in terminals); hint updated.
- PR detail: plain body/comment rows wrap to the pane; interactive/actor rows stay one line; visible-row count reflects rows that fit.
Gates: origin_default 4 code/4 PTY; local_confirm_shortcut 1/2; pr_body_wrap 1/2; tui_e2e 'actions' 447 PTY pass; lib 3355 pass; fmt and strict clippy clean.
Verification environment note: TMPDIR must be outside the repository, or git discovery tests find ghtui's own .git.
Deferred: Inbox auto-load on first visit (about 53 PTY files assert the manual-refresh sequence); status-line jargon.

Batch verification 2026-10-17: lib 3357 (one SQLite 'database is locked' flake under 6 threads in pr_lifecycle_v68 receipts test, which passed 3/3 alone); integration+PTY 2271 passed, 0 failed (1560 PTY). Later fixes (offline label order, wrap limited to Summary/Conversation, plain-language status/coverage strings) were verified by targeted reruns of every affected test, not another full run. fmt and strict clippy clean. Inbox auto-load is intentionally not done: ~53 PTY files would change to save one keypress.
<!-- SECTION:NOTES:END -->
