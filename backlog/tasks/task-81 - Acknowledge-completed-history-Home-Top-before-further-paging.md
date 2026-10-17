---
id: TASK-81
title: Acknowledge completed history Home Top before further paging
status: Done
assignee:
  - '@root'
created_date: '2026-10-17 17:50'
updated_date: '2026-10-17 19:09'
labels:
  - review
  - e2e
  - stabilization
dependencies:
  - TASK-79
  - TASK-4
  - TASK-5
references:
  - tests/tui_e2e/review_thread_history_v1.rs
  - tests/support/receiver_delivery_gate_v1.rs
  - target/build/task7-all-2026-10-17T16-55-06-555Z.json
documentation:
  - backlog/docs/doc-4 - Code-and-TUI-end-to-end-testing-contract.md
  - backlog/docs/doc-13 - Sequential-delivery-reset-and-current-state-audit.md
priority: high
type: bug
ordinal: 10069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The completed Task7 unfiltered gate failed the existing review_threads_v1_history_mouse_historical_head_remains_distinct_from_fresh_current_head at tests/tui_e2e/review_thread_history_v1.rs:352. Its strict actual-input trace is direct evidence: Home Top at x47/y3 and the next PageDown at x35/y3 both recorded completed_cells_end24022, so maximum_unacknowledged was2 instead of the required1. The history image correctly distinguishes fresh Hdddd from historical Haaaa; this does not establish a product head-mixing defect. The changed-head branch currently calls history_ready for the already-visible current head after Home Top, permitting the old completed viewport to satisfy the wait. Do not weaken the strict one-input assertion or infer acknowledgement from elapsed time.

Full failure: target/build/task7-all-2026-10-17T16-55-06-555Z.json SHA5642c762f87700f2472188366ce62289d2228dfc016317b887aa3876150d6828, source1532/803fd74279b5452e1a0a57f82bd4031f4d3d184d7ddb370212b2d2c54cba055d. It completed4053code/provider and1548native passes,2native failures,0ignored; other failure is the separate Task80 saved-tree queue retry. History artifact: target/tui-artifacts/4054056-1792258908243411231; its prior already-exited session also saved an empty screen at4054056-1792258907262804451 during unwinding, which is not positive failure-view evidence. Both sessions completed owned cleanup but panicking rows cannot count as positive journeys. Existing Task78 covers earlier form/catalogue/overlay failures; this task is the newly observed atomic HomeTop paging regression, not blanket completion of Task78.

## Testing steps

1. Code/provider tests: review the actual completed-frame and receiver-delivery observer contracts plus protected history/current-head/original-target tests. Add or strengthen a meaningful controlled causal observer regression where necessary so an old already-matching head cannot acknowledge HomeTop. Preserve actual original journal/draft/account/PR identity and all budgets.
2. Real-binary TUI E2E (actual PTY) scenario review_threads_v1_history: fresh keyboard and mouse historical-head journeys open actual protected publication history across restart, inspect the original private body, move HomeTop, and wait for its new owned completed viewport before any paging. Retain wide/narrow/resize and bounded200ms genuine receiver-delivery control, exact maximum_unacknowledged1, strict gh counts, protected SQL/config/Git/index state and terminal/process cleanup. Never use a direct-handler or snapshot-only replacement.
3. Run checked nonzero node scripts/test-task.mjs review_threads_v1_history and relevant owned_input_frames_v1 observer regression, plus configuration/scope checks as impacted; then a fresh unfiltered node scripts/test-all.mjs, fmt, strict all-target Clippy and independent audits on the final integrated source after the separate Task80 blocker is fixed. Fix focused failures before moving to the next bug; do not run known-red full gates between the two isolated repairs. Neither task is Done until the shared final full gate passes. Root alone/shared verification lock/fixed resource and child-only Git safeguards; no timeout/cap increases, queued extra input, blind retry, ignored case or assertion weakening. Native macOS stays required before release.

## Explicit checked command mapping

