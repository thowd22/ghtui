# Performance budgets and benchmarks (TASK-66)

ghtui's performance targets come from the architecture specification
(`backlog/docs/doc-2`, "Performance targets"). They are budgets to validate, not promises
for every repository. This page records how they are measured, on which fixtures and
hardware, and which ceilings the automated benchmarks enforce.

## Budgets

| Metric | Budget (release build) | Measured by |
| --- | --- | --- |
| First usable cached frame (cold and warm start) | <= 300 ms | spawn -> first completed PTY frame showing `Recent commits` |
| Input-to-render, p95 | <= 50 ms | key or click written -> first new completed frame showing its effect |
| Render cost, p95 at 120x40 | <= 16 ms | the application's own frame timings (`GHTUI_RENDER_TIMINGS`) |
| Warmed search first page | <= 100 ms | Enter or `[Enter Search]` -> first settled results frame |
| Cached selected-file diff | <= 150 ms | reopening the diff that was just shown |

Initial indexing, cold loads of very large objects, and GitHub network latency are
measured separately and have their own ceilings. They are never folded into the budgets
above.

| Separate measurement | Ceiling |
| --- | --- |
| Initial ranked-search indexing of the working tree | 120 s |
| First open of the 5 MB diff, the 50,000-file tree and its path index | 20 s each |
| Peak RSS during a large-repository journey | 1 GiB |
| Idle CPU (2 s with no input) | 50 % of one CPU |
| Idle subprocesses | 0 live children and 0 Git starts |
| Memory kept for a refused 20 MB Actions log | 64 MiB of RSS growth |

## Build profile factor

The PTY journeys drive the unoptimized test binary (`cargo test` uses the `test` profile,
opt-level 0) on a shared development host, where other builds compete for CPU. Each budget is
therefore multiplied by a factor: 10 for debug builds and 1 for release builds. Set
`GHTUI_PERF_BUDGET_FACTOR` to override it. For example, `GHTUI_PERF_BUDGET_FACTOR=1` applies
the release budgets exactly. Every report records the factor it used.

## Fixtures

The fixture is generated once per test process with `git fast-import` (about 11 MB of
stream) and copied fresh for each journey:

- 20,000 linear commits (one per minute) over a working tree of 5,000 source files and one
  large file. History loads in bounded pages, so it never loads all 20,000 commits at once.
- A commit that adds a 50,000-file tree (`wide/d000-d499/f00-f99.txt`) and the next commit,
  which removes it. The History detail lists a bounded part of the changed files and the
  number of files it left out. The saved tree opens on it, and its Jump path index admits at
  most 50,000 rows (about 55,500 paths here). The index says `Partial saved tree; unseen
  paths unknown` and how many it omitted, and the benchmark searches for a path inside the
  admitted rows.
- At HEAD, a rewrite of a 2.5 MB, 40,000-line file in which every line changes: a diff of
  about 5 MB.
- An Actions job log of 20 MB, served by the strict offline fake gh after a 3 s delay. The
  command runner's 4 MiB output cap refuses the log, and the view says `GitHub response
  exceeded its bounded capture`, so memory never grows by the size of the log. A keypress
  made while gh holds the request is answered before the delay ends.

Search uses the working tree. Exact search runs a query that matches all 5,000 files, and
Ranked search runs a query for one unique identifier. The doc's 100k-document ranked fixture
is not used, because initial indexing of 100k documents does not fit the 2-minute budget
for the PTY suite in a debug build. Ranked first-page timing is warm: it is measured on the
second query, after the index for this corpus has been built.

## Known gaps

- **Warmed Ranked first page** (measured at 3.6 to 6.0 s in debug, against a 1 s debug
  ceiling). Every explicit Ranked query collects the selected corpus again and reserves a
  new canonical generation before it reuses the committed Tantivy index. A warm query
  therefore costs about as much as reading the corpus. The report marks the metric
  `known_gap` with `over_budget`, and a 15 s regression ceiling still fails the journey.
  Meeting the budget needs a follow-up: reuse the corpus when the selected checkouts are
  unchanged, for example by comparing the index fingerprint before collecting.

- **Cached diff reopen** is within its debug ceiling (about 0.9 s against 1.5 s). However,
  reopening the 5 MB commit diff costs the same as the first open: the comparison is computed
  again and not cached. A release build should be measured before this budget is treated as
  met.

## Running

```sh
cargo test --test tui_e2e performance -- --nocapture
```

This selects the `performance` journeys and their code tests. The four large-fixture
journeys hold a shared lock and run one at a time, so their timings never compete with each
other. Each journey writes a JSON report to `target/tui-artifacts/perf-v66/<test>.json` and
prints one line per metric. Every report holds the hardware (CPU model, logical CPUs,
memory, kernel, load average), the build profile, the budget factor, the fixture shape and
its generation time, and each metric's sample count, min, p50, p95 and max. It also holds
RSS, peak RSS, threads, idle CPU, live children, Git process counts (from
`GIT_TRACE2_EVENT`) and disk use for the cache, index and repository. A journey measures
everything first and judges the budgets at the end. A failing run still reports every
metric, and its message names each one that is over budget.

`GHTUI_RENDER_TIMINGS=<file>` is an opt-in diagnostic in the application. With it set, the
event loop times each frame it draws and writes `cols rows micros` lines when the application
exits. It keeps at most 100,000 samples. Without it, nothing is timed or written.

## Reference hardware

The ceilings are calibrated on the Linux development host: an AMD Ryzen 7 5700G (16 logical
CPUs) with 27 GiB of memory. Builds and tests run in a systemd scope with a 400 % CPU quota,
an 8 GiB memory cap, `nice 10` and idle I/O priority. Each run records its actual hardware
in its report. macOS measurement is deferred with the rest of native macOS verification.
