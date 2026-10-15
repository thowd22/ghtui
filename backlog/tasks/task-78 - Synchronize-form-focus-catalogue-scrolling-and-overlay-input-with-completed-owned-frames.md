---
id: TASK-78
title: >-
  Synchronize form focus catalogue scrolling and overlay input with completed
  owned frames
status: In Progress
assignee:
  - '@graph_parallel'
created_date: '2026-10-15 03:47'
updated_date: '2026-10-15 05:45'
labels:
  - e2e
  - foundation
dependencies:
  - TASK-2
  - TASK-4
  - TASK-77
references:
  - target/build/root-native-selected-plan-completed-checks-whole-first.log
  - >-
    target/build/graph-parallel-task77-pr-row-navigation-v1-handoff-20261015.json
priority: high
type: bug
ordinal: 7069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Retained whole-suite failures show keyboard form confirmation closing a composer, catalogue Close exiting the application, avatar details scrolling away before pixel verification, and an Actions approval notice reaching an underlay-control refusal. Output-only recordings do not establish exact input timing or root cause. The latest navigation-integrated full Main run passes; that does not erase earlier negatives.

## Testing steps

1. Code/provider tests: Reuse existing CursorlessFrames/vt100 ownership/focus observers. Add meaningful partial/completed/resize/selection contracts and controlled actual receiver/paint/input regressions that fail on the original behavior. No fabricated success, second parser, timeout increase, repeated queued input or weakened assertion.
2. Real-binary TUI E2E scenario owned_input_frames_v1: Exercise fresh keyboard and mouse forms, catalogue detail scroll/Close and Actions overlay dismissal at120x40 and80x24. Preserve original8s/400ms budgets, strict38/13/22gh protocols as relevant, explicit publication targets, SQL/config/raw+semantic Git effects and full terminal restoration. Retain original and controlled failures even if whole runs pass. The actual Session delivery gate may withhold receiver bytes for bounded200ms to prove one outstanding input, never alter the product binary.
3. Run node scripts/test-task.mjs owned_input_frames_v1 and cargo test --test tui_e2e owned_input_frames_v1 -- --nocapture with nonzero meaningful code and actual keyboard/mouse PTY counts, then affected metadata/catalogue/avatar/Actions selectors, cargo test --all-targets, cargo fmt --check and cargo clippy --all-targets -- -D warnings. Record commands/counts/artifacts; native macOS remains unverified until executed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A controlled real-binary causal regression distinguishes completed owned input transitions from partial/stale frames for each reproduced failure; original negative evidence is retained.
- [ ] #2 Form confirmation and catalogue focus/scroll/Close use one acknowledged current target per input; keyboard and mouse retain equivalent original effects without queued extra actions.
- [ ] #3 Dismissing an Actions overlay cannot invoke or page an underlying control when its own complete current geometry has not been observed.
- [ ] #4 Meaningful code and task-specific both-input PTY tests pass with unchanged independent provider/storage/Git/config/TTY assertions and original time budgets.
- [ ] #5 Affected focused and full all-target/fmt/strict-Clippy/audit gates pass on a frozen source cohort, with actual counts/artifacts and platform limits recorded.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Actual checked code/provider and both-input real-binary PTY gates, frozen-source all-target/fmt/strict/audits and earlier failures are recorded; no planned or ignored coverage counts.
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Read-only diagnosis has started in new isolated target/parallel-input-audit.4L8YKq from exact frozen536+25+navigation10, preserving all old source leases. First replay output-only artifacts through existing parser without inferring input timing; then propose controlled receiver/paint acknowledgements and narrow shared-helper fixes before implementation. Root owns Main integration and whole gates.

Approved first causal implementation API: new test-only tests/support/receiver_delivery_gate_v1.rs provides a capped200ms arm-after-next-successful-PTY-write delivery hold and bounded actual write journal keyed by existing completed_cells_end. Optional Session field only; inactive originalreceiver/parser paths unchanged, no product hooks or second parser. Arm before existing metadata stale-tip submit; new owned_input_frames_v1 keyboard/mouse wrappers preserve exact38GH/noeffects/draft/Git/config/SQL/TTY and demand maxONE unacknowledged actual write. Pure bounds/owned-frame journal contracts. Capture old50ms focus helper genuine RED before observer repair; keep original negative and positive source frozen. Then design completed owned focus acknowledgment from actual rendered markers/styles with explicit ownership, preserving8s deadline. Avatar/detail and Actions notice controlled cases follow without blanket globalinput retries.

