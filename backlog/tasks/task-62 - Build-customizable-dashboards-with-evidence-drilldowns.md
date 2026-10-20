---
id: TASK-62
title: Build customizable dashboards with evidence drilldowns
status: In Progress
assignee:
  - '@root'
created_date: '2026-10-12 23:13'
updated_date: '2026-10-20 14:09'
labels:
  - delivery
  - e2e
milestone: m-5
dependencies:
  - TASK-61
  - TASK-51
  - TASK-35
  - TASK-34
documentation:
  - backlog/docs/doc-2 - ghtui-product-and-architecture-specification.md
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
priority: medium
type: feature
ordinal: 62
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The user wants an easy overview across the repos they care about. Dashboard widgets should surface review/CI bottlenecks and lead to the exact search, PR or run rather than disconnected numbers.

## Testing steps

1. Code/provider tests: Test widget aggregation/sorting, filters, drilldown identities, saved layouts and empty/partial states.
2. Real-binary TUI E2E scenario `dashboards`: Build/save/reopen a dashboard through keys/clicks, inspect a failing-run or review-age widget, and assert its contributing view matches the displayed metric. Run fresh fixtures for keyboard and mouse paths and verify equivalent outcomes at 120x40 and 80x24 where applicable.
3. Run `cargo test --test tui_e2e dashboards -- --nocapture` and verify a nonzero selected test count; run relevant code/provider tests and `cargo test --all-targets`. Record actual results and failure artifacts before completion. Use the shared framework established by the bootstrap/framework prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Saved dashboards contain review queues, stale PRs, unresolved threads, failing Actions and defined metric trends.
- [x] #2 Scope/window/layout preferences persist and every displayed action/metric is clickable and keyboard reachable.
- [x] #3 Actor widgets, colors and partial-data indicators agree with underlying records.
- [ ] #4 Automated code/provider tests pass for the task-specific scenarios in Testing steps.
- [x] #5 Real-binary PTY E2E tests pass for the specified keyboard and mouse journeys; all exposed actions are clickable and keyboard accessible with equivalent effects.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Run the task-specific code/provider tests and PTY scenario with nonzero selected tests; save command results and artifacts in task notes.
- [ ] #2 Run cargo test --all-targets, cargo fmt --check, and cargo clippy --all-targets -- -D warnings; fix failures before marking Done.
- [ ] #3 Verify mouse/keyboard parity and applicable resize/loading/error states using the shared framework; no placeholder or ignored required E2E tests.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Root fourth parallel implementation lane2026-10-15 from verified96b7f62c: reuse existing prepared MetricsWorkspace, same card/contributor sources, typed actions and Dashboard worker. Implement bounded account-scoped default dashboard profile (layout order/visibility, explicit repository selection, UTC window preference), editable through keyboard and clickable controls, persist via existing SQLite saved_views without schema/auth/queue changes. Restore is explicit navigation intent only: validate profile shape/bounds/principal and saved subset against current declared/excluded repos, then existing fresh Dashboard provider route; never restore metrics/actors/permission from SQL. Layout operations reproject only8original prepared card actions with exact inspect/Open mapping and full coverage labels. Keep local save distinguished from refreshed evidence; stale source/SQL/profile errors explained with no hidden fallback/scope expansion. Add meaningful model/storage/mapping/guard tests and dashboards real-binary keyboard/mouse save/edit/restore/restart/window/scope/drilldown/resize/failure journeys with independent SQL/Git/strictgh/config/TTY effects. Focused checked dashboards, affected metrics, whole all-target/fmt/strict/audits before completion. Named multiple profiles, unresolved-thread/trend widgets and formal61/51/35/34/nativeMac dependencies remain required follow-ons, not implicitly done by initial layout slice.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-15 Root active implementation update after96b7: added bounded account-scoped default-profile model and real SQLite preservation/isolation/refusal tests, typed8-card layout projection with canonical inspect/Open mapping, mouse/key layout controls, and same Dashboard worker explicit save/restore/default routes. Actual focused cargo test --offline --locked --lib dashboards:8passed0failed0ignored0.03s (root-dashboards-model-third.log). Earlier compile API mistakes and initial2/4 SQL fixture-directory permission failures retained in first logs; corrected fixtures use explicit0700. Task-specific checked dashboards real-binary journeys are RUNNING, not verified; no completion/AC claim, full/fmt/strict still required. Task78 first7 test-only owned-focus V2 paths independently hash-verified and applied for combined integration, no production hook and no blanket claim for remaining avatar/approval callers.

Actual checked dashboards negatives retained: first8code/0of2PTY (restart fixture incorrectly rewrote step0 while counter persisted39; corrected one immutable86-step script across both actual processes). Second8code/3of6PTY: genuine mouse save/restore/restart/drilldown and both malformed-profile recovery passed; outside-scope pair expected unpainted tail of clipped status, and keyboard Open ran before completed fresh Refresh publication. Corrected product scope-refusal wording to front-load actionable reason; strengthened keyboard observer to require NEW completed owned Dashboard Refresh frame and no Cancel before next key. No sleep, retry, assertion weakening or fixture acceptance. Third checked gate now running.

