---
id: TASK-78
title: >-
  Synchronize form focus catalogue scrolling and overlay input with completed
  owned frames
status: In Progress
assignee:
  - '@graph_parallel'
created_date: '2026-10-15 03:47'
updated_date: '2026-10-15 03:48'
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
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Creation audit caught missing explicit Testing-steps/code-provider/real-binary headings, exact PTY command and DoD; corrected through CLI, original failed audit retained at target/build/root-inline-normal-readonly-selection-backlog-after-followup.log. New audit passes78tasks0errors. Root current Main full2141code/690PTY0failed/ignored does not resolve prior failures: same-parser replay metadata frames43Confirm→44Close→45closed; schema27EscClose→28Quit→clear; avatar52ID/FullName/Owner→57/58Owner withID gone. These prove target traversal, not yet queued-input timing; approved bounded200ms test-only Session receiver gate and actual write journal before repair, within original8s.
<!-- SECTION:NOTES:END -->