2026-10-15 remaining diagnostic slice moved to NEW target/parallel-owned-rest.DeCSP5 exact frozen focusV2 so first7 source gates remain immutable. Approved literal withholding extends SAME optional receiver gate: after actual contiguous ActionUnavailable title bytes, deliver genuine prefix through existing parser and hold only one original reader-chunk tail(max8192) plus bounded matching suffix, at most200ms; other chunks remain in SAME existing receiver. No fake frame, bytes, second parser or new product hooks. Setup refuses absent/noncontiguous marker; original raw has exact titleoffset39248/footer40992. Add bounded byte-preservation/partialmarker/deadline contracts and real22GH forkapproval both-input partial-footer causal RED before replacing only approval-close with existing wait_for_owned_completed_action. Avatar owned-detail scrolling requires one real observed viewport field/line shift before another current-region wheel/clickableCtrlPg control, no unobservableinitialfocusclick/Tab; both preserve original asset/publicprivate/Git/SQL/config/TTY and8s.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Creation audit caught missing explicit Testing-steps/code-provider/real-binary headings, exact PTY command and DoD; corrected through CLI, original failed audit retained at target/build/root-inline-normal-readonly-selection-backlog-after-followup.log. New audit passes78tasks0errors. Root current Main full2141code/690PTY0failed/ignored does not resolve prior failures: same-parser replay metadata frames43Confirm→44Close→45closed; schema27EscClose→28Quit→clear; avatar52ID/FullName/Owner→57/58Owner withID gone. These prove target traversal, not yet queued-input timing; approved bounded200ms test-only Session receiver gate and actual write journal before repair, within original8s.

2026-10-15 causal actual RED before focus repair: input-audit-gate-contracts-first.log3/3meaningful bounded gate/journal contracts green. input-audit-held-focus-native-red.log1pass(mouse)/1fail(keyboard)0ignored1.18s. Actual successful flush journal FOUR Tabs each completed_cells_end31880 during bounded200ms receiver hold; maxpending4 vs required1. SAME existing CursorlessFrames replay of genuine raw then proves Preview31951→Editor32072→Confirm32200→Close32331. Exact controlled artifact target/parallel-input-audit.4L8YKq/target/tui-artifacts/2276635-1792036281551049884 rawSHA6ebfd36fa24eed99d01ccb68770ba5251efac02181cbfb6e9153694389d86f96; failedlogSHAba20e54e5e8fdfcd6c65ca5e92db4dc27efe0a8e03ec0f8eb98e52dc7530a5ec. Original50ms focus helper untouched for RED; no product/parser change. Controlled positive mouse preserves complete38GH/noeffect/Git/draft/TTY scenario. All four original raw replays reproduce exact saved finalscreen contents; metadataConfirm→Close, schemaClose→Quit→empty, avatarID+FullName+Owner→Owneronly, approvaloverlay overpage3 all retained. Input causality proven only for controlled Tab case so far; otheroriginaltiming still notclaimed.

2026-10-15 FIRST bounded focus slice frozen, not full Task78 completion: target/build/graph-parallel-task78-owned-focus-v1-20261015.patch SHA910311ea5dbeaf87b21c4fabbc2f735916fb18a760c8f13951b47628d2ddc269 then exact equivalent one-line V2 Clippy addendum e0b102fe63b1f4bdf1acaee2d643859bff4c5ae72b8eb917f6ee66910cddbbc2. Final V2 handoff JSON c8abbacf403ad0b2ea6286c2b1d950ecf0222bff0740a0e6f0c4e131d01d9f60 includes7baseline/output,25+navigation10 prerequisite and14actual log hashes, original/controlled negatives and private artifact paths. V2 archive bb64d9f6b10e682b0c79e80d0e50819eb5ad7a7120a7f1e80fad03b05689fb77. Actual V2 full2104code/678PTY0failed/ignored89.48s, checked owned_input_frames_v1 10code/4PTY0ignored1.50s(q2GgTV4positive cleaned actual sessions), fmt0, strictClippy0(10.35s). Original impacted Base14/14+schema4/4green; all effect/Git/SQL/config/draft/publication/restart/TTY assertions retained. Separate earlier V1 full2104/678green94.61s and strict style101 preserved. BOTH independent controlled old-helper REDs retained: Base4Tabs/same31880 and catalogue4Tabs/same39330, keyboardfailed/mousepassed; failed Base stopped at29commands before fullpositive38oracle, so not claimed completed coverage. New explicit owned completed-CELLS focus anchors reject unrelated body/selection/avatar styling, partial/resize/nested/duplicates and require Some changed identity under same8s. Only Base picker/Confirm and schemaClose migrated; remaining avatar, Actions partialfooter and legacy generic callers remain OPEN/noAC/Done/nativeMac claim. Remaining work uses separate target/parallel-owned-rest.DeCSP5 from exact V2; first frozen7 unchanged.

