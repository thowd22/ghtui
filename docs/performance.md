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

By default the PTY journeys drive the unoptimized test binary (`cargo test` uses the `test`
profile, opt-level 0) on a shared development host, where other builds compete for CPU. Each
budget is therefore multiplied by a factor: 10 for debug builds and 1 for release builds. Set
`GHTUI_PERF_BUDGET_FACTOR` to override it. For example, `GHTUI_PERF_BUDGET_FACTOR=1` applies
the release budgets exactly. Every report records the factor it used.

`GHTUI_PERF_BINARY=<path>` makes the journeys launch that application instead, normally one
built with `cargo build --release` (thin LTO, one codegen unit, as shipped). The factor then
defaults to 1, and each report's `build_profile` names the binary. The test harness itself
stays a debug build. Its vt100 parsing is part of every input-to-render sample, so release
input-to-render figures are slightly pessimistic.

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

### Warm Ranked reuse (TASK-106)

The warmed Ranked first page is held to the warmed search budget (`search_ranked_warm_first_page`,
100 ms × the build factor). It was measured at 3.6 to 6.0 s in debug before this change and
about 0.3 s after it, against the 1 s debug ceiling. The fixture's working files are dated an
hour back, as for a checkout nobody is editing (see the racy rule below). Before collecting, a Ranked query fingerprints its inputs without
reading file contents: for each selected checkout, HEAD, the index, config and exclude file
stats, replacement refs and grafts, and the path, size, mtime, ctime and inode of every file
the ignore-aware walk visits (a saved version uses its resolved commit). The fingerprint is
committed with the source generation. When the next query's fingerprint matches and every
published document is still a canonical member, the query searches that committed generation
directly: it neither collects the corpus nor reserves a new generation. The canonical
documents decoded by the last open stay in memory while a SQLite change token (record counts,
sizes and times, and search checkpoints) and the Tantivy opstamp are unchanged, so a warm
open does not decode them again.

Any changed input collects again: an edited, added or deleted file, a moved HEAD, a changed
index or config, or a revoke or clear of the canonical documents. A file modified within the
last 2 seconds cannot prove itself unchanged by its timestamp, so such a generation stores no
fingerprint and the next query collects as well. Cached discussions and commit messages, and
`rev:` filters, always collect. The journey also checks the storage effect: the warm query
leaves every `search-index:` checkpoint and reservation generation unchanged.

## Bounded work while it runs (TASK-66 AC3)

`tests/tui_e2e/performance_bounded_v66.rs` holds two more journeys, each run by keyboard and by
mouse at 120x40 with an 80x24 segment. They use the same report and budgets as the others.

- **Many avatars.** The Inbox lists 150 PRs, each by a different author with a different
  avatar URL (three queues of 50, served by strict offline fake gh). A fake `curl` holds every
  image fetch for 1 s, records how many fetches were running when it started, and prints a
  real PNG. A sampler thread reads the application's live avatar children from `/proc` every
  millisecond. The journey checks four things. At most 4 fetches run at once (sampled and
  self-reported). At most 128 actors are requested, and each URL is fetched exactly once. The
  disk cache keeps at most 128 records. All 10 gh calls are the scripted ones. While the
  fetches are still running (none had finished when the input ended), the keyboard moves the
  selection 20 rows down and back up, and the mouse turns the wheel over the list (a row
  click would open the PR). Both are judged against the 50 ms input-to-render p95.
- **History scrolled while loading.** On the 20,000-commit repository, History is scrolled
  past its first 200 commits (PageDown, or the wheel over the table), which reads the next
  page. A Git wrapper on `PATH` holds every `git log --skip=N` read with N > 0 for 4 s. The
  first page is read with `--skip=0` and is not held. While that read is held, three more
  scrolls past the boundary are sent, and the trace2 journal must still show exactly one
  continuation read (`--skip=200`). Scrolling back through the loaded rows is timed. A sample
  counts as "during the read" only while Git has not yet logged the held read's start.
  Afterwards 400 of 20,000 commits are loaded, Git state is unchanged and there are no gh
  calls.

## Measured results (release build, Linux)

Run on 2026-10-21 with
`GHTUI_PERF_BINARY=<target>/release/ghtui GHTUI_PERF_BUDGET_FACTOR=1 cargo test --locked
--test tui_e2e -- performance:: performance_bounded_v66:: --nocapture --test-threads=1`. The
binary came from `cargo build --locked --release --bin ghtui` at the TASK-66 commit. The
hardware was an AMD Ryzen 7 5700G (16 logical CPUs) with 27 GiB of memory, running Linux
7.0.0-38 inside the agent build scope (400 % CPU quota, 8 GiB memory cap, `nice 10`). Other
agents' builds shared the host, with a load average of 6 to 9. Times are in milliseconds.
Percentiles are nearest-rank over the listed samples (n).

Interactive budgets (keyboard / mouse journey):

| Metric | Budget | Keyboard | Mouse | Result |
| --- | --- | --- | --- | --- |
| Cold start, first usable frame | 300 | 297 | 180 | met |
| Warm start, first usable frame | 300 | 178 | 179 | met |
| History first page (20k commits) | 300 | 232 | 186 | met |
| Input-to-render 120x40, History (n=30) | p95 50 | p50 7.2, p95 11.8 | p50 14.1, p95 21.4 | met |
| Input-to-render 80x24, History (n=10) | p95 50 | p50 4.5, p95 5.8 | p50 7.5, p95 9.6 | met |
| Render cost 120x40, History journey | p95 16 | p50 0.8, p95 3.3 (n=209) | p50 0.8, p95 3.3 (n=245) | met |
| Render cost 80x24, History journey | p95 16 | p95 2.8 (n=11) | p95 2.8 (n=21) | met |
| Render cost 120x40, Search journey | p95 16 | p95 0.6 (n=283) | p95 0.7 (n=178) | met |
| Keypress while the ranked index builds | 50 | 2 | 4 | met |
| Exact search first page (matches all 5,000 files) | 100 | 123 | 125 | **over** |
| 50k-file tree path search first page | 100 | 42 | 42 | met |
| Cached 5 MB diff reopen | 150 | 554 | 516 | **over** (known gap) |
| Warmed Ranked first page | 100 | 1,376 | 1,622 | **over** before TASK-106 reuse landed; not yet re-measured in release |

