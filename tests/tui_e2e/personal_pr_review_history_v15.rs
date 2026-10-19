//! Real ordinary Personal Summary -> protected Review history -> restart ->
//! fresh outcome -> SEPARATE local ACK. No Files, POST, retry or old-body restore.
use super::*;
use ghtui::{
    pr_write_journal::{PrWriteIntent, PrWriteJournal, PrWriteTarget},
    providers::{gh::GithubProvider, runner::CancellationToken},
};
const TITLE: &str = "Review operation history • inspect only";
const PROTECTED: &str = "PROTECTED_PERSONAL_REVIEW_UNKNOWN_ORIGIN_BODY";
fn phase() -> Vec<GhStep> {
    let mut steps = identity("octocat");
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    steps.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.extend(identity("octocat"));
    assert_eq!(steps.len(), 8);
    steps
}
fn inspection() -> Vec<GhStep> {
    let mut steps = identity("octocat");
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    let review = json!({"id":810,"node_id":"ACTUAL_PENDING_REVIEW_810","state":"PENDING","body":"Current server review, not the original attempt", "commit_id":"a".repeat(40),"pull_request_url":format!("https://github.invalid/api/v3/repos/{REPO}/pulls/7"),"user":{"id":42,"login":"octocat","type":"User"}});
    steps.push(read(
        api(&format!("repos/{REPO}/pulls/7/reviews?per_page=100&page=1")),
        json!([review.clone()]),
    ));
    steps.push(read(
        api(&format!("repos/{REPO}/pulls/7/reviews/810")),
        review.clone(),
    ));
    steps.push(read(
        api(&format!(
            "repos/{REPO}/pulls/7/reviews/810/comments?per_page=100&page=1"
        )),
        json!([]),
    ));
    steps.push(read(
        api(&format!("repos/{REPO}/pulls/7/reviews/810")),
        review,
    ));
    steps.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.extend(identity("octocat"));
    assert_eq!(steps.len(), 12);
    steps
}
fn control(s: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    let position = s.wait_for_owned_completed_action(TITLE, label, &[]);
    if mouse {
        s.click(position.0, position.1)
    } else {
        s.send_key(key)
    }
}
fn load_summary(f: &Fixture, s: &mut Session, mouse: bool, commands: usize) {
    s.wait_for_screen("ghtui");
    let refreshed = s.completed_cursorless_end();
    refresh(s, mouse);
    // Restart restores the old queue immediately. Its unchanged PR caption
    // cannot acknowledge this NEW refresh; opening it early cancels the real
    // refresh between --version and user. Require the fresh completed result
    // and exact queue-read journal before issuing the next Open action.
    s.wait_for_new_cursorless_condition(
        refreshed,
        |screen| {
            let text = screen.contents();
            crate::support::main_footer_visible(screen)
                && text.contains("Fresh • fetched")
                && text.contains("1 unique PRs across 3 queue matches")
        },
        "Fresh original personal queue after explicit refresh, not restored cached rows",
        Duration::from_secs(8),
    );
    journal(
        s,
        f,
        commands
            .checked_sub(7)
            .expect("Seven exact selected PR detail children"),
    );
    let before = s.completed_cursorless_end();
    if mouse {
        s.click_visible_action("Actual personal source")
    } else {
        s.send_key(b"\r")
    }
    frame(s, before, |text| {
        text.contains(BODY)
            && crate::support::controls::text_has_control(text, "[F9 Review history]")
    });
    journal(s, f, commands);
}
fn history_open(s: &mut Session, mouse: bool) {
    let before = s.completed_cursorless_end();
    choose(s, mouse, "[F9 Review history]", b"\x1b[20~");
    frame(s, before, |text| {
        text.contains(TITLE)
            && text.contains("review-submit-v2")
            && text.contains("[Enter Inspect]")
    });
    let before = s.completed_cursorless_end();
    control(s, mouse, "[Enter Inspect]", b"\r");
    frame(s, before, |text| {
        text.contains(TITLE)
            && text.contains("Recorded attempt")
            && text.contains("[i Inspect current]")
    });
    assert!(!s
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .contains(PROTECTED));
}
fn generation(database: &std::path::Path) -> i64 {
    let db =
        rusqlite::Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    db.query_row("SELECT generation FROM pr_write_scopes WHERE host='github.invalid' AND principal='user-id:42' AND pr_node='PERSONAL_PR_1007'",[],|row|row.get(0)).unwrap()
}
fn exact_commands(s: &Session, f: &Fixture, steps: &[GhStep]) {
    let rows = s
        .inspect_command_journal(f)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), steps.len());
    for (row, step) in rows.iter().zip(steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["stdin_hex"], "");
        assert!(step
            .args
            .iter()
            .all(|arg| !["POST", "PATCH", "PUT", "DELETE"].contains(&arg.as_str())));
    }
}
fn journey(mouse: bool) {
    let fixture = configured();
    fixture.git(&["init", "-q"]);
    fixture.git(&["config", "user.name", "Personal history"]);
    fixture.git(&["config", "user.email", "history@example.invalid"]);
    fs::write(fixture.repo().join("kept"), b"saved\n").unwrap();
    fixture.git(&["add", "--", "kept"]);
    fixture.git(&["commit", "-qm", "Independent history"]);
    fs::write(fixture.repo().join("kept"), b"dirty raw\n").unwrap();
    fs::write(fixture.repo().join("untracked"), b"raw untracked\n").unwrap();
    let paths = [
        ".git/HEAD",
        ".git/index",
        ".git/config",
        "kept",
        "untracked",
    ];
    let git = paths.map(|path| fs::read(fixture.repo().join(path)).unwrap());
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let (mut startup, loaded) = complete_steps();
    startup.truncate(loaded);
    let mut steps = vec![read(api(&format!("repos/{REPO}/pulls/7")), parent())];
    steps.extend(startup.clone());
    let first_summary = steps.len();
    steps.extend(phase());
    steps.extend(phase());
    let first_end = steps.len();
    steps.extend(startup);
    let second_summary = steps.len();
    steps.extend(phase());
    steps.extend(phase());
    steps.extend(inspection());
    let inspected = steps.len();
    steps.extend(phase());
    fixture.scripted_gh(&steps);
    // Seed only durable test history through the actual installed transport's
    // parsed PR. No access/source/Files/approval object is manufactured.
    let api = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    let summary = api
        .pr_summary(REPO, 7, &CancellationToken::default())
        .unwrap();
    let identity = HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "cached fixture identity; not permission".into(),
    };
    let target = PrWriteTarget::new(&identity, &summary).unwrap();
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    let mut journal_db = PrWriteJournal::open(&database).unwrap();
    let intent = PrWriteIntent {
        command: "review-submit-v2".into(),
        selected_resource: "review/810".into(),
        observed_head: summary.head_sha,
        values: std::collections::BTreeMap::from([("body".into(), PROTECTED.into())]),
    };
    let permit = journal_db.begin(&target, &intent).unwrap();
    journal_db.claim(&permit, &target, &intent).unwrap();
    journal_db.mark_uncertain(&permit).unwrap();
    let reference = permit.marker().publication_reference();
    drop(journal_db);
    for uid in [42, 43] {
        Cache::open(&database)
            .unwrap()
            .put_draft(
                &Scope::github("github.invalid", Some(uid)).unwrap(),
                "protected-v2-no-origin",
                if uid == 42 {
                    PROTECTED
                } else {
                    "FOREIGN43_DRAFT"
                },
            )
            .unwrap();
    }
    let mut first = Session::launch(&fixture, 120, 40);
    load_summary(&fixture, &mut first, mouse, first_summary);
    let baseline = sql(&fixture);
    history_open(&mut first, mouse);
    journal(&mut first, &fixture, first_end);
    let before = first.completed_cursorless_end();
    first.resize_owned_single_action_modal_v2(
        40,
        14,
        "Review operation history • inspect on",
        "[Esc Back]",
    );
    frame(&mut first, before, |text| {
        text.contains("Resize to inspect")
            && text.contains("[Esc Back]")
            && !text.contains("[i Inspect current]")
    });
    assert_eq!(sql(&fixture), baseline);
    journal(&mut first, &fixture, first_end);
    let before = first.completed_cursorless_end();
    first.resize_owned_modal(120, 40, TITLE, &["[Esc Back]", "[Enter Inspect]"]);
    frame(&mut first, before, |text| {
        text.contains(TITLE)
            && text.contains("Recorded attempt")
            && text.contains("[i Inspect current]")
    });
    let before = first.completed_cursorless_end();
    control(&mut first, mouse, "[Esc Back]", b"\x1b");
    frame(&mut first, before, |text| {
        text.contains(TITLE)
            && text.contains("[Enter Inspect]")
            && !text.contains("[i Inspect current]")
    });
    let before = first.completed_cursorless_end();
    control(&mut first, mouse, "[Esc Back]", b"\x1b");
    frame(&mut first, before, |text| {
        text.contains(BODY)
            && crate::support::controls::text_has_control(text, "[F9 Review history]")
            && !text.contains(TITLE)
    });
    first.quit_and_assert_restored_terminal(mouse);
    assert_eq!(sql(&fixture), baseline);
    assert!(PrWriteJournal::open(&database)
        .unwrap()
        .blocking(&target)
        .unwrap()
        .is_some());
    let mut second = Session::launch(&fixture, 120, 40);
    load_summary(&fixture, &mut second, mouse, second_summary);
    let before_ack = sql(&fixture);
    let original_generation = generation(&database);
    let expected_sql = before_ack
        .iter()
        .map(|line| {
            if line.starts_with("pr_write_attempts:") {
                let old = format!(
                    "{:?}",
                    format!("{:?}", rusqlite::types::ValueRef::Text(b"uncertain"))
                );
                let new = format!(
                    "{:?}",
                    format!("{:?}", rusqlite::types::ValueRef::Text(b"acknowledged"))
                );
                assert!(line.contains(&old));
                line.replace(&old, &new)
            } else if line.starts_with("pr_write_scopes:") {
                let suffix = format!(
                    "{:?}]",
                    format!(
                        "{:?}",
                        rusqlite::types::ValueRef::Integer(original_generation)
                    )
                );
                assert!(line.ends_with(&suffix));
                format!(
                    "{}{:?}]",
                    &line[..line.len() - suffix.len()],
                    format!(
                        "{:?}",
                        rusqlite::types::ValueRef::Integer(original_generation + 1)
                    )
                )
            } else {
                line.clone()
            }
        })
        .collect::<Vec<_>>();
    history_open(&mut second, mouse);
    let before = second.completed_cursorless_end();
    control(&mut second, mouse, "[i Inspect current]", b"i");
    frame(&mut second, before, |text| {
        text.contains(TITLE)
            && text.contains("FRESH CURRENT OUTCOME")
            && text.contains("[a Acknowledge local]")
    });
    journal(&mut second, &fixture, inspected);
    assert_eq!(sql(&fixture), before_ack);
    let before = second.completed_cursorless_end();
    control(&mut second, mouse, "[a Acknowledge local]", b"a");
    frame(&mut second, before, |text| {
        text.contains("locally acknowledged")
            && text.contains("nothing resent")
            && !text.contains(TITLE)
            && !text.contains(PROTECTED)
    });
    journal(&mut second, &fixture, steps.len());
    assert!(PrWriteJournal::open(&database)
        .unwrap()
        .blocking(&target)
        .unwrap()
        .is_none());
    let connection = rusqlite::Connection::open_with_flags(
        &database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let actual: (String, String) = connection
        .query_row(
            "SELECT state,payload_sha256 FROM pr_write_attempts WHERE attempt_id=?1",
            [reference.attempt_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(actual, ("acknowledged".into(), reference.payload_sha256));
    drop(connection);
    for uid in [42, 43] {
        assert_eq!(
            Cache::open(&database)
                .unwrap()
                .bounded_draft(
                    &Scope::github("github.invalid", Some(uid)).unwrap(),
                    "protected-v2-no-origin",
                    128 * 1024
                )
                .unwrap()
                .as_deref(),
            Some(if uid == 42 {
                PROTECTED
            } else {
                "FOREIGN43_DRAFT"
            })
        );
    }
    assert_eq!(generation(&database), original_generation + 1);
    assert_eq!(sql(&fixture), expected_sql);
    exact_commands(&second, &fixture, &steps);
    second.quit_and_assert_restored_terminal(mouse);
    assert_eq!(sql(&fixture), expected_sql);
    for (path, expected) in paths.into_iter().zip(git) {
        assert_eq!(fs::read(fixture.repo().join(path)).unwrap(), expected)
    }
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
}
#[test]
fn personal_pr_summary_v15_keyboard_restart_protected_history_fresh_inspect_explicit_ack_no_resend()
{
    journey(false)
}
#[test]
fn personal_pr_summary_v15_mouse_restart_protected_history_fresh_inspect_explicit_ack_no_resend() {
    journey(true)
}
