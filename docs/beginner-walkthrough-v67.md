# Beginner walkthrough (TASK-67, heuristic)

This is an author's heuristic walkthrough of the six beginner tasks at 80x24, not a study
with real users (TASK-67 AC2 still needs real participants, timings and wrong-turn counts).
Each task is encoded as a keyboard and a mouse real-binary journey in
`tests/tui_e2e/novice_usability_v67.rs`. The rule in those journeys: the keyboard path only
presses a key whose control is painted at that moment (key bar token, tab, bracketed button,
or a form hint such as `Ctrl-P: preview`), uses `: Cmds` (the command palette, searched by
plain words) when the bar has no room, opens `? Help`, and uses arrow keys only to move a
visible selection bar.

Gate: `cargo test --test tui_e2e -- novice_usability_v67` (12 journeys).

## 1. Locate my PR

Steps: `1 Inbox` tab -> `r Refresh` -> `c Created by me` queue tab -> click the PR row, or
arrows plus `Enter Open` -> the PR opens (`<title> • open`).

Wrong turns a beginner could take:
- Pressing Enter before Refresh: the queue is empty until the first load; the empty-queue
  text points at Refresh.
- Looking for "my PRs" under `All`: the PR is there too, but `Created by me` is the direct
  answer.

Terms changed: none. The queue tabs already use plain words.

## 2. Explain a merge

Steps: `g History` -> select the merge row (`●─╮` in the graph) -> `D Details` -> read the
`Merge` row -> `Enter Open` shows what the merge brought in (changes against parent 1).

Wrong turns:
- Reading "Parents  9ca6004  288e8b0" without knowing which is which. Fixed: the detail now
  adds a `Merge` row: "288e8b0 (parent 2) was merged into 9ca6004 (parent 1, the branch that
  was checked out); changes below are compared with parent 1".
- Expecting the merge diff to show both sides: it is against parent 1 only. The Changes
  view lists "Compare to parent 2" for the other side.

Terms changed: added the plain-language `Merge` row (`src/history/detail.rs`,
`merge_explanation`).

## 3. Compare two saved versions

Steps: `C Compare` -> type A in the first field (`Ctrl-A: select all` is shown) -> `F6
Fields↓` -> type B -> `Ctrl-P: preview` / `[Preview]` -> `Ctrl-S: confirm` /
`[Open comparison]`.

Wrong turns:
- Swapping A and B: `F9 Swap A/B` is painted on the form; the preview says
  "Direction: A → B".
- Choosing `proposed` without knowing what it means: the form line explains
  "snapshot = all differences A→B • proposed = only B's changes since …".

Terms changed: "Target is 1 commits ahead" became "1 commit ahead" (plural-aware), and
"1 changed files • HEAD, working files and next commit remain untouched" became
"1 changed file • nothing in your checkout changes" (`src/comparison.rs`).

## 4. Browse an old file

Steps: `g History` -> select the old commit -> `t Tree` tab -> select the file -> `Enter
Open` (or click the row) -> "notes.txt at saved version <sha>".

Wrong turns:
- The Tree's key bar offered no way to open a row; beginners did not guess Enter. Fixed:
  the bar paints `Enter Open` while a file or folder row is selected and the rows own the
  keyboard. The token runs the same row action as Enter and a click.
- Expecting the old bytes to replace the working file: the Tree heading says browsing never
  checks out or changes working files, and the journey checks the working file is unchanged.

Terms changed: new `Enter Open` key bar token in the saved Tree (`src/repository_ui.rs`).

## 5. Resolve a conflict

Steps: the `⚠ MERGE PAUSED` banner -> open the conflicted file row (its hint says
"Enter: compare base/ours/theirs") -> `: Cmds`, type "Resolve one conflict block" -> keep
`[theirs]` -> Preview -> Confirm -> `: Cmds` "Stage selected file" -> Preview -> Confirm ->
`: Cmds` "Continue operation" -> Preview -> Confirm.

Wrong turns:
- Pressing Down three times from the banner lands on `[Abort merge]`; Enter then opens the
  Abort form. It still needs Preview and Confirm, so nothing is lost, but it is a real
  trap. Not changed in this pass.
- `m Git ops` opens a long Local operations menu where "Resolve one conflict block" is on a
  later page. The palette finds it by its words.
- The conflict view shows "Stage 1: saved object <id>" (index stage numbers) beside the
  plain "Base / Ours / Theirs" captions. Left as is: tests and recovery docs use them.

Terms changed: the palette header no longer says "Prepared commands · not current
permission/target authority"; it says "Type to filter · Enter runs the selected command ·
writes ask first", and the filter status says "Filtered; each command still checks its
target before it runs." (`src/command_palette.rs`).

## 6. Find the failing CI step

Steps: `4 Actions` tab -> `? Help` (workflow, run, attempt and step are defined) -> click
the failed run, or arrows plus Enter -> click the failed job, or arrows plus Enter -> the
failing step "3 Run tests" is listed -> `l Logs` (bar or `: Cmds`) -> the error line.

Wrong turns:
- The Actions key bar has no `Enter Open` token, so Enter on a run or job row must be
  guessed (clicks work). Not changed in this pass.
- Opening the run's Logs before choosing a job: the job is the unit that has steps and logs.

Terms changed: none.

## Remaining findings (not fixed here)

- Form previews wrap mid-word at 80 columns ("in / dex", "Insp / ect").
- Repository headers show `HEAD:` and `Saved:` full SHAs to beginners; many journeys match
  these rows, so a relabel needs its own migration.
- The `Ours (HEAD)` caption in conflict previews keeps Git's marker name.
- The Actions bar lacks an `Enter Open` token for runs and jobs.
