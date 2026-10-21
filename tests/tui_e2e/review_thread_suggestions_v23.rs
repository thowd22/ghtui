//! TASK-23 `suggestions`: a posted review comment with a ```suggestion block is
//! chosen from the threads overlay, `p Plan` previews the exact commit (no gh
//! call, no write), `C Commit & push` then `y Confirm push` pushes exactly that
//! commit to the PR head branch of a LOCAL bare remote, and the PR write
//! journal records the outcome. A head that moved after the plan is refused
//! with nothing pushed. The checkout, index, refs and config never change.
use super::*;
use crate::immutable_git_v1::{GitImagesV1, PATH};

const HEAD_REPO: &str = "contributor/source";
const BRANCH: &str = "feature/scoped-open";
const SUGGESTED: &str = "applied-suggestion 界";
const PANE: &str = "Suggested changes";

fn suggestion_page(h: &GitImagesV1) -> Value {
    let mut page = page();
    let thread = &mut page["data"]["node"]["reviewThreads"]["nodes"][0];
    thread["isOutdated"] = json!(false);
    thread["path"] = json!(PATH);
    thread["diffSide"] = json!("RIGHT");
    thread["startDiffSide"] = json!("RIGHT");
    thread["line"] = json!(2);
    thread["originalLine"] = json!(2);
    thread["startLine"] = Value::Null;
    thread["originalStartLine"] = Value::Null;
    let comment = &mut thread["comments"]["nodes"][0];
    comment["body"] = json!(format!("Clearer:\n```suggestion\n{SUGGESTED}\n```"));
    comment["path"] = json!(PATH);
    comment["line"] = json!(2);
    comment["originalLine"] = json!(2);
    comment["startLine"] = Value::Null;
    comment["originalStartLine"] = Value::Null;
    comment["commit"] = json!({"oid": h.h});
    comment["originalCommit"] = json!({"oid": h.h});
    let text = page.to_string().replace(&"a".repeat(40), &h.h);
    serde_json::from_str(&text).unwrap()
}