2026-10-15 RECOVERY-ONLY successor frozen after host reboot. Seven-existing-test-path V3 delta relative already Main-admitted focusV2: target/build/graph-parallel-task78-owned-detail-overlay-v3-20261015.patch SHAd7b54307b3a2cba1b00893e5b43151e5c82a45ac5dc35266605c0ed4524c60a5; all7 immutable baseline/output hashes revalidated, all7 currentMain baselines exact, read-only git apply --check exit0. Final V3 handoff JSON and exact7-output archive in target/build preserve all log/source/RED artifact hashes and firstV1/V2 history. ACTUAL terminal before reboot: checked owned_input_frames_v1 18code/8PTY0failedignored4.73s with8positive cleaned sessions; test_framework29code/17PTY0failedignored4.58s, deliberatepanic cleanup records excluded; avatars34/34PTY3.62s; approvals10/10PTY7.15s; fmt0, strictClippy0(255s), dependency audit86sources/10300files/108patches/117registry, private77task snapshot audit0 only. Both new controlled original-helper REDs retained: approval0/2(3.91s), avatar0/2(9.38s); exact actual input journals prove premature notice Close and11unacknowledged detail inputs before correction. Interrupted private whole completed2112code/provider but682PTY has NO final summary/exit: NONPASS, not restarted or called fullgreen. Root now serializes all verification after reboot/prior8GiB swap100%. Review/artifact reads only since recovery; no Cargo/rustc/tests started, no stalePID signals. Main independent gates, foundation dependencies and nativeMac remain OPEN; no AC/DoD/Done claim.

Root postboot independent Main admission and checked gate genuinely terminal0: all7 successor baseline/output hashes exact after full patch review/application; hard-capped node scripts/test-task.mjs owned_input_frames_v1 PASS18code (12completed-navigation+6actualreceivercontracts)/8realbinaryPTY0failed0ignored, native5.14s. Log target/build/root-owned-input-v3-postboot-limited-first.log; target/test-tmp/ghtui-test-coverage-TdZ4cA/journeys.jsonl records actual keyboard+mouse and cleanup. Original GH publication/repository targets, storage/Git/config/avatar/TTY assertions retained; no product hook/parser/timeout budget increase. Broader legacy-callsite audit, current combined whole/fmt/strict, foundation/formal dependencies and nativeMac remain open. No AC/DoD/Done claim.

2026-10-15 Root independent composed Main gate GREEN: node scripts/test-all.mjs executed unfiltered cargo test --locked --all-targets under offline/locked fixed CPU200%,MemoryHigh4GiB/Max6GiB/Swap512MiB/Tasks256 scope, shared verification.lock, jobs1/testthreads2/Rayon2/disk target+TMP. ALL2233code/provider including318strictGH +720real-binaryPTY,0failed/ignored; native601.31s. Logs root-fullside-cache-dashboard-maintenance-{whole,fmt,clippy}-v3-first.log. fmt--check0 and strict alltargetClippy -Dwarnings0(59.99s); no lint waiver or vendor source edits. All66 source/config/docs hashes stayed frozen through these gates; all57 original earlier-negative source paths unchanged. Independent whole journal868records/866nonpanicking exited+cleanup-complete positive sessions759keyboard438mouse;2deliberatepanics excluded,0failedcleanup. Dependency audit86/10300/108/117 and backlog78valid0errors, diff--check0. Host hardcaps unchanged; recorded pidspeak48/max256 and no max/OOM events in live observations. Earlier2232code/165of718PTY negative and causal detached-maintenance control remain preserved; this pass is not proof every old failure had identical cause, nor proof of previous host-reboot cause. README refreshed only actual verified checkpoint/fullside/default-profile features and remaining PR/Mac/release scope. Current full does not complete broader task/dependencies/nativeMac acceptance; no AC/DoD/Done checked.
<!-- SECTION:NOTES:END -->
