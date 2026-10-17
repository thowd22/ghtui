---
id: doc-13
title: Sequential delivery reset and current-state audit
type: guide
created_date: '2026-10-16 23:29'
updated_date: '2026-10-17 04:53'
---
# Sequential delivery reset and current-state audit

Audit date: 2026-10-16 UTC. This records the user's instruction to stop all sub-agents, review the backlog, and deliver one feature at a time, with tests after each feature implementation. It supersedes the previous parallel implementation loop, not the product scope or acceptance requirements.

## Stop state

- All three active sub-agents are interrupted: github_parallel, graph_parallel, tree_parallel. None is authorized to restart automatically.
- Root stopped only the verified, owned verification scope `ghtui-verify-coherent-feedback-v307-20261016.scope` with SIGTERM at approximately 23:27 UTC. The original exec session 1330 closed with exit 143; systemd subsequently reported inactive/dead.
- The broad autonomous goal is paused. No new feature implementation, Cargo build, or PTY run was started for this audit.
- Existing dirty files, private candidate source, unfinished work, screenshots, and logs are preserved. No reset, cleanup, commit, push, or live GitHub mutation was performed.

The interrupted V307 run was still compiling its test inventory. It produced no completed application code-test or PTY result. Its report intentionally remains unfinished; its old VERIFYING status is not evidence that a verifier is still running.

Stop evidence:

| Artifact | SHA-256 |
| --- | --- |
| `target/build/root-coherent-feedback-v307-20261016-run.json` | `ab7fc216440078b3fb7698a5ab1b57348a232a14b686b0a49d05466883d3b8f2` |
| `target/build/root-coherent-feedback-v307-20261016-run-list-code.log` | `764cf612360d9d96767bb51c587d61b75bcc86f7d97f04f2ddcf416aac0fe60d` |
| Interrupted input `target/build/root-coherent-feedback-v306-20261016.json` | `498095303b8d925c19a54ebe6bd1936631ee22f9e00b785953d86c06e1236b02` |

## Backlog: recorded status is not product completion

At the 2026-10-16 audit, the CLI reported 78 tasks: 2 Done, 68 In Progress, 8 To Do. Only 38 of 418 acceptance boxes are checked. Neither count is an implementation percentage: much code exists without complete product acceptance, and some tasks still marked To Do already have partial code.

The two Done tasks are TASK-70 (dependency-source/build setup) and TASK-71 (initial Git metadata), not two complete user-facing features.

To Do: TASK-23 suggestions, TASK-27 isolated PR-branch editing/conflicts, TASK-63 activity inbox, TASK-64 parity audit, TASK-65 complete journeys/recovery, TASK-66 performance, TASK-68 curl-to-Bash installer/releases, TASK-69 GitHub CI/CD. CI/CD remains near the end.

The 68 In Progress entries describe historical unfinished work, not 68 active workers. This audit does not mass-reset their statuses or erase evidence. Review each task's actual implementation and dependencies before reactivation; operational work-in-progress is now zero while paused, and must be one when resumed.

## What exists versus what remains

This is an implementation/evidence inventory, not a new full parity certification.

| Area | Existing implementation / evidence | Remaining acceptance |
| --- | --- | --- |
| Foundation, input, identity, colors (TASK-1–11, 34–35) | Executable five-view shell; real-binary PTY framework; command adapters, cache, shared input and actor/theme code; many executed Linux checks | One current clean baseline, native macOS evidence, final shared interaction/identity audit |
| PR tracking and review (TASK-12–27) | Inbox/details/checks/diffs, comment/review workflows and portions of metadata, lifecycle, source checking and recovery | Complete cross-repository/org tracking; all metadata/project/milestone and lifecycle/merge paths; suggestions and isolated branch editing; complete mouse/keyboard recovery journeys |
| Repository graph/tree/diff (TASK-28–33) | Graph, compare, revision tree, diff and history paths | Full edge-case and beginner-UX acceptance, integration, both-platform verification |
| Local Git management (TASK-36–44) | Status, staging, commit/branch/remote/stash/worktree and operation/recovery implementations | All requested management paths and safety/recovery requirements verified together; full platform acceptance |
| Search (TASK-45–51) | Scoped exact/regex/ranked search, indexing, hydration and saved-context code | Org-scale discovery/completeness, relevance and performance evidence, complete cross-feature navigation |
| Actions (TASK-52–60) | Workflow/run/job/log/artifact browsing and portions of controls/settings/recovery | Full Actions feature parity and all mutation/permission/failure/restart paths; not complete |
| Metrics/dashboard/activity (TASK-61–63) | Metrics/dashboard implementations and some activity code | Complete formulas/data coverage, drilldowns, activity and UX acceptance |
| Release readiness (TASK-64–69) | Research/contracts and portions of UX/performance infrastructure | Full parity/journeys, measured performance, final UX pass, macOS/Linux installer/release verification, GitHub CI/CD |