Third actual checked dashboards gate GREEN8code/6PTY0failed0ignored,0.04s/3.98s; artifacts target/test-tmp/ghtui-test-coverage-tECSry include actual both-input cleanup evidence. Added four further real save-specific final-UID-change/live-cancellation cases (independent unchanged profile/draft/new-UID empty/child reaping/raw Git); expanded gate now running after cargo fmt --all. Broader dashboard widgets/named profiles/dependencies still open; no AC/DoD checked.

Frozen V1 expanded actual gates: checked8code/8of10PTY; whole2159code GREEN and701of704PTY (101.05s),0ignored. New save account-change pair passed. Both live-save Cancel tests first clicked disabled Layout, which genuinely opens Action unavailable; the next Escape correctly dismissed that Notice instead of canceling. Actual raw artifact3764816-1792037912289590730 confirms owned title/Close before stale test input; V2 now explicitly proves/closes complete owned Notice, proves completed busy frame, then independently cancels. Whole third negative is actual Corpus clone Preview private-journal rejection4037718-1792037985649272795; Task8 owner notified, no unrelated assertion. V1 strict negative one needless borrow; V2 removes it and adds last cancellation/deadline check after local Cache/guard checks before profile write. Original V1 files/logs/failures preserved; no whole-green claim.

2026-10-15 incident: user reported host crash. Read-only uptime/boot journal confirms reboot04:27UTC; previous journal records orderly systemctl reboot request04:25:41, not a demonstrated kernel panic. sysstat04:10/04:20 records all8GiB swap in use, availableRAM7.7GB→4.2GB and load15.57 on16CPUs; exact freeze/root cause unproven. Root V2 dashboards/focus/strict sessions44292/43445/24180 all terminal143SIGTERM, not passes or test failures; no automatic gate restart. All18 current source-freeze files independently hash-match after reboot, HEAD96b7f62c retained. All three agent turns interrupted before reinitialization; no Cargo/Rust/PTY verification jobs observed live. Heavy fan-out stopped. Future build/test verification must be serialized with conservative compiler/test concurrency and disk-backed temporary/build paths, while lightweight parallel code/review may resume separately. No completion claim; Root V1 full2159code/701of704PTY and earlier actual8code/6PTY dashboard gate retained separately from interrupted V2.

Root postboot V3 checked dashboard gate is genuinely terminal0: hard-capped node scripts/test-task.mjs dashboards PASS8meaningfulcode/10realbinaryPTY0failed0ignored, code0.04s/native8.59s; target/build/root-dashboards-v3-postboot-limited-first.log, target/test-tmp/ghtui-test-coverage-F8F7eN/journeys.jsonl. Includes both-input immutable86GET save/restart/restore/scope/window/drilldown plus malformed/outside-scope recovery, finalUID refusal, and genuine heldGH cancel with childreaping/unchangedSQL/rawGit/config/TTY. Cancel tests now assert actual Dashboard read cancelled, distinct from genuine auth/source Dashboard values unavailable; complete Action unavailable Notice is explicitly closed before actual busy Cancel. Earlier8/8of10 and whole701of704 negatives retained. Current later cache13/inputV3 integration requires fresh whole/fmt/strict; no product/task/dependency/nativeMac completion.

2026-10-15 Root independent composed Main gate GREEN: node scripts/test-all.mjs executed unfiltered cargo test --locked --all-targets under offline/locked fixed CPU200%,MemoryHigh4GiB/Max6GiB/Swap512MiB/Tasks256 scope, shared verification.lock, jobs1/testthreads2/Rayon2/disk target+TMP. ALL2233code/provider including318strictGH +720real-binaryPTY,0failed/ignored; native601.31s. Logs root-fullside-cache-dashboard-maintenance-{whole,fmt,clippy}-v3-first.log. fmt--check0 and strict alltargetClippy -Dwarnings0(59.99s); no lint waiver or vendor source edits. All66 source/config/docs hashes stayed frozen through these gates; all57 original earlier-negative source paths unchanged. Independent whole journal868records/866nonpanicking exited+cleanup-complete positive sessions759keyboard438mouse;2deliberatepanics excluded,0failedcleanup. Dependency audit86/10300/108/117 and backlog78valid0errors, diff--check0. Host hardcaps unchanged; recorded pidspeak48/max256 and no max/OOM events in live observations. Earlier2232code/165of718PTY negative and causal detached-maintenance control remain preserved; this pass is not proof every old failure had identical cause, nor proof of previous host-reboot cause. README refreshed only actual verified checkpoint/fullside/default-profile features and remaining PR/Mac/release scope. Current full does not complete broader task/dependencies/nativeMac acceptance; no AC/DoD/Done checked.

2026-10-20 audit at 74360e48 (3/5 ACs met): Single default dashboard with saved layout/scope/window and exact drill-downs works, with keyboard/mouse PTY coverage. Missing: unresolved-thread and trend widgets (blocked on TASK-61 history). Remaining: AC1: the dashboard holds only the 8 metric cards (open PRs, needs my review, age, review wait, first review, merge latency, checks, workflows). There is no unresolved-threads widget (src/metrics/threads.rs exists but no card uses it) and no metric-trend widget, because historical metrics are not built (TASK-61 AC2). Only one default profile, not named dashboards. AC4: tests cover layout, profile, drill-down identity and empty/partial states, but not the missing widgets.
<!-- SECTION:NOTES:END -->
