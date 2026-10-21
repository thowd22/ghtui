//! TASK-67 `novice_usability`: the six beginner tasks on the real executable at 80x24, by
//! keyboard and by mouse, using only what the screen shows at that moment: key-bar tokens,
//! tabs, bracketed buttons, the hints a form paints (`Ctrl-P: preview`, `Ctrl-S: confirm`),
//! `: Cmds` (the command palette, searched by the words a beginner would type) and `? Help`.
//! The keyboard path never presses a key whose control is not painted first; arrow keys only
//! move a visible selection. Each journey checks the visible outcome and, independently, the
//! command journal, Git state and working files.
//!
//! 1. Locate my PR: Inbox -> Refresh -> Created by me -> open the PR.
//! 2. Explain a merge: History -> the merge commit -> Details (plain-language parents) ->
//!    Open its changes against parent 1.
//! 3. Compare two saved versions: Compare -> A and B -> Preview -> Open comparison.
//! 4. Browse an old file: History -> an old commit -> Tree -> Enter Open the file.
//! 5. Resolve a conflict: the paused-merge banner -> the conflicted file -> Commands
//!    "Resolve one conflict block" -> Stage -> Continue.
//! 6. Find the failing CI step: Actions -> the failed run -> the failed job -> Logs.
use crate::support::{Fixture, GhStep, Session};
use ghtui::config::Config;
use serde_json::{json, Value};
use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};

const HELP: &str = "Help • keyboard and mouse";

/// A painted control, used the way the screen offers it: clicked, or its shown key pressed.
/// Waiting for the control first is the discoverability check for the keyboard path too.
fn step(session: &mut Session, mouse: bool, label: &str) {
    session.wait_for_control(label);
    session.choose_control(mouse, label);
}

/// A control on the bar when it fits at 80 columns, otherwise the same command through
/// `: Cmds` (what a beginner does when the bar has no room for it).
fn step_or_command(session: &mut Session, mouse: bool, label: &str) {
    if session
        .try_wait_for_control(label, Duration::from_secs(2))
        .is_some()
    {
        session.choose_control(mouse, label);
    } else {
        command(session, mouse, label);
    }
}

/// A header view tab such as `1 Inbox`: painted in the header row, clicked or its digit pressed.
fn view_tab(session: &mut Session, mouse: bool, tab: &str, key: &[u8]) {
    session.wait_for_screen(&format!(" {tab} "));
    session.choose(mouse, tab, key);
}

/// A form's Preview: the `[Preview]` button, or the `Ctrl-P: preview` hint the form paints.
fn preview(session: &mut Session, mouse: bool, ready: &str) {
    session.wait_for_screen("Ctrl-P: preview");
    session.choose(mouse, "[Preview]", b"\x10");
    // The prepared preview replaces the intent before anything can be confirmed.
    session.wait_for_screen(ready);
}

/// A form's confirming button, or the `Ctrl-S: confirm` hint the form paints.
fn confirm(session: &mut Session, mouse: bool, button: &str) {
    session.wait_for_screen("Ctrl-S: confirm");
    session.wait_for_control(button);
    session.choose(mouse, button, b"\x13");
}

/// A command found through `: Cmds` by its plain words.
fn command(session: &mut Session, mouse: bool, caption: &str) {
    session.wait_for_control("[: Cmds]");
    session.invoke_via_palette(mouse, caption);
}

/// `? Help` opens this view's help with its heading; it closes by Esc or `[Esc Close]`.
fn help(session: &mut Session, mouse: bool, heading: &str) {
    step(session, mouse, "[? Help]");
    session.wait_for_screen(heading);
    if mouse {
        session.close_overlay_by_mouse(HELP);
    } else {
        session.close_overlay_by_key(HELP);
    }
}

fn local_config(fixture: &Fixture) {
    Config {
        refresh_seconds: 3600,
        avatars: false,
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
}

/// HEAD, every ref, the index bytes and porcelain status: what read-only browsing keeps.
fn git_state(fixture: &Fixture) -> (String, String, Vec<u8>, String) {
    (
        fixture.git(&["rev-parse", "HEAD"]),
        fixture.git(&["show-ref"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fixture.git(&["status", "--porcelain=v2"]),
    )
}

fn journal(session: &mut Session, fixture: &Fixture, count: usize) -> Vec<Value> {
    session.wait_for_command_count(fixture, count);
    let rows: Vec<Value> = session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), count, "{rows:?}");
    assert!(rows.iter().all(|row| row["matched"] == true), "{rows:?}");
    rows
}

fn writes(records: &[Value]) -> usize {
    records
        .iter()
        .filter(|record| {
            record["args"].as_array().is_some_and(|args| {
                args.iter()
                    .any(|arg| matches!(arg.as_str(), Some("POST" | "PUT" | "PATCH" | "DELETE")))
            })
        })
        .count()
}

/// Arrow keys move the visible `▌` bar until it sits on the row holding `text`.
fn arrow_to(session: &mut Session, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(30));
        let screen = session.screen();
        if screen
            .lines()
            .any(|line| line.contains('▌') && line.contains(text))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{text:?} never gained the selection bar\n{screen}"
        );
        session.send_key(b"\x1b[B");
        session.poll_output(Duration::from_millis(120));
    }
}