Code/provider tests: `node scripts/test-task.mjs review_threads_v1_history` selects and executes meaningful code plus actual keyboard/mouse native journeys with positive nonzero coverage. Real-binary TUI E2E child command: `cargo test --test tui_e2e review_threads_v1_history -- --nocapture`. The `cargo test --all-targets` acceptance gate is executed as unfiltered locked `cargo test --locked --all-targets` by `node scripts/test-all.mjs`. These command spellings document the actual checked runner contract; on this development host launch them only through the Root shared-lock/fixed bounded scope and child-only foreground-Git environment, never a second or unbounded Cargo process. Preserve failed logs and every strict visible/effect/cleanup oracle.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A HomeTop input cannot be acknowledged by a previous already-matching historical/current-head viewport; further paging requires the actual new owned completed viewport or a proven already-at-target no-op path.
- [x] #2 Meaningful code/provider and controlled real-binary keyboard/mouse history tests pass with maximum_unacknowledged exactly1, original8second/200ms budgets and unchanged target/request/storage/Git/TTY assertions.
- [x] #3 Historical original head/body/account identity stays distinct from fresh current head and the viewer remains inspect-only with no resend, restore, write permission or reconciliation authority.
- [x] #4 Focused nonzero gates plus one fresh final all-target/fmt/strict-Clippy/independent-audit cohort pass before Done; original failures stay preserved and native macOS/release limits stay explicit.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [x] #1 Run meaningful checked task code/provider and actual keyboard/mouse PTY gates with nonzero counts, preserve the original failure trace and exact visible/command/storage/Git/TTY assertions, and record actual artifacts.
- [x] #2 Pass one fresh final integrated unfiltered all-target/fmt/strict-Clippy/independent-audit cohort after both known blockers are fixed; keep all acceptance checks and Done pending until that evidence exists.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Root sequential Task81 repair after completed Task7 fullFAIL5642c762 (4053code/provider+1548nativePASS,2nativeFAIL). TreeTask80 is separate and remainsToDo; Task7 implementation is frozen while verification blockers are repaired one at a time. 1. Use the retained actual-input trace to pin HomeTop -> nextPageDown both at completed_cells_end24022; review all history journey variants, the existing actual receiver gate and exact observer contracts. 2. Add only a fresh completed owned HomeTop viewport acknowledgement before any further paging. Preserve an already-at-top path without requiring a false text change. Reuse the existing bounded200ms real receiver-delivery control specifically at HomeTop to make the old acknowledgement assumption causally fail; retain maxunack1, exact8s bounds, requests, SQL/Git/config/head/body/private-state/TTY assertions. Do not change production history or auth behavior. 3. Run fresh checked review_threads_v1_history (currentlycompiled18code/16native) and owned_input_frames_v1 (18code/8native), format and strictClippy with exact positive per-name physical/cleanup evidence. Preserve every negative; no blanket test or input retries. Fix focused failures before moving to Task80. 4. After both isolated repairs have focused PASS, run one new unfiltered integrated full/fmt/strict/independent audit cohort before Task81/80/7 Done. Do not waste a full run while another known blocker is still unfixed; no criterion is checked from partial validation. No agents, fixed verified lock/resource/disk/foregroundGit policy; native macOS/release gates remain open.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented only the HomeTop test-body acknowledgement repair and controlled200ms receiver hold at that boundary; preserves the separate earlier trace and both max-unacknowledged1 assertions, unchanged8s bounds, exact original target/body/account SQL/Git/gh/TTY checks. Focused review_threads_v1_history and owned_input_frames_v1 RUNNING at target/build/task81-review_threads_v1_history-2026-10-17T17-54-55-754Z.json, original41972, fixed verified Root scope. No acceptance check or Done claim before focused and final joint full evidence.

Fresh Task81 focused PASS: target/build/task81-review_threads_v1_history-2026-10-17T17-54-55-754Z.json SHA68ffa080c38da088a88f280745da14f667a3ae4b6060f351a204cf1064bd9e22; source1532/e3315e047c4425ff15b6834b98e053ed8550845e39073fcd146293c92992446f. Original41972exit0; actual review_threads_v1_history18code/16native (30positive owned sessions/16names, journalZHU289 SHA1877dbff269ad1fc09ec45c64d303c47d04e5c03287cb6d9101040e06e6f07f6), owned_input_frames_v118code/8native (8positive sessions/8names, BJPdUS SHAa69d63c34d3bd08a1fd36171261f10c35c308878c73919b32d577935be481710). All selected counts nonzero,0failed/ignored/panics, independent named physical routes and allownedcleanup. fmt0/strictalltargetClippy0. The original same24022 HomeTop/PageDown negative remains preserved; new bounded200ms HomeTop control and fresh top-view expectation passed in both input modes without weakening strict maxunack1, effect assertions or deadlines. Implementation frozen while Root proceeds to separate Task80. All AC/DoD and Done remain pending one final clean integrated full/fmt/strict/audits after both repairs; native macOS/release not claimed.

After the isolated81 focusedPASS, Task80 was repaired sequentially and passed its current-source history18code/16native regression andfmt/strict in report14b4bbd6. Current final source1532/ce102c43 is frozen for fresh Task7 full18:17/original11030; no new full result yet. Observer18code/8native is also required in the final full inventory/audit. All AC/DoD/Done remain pending, old same24022 negative retained.

Final sequential Linux acceptance: HomeTop and the next PageDown originally shared completed byte24022. The observer-only repair independently asserts the Open/paging trace, applies the existing bounded200ms real-output hold again at HomeTop, and requires a new completed owned top detail before further paging. Both strict max-unacknowledged1 checks,8s budgets, original/current-head separation, inspect-only authority and exact gh/SQL/Git/config/TTY oracles remain. Fresh18code/16native history and final18code/8native observer regressions pass.