Usernames/avatar fallback, semantic colors, and equivalent clickable/keyboard actions remain requirements throughout, not optional polish. Public GitHub/auth transport stays through gh; local semantics stay through installed git. Support remains macOS and Linux only.

## Reset-time verification: V304 failing

At the reset, the latest completed unfiltered attempt was V304, on a private coherent candidate, not a clean certification of the current dirty main checkout.

- Actual command: `node scripts/test-all.mjs` (unfiltered locked all-targets gate).
- Library result: **3302 passed, 7 failed, 0 ignored**, 946.30 seconds.
- Full native TUI suite: **not reached**, because the library stage failed.
- Separate provider contracts: 4 passed.
- Formatting: passed. Strict Clippy: failed with 14 diagnostics.
- Focused Issue cohort: 42 code + 2 native passed. Base cohort: 44 code passed, 25/26 native passed. A separate retained 58-native cohort passed 56 and failed 2.
- These focused successes do not replace the failed full gate or establish complete PR/Actions support.

Report: `target/build/root-coherent-bridges-v304-20261016.json`, SHA-256 `8316ec1e0eccbdeb856cad87bcb0e3b95e3d8f1f4b854066ae63affea0220285`.

Full log: `target/build/root-coherent-bridges-v304-20261016-all-targets-unfiltered.log`; its final result independently records 3302/7.

The seven library failures cover four state-model test oracles, two cross-family command/storage oracles, and one inline-comment owned-process polling failure. Separately, the focused native failures include late-save failure feedback and two label-query input/readiness journeys. Prepared corrections are **not** executed proof; further failures can appear after these are fixed. Do not describe this as “only seven bugs left.”

Native macOS, complete live-host API parity, performance budgets and full product UX remain unverified. Prior green checkpoints and screenshots must retain their original source/fixture provenance.

## Preserve unfinished work before any consolidation

**Do not run cargo clean or remove target: it contains unmerged source, not just disposable compiler output.** Last committed checkpoint at audit: `188eb560` (2026-10-15). The checkout has hundreds of changed/untracked entries; no broad overwrite or reset is safe.

Use the manifests and their exact before/output vectors when reviewing changes. Do not overlay whole private copies or assume later version numbers supersede every earlier branch.

| Preserved work | Location / state |
| --- | --- |
| Coherent interrupted input | `target/root-coherent-feedback-v306-private/source`; manifest `target/build/root-coherent-feedback-v306-20261016.json`; 1508 source files; compilation interrupted, corrections unverified |
| Cross-family test corrections | `target/build/provider-integration-v117-negative-20261016.json` and `provider-integration-v118-operation-20261016.json`; included in coherent input, not successfully executed afterward |
| Label-query readiness correction | `target/build/root-label-query-repair-v305-20261016.json`; included in coherent input, not successfully executed afterward |
| Late-save failure-pane correction | `target/build/parallel-tree-base-feedback-v314-sourceonly-20261016.json`; one production path plus code/native regression coverage included in coherent input; unverified |
| State-model test correction | `target/build/provider-state-model-v120-adjacent-20261016.json`; separate frozen test-only successor; not integrated into coherent input or executed |
| Inline owned-process polling correction | `target/build/root-inline-reap-v308-20261016.json`; separate test-only successor; not integrated or executed |
| Selected-repository issue-link UI | `target/build/graph-issue-links-v116-selected-ui-sourceonly-v1-20261016.json`; frozen handoff, full Root review/compilation/tests still outstanding |
| Current-base context display | `target/parallel-tree-base-context-v313.h161X6/source`; interrupted private work-in-progress, no final frozen handoff |
| Organization PR tracking | `target/build/provider-org-tracking-v121-material-plan-20261016.json`; design only, no implemented org workflow |