// ---------------------------------------------------------------------------------------
// 1. Locate my PR.
// ---------------------------------------------------------------------------------------

fn locate_my_pr(mouse: bool) {
    use crate::pr_actions::{configured, details, initial, local_git_witness, starter};
    let fixture = configured();
    let git = local_git_witness(&fixture);
    let mut steps = initial();
    let listed = steps.len();
    steps.extend(details(vec![starter()]));
    fixture.scripted_gh(&steps);

    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("ghtui");
    view_tab(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Inbox • focused view");
    step(&mut session, mouse, "[r Refresh]");
    session.wait_for_screen("Native collaboration");
    journal(&mut session, &fixture, listed);
    // "My PR" is the Created by me queue tab, painted beside the others.
    step(&mut session, mouse, "[c Created by me]");
    session.wait_for_screen("Queue: Created by me");
    session.wait_for_screen("Native collaboration");
    if mouse {
        session.click_visible_action("Native collaboration");
    } else {
        arrow_to(&mut session, "Native collaboration");
        step(&mut session, false, "[Enter Open]");
    }
    session.wait_for_screen("Native collaboration • open");
    let records = journal(&mut session, &fixture, steps.len());
    assert_eq!(writes(&records), 0, "locating a PR only reads");
    assert_eq!(local_git_witness(&fixture), git, "local checkout untouched");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn novice_usability_v67_keyboard_locate_my_pr_from_the_inbox_at_80x24() {
    locate_my_pr(false)
}
#[test]
fn novice_usability_v67_mouse_locate_my_pr_from_the_inbox_at_80x24() {
    locate_my_pr(true)
}

// ---------------------------------------------------------------------------------------
// 2. Explain a merge.
// ---------------------------------------------------------------------------------------

fn explain_merge(mouse: bool) {
    let fixture = Fixture::new();
    local_config(&fixture);
    fixture.init_git();
    fixture.commit_file("root.txt", b"root\n", "Start the project");
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file("feature.txt", b"feature-only line\n", "Feature work");
    let feature = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.git(&["switch", "main"]);
    fixture.commit_file("main.txt", b"main-only line\n", "Main work");
    let main = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.git(&[
        "merge",
        "--no-ff",
        "feature",
        "-m",
        "Merge feature into main",
    ]);
    let merge = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.commit_file("after.txt", b"after\n", "Work after the merge");
    let before = git_state(&fixture);

    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("Recent commits");
    step(&mut session, mouse, "[g History]");
    session.wait_for_screen("commits loaded");
    session.open_saved_graph_subject(mouse, "Merge feature into main", &merge);
    step(&mut session, mouse, "[D Details]");
    // The detail names both parents and says, in plain words, which one was merged in.
    session.wait_for_screen("Parents");
    session.wait_for_screen(&format!("{}  {}", &main[..7], &feature[..7]));
    session.wait_for_screen(&format!(
        "{} (parent 2) was merged into {}",
        &feature[..7],
        &main[..7]
    ));
    session.wait_for_screen("(parent 1, the branch");
    // What the merge brought in: its changes against parent 1 are the feature's line only.
    step(&mut session, mouse, "[Enter Open]");
    session.wait_for_screen("Compared parent 1:");
    session.wait_for_screen("+feature-only line");
    assert!(!session.screen().contains("+main-only line"));
    assert_eq!(git_state(&fixture), before, "History is read-only");
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn novice_usability_v67_keyboard_explain_a_merge_from_history_details_at_80x24() {
    explain_merge(false)
}
#[test]
fn novice_usability_v67_mouse_explain_a_merge_from_history_details_at_80x24() {
    explain_merge(true)
}

// ---------------------------------------------------------------------------------------
// 3. Compare two saved versions.
// ---------------------------------------------------------------------------------------

fn type_field(session: &mut Session, mouse: bool, label: &str, value: &str, next: bool) {
    if mouse {
        let (column, row) = session.locate(label).unwrap();
        session.click(column, row + 1);
    } else if next {
        // `[F6 Fields↓]` is painted on the form.
        session.wait_for_control("[F6 Fields↓]");
        session.send_key(b"\x1b[17~");
    }
    // `Ctrl-A: select all` is painted on the form; typing replaces the selection.
    session.wait_for_screen("Ctrl-A: select all");
    session.send_key(b"\x01");
    session.paste(value);
    session.wait_for_screen(value);
}

fn compare_versions(mouse: bool) {
    let fixture = Fixture::new();
    local_config(&fixture);
    fixture.init_git();
    fixture.commit_file("notes.txt", b"first draft\n", "First notes");
    let first = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.commit_file("notes.txt", b"second draft\n", "Second notes");
    let second = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.commit_file("other.txt", b"later\n", "Later work");
    fs::write(fixture.repo().join("notes.txt"), b"unsaved edit\n").unwrap();
    let before = git_state(&fixture);

    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("Recent commits");
    step(&mut session, mouse, "[C Compare]");
    session.wait_for_screen("Compare two saved versions");
    session.wait_for_screen("READ-ONLY COMPARISON");
    type_field(
        &mut session,
        mouse,
        "A: from (branch, tag or commit):",
        &first,
        false,
    );
    type_field(
        &mut session,
        mouse,
        "B: to (branch, tag or commit):",
        &second,
        true,
    );
    preview(&mut session, mouse, "Snapshot answer: what differs");
    // The preview body, not the status line: a late local draft ACK may replace the status.
    session.wait_for_screen("Target is 1 commit ahead and 0 behind base.");
    confirm(&mut session, mouse, "[Open comparison]");
    session.wait_for_screen("Comparison opened • read-only");
    session.wait_for_screen("1 changed file • nothing in your checkout changes");
    session.wait_for_screen("-first draft");
    session.wait_for_screen("+second draft");
    assert_eq!(git_state(&fixture), before, "comparing changes nothing");
    assert_eq!(
        fs::read(fixture.repo().join("notes.txt")).unwrap(),
        b"unsaved edit\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn novice_usability_v67_keyboard_compare_two_saved_versions_at_80x24() {
    compare_versions(false)
}
#[test]
fn novice_usability_v67_mouse_compare_two_saved_versions_at_80x24() {
    compare_versions(true)
}

// ---------------------------------------------------------------------------------------
// 4. Browse an old file.
// ---------------------------------------------------------------------------------------

fn browse_old_file(mouse: bool) {
    let fixture = Fixture::new();
    local_config(&fixture);
    fixture.init_git();
    fixture.commit_file("notes.txt", b"the old wording\n", "Old notes");
    let old = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.commit_file("notes.txt", b"the new wording\n", "New notes");
    fs::write(fixture.repo().join("notes.txt"), b"my unsaved wording\n").unwrap();
    let before = git_state(&fixture);

    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("Recent commits");
    step(&mut session, mouse, "[g History]");
    session.wait_for_screen("commits loaded");
    session.open_saved_graph_subject(mouse, "Old notes", &old);
    step(&mut session, mouse, "[t Tree]");
    session.wait_for_screen("Saved file tree");
    session.wait_for_screen("notes.txt");
    if mouse {
        session.click_visible_action("· notes.txt");
        // A click opens the row; when it only selected it, the painted Enter Open opens it.
        let opened = session.try_wait_for_condition(
            |screen| screen.contains("notes.txt at saved version"),
            "file opened by its click",
            Duration::from_secs(2),
        );
        if opened.is_err() {
            step(&mut session, true, "[Enter Open]");
        }
    } else {
        // The bar paints `Enter Open` once a file row is selected.
        let deadline = Instant::now() + Duration::from_secs(8);
        while session
            .try_wait_for_control("[Enter Open]", Duration::from_millis(400))
            .is_none()
        {
            assert!(Instant::now() < deadline, "{}", session.screen());
            session.send_key(b"\x1b[B");
        }
        step(&mut session, false, "[Enter Open]");
    }
    session.wait_for_screen(&format!("notes.txt at saved version {}", &old[..8]));
    session.wait_for_screen("the old wording");
    let screen = session.screen();
    assert!(!screen.contains("the new wording"), "{screen}");
    assert!(!screen.contains("my unsaved wording"), "{screen}");
    assert_eq!(git_state(&fixture), before, "browsing changes nothing");
    assert_eq!(
        fs::read(fixture.repo().join("notes.txt")).unwrap(),
        b"my unsaved wording\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn novice_usability_v67_keyboard_browse_an_old_file_from_a_saved_tree_at_80x24() {
    browse_old_file(false)
}
#[test]
fn novice_usability_v67_mouse_browse_an_old_file_from_a_saved_tree_at_80x24() {
    browse_old_file(true)
}

// ---------------------------------------------------------------------------------------
// 5. Resolve a conflict.
// ---------------------------------------------------------------------------------------

/// The first actionable Overview row (the conflicted file): clicked, or reached by arrows.
fn open_conflict(session: &mut Session, mouse: bool) {
    session.wait_for_screen("conflicted  notes.txt");
    if mouse {
        session.click_visible_action("conflicted  notes.txt");
    } else {
        // The first Down focuses the rows pane; Up then reaches the first row.
        session.send_key(b"\x1b[B");
        session.send_key(&b"\x1b[A".repeat(32));
        session.send_key(b"\r");
    }
    session.wait_for_screen("Conflict in notes.txt");
}

fn resolve_conflict(mouse: bool) {
    let fixture = Fixture::new();
    local_config(&fixture);
    fixture.init_git();
    fixture.commit_file("notes.txt", b"base\n", "Base notes");
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file("notes.txt", b"theirs\n", "Their notes");
    let theirs = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.git(&["switch", "main"]);
    fixture.commit_file("notes.txt", b"ours\n", "Our notes");
    let ours = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let merge = Command::new("git")
        .args(["merge", "feature"])
        .current_dir(fixture.repo())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(!merge.status.success(), "the fixture merge must conflict");
    let unmerged = fixture.git(&["ls-files", "--unmerged"]);
    assert!(!unmerged.is_empty());

    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("Recent commits");
    session.wait_for_screen("⚠ MERGE PAUSED • 1 unresolved conflict");
    help(&mut session, mouse, "Repository • help for this view");
    // The banner row itself says what Enter (or a click) does.
    session.wait_for_screen("Enter: compare base/ours/theirs");
    open_conflict(&mut session, mouse);
    session.wait_for_screen("Ours — current branch content");
    session.wait_for_screen("Theirs — incoming branch content");

    // Keep the incoming version of the one conflict block, found by its plain name.
    command(&mut session, mouse, "Resolve one conflict block");
    session.wait_for_screen("LOCAL INTENT • Preview first");
    session.wait_for_screen("Keep (ours, theirs, both, base, edited):");
    if mouse {
        session.click_visible_action("[theirs]");
    } else {
        session.wait_for_screen("Tab: controls");
        session.send_key(b"\t");
        session.send_key(b"\x01");
        session.paste("theirs");
    }
    preview(&mut session, mouse, "READ-ONLY PREVIEW • Confirm action");
    session.wait_for_screen("Git effect: block 1 → theirs in notes.txt");
    assert!(
        fs::read_to_string(fixture.repo().join("notes.txt"))
            .unwrap()
            .contains("<<<<<<<"),
        "Preview writes nothing"
    );
    confirm(&mut session, mouse, "[Confirm action]");
    session.wait_for_screen("Completed • Resolve one conflict block");
    assert_eq!(
        fs::read(fixture.repo().join("notes.txt")).unwrap(),
        b"theirs\n"
    );
    assert_eq!(
        fixture.git(&["ls-files", "--unmerged"]),
        unmerged,
        "the index waits for an explicit Stage"
    );

    // Stage the resolved file, then continue the merge, each through Commands.
    open_conflict(&mut session, mouse);
    command(&mut session, mouse, "Stage selected file");
    session.wait_for_screen("LOCAL INTENT • Preview first");
    preview(&mut session, mouse, "READ-ONLY PREVIEW • Confirm action");
    confirm(&mut session, mouse, "[Confirm action]");
    session.wait_for_screen("Completed • Stage selected file");
    assert_eq!(fixture.git(&["ls-files", "--unmerged"]), "");
    session.wait_for_screen("⚠ MERGE PAUSED • all conflicts resolved and staged");
    command(&mut session, mouse, "Continue operation");
    session.wait_for_screen("LOCAL INTENT • Preview first");
    preview(&mut session, mouse, "READ-ONLY PREVIEW • Confirm action");
    session.wait_for_screen("merge --continue");
    confirm(&mut session, mouse, "[Confirm action]");
    session.wait_for_screen("Completed • Continue operation");
    assert_eq!(fixture.git(&["rev-parse", "HEAD^1"]), format!("{ours}\n"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD^2"]), format!("{theirs}\n"));
    assert_eq!(fixture.git(&["show", "HEAD:notes.txt"]), "theirs\n");
    assert!(!fixture.repo().join(".git/MERGE_HEAD").exists());
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn novice_usability_v67_keyboard_resolve_a_conflict_stage_and_continue_at_80x24() {
    resolve_conflict(false)
}
#[test]
fn novice_usability_v67_mouse_resolve_a_conflict_stage_and_continue_at_80x24() {
    resolve_conflict(true)
}

// ---------------------------------------------------------------------------------------
// 6. Find the failing CI step.
// ---------------------------------------------------------------------------------------

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn read(endpoint: &str, value: Value, pages: bool) -> GhStep {
    let mut args = vec![
        "api".to_owned(),
        endpoint.into(),
        "--hostname".into(),
        "github.invalid".into(),
        "--method".into(),
        "GET".into(),
    ];
    if pages {
        args.extend(["--paginate".into(), "--slurp".into()]);
    }
    GhStep {
        args,
        stdout: serde_json::to_vec(&value).unwrap(),
        stdin: vec![],
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}

fn failed_run() -> Value {
    json!({"id":123,"workflow_id":7,"name":"CI matrix","display_title":"Fix checkout",
        "head_sha":SHA,"head_branch":"feature","event":"pull_request","status":"completed",
        "conclusion":"failure","run_attempt":2,"created_at":"2026-10-13T12:00:00Z",
        "actor":{"login":"alice","type":"User"},"triggering_actor":{"login":"octocat","type":"User"}})
}

fn find_failing_step(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("unchanged.txt", b"preserve this checkout\n", "initial");
    Config {
        host: "github.invalid".into(),
        avatars: false,
        repository_sets: std::collections::BTreeMap::from([(
            "actions".into(),
            vec!["example/repo".into()],
        )]),
        active_set: Some("actions".into()),
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    let before = git_state(&fixture);
    let steps = vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
        read(
            "repos/example/repo/actions/runs?per_page=100",
            json!({"total_count":1,"workflow_runs":[failed_run()]}),
            false,
        ),
        read(
            "repos/example/repo/actions/runs/123/attempts/2",
            failed_run(),
            false,
        ),
        read(
            "repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100",
            json!([{"jobs":[{"id":501,"run_id":123,"run_attempt":2,"head_sha":SHA,
            "name":"matrix test linux","status":"completed","conclusion":"failure",
            "runner_name":"hosted","labels":["ubuntu-latest"],
            "check_run_url":"https://github.invalid/api/v3/repos/example/repo/check-runs/600",
            "steps":[
                {"number":1,"name":"Set up job","status":"completed","conclusion":"success"},
                {"number":3,"name":"Run tests","status":"completed","conclusion":"failure"}
            ]}]}]),
            true,
        ),
        GhStep::read(
            &[
                "run",
                "view",
                "123",
                "--repo",
                "github.invalid/example/repo",
                "--attempt",
                "2",
                "--log",
                "--job",
                "501",
            ],
            "matrix test linux\tRun tests\tERROR fixture assertion failed\nA useful next line\n",
        ),
    ];
    fixture.scripted_gh(&steps);

    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("ghtui");
    view_tab(&mut session, mouse, "4 Actions", b"4");
    session.wait_for_screen("123 #2");
    journal(&mut session, &fixture, 3);
    help(&mut session, mouse, "Actions • help for this view");
    // The failed run: clicked, or selected and opened with Enter.
    if mouse {
        session.click_visible_action("123 #2");
    } else {
        arrow_to(&mut session, "123 #2");
        session.send_key(b"\r");
    }
    session.wait_for_screen("matrix test linux");
    journal(&mut session, &fixture, 5);
    // The failed job, then its steps: the failing step is named.
    if mouse {
        session.click_visible_action("matrix test linux");
    } else {
        arrow_to(&mut session, "matrix test linux");
        session.send_key(b"\r");
    }
    session.wait_for_screen("3 Run tests");
    step_or_command(&mut session, mouse, "[l Logs]");
    session.wait_for_screen("ERROR fixture assertion failed");
    let records = journal(&mut session, &fixture, 6);
    assert!(records[5]["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "501"));
    assert_eq!(writes(&records), 0, "finding a failure only reads");
    assert_eq!(git_state(&fixture), before);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn novice_usability_v67_keyboard_find_the_failing_ci_step_at_80x24() {
    find_failing_step(false)
}
#[test]
fn novice_usability_v67_mouse_find_the_failing_ci_step_at_80x24() {
    find_failing_step(true)
}