Bounded-work journeys (keyboard / mouse):

| Metric | Budget | Keyboard | Mouse |
| --- | --- | --- | --- |
| Input-to-render 120x40 while 128 avatars load | p95 50 | p50 2.9, p95 3.6 (n=40) | debug only, see below |
| Input-to-render 80x24 while avatars load | p95 50 | p95 2.3 (n=6) | debug only |
| Render cost 120x40, avatar journey | p95 16 | p50 1.2, p95 2.0 (n=941) | debug only |
| Input-to-render while the next History page is held | p95 50 | p50 7.4, p95 11.6 (n=8) | p50 9.9, p95 12.8 (n=8) |
| History scroll to the page boundary, 120x40 | p95 50 | p95 8.1 (n=19 PageDown) | p95 12.7 (n=59 wheel) |
| History scroll, 80x24 | p95 50 | p95 5.5 (n=6) | p95 6.4 (n=6) |
| Render cost 120x40, History scroll journey | p95 16 | p95 2.7 (n=187) | p95 3.3 (n=224) |

Bounds observed in the same runs (debug runs show the same counts): at most 4 avatar fetches
running at once (sampled peak 4 over about 28,000 samples), 128 fetches of 128 distinct URLs
for 150 authors, 128 cache records, and 35 s for all 128 fetches at 1 s each. History made
exactly one continuation read (`--skip=200`) despite the extra scrolls, and ended with 400 of
20,000 commits loaded. Peak RSS was 23 MiB for the avatar journey, 21 MiB for History
scrolling, 139 to 141 MiB for the 5 MB diff and 50k-file tree, and 214 MiB for search with
its index. Idle CPU was 3 to 7 % of one CPU, and no Git or gh child ran while idle.

Separate (non-budget) timings, measured apart from the interactive budgets:

| Measurement | Keyboard | Mouse | Ceiling |
| --- | --- | --- | --- |
| Initial ranked indexing of the 5,000-file tree | 7,030 | 1,711 | 120,000 |
| First open of the 5 MB diff | 555 | 520 | 20,000 |
| Open the 50k-file saved tree | 170 | 182 | 20,000 |
| Build its Jump path index | 179 | 206 | 20,000 |
| Fake network: avatar fetch (held by the fixture) | 1,000 each | 1,000 each | n/a |
| Fake network: 20 MB Actions log (gh delay), debug run | 3,145 | 3,267 | n/a |

Network latency is never part of a budget above. It is simulated by the fixtures (gh `delay_ms`,
the held `curl` and Git reads). The interactive metrics measure only input answered while
that latency runs. The keyboard indexing time includes the first-run cache creation that a
concurrent build slowed down. The mouse run shows the uncontended cost.

The mouse avatar journey passed in the debug gate (input-to-render p95 20.9 ms at 120x40,
13.0 ms at 80x24, all bounds as above). Its release run hit a product race: a Refresh clicked
while startup was still running failed with `Error: database is locked • r Refresh to retry`
(see below). The journey now waits for the startup screen to settle before refreshing. That
change passed in debug but has not been run against the release binary yet.

## Known gaps

- **Exact search first page** misses its 100 ms budget in release (123 to 125 ms) for a query
  that matches all 5,000 files. Selective queries are well within it.

- **History loses the reader's place after the next page loads.** Scrolling past the 200th
  commit loads 200 more (`400 commits loaded · showing 1–200`), but the table returns to the
  newest commit, with the bar on row 0 (`history_after_next_page` in the report). Responsiveness
  and bounds are fine; this is a usability follow-up for the History owner.

- **`database is locked` on an early Inbox refresh** (seen once, release build). A refresh sent
  right after launch, while the startup Inbox work still held the account cache, failed after
  the 2 s lock wait instead of waiting for it or queueing behind it. The Inbox storage owner
  should follow up.

- **Cached diff reopen** is within its debug ceiling (about 0.9 s against 1.5 s). However,
  reopening the 5 MB commit diff costs the same as the first open: the comparison is computed
  again and not cached. The release build misses the budget too: 516 to 554 ms against
  150 ms, the same as its first open (520 to 555 ms). Meeting it needs a cached comparison.

## Running

```sh
cargo test --test tui_e2e performance -- --nocapture
```

Against a release application (the doc budgets exactly):

```sh
cargo build --locked --release --bin ghtui
GHTUI_PERF_BINARY=$CARGO_TARGET_DIR/release/ghtui GHTUI_PERF_BUDGET_FACTOR=1 \
  cargo test --locked --test tui_e2e -- performance:: performance_bounded_v66:: \
  --nocapture --test-threads=1
```

Under factor 1, the exact-search and cached-diff gaps above count as violations, so the two
History/diff journeys and the two Search journeys fail this run as expected. The warmed Ranked
gap is only printed. The JSON reports are the record. The debug gate (factor 10) is the one that
must pass.

This selects the `performance` journeys and their code tests, including the bounded-work
journeys in `performance_bounded_v66` and the Actions log journeys in `actions::perf_log_v66`.
The journeys hold a shared lock and run one at a time, so their timings never compete with each
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