Main/source preservation reference: `target/build/root-conversation-v109-native-corrective-20261015-source-freeze.json` (1148 pinned main paths). This audit changes workflow documentation and notes, not product source.

Existing rendered offline Linux fixture screenshots are in `target/screenshots/verify-v303-private-rZ52mQ/`. They are not native macOS or live GitHub evidence.

## Sequential delivery policy

1. No sub-agents or background parallel feature branches. One coordinating worker, one current feature, one integrated candidate.
2. Read the selected task, dependency readiness, specification and actual evidence first. Record a bounded implementation plan in that task; do not reimplement already-working components merely because their acceptance status is open.
3. Finish that feature's implementation and meaningful code/provider plus real-binary PTY coverage. Preserve unfinished unrelated work; avoid new abstractions or scope expansion unless needed for the selected requirement.
4. **Then test that feature:** run its checked focused gate with nonzero selected tests; exercise fresh keyboard and mouse journeys, visible state and independent Git/storage/command effects, relevant resize/loading/failure/recovery states.
5. Fix failures in the same feature before switching. Targeted diagnostics are allowed; do not rerun the entire suite after each small edit.
6. Before declaring the task Done or moving to another feature, run the unfiltered `node scripts/test-all.mjs`, `cargo fmt --check`, and strict all-targets Clippy. Record exact source, commands, counts, results and artifacts. Do not remove assertions, skip failures, relax resource limits or substitute source inspection for execution.
7. Update acceptance boxes/status only when the actual requirements and required platforms have evidence. Keep the full original product scope and testing contract.

All existing development-host safeguards remain: Root-only verification under the shared lock; jobs=1, test/Rayon threads=2; explicit disk-backed working/temp paths; transient user scope CPU 200%, memory high/max 4/6 GiB, swap 512 MiB, tasks 256; verified active limits and low CPU/I/O priority. No unbounded fallback.

## First sequential workstream

**Consolidate and stabilize one existing baseline before adding another feature.** Review the preserved deltas, apply only necessary corrections to one candidate, resolve the known code/native/Clippy failures, and obtain one honest complete Linux regression result. Do not start another org-tracking design or metadata expansion in parallel.

TASK-1 is the only currently ready unfinished task; TASK-2 depends on it, and all other unfinished tasks are dependency-blocked. The shell/framework already exist. Their open requirements include native macOS verification and current global checks, not rebuilding the scaffold.

There is a planning tension to resolve explicitly: foundation acceptance requires macOS evidence, while hosted macOS CI is scheduled under near-end TASK-69. Do not silently waive macOS, mark foundation Done on Linux evidence, or pretend all downstream dependencies are complete. Reconcile the acceptance/verification sequence before formally activating downstream tasks; preserve the platform requirement and near-end CI/CD scope.

After the baseline/foundation requirements are reconciled, take the next dependency-ready user-facing task through implementation, focused keyboard/mouse verification, full regression, and acceptance **before** selecting the next. No completion percentage or reliable finish estimate is justified yet.

## Sequential work resumed — 2026-10-17

The user explicitly requested baseline stabilization followed by a continuous feature loop without agents. This supersedes the historical paused-work state above; sub-agents remain stopped. TASK-79 tracks the bounded Linux baseline stabilization, depending on completed TASK-70/71. Formatting and strict all-target Clippy passed on the prepared candidate. That candidate is now integrated into the main checkout with exact source before/after checks (225 modified paths, 360 added, current AGENTS.md preserved). Behavioral tests are running on the actual checkout, not the archived private copy. Full regression and macOS/product acceptance are not yet claimed. See TASK-79 and target/build/task79-checkout-integration-result.json for current evidence.

## Approved readiness sequence — 2026-10-17

The user explicitly selected: “Separate Linux readiness; keep macOS as a required release gate.” This resolves the planning tension identified above without changing platform scope or accepting cross-compilation as native evidence.

After TASK-79 actually passes the full Linux gate and is Done, downstream development prerequisites may use that verified Linux executable/framework checkpoint instead of waiting on TASK-1/2 final native macOS acceptance. Keep their original macOS acceptance items open and release-blocking; Linux completion is not macOS completion. Do not mark planned or still-running tests as passed. One worker, one feature and the existing focused/full testing requirements remain in force.