/// One posted suggestion for a batch page: RIGHT lines `start..=end` of PATH
/// at the PR head, replaced by `code`, written by `(databaseId, login)`.
struct Posted {
    start: u32,
    end: u32,
    code: &'static str,
    author: (u64, &'static str),
}

/// A thread page with one unresolved RIGHT thread per posted suggestion. Each
/// thread and comment has its own node and database ID (9, 10, ...).
fn batch_page(h: &GitImagesV1, posted: &[Posted]) -> Value {
    let template = suggestion_page(h);
    let mut page = template.clone();
    let mut threads = vec![];
    for (offset, suggestion) in posted.iter().enumerate() {
        let number = 9 + offset;
        let mut thread = template["data"]["node"]["reviewThreads"]["nodes"][0].clone();
        thread["id"] = json!(format!("NATIVE_THREAD_{number}"));
        let start = (suggestion.start != suggestion.end).then_some(suggestion.start);
        thread["line"] = json!(suggestion.end);
        thread["originalLine"] = json!(suggestion.end);
        thread["startLine"] = json!(start);
        thread["originalStartLine"] = json!(start);
        let comment = &mut thread["comments"]["nodes"][0];
        comment["id"] = json!(format!("NATIVE_COMMENT_{number}"));
        comment["fullDatabaseId"] = json!(format!("{}", 9000 + number));
        comment["body"] = json!(format!(
            "Batch me:\n```suggestion\n{}\n```",
            suggestion.code
        ));
        comment["line"] = json!(suggestion.end);
        comment["originalLine"] = json!(suggestion.end);
        comment["startLine"] = json!(start);
        comment["originalStartLine"] = json!(start);
        comment["author"]["databaseId"] = json!(suggestion.author.0);
        comment["author"]["login"] = json!(suggestion.author.1);
        threads.push(thread);
    }
    page["data"]["node"]["reviewThreads"]["totalCount"] = json!(threads.len());
    page["data"]["node"]["reviewThreads"]["nodes"] = Value::Array(threads);
    page
}

struct Setup {
    fixture: Fixture,
    h: GitImagesV1,
    bare: std::path::PathBuf,
    steps: Vec<GhStep>,
}

fn setup() -> Setup {
    setup_with(suggestion_page)
}

fn setup_with(page: impl FnOnce(&GitImagesV1) -> Value) -> Setup {
    let fixture = resource_configured(&[]);
    let h = GitImagesV1::create(&fixture.repo(), false);
    let mut steps = initial();
    steps.extend(resource_operation(false));
    let mut threads = phase();
    threads[9].stdout = page(&h).to_string().into_bytes();
    steps.extend(threads);
    super::super::immutable_images::bind_steps(&mut steps, &h);
    assert_eq!(steps.len(), 58);
    fixture.scripted_gh(&steps);
    // The PR head lives in a LOCAL bare remote. The checkout names it by its
    // GitHub URL and Git rewrites that URL to the bare path (insteadOf).
    let bare = fixture.root.join("suggestion-head-remote.git");
    fixture.git(&["init", "--quiet", "--bare", bare.to_str().unwrap()]);
    fixture.git(&[
        "push",
        "--quiet",
        bare.to_str().unwrap(),
        &format!("{}:refs/heads/{BRANCH}", h.h),
    ]);
    let url = format!("https://{HOST}/{HEAD_REPO}.git");
    fixture.git(&["remote", "add", "contributor", &url]);
    fixture.git(&["config", &format!("url.{}.insteadOf", bare.display()), &url]);
    Setup {
        fixture,
        h,
        bare,
        steps,
    }
}

fn bare_git(bare: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(bare)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(output.status.success(), "{args:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn pane(session: &mut Session, words: &[&str], what: &str) {
    session.wait_for_cursorless_condition(
        |frame| {
            // Status sentences wrap; compare the pane's words row by row.
            let text = super::super::visible_words(&frame.contents());
            text.contains(PANE)
                && text.contains("[Esc Back]")
                && words.iter().all(|word| text.contains(word))
        },
        what,
        Duration::from_secs(20),
    );
}

fn attempts(fixture: &Fixture) -> Vec<(String, String, String, Option<String>)> {
    let db = raw(fixture);
    let mut statement = db
        .prepare("SELECT command,selected_resource,state,confirmed_json FROM pr_write_attempts ORDER BY attempt_id")
        .unwrap();
    statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn journey(mouse: bool, moved: bool) {
    let Setup {
        fixture,
        h,
        bare,
        steps,
    } = setup();
    let git = git_state(&fixture);
    let config_bytes = fs::read(fixture.repo().join(".git/config")).unwrap();
    let app_config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files(&mut session, &fixture, mouse);
    choose(&mut session, mouse, "[T Threads]", b"T");
    session.wait_for_command_count(&fixture, 58);
    owned(&mut session, &["[s Suggestions]", "NATIVE_THREAD_9"]);
    choose(&mut session, mouse, "[s Suggestions]", b"s");
    pane(
        &mut session,
        &["1 posted suggestion(s), 1 can be applied", "[ ] "],
        "Chooser lists the posted suggestion with its author, range and code",
    );
    // The controls fit at 80×24; the plan is then read at 120×40.
    session.resize_owned_header(80, 24, PANE, &["[C Commit & push]", "[Esc Back]"]);
    session.resize_owned_header(120, 40, PANE, &["[C Commit & push]", "[Esc Back]"]);
    let screen = session.screen();
    assert!(screen.contains("native-thread-actor"), "{screen}");
    assert!(screen.contains(&format!("{PATH} lines 2–2")), "{screen}");
    assert!(screen.contains(&format!("+{SUGGESTED}")), "{screen}");
    // Planning needs a choice: the disabled control explains and sends nothing.
    choose(&mut session, mouse, "[p Plan]", b"p");
    pane(
        &mut session,
        &["Plan unavailable • choose at least one suggestion first"],
        "Plan refused without a choice",
    );
    if mouse {
        let (column, row) = session.locate("[ ] ").unwrap();
        session.click(column, row);
    } else {
        session.send_key(b" ");
    }
    pane(&mut session, &["[x] "], "Suggestion chosen");
    choose(&mut session, mouse, "[p Plan]", b"p");
    pane(
        &mut session,
        &["Planned commit", "nothing written yet", "-new-no-newline"],
        "Plan shows the exact commit and resulting change",
    );
    let screen = session.screen();
    for expected in [
        format!("Parent {} (PR head)", h.h),
        format!("Push to contributor {}", bare.display()),
        format!("Branch {BRANCH} • lease on {}", &h.h[..12]),
        "Committer Immutable fixture <immutable@example.invalid>".to_owned(),
        format!("File {PATH} → blob"),
        "Credit: Co-authored-by: native-thread-actor <77+native-thread-actor@users.noreply.github.com>"
            .to_owned(),
        format!("+{SUGGESTED}"),
    ] {
        assert!(screen.contains(&expected), "{expected:?} missing:\n{screen}");
    }
    let commit = screen
        .split("Plan commit ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap()
        .to_owned();
    assert_eq!(commit.len(), 40, "{screen}");
    // Planning wrote nothing anywhere.
    assert_eq!(
        bare_git(&bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]),
        h.h
    );
    assert!(attempts(&fixture).is_empty());
    assert_eq!(commands(&session, &fixture).len(), 58, "no gh call");
    let moved_to = moved.then(|| {
        let other = fixture.root.join("suggestion-other");
        bare_git(
            &fixture.root,
            &[
                "clone",
                "--quiet",
                "-b",
                BRANCH,
                bare.to_str().unwrap(),
                other.to_str().unwrap(),
            ],
        );
        fs::write(other.join("moved.txt"), b"someone else pushed\n").unwrap();
        bare_git(&other, &["add", "moved.txt"]);
        bare_git(
            &other,
            &[
                "-c",
                "user.name=Other",
                "-c",
                "user.email=other@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "moved",
            ],
        );
        bare_git(&other, &["push", "--quiet", "origin", BRANCH]);
        bare_git(&bare, &["rev-parse", &format!("refs/heads/{BRANCH}")])
    });
    choose(&mut session, mouse, "[C Commit & push]", b"C");
    pane(
        &mut session,
        &[
            "[y Confirm push]",
            "[n Keep plan]",
            &format!("to {BRANCH} on contributor"),
        ],
        "Explicit push confirmation names commit, branch and remote",
    );
    choose(&mut session, mouse, "[y Confirm push]", b"y");
    if let Some(moved_to) = moved_to {
        pane(
            &mut session,
            &["The PR head moved from", "nothing was pushed"],
            "Moved head refused; nothing pushed",
        );
        assert_eq!(
            bare_git(&bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]),
            moved_to
        );
        let rows = attempts(&fixture);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].2, "acknowledged", "closed without an effect");
        assert_eq!(rows[0].3, None);
    } else {
        pane(
            &mut session,
            &[&format!("Pushed {commit} to {BRANCH}"), "confirmed"],
            "Pushed exactly the previewed commit",
        );
        assert_eq!(
            bare_git(&bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]),
            commit
        );
        assert_eq!(bare_git(&bare, &["rev-parse", &format!("{commit}^")]), h.h);
        let blob = std::process::Command::new("git")
            .args(["cat-file", "blob", &format!("{commit}:{PATH}")])
            .current_dir(&bare)
            .output()
            .unwrap()
            .stdout;
        assert_eq!(blob, format!("context\r\n{SUGGESTED}").into_bytes());
        let rows = attempts(&fixture);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].0.as_str(), rows[0].1.as_str(), rows[0].2.as_str()),
            ("suggestion-apply-v23", "NATIVE_COMMENT_9", "confirmed")
        );
        let effect: Value = serde_json::from_str(rows[0].3.as_deref().unwrap()).unwrap();
        assert_eq!(effect["kind"], "branch_content");
        assert_eq!(effect["resource_id"], commit.as_str());
    }
    // No gh call at all, and the checkout, index, refs and config are intact.
    assert_eq!(commands(&session, &fixture).len(), steps.len());
    assert_eq!(git_state(&fixture), git);
    assert_eq!(
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        config_bytes
    );
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        app_config
    );
    assert!(!fixture.repo().join(".git/refs/ghtui").exists());
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    owned(&mut session, &["[s Suggestions]"]);
    session.quit_and_assert_restored_terminal(mouse);
}

