---
id: TASK-73
title: Refresh bundled SQLite to include the upstream WAL-reset integrity fix
status: To Do
assignee: []
created_date: '2026-10-14 21:02'
updated_date: '2026-10-14 21:51'
labels: []
dependencies:
  - TASK-70
  - TASK-2
  - TASK-8
references:
  - 'https://sqlite.org/wal.html#walreset'
documentation:
  - vendor/README.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: high
type: bug
ordinal: 2069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The current bundled SQLite is 3.50.2 in vendor/upstream/rusqlite/libsqlite3-sys/sqlite3/sqlite3.c and Cargo selects bundled rusqlite. Official SQLite documentation reports a WAL-reset corruption fix in 3.51.3 and a compatible 3.50.7 backport. Legacy caches and supported verification use WAL and multiple connections, so release integrity must include this upstream correction. This is separate from TASK-8 optional-sidecar disappearance/owned-file diagnostics; do not conflate the bugs or claim a runtime corruption was observed.

## Testing steps

1. Code/provider tests: verify the actual linked SQLite version/source and upstream fixed implementation; use real concurrent WAL writes/checkpoints and restart to assert exact committed private drafts/records and integrity_check, plus preserved account isolation and unsafe alias refusal.
2. Real-binary TUI E2E scenario `sqlite_dependency`: use the exact compiled/release binary with actual WAL cache, perform refresh/composer/restart through fresh keyboard and mouse fixtures and both 120x40/80x24, independently assert committed SQL/draft bytes, strict gh targets, unchanged Git/config and terminal restoration; no live mutation.
3. Run cargo test --test tui_e2e sqlite_dependency -- --nocapture and node scripts/test-task.mjs sqlite_dependency with nonzero executed code/PTY and completed both-input evidence, impacted cache_storage/composer/corpus, cargo test --all-targets, cargo fmt --check and cargo clippy --all-targets -- -D warnings. Record actual counts, upstream pins/licenses/checksums and artifacts; no placeholder/ignored coverage.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The actual bundled runtime includes the official WAL-reset fix (compatible 3.50.7 or a verified newer fixed upstream), with explicit origin/pin/license/checksum provenance and no guessed patch or falsely changed version label.
- [ ] #2 Existing database schema, committed private drafts/records, numeric-account isolation, owned-file permissions and symlink/hardlink/device refusals remain correct across real WAL checkpoint/restart journeys.
- [ ] #3 Nonzero meaningful code/provider and real-binary keyboard/mouse PTY tests execute and pass; all visible actions preserve shared semantics and independent command/storage/Git/TTY effects.
- [ ] #4 Offline locked all-target tests, formatting, strict Clippy, dependency integrity audit and clean-copy dependency build pass on supported targets; native macOS evidence is explicit rather than silently skipped.
- [ ] #5 Release gating depends on this correction and documentation clearly distinguishes upstream WAL-reset risk from TASK-8 sidecar lifecycle repairs.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Meaningful code/provider tests execute with a nonzero selected count and pass; no ignored or placeholder assertions.
- [ ] #2 Task-specific real-binary keyboard and mouse PTY journeys execute and verify visible state plus independent command/storage/Git effects, resizing, focus, failure/cancellation and terminal restoration.
- [ ] #3 cargo test --all-targets, cargo fmt --check and cargo clippy --all-targets -- -D warnings pass; actual commands, test counts and artifacts are recorded.
- [ ] #4 Upstream provenance/license/checksum audit and supported macOS/Linux verification are recorded honestly; unverified acceptance criteria remain unchecked.
<!-- DOD:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Root read-only backlog audit initially failed four format/DoD omissions on newlycreatedTASK73, retained root-metadata-cache-tree-remote-actions-backlog-audit.log. Corrected onlyCLIdescription testing-step labels/explicitnativecommand and added requireduncheckedDoD items; noimplementation/test/dependency/Done claim.
<!-- SECTION:NOTES:END -->
