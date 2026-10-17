//! SOURCE-ONLY future child of scoped_pr_read_v1: actual key/mouse resource
//! journeys; registration/product controls remain Root review/integration work.
use super::*;
#[path = "repository_catalogue_scoped_pr_immutable_images_v1.rs"]
mod immutable_images;
#[path = "pr_conversation_v17.rs"]
pub(crate) mod pr_conversation_v17;
#[path = "review_threads_v1.rs"]
pub(crate) mod review_threads_v1;
#[path = "repository_catalogue_scoped_pr_checks_v1.rs"]
mod scoped_checks;

fn parent_resource() -> Value {
    let mut raw = summary();
    raw["url"] = json!(format!("https://{HOST}/api/v3/repos/{FIRST}/pulls/7"));
    raw
}
fn resource_operation(checks: bool) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    if checks {
        steps.push(get(&format!("repos/{FIRST}/commits/{}/check-runs?filter=latest&per_page=100&page=1","a".repeat(40)),json!({"total_count":1,"check_runs":[{"id":600,"node_id":"CHECK_600","name":"Exact observed build","head_sha":"a".repeat(40),"status":"completed","conclusion":"failure","check_suite":{"id":900},"user":{"id":70,"login":"ci-actor","type":"User"}}]})));
        steps.push(get(&format!("repos/{FIRST}/commits/{}/status?per_page=100&page=1","a".repeat(40)),json!({"sha":"a".repeat(40),"total_count":1,"repository":{"id":10000,"node_id":"REPO_catalogue_10000","full_name":FIRST},"statuses":[{"id":700,"node_id":"STATUS_700","context":"external-lint","state":"pending","creator":{"id":71,"login":"status-actor","type":"User"}}]})));
    } else {
        steps.push(get(&format!("repos/{FIRST}/pulls/7/files?per_page=100&page=1"),json!([{"filename":"src/界.rs","status":"modified","additions":2,"deletions":1,"patch":"@@ -1,2 +1,3 @@\n context\n-old\n+new\n+next"}])));
        steps.push(GhStep::read(&["pr","diff","7","--repo",&format!("{HOST}/{FIRST}"),"--color","never"],"diff --git a/src/界.rs b/src/界.rs\n--- a/src/界.rs\n+++ b/src/界.rs\n@@ -1,2 +1,3 @@\n context\n-old\n+new\n+next\n"));
    }
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 10);
    steps
}
fn initial() -> Vec<GhStep> {
    let mut steps = collection(vec![json!([repository(0)])], 42);
    steps.extend(open_inbox());
    steps.extend(open_pr());
    assert_eq!(steps.len(), 31);
    steps
}
fn selected(session: &mut Session, fixture: &Fixture, mouse: bool) {
    open_catalogue(session, fixture, mouse, 0);
    discover(session, mouse);
    session.wait_for_command_count(fixture, 7);
    ready(session, &[TITLE, FIRST]);
    if !mouse {
        session.send_key(b"\t");
    }
    choose(session, mouse, "[Open My PRs]", b"\r");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_command_count(fixture, 18);
    ready(session, &[ROW_TITLE]);
    choose(session, mouse, "1 Inbox", b"1");
    choose(session, mouse, ROW_TITLE, b"\r");
    session.wait_for_command_count(fixture, 31);
    ready(session, &[DETAIL_TITLE, DETAIL_BODY, "[b Back]"]);
}
fn resource_configured(steps: &[GhStep]) -> Fixture {
    let fixture = configured(steps);
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&database(&fixture)).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&database(&fixture)).unwrap());
    let cache = Cache::open(&database(&fixture)).unwrap();
    cache
        .put_draft(
            &Scope::github(HOST, Some(42)).unwrap(),
            "resource-private-original",
            PRIVATE_DRAFT,
        )
        .unwrap();
    fixture
}
fn unchanged(session: &Session, fixture: &Fixture, before: &[String], steps: &[GhStep]) {
    assert_eq!(sql_state(fixture), before);
    no_attempts(fixture);
    exact_reads(session, fixture, steps);
    assert_read_only_git(fixture);
}
fn visible_words(screen: &str) -> String {
    screen
        .lines()
        .map(|line| line.trim().trim_matches('│').trim())
        .collect::<Vec<_>>()
        .join(" ")
}
fn happy(mouse: bool) {
    let (fixture, history, mut steps) = immutable_images::fixture_and_steps(|_| {});
    let mut checks = resource_operation(true);
    immutable_images::bind_steps(&mut checks, &history);
    steps.extend(checks);
    let mut bridge = scoped_checks::bridge_for_head(&history.h);
    immutable_images::bind_steps(&mut bridge, &history);
    steps.extend(bridge);
    let mut logs = scoped_checks::logs_for_head(&history.h);
    immutable_images::bind_steps(&mut logs, &history);
    steps.extend(logs);
    assert_eq!(steps.len(), 101);
    fixture.scripted_gh(&steps);
    let git_before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    selected(&mut session, &fixture, mouse);
    let before = sql_state(&fixture);
    let git_journal = git_commands(&fixture);
    choose(&mut session, mouse, "[f Files]", b"f");
    session.wait_for_command_count(&fixture, 41);
    ready(
        &mut session,
        &["Old side: current base-branch tip", "src/界.rs", "[b Back]"],
    );
    assert!(!session.screen().contains(DETAIL_BODY));
    assert!(!session.screen().contains(PRIVATE_DRAFT));
    // New controls dispatch one typed read-only action by both inputs. No more
    // gh/SQL work for loaded patch file/hunk/context/layout/range navigation.
    choose(&mut session, mouse, "[N Next file]", b"N");
    // Mere file/patch text was already visible before either worker. Require
    // the actual selected target after the page-only completion before issuing
    // another action; preparing pages intentionally suppress loaded controls.
    ready(&mut session, &["> File src/界.rs"]);
    choose(&mut session, mouse, "[} Next hunk]", b"}");
    ready(&mut session, &["> @@ -1,2 +1,2", "new-no-newline"]);
    choose(&mut session, mouse, "[X Patch context]", b"X");
    overlay_ready(
        &mut session,
        "Loaded patch context",
        &["old Some", "new Some", "[Esc Close]"],
    );
    if mouse {
        session.close_overlay_by_mouse("Loaded patch context");
    } else {
        session.close_overlay_by_key("Loaded patch context");
    }
    choose(&mut session, mouse, "[L Layout]", b"L");
    session
        .try_wait_for_condition(
            |screen| {
                screen
                    .lines()
                    .any(|line| line.matches("context\\r").count() == 2 && line.contains('│'))
                    && screen.contains("[L Layout]")
                    && !screen.contains("Preparing observed PR section")
            },
            "Actual split diff independently displays the same canonical context on both sides",
            Duration::from_secs(8),
        )
        .unwrap();
    choose(&mut session, mouse, "[O Old image]", b"O");
    session.wait_for_command_count(&fixture, 58);
    overlay_ready(
        &mut session,
        "Immutable file images",
        &[
            "OLD (merge-base M)",
            "NEW (review head H)",
            "old-no-newline",
            "new-no-newline",
            "[Esc Close]",
        ],
    );
    assert!(session.screen().contains(&history.m));
    assert!(session.screen().contains(&history.h));
    assert_ne!(history.m, history.b);
    if mouse {
        session.close_overlay_by_mouse("Immutable file images");
    } else {
        session.close_overlay_by_key("Immutable file images");
    }
    session.resize(if mouse { 80 } else { 120 }, if mouse { 24 } else { 40 });
    ready(&mut session, &["[f Files]", "[k Checks]", "[b Back]"]);
    assert_eq!(commands(&session, &fixture).len(), 58);
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(git_state(&fixture), git_before);
    choose(&mut session, mouse, "[k Checks]", b"k");
    session.wait_for_command_count(&fixture, 68);
    ready(
        &mut session,
        &[
            "Required rules unavailable",
            "Exact observed build",
            "ci-actor",
            "external-lint",
        ],
    );
    assert!(!session.screen().contains(DETAIL_BODY));
    assert!(!session.screen().contains(PRIVATE_DRAFT));
    scoped_checks::select_observed_row(&mut session, mouse, "Exact observed build");
    choose(&mut session, mouse, "[Enter Open check]", b"\r");
    session.wait_for_command_count(&fixture, 86);
    ready(
        &mut session,
        &["Run 123 • attempt 2", "Job 501", &history.h],
    );
    assert!(!session.screen().contains(DETAIL_BODY));
    assert!(!session.screen().contains(PRIVATE_DRAFT));
    scoped_checks::control(&mut session, mouse, "[l Logs]", b"l");
    session.wait_for_command_count(&fixture, 101);
    ready(&mut session, &["EXACT_MATCHED_JOB_LOG_CANARY"]);
    assert_eq!(sql_state(&fixture), before);
    scoped_checks::control(&mut session, mouse, "[b Back to PR checks]", b"b");
    ready(
        &mut session,
        &[
            "Exact observed build",
            "external-lint",
            "[Enter Open check]",
        ],
    );
    assert!(!session.screen().contains("EXACT_MATCHED_JOB_LOG_CANARY"));
    // An actual status-context member remains non-bridgeable even after the
    // separate genuine CheckRun successfully opened its exact run and logs.
    scoped_checks::select_observed_row(&mut session, mouse, "external-lint");
    choose(&mut session, mouse, "[Enter Open check]", b"\r");
    overlay_ready(&mut session, "Action unavailable", &["[Esc Close]"]);
    // Assert the entire genuine status-context explanation across physical
    // terminal wrapping, not only a coincidentally unbroken screen substring.
    let explanation = session
        .wait_for_overlay_body("Action unavailable")
        .split_whitespace()
        .collect::<String>();
    assert!(
        explanation.contains("CommitstatuscontextsdonotidentifyanActionsrun"),
        "Actual owned status-context refusal, including words split by terminal wrapping: {explanation}"
    );
    assert_eq!(commands(&session, &fixture).len(), 101);
    assert_eq!(sql_state(&fixture), before);
    if mouse {
        session.close_overlay_by_mouse("Action unavailable");
    } else {
        session.close_overlay_by_key("Action unavailable");
    }
    if mouse {
        let (column, row) = session.locate("Exact observed build").unwrap();
        session.wheel(column, row, true);
    } else {
        session.send_key(b"\x1b[6~");
    }
    ready(&mut session, &["not an Actions run"]);
    assert_eq!(commands(&session, &fixture).len(), 101);
    choose(&mut session, mouse, "[b Back]", b"b");
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains(ROW_TITLE)
                    && screen.contains("Cached • fetched")
                    && !screen.contains("Exact observed build")
                    && !screen.contains(DETAIL_TITLE)
            },
            "Pure resource Back erases private section without source reload",
            Duration::from_secs(8),
        )
        .unwrap();
    unchanged(&session, &fixture, &before, &steps);
    assert_eq!(git_state(&fixture), git_before);
    assert_eq!(git_commands(&fixture), git_journal);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
    exact_reads(&session, &fixture, &steps);
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(git_state(&fixture), git_before);
}
fn loss(mouse: bool) {
    for drift in [false, true] {
        let mut steps = initial();
        let mut files = resource_operation(false);
        if drift {
            steps.extend(files);
            let mut checks = resource_operation(true);
            checks[9] = get("user", json!({"id":43,"login":"octocat"}));
            steps.extend(checks);
            assert_eq!(steps.len(), 51);
        } else {
            let mut bad = parent_resource();
            bad["base"]["repo"]["id"] = json!(999999);
            files[3] = get(&files[3].args[1], bad);
            files.truncate(4);
            steps.extend(files);
            assert_eq!(steps.len(), 35);
        }
        let fixture = resource_configured(&steps);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = launch(&fixture, mouse);
        selected(&mut session, &fixture, mouse);
        let before = sql_state(&fixture);
        choose(&mut session, mouse, "[f Files]", b"f");
        session.wait_for_command_count(&fixture, if drift { 41 } else { 35 });
        if drift {
            ready(
                &mut session,
                &["Old side: current base-branch tip", "[k Checks]"],
            );
            choose(&mut session, mouse, "[k Checks]", b"k");
            session.wait_for_command_count(&fixture, 51);
        }
        session
            .try_wait_for_condition(
                |screen| {
                    visible_words(screen).contains("Original repository scope is fenced • private pages hidden • no fallback or query sent")
                        && !screen.contains(DETAIL_BODY)
                        && !screen.contains(DETAIL_TITLE)
                        && !screen.contains(SEARCH_BODY)
                },
                "Failed resource source is fenced; old private models never become empty/success/cached fallback",
                Duration::from_secs(8),
            )
            .unwrap();
        let gh = session.inspect_command_journal(&fixture);
        session.resize(80, 24);
        choose(&mut session, mouse, "1 Inbox", b"1");
        if mouse {
            choose(&mut session, true, "[r Refresh]", b"r");
        } else {
            session.send_key(b"r");
        }
        session
            .try_wait_for_condition(
                |screen| {
                    !screen.contains(DETAIL_BODY)
                        && !screen.contains(SEARCH_BODY)
                        && !screen.contains("complete0")
                },
                "Fenced subsequent navigation has no default Inbox fallback",
                Duration::from_secs(8),
            )
            .unwrap();
        assert_eq!(session.inspect_command_journal(&fixture), gh);
        choose(&mut session, mouse, "2 Repository", b"2");
        ready(
            &mut session,
            &["Repository • focused view", "Recent commits"],
        );
        unchanged(&session, &fixture, &before, &steps);
        assert_eq!(git_state(&fixture), git);
        assert_eq!(
            fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        session.quit_and_assert_restored_terminal(mouse);
        exact_reads(&session, &fixture, &steps);
        assert_eq!(sql_state(&fixture), before);
        assert_eq!(git_state(&fixture), git);
    }
}
#[test]
fn scoped_pr_resource_keyboard_files_checks_current_head_patch_context_cached_back() {
    happy(false);
}
#[test]
fn scoped_pr_resource_mouse_files_checks_current_head_patch_context_cached_back() {
    happy(true);
}
#[test]
fn scoped_pr_resource_keyboard_replaced_parent_and_reused_login_hide_private_no_default() {
    loss(false);
}
#[test]
fn scoped_pr_resource_mouse_replaced_parent_and_reused_login_hide_private_no_default() {
    loss(true);
}