const FIRST_CODE: &str = "batch-first 界";
const SECOND_CODE: &str = "batch-second";

fn batch_posted() -> [Posted; 2] {
    [
        Posted {
            start: 1,
            end: 1,
            code: FIRST_CODE,
            author: (77, "native-thread-actor"),
        },
        Posted {
            start: 2,
            end: 2,
            code: SECOND_CODE,
            author: (88, "second-reviewer"),
        },
    ]
}

fn overlap_posted() -> [Posted; 2] {
    [
        Posted {
            start: 1,
            end: 2,
            code: "overlap-wide",
            author: (77, "native-thread-actor"),
        },
        Posted {
            start: 2,
            end: 2,
            code: "overlap-narrow",
            author: (77, "native-thread-actor"),
        },
    ]
}

/// Waits for `expected` as consecutive whole rows of the pane.
fn wait_rows(session: &mut Session, expected: &[String], what: &str) {
    session.wait_for_cursorless_condition(
        |frame| {
            let screen = frame.contents();
            let rows: Vec<&str> = screen
                .lines()
                .map(|line| line.trim().trim_matches('│').trim())
                .collect();
            rows.windows(expected.len())
                .any(|window| window.iter().zip(expected).all(|(row, want)| row == want))
        },
        what,
        Duration::from_secs(20),
    );
}