TASK-69 CI/CD remains near the end. Native macOS builds and real-binary keyboard/mouse PTY journeys must execute and satisfy the retained platform requirements before release promotion; local Linux and cross-compiled artifacts do not substitute. This approval does not waive product parity, performance, installer or release verification. Current baseline execution and exact evidence remain in TASK-79; its full gate is still running at this update.

## Verified Linux baseline — 2026-10-17

TASK-79 now establishes one verified integrated Linux development baseline in the actual main checkout. No agents were restarted. The reviewed integration preserved current instructions and unrelated work; interrupted private sources remain archived, not overlaid or deleted.

Actual unfiltered `node scripts/test-all.mjs` (`cargo test --locked --all-targets`) passed **4,008 code/provider tests** (3,314 library, 349 strict GitHub contracts, 345 others) plus **1,526 real-binary TUI tests**: **5,534 total, zero failed or ignored**. Library execution took 942.87 seconds and the TUI suite 1,950.87 seconds; these are test durations, not application latency benchmarks. Formatting and strict all-target Clippy have recorded exit-0 results on the same exact 1,508-file candidate.

The independent physical-input audit checked all 1,526 passing TUI names against completed sessions and expected input routes: 2,318 total sessions, 2,316 positive, two deliberate panic/cleanup sessions excluded. Positive sessions include 2,135 keyboard and 1,163 mouse records, with overlap. One invalid-configuration startup intentionally exits before interactive input. Every owned cleanup completed. Node gate-negative tests passed 3/3; dependency integrity checked 86 upstream sources/10,300 upstream files/108 patches/117 registry packages; all 79 backlog tasks passed the testing/dependency audit and doctor found no duplicate IDs, self-links or cycles.

The daemon restarted during verification. The already-running scope survived with its correct owner, lock, limits, working directory and zero source drift. The full test result and original format check recorded PASS/exit 0, but the old coordinator ended without persisting its final lint exit; its original wrapper JSON remains unfinished and is not relabelled PASS. A fresh check-only formatting/strict-lint run and separate audits passed on byte-identical source. The final evidence receipt explicitly joins those actual results and preserves the incomplete original metadata.

Evidence:

- Receipt: `target/build/task79-final-evidence-20261017.json`, status `LINUX_BASELINE_VERIFIED`; source-vector SHA-256 `7c353514f14ef0054b2b9a9e8931f96acab6f9e8d065e7361a2bf28a4a305a34`.
- Completed unfiltered test result: `target/build/task79-all-2026-10-17T04-00-46-867Z.json` and its `-all-targets.log`; wrapper-terminal caveat above applies.
- Final check-only quality: `target/build/task79-quality-check-2026-10-17T04-51-27-021Z.json` (PASS).
- Final input/integrity/backlog audits: `target/build/task79-audits-2026-10-17T04-51-58-047Z.json` (PASS).
- Physical journal: `target/test-tmp/task79-all-lHWnfO/journeys.jsonl`, SHA-256 `c2c26f08aea82294123c838578f467f9d0f3faf0ab37f72f5a44a101bfa9e731`.
- Actual offline screenshots: `target/screenshots/task79-all-5m7QpI/`, including `4083102-git-history-120x40-9207.png`; earlier passing diff/PR PNGs remain in `task79-native-6b7XTt/` and `task79-task-H0jBf8/`. SVG/ANSI/raw/provenance sidecars are retained. These are not live GitHub, native macOS or novice-user acceptance evidence.

The two real over-cap request bugs now stop immediately: remote payload and original repository-access guards recognize the newer distinct OutputLimit kind. A genuine red regression observed an illegal extra principal request before the guard fix; independent provider and both-input native cases now require exact request counts, hidden private content and preserved storage/Git/config. Other stabilization corrections retained meaningful assertions and fixed stale fixture/oracle/synchronization mistakes; required tests were not removed or ignored, and no runtime caps or host limits were raised.

The user's approved readiness split now applies: TASK-79 is the Linux executable/framework prerequisite for subsequent development. Original TASK-1/2 native macOS acceptance stays open and release-blocking; TASK-69 CI/CD stays near the end and must supply native build/PTY evidence before release. Next sequential work is TASK-3's remaining prepared/visible rendering acceptance, not another parallel metadata/org branch. Full PR/Actions/search/local-Git parity, performance, beginner UX, installer/release and native macOS requirements remain open. There is no whole-product completion claim.