Same frozen1532/ce102c4374275cbf7967375cc1bdc3d6439145d176ca8031457c18cb5b36b582 passed3343library+711other=4054code/provider+1550native,5604total,0failed/ignored,37harnesses,fmt0/strictalltargetClippy0. Independent1550-name physical/owned-cleanup audit,Node3/3negative,dependency/Backlog/doctor pass. One actual integrated full execution is shared by these isolated sequential repairs; no repeated/global source relabelling. Evidence {"full":{"report":"/home/admin2/CODE/ghtui/target/build/task7-all-2026-10-17T18-17-10-007Z.json","sha256":"90fcae079bd4c5f65656bfd04856d6e46f470b51aa359356ca173e34d4002043"},"audits":{"report":"/home/admin2/CODE/ghtui/target/build/task7-audits-2026-10-17T19-08-07-156Z.json","sha256":"d0d1cad274154677c157014d53f1ded5ce88296f06480040bca22f2ed66a3f56"},"configuration":{"report":"/home/admin2/CODE/ghtui/target/build/task7-configuration-2026-10-17T18-14-52-319Z.json","sha256":"b6b8ea9dd1b92539550a8c29abd1d35ad6ed0afdee4569466d9794464042a657"},"treeAndHistory":{"report":"/home/admin2/CODE/ghtui/target/build/task80-original_tree_viewport-2026-10-17T18-06-10-216Z.json","sha256":"14b4bbd698146520deed393f2607dcb917aa14456113cedfcff7a5d9b5b1e8e6"},"configurationScreens":{"report":"/home/admin2/CODE/ghtui/target/build/task7-configuration-screenshots-2026-10-17T18-16-52-327Z.json","sha256":"54c79c1c15a93a581a4ebfcd4bcd71c56c140510bf8bb53e395b6622c637826c"},"treeScreens":{"report":"/home/admin2/CODE/ghtui/target/build/task80-tree-queue-screenshots-2026-10-17T18-14-45-329Z.json","sha256":"c0eb1c32c0b5def3b1cfe2e3e9b85b72a492dbf6860c056db97d338b6edf2010"}}. Preserved global negative /home/admin2/CODE/ghtui/target/build/task7-all-2026-10-17T16-55-06-555Z.json and targeted negative /home/admin2/CODE/ghtui/target/build/task80-original_tree_viewport-2026-10-17T18-00-29-922Z.json, plus capture interruption /home/admin2/CODE/ghtui/target/build/task7-capture-interruption-20261017.json. Focus selectors overlap and are not summed. Root alone/shared lock/fixed resource/disk/foreground Git safeguards; no remote/push/live write. User-approved Task79 Linux prerequisite does not waive native macOS Task1/2, complete downstream workflows, live-host parity, performance, beginner UX, curl bootstrap, near-endCI or release gates. Receipt /home/admin2/CODE/ghtui/target/build/task81-final-linux-evidence-20261017.json.

Local sequential configuration/repair checkpoint: receipts /home/admin2/CODE/ghtui/target/build/task7-final-linux-evidence-20261017.json SHA44663c57ac953a28c6b34085c517842d61d916a57a6ec52af1e00060b6ccdc62, /home/admin2/CODE/ghtui/target/build/task80-final-linux-evidence-20261017.json SHAd51214ee6fd68fd931eed9934d69e1504748b52bcff95fb5e4f3089d3465b57c, /home/admin2/CODE/ghtui/target/build/task81-final-linux-evidence-20261017.json SHA729c572536b9df3d6b4db3a06d7e32654a4d64b2b4e85c5ced3f998f82d1733d. Their ONE shared fresh5604/fmt/strict/audit execution is preserved at source1532/ce102c4374275cbf7967375cc1bdc3d6439145d176ca8031457c18cb5b36b582. The later README status synchronization changes exactly1prose paragraph (1added/1removed); all1531other code/test/build inputs are byte-identical. Current documentation-only successor1532/d269ca65ed21f6d296d0ed50a1bc5b969d0c1a7b51879e46147debee8e5efa35, README f07cb32cb33058a77830dc11f64c128fe53f90c2f3b7274eeff3e9c6e6711537 -> ae05b6644e11d90b424454c348011858e1c30a8e34241fb6940bdeb0094361bf. No new full execution or source-vector relabelling is claimed for this prose change. Explicit13-file checkpoint owns only the sequential Config/history/tree implementation/tests and associated Task7/80/81/Doc13/README updates. Four unrelated user logs remain untracked and unstaged. Native macOS/downstream product/live-host/performance/noviceUX/curl bootstrap/near-endCI/release remain open; no remote/push.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Verify fresh completed HomeTop acknowledgement before further history paging. Verified focused code/physical keyboard+mouse gates and one fresh5604-test integrated full/fmt/strict/audit cohort; all original negatives retained. Linux acceptance only, native macOS/release still required.
<!-- SECTION:FINAL_SUMMARY:END -->