/// Chooses one suggestion, then `A All` completes the batch: with only part
/// of the batch chosen, All chooses every admissible suggestion.
fn choose_all(session: &mut Session, mouse: bool) {
    if mouse {
        let (column, row) = session.locate("[ ] ").unwrap();
        session.click(column, row);
    } else {
        session.send_key(b" ");
    }
    session.wait_for_cursorless_condition(
        |frame| frame.contents().matches("[x] ").count() == 1,
        "Exactly one suggestion chosen",
        Duration::from_secs(20),
    );
    choose(session, mouse, "[A All]", b"A");
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            text.matches("[x] ").count() == 2 && !text.contains("[ ] ")
        },
        "A All chose both suggestions",
        Duration::from_secs(20),
    );
}

fn open_chooser(session: &mut Session, fixture: &Fixture, mouse: bool, total: usize) {
    files(session, fixture, mouse);
    choose(session, mouse, "[T Threads]", b"T");
    session.wait_for_command_count(fixture, total);
    owned(session, &["[s Suggestions]", "NATIVE_THREAD_9"]);
    choose(session, mouse, "[s Suggestions]", b"s");
    pane(
        session,
        &["2 posted suggestion(s), 2 can be applied"],
        "Chooser lists both posted suggestions",
    );
}

/// Two posted suggestions (two authors, two ranges of one file) are chosen
/// as one batch, previewed as one commit with every range and its resulting
/// diff, confirmed, and pushed to the PR head of a LOCAL bare remote.
fn batch_journey(mouse: bool) {
    let posted = batch_posted();
    let Setup {
        fixture,
        h,
        bare,
        steps,
    } = setup_with(|h| batch_page(h, &posted));
    let git = git_state(&fixture);
    let config_bytes = fs::read(fixture.repo().join(".git/config")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_chooser(&mut session, &fixture, mouse, steps.len());
    let screen = session.screen();
    for expected in [
        format!("{PATH} lines 1–1 → +{FIRST_CODE}"),
        format!("{PATH} lines 2–2 → +{SECOND_CODE}"),
        "second-reviewer".to_owned(),
    ] {
        assert!(
            screen.contains(&expected),
            "{expected:?} missing:\n{screen}"
        );
    }
    choose_all(&mut session, mouse);
    choose(&mut session, mouse, "[p Plan]", b"p");
    pane(
        &mut session,
        &["Planned commit", "nothing written yet"],
        "Batch plan shows one commit with every range",
    );
    // The resulting diff lists each range with its old and new lines, in
    // file order, as consecutive plan rows.
    let diff = [
        format!("@@ {PATH} lines 1–1"),
        "-context".to_owned(),
        format!("+{FIRST_CODE}"),
        format!("@@ {PATH} lines 2–2"),
        "-new-no-newline".to_owned(),
        format!("+{SECOND_CODE}"),
    ];
    // At 80×24 the plan is taller than the pane: PgDn (key or click) shows
    // its tail, the second range, without losing the controls.
    if !mouse {
        session.resize_owned_header(80, 24, PANE, &["[C Commit & push]", "[Esc Back]"]);
    }
    choose(&mut session, mouse, "[PgDn]", b"\x1b[6~");
    wait_rows(
        &mut session,
        &diff[3..],
        "PgDn shows the second range at 80×24",
    );
    session.resize_owned_header(120, 40, PANE, &["[C Commit & push]", "[Esc Back]"]);
    wait_rows(&mut session, &diff, "Whole batch diff at 120×40");
    let screen = session.screen();
    // One file row for the one changed file; one credit row per author.
    assert_eq!(screen.matches(" → blob ").count(), 1, "{screen}");
    for expected in [
        format!("Parent {} (PR head)", h.h),
        format!("Push to contributor {}", bare.display()),
        "Message: Apply suggestions from code review".to_owned(),
        "Credit: Co-authored-by: native-thread-actor <77+native-thread-actor@users.noreply.github.com>".to_owned(),
        "Credit: Co-authored-by: second-reviewer <88+second-reviewer@users.noreply.github.com>".to_owned(),
    ] {
        assert!(screen.contains(&expected), "{expected:?} missing:\n{screen}");
    }
    let commit = screen
        .split("Plan commit ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap()
        .to_owned();
    assert_eq!(commit.len(), 40, "{screen}");
    let blob = screen
        .split(&format!("File {PATH} → blob "))
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap()
        .to_owned();
    assert_eq!(
        bare_git(&bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]),
        h.h,
        "planning pushed nothing"
    );
    assert!(attempts(&fixture).is_empty());
    choose(&mut session, mouse, "[C Commit & push]", b"C");
    pane(
        &mut session,
        &["[y Confirm push]", &format!("to {BRANCH} on contributor")],
        "Batch push confirmation",
    );
    choose(&mut session, mouse, "[y Confirm push]", b"y");
    pane(
        &mut session,
        &[&format!("Pushed {commit} to {BRANCH}"), "confirmed"],
        "Pushed exactly the previewed batch commit",
    );
    // The bare remote independently holds ONE commit on the old head with
    // both ranges replaced, CRLF and the missing final newline kept.
    assert_eq!(
        bare_git(&bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]),
        commit
    );
    assert_eq!(bare_git(&bare, &["rev-parse", &format!("{commit}^")]), h.h);
    assert_eq!(
        bare_git(&bare, &["rev-parse", &format!("{commit}:{PATH}")]),
        blob
    );
    let bytes = std::process::Command::new("git")
        .args(["cat-file", "blob", &blob])
        .current_dir(&bare)
        .output()
        .unwrap()
        .stdout;
    assert_eq!(bytes, format!("{FIRST_CODE}\r\n{SECOND_CODE}").into_bytes());
    assert_eq!(
        bare_git(
            &bare,
            &[
                "-c",
                "core.quotepath=false",
                "diff-tree",
                "--no-commit-id",
                "--name-only",
                "-r",
                &commit
            ]
        ),
        PATH,
        "only the suggested file changed"
    );
    let message = bare_git(&bare, &["log", "-1", "--format=%B", &commit]);
    assert!(
        message.starts_with("Apply suggestions from code review"),
        "{message}"
    );
    assert!(message.contains(
        "Co-authored-by: native-thread-actor <77+native-thread-actor@users.noreply.github.com>"
    ));
    assert!(message
        .contains("Co-authored-by: second-reviewer <88+second-reviewer@users.noreply.github.com>"));
    let rows = attempts(&fixture);
    assert_eq!(rows.len(), 1, "one journaled write for the whole batch");
    assert_eq!(
        (rows[0].0.as_str(), rows[0].1.as_str(), rows[0].2.as_str()),
        (
            "suggestion-apply-v23",
            "NATIVE_COMMENT_9+NATIVE_COMMENT_10",
            "confirmed"
        )
    );
    let effect: Value = serde_json::from_str(rows[0].3.as_deref().unwrap()).unwrap();
    assert_eq!(effect["kind"], "branch_content");
    assert_eq!(effect["resource_id"], commit.as_str());
    assert_eq!(
        commands(&session, &fixture).len(),
        steps.len(),
        "no gh call"
    );
    assert_eq!(git_state(&fixture), git);
    assert_eq!(
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        config_bytes
    );
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    owned(&mut session, &["[s Suggestions]"]);
    session.quit_and_assert_restored_terminal(mouse);
}

/// Two posted suggestions whose ranges overlap are refused as a batch while
/// planning: the explanation names both ranges, no plan exists, Commit &
/// push stays disabled, and nothing reaches the remote or the journal.
fn overlap_journey(mouse: bool) {
    let posted = overlap_posted();
    let Setup {
        fixture,
        h,
        bare,
        steps,
    } = setup_with(|h| batch_page(h, &posted));
    let git = git_state(&fixture);
    let mut session = launch(&fixture, mouse);
    open_chooser(&mut session, &fixture, mouse, steps.len());
    let screen = session.screen();
    assert!(
        screen.contains(&format!("{PATH} lines 1–2 → +overlap-wide")),
        "{screen}"
    );
    assert!(
        screen.contains(&format!("{PATH} lines 2–2 → +overlap-narrow")),
        "{screen}"
    );
    choose_all(&mut session, mouse);
    choose(&mut session, mouse, "[p Plan]", b"p");
    pane(
        &mut session,
        &[
            &format!("Suggestions on {PATH}"),
            "lines 1–2 and 2–2 overlap.",
            "Apply them separately; nothing was written.",
        ],
        "Overlapping batch refused with both ranges named",
    );
    assert!(!session.screen().contains("Plan commit "));
    choose(&mut session, mouse, "[C Commit & push]", b"C");
    pane(
        &mut session,
        &["Commit & push unavailable • p Plan first"],
        "No push without a plan",
    );
    assert!(!session.screen().contains("[y Confirm push]"));
    assert_eq!(
        bare_git(&bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]),
        h.h,
        "nothing pushed"
    );
    assert!(attempts(&fixture).is_empty(), "nothing journaled");
    assert_eq!(
        commands(&session, &fixture).len(),
        steps.len(),
        "no gh call"
    );
    assert_eq!(git_state(&fixture), git);
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    owned(&mut session, &["[s Suggestions]"]);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn suggestions_batch_keyboard_all_plan_confirm_and_push_one_commit() {
    batch_journey(false)
}
#[test]
fn suggestions_batch_mouse_all_plan_confirm_and_push_one_commit() {
    batch_journey(true)
}
#[test]
fn suggestions_batch_keyboard_overlap_refused_nothing_written() {
    overlap_journey(false)
}
#[test]
fn suggestions_batch_mouse_overlap_refused_nothing_written() {
    overlap_journey(true)
}

#[test]
fn suggestions_keyboard_plan_confirm_and_push_exact_commit_to_local_bare_remote() {
    journey(false, false)
}
#[test]
fn suggestions_mouse_plan_confirm_and_push_exact_commit_to_local_bare_remote() {
    journey(true, false)
}
#[test]
fn suggestions_keyboard_moved_head_after_plan_pushes_nothing() {
    journey(false, true)
}
#[test]
fn suggestions_mouse_moved_head_after_plan_pushes_nothing() {
    journey(true, true)
}
