//! DECLARED/UNRUN. Real binary, original strict source/PR/Files producer,
//! actual key/click byte paths; independent SQL/config/Git/TTY oracles.
use super::*;
#[path = "file_comments_v75.rs"]
pub(crate) mod file_comments_v75;
#[path = "review_thread_publication_v1.rs"]
mod publication_v1;
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession};
use ghtui::providers::gh::METRICS_REVIEW_THREADS_QUERY;
const THREAD_BODY: &str = "PRIVATE_NATIVE_THREAD_BODY 界";
const THREAD_TITLE: &str = "PR review threads • read only";
fn graph_repo() -> Value {
    json!({"id":"REPO_catalogue_10000","databaseId":10000,"nameWithOwner":FIRST})
}
fn graph_parent() -> Value {
    json!({"__typename":"PullRequest","id":"GENUINE_CANONICAL_PR_NODE_1007","fullDatabaseId":"1007","number":7,"headRefOid":"a".repeat(40),"repository":graph_repo()})
}
fn page() -> Value {
    let mut parent = graph_parent();
    let comment = json!({"id":"NATIVE_COMMENT_9","fullDatabaseId":"9009","body":THREAD_BODY,"createdAt":"2026-10-01T00:00:00Z","updatedAt":"2026-10-02T00:00:00Z","path":"src/old.rs","line":null,"startLine":null,"originalLine":7,"originalStartLine":6,"commit":{"oid":"c".repeat(40)},"originalCommit":{"oid":"d".repeat(40)},"replyTo":null,"author":{"__typename":"User","databaseId":77,"login":"native-thread-actor","name":"Thread actor","avatarUrl":null},"pullRequest":graph_parent(),"repository":graph_repo()});
    let comments =
        json!({"totalCount":1,"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[comment]});
    let thread = json!({"id":"NATIVE_THREAD_9","subjectType":"LINE","isResolved":false,"isOutdated":true,"path":"src/界.rs","diffSide":"LEFT","startDiffSide":"LEFT","line":null,"startLine":null,"originalLine":7,"originalStartLine":6,"viewerCanReply":false,"viewerCanResolve":false,"viewerCanUnresolve":false,"resolvedBy":null,"pullRequest":graph_parent(),"repository":graph_repo(),"comments":comments});
    parent["reviewThreads"] =
        json!({"totalCount":1,"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[thread]});
    json!({"data":{"node":parent}})
}
fn phase() -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(get("user", json!({"id":42,"login":"octocat"})));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    let mut gql = GhStep::read(
        &[
            "api",
            "graphql",
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &page().to_string(),
    );
    gql.stdin=serde_json::to_vec(&json!({"query":METRICS_REVIEW_THREADS_QUERY,"variables":{"id":"GENUINE_CANONICAL_PR_NODE_1007","first":25,"after":null,"commentsFirst":20,"includeComments":true}})).unwrap();
    steps.push(gql);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(get("user", json!({"id":42,"login":"octocat"})));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 17);
    steps
}
fn owned(session: &mut Session, words: &[&str]) {
    session.wait_for_cursorless_condition(
        |frame| {
            let body = crate::support::overlay_body(frame, THREAD_TITLE);
            body.is_some_and(|body| {
                body.contains("[Esc Back]")
                    && body.contains("[q Quit]")
                    && words.iter().all(|word| body.contains(word))
            })
        },
        "Actual complete owned thread controls/body",
        Duration::from_secs(8),
    );
}
fn exact_threads(session: &Session, fixture: &Fixture, steps: &[GhStep]) {
    let actual = commands(session, fixture);
    assert_eq!(actual.len(), steps.len());
    for (row, step) in actual.iter().zip(steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        if step.args.iter().any(|arg| arg == "POST") {
            assert_eq!(step.args[1], "graphql");
            let body: Value = serde_json::from_slice(&step.stdin).unwrap();
            assert!(body["query"].as_str().unwrap().starts_with("query("));
            assert!(!body["query"].as_str().unwrap().contains("mutation"));
        }
        assert!(!step
            .args
            .iter()
            .any(|arg| matches!(arg.as_str(), "PUT" | "PATCH" | "DELETE")));
    }
    no_attempts(fixture);
}
fn files(session: &mut Session, fixture: &Fixture, mouse: bool) {
    selected(session, fixture, mouse);
    choose(session, mouse, "[f Files]", b"f");
    session.wait_for_command_count(fixture, 41);
    ready(session, &["src/界.rs", "[T Threads]"]);
    choose(session, mouse, "[N Next file]", b"N");
    ready(session, &["> File src/界.rs"]);
}
fn back(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            !text.contains(THREAD_TITLE)
                && !text.contains(THREAD_BODY)
                && crate::support::controls::control_painted(frame, "[T Threads]")
                && text.contains("> File src/界.rs")
        },
        "Actual completed original Files restored and thread private body erased",
        Duration::from_secs(8),
    );
}
fn wait_original_child_gone(
    session: &mut Session,
    child: &crate::support::owned_pty_processes_v1::ProcessIdentity,
) {
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut last_error = None;
    loop {
        match process_identity(child.pid) {
            Ok(None) => return,
            Ok(Some(now))
                if (now.birth, now.uid, now.session) != (child.birth, child.uid, child.session) =>
            {
                return;
            }
            Ok(Some(_)) => {}
            // Kernel identity can be torn down while the owned process exits.
            // Unknown is NOT absence: keep observing until a definite result.
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                last_error = Some(error.to_string());
            }
            Err(error) => panic!("Exact child identity observation failed: {error}"),
        }
        assert!(
            Instant::now() < deadline,
            "Exact original PID/UID/birth/session absence not proven: {last_error:?}"
        );
        session.poll_output(Duration::from_millis(1));
    }
}
fn happy(mouse: bool, partial: bool) {
    let mut operation = phase();
    if partial {
        // Genuine bounded inner content exclusion, not an accepted empty page.
        let mut value = page();
        value["data"]["node"]["reviewThreads"]["nodes"][0]["comments"]["nodes"][0]["body"] =
            json!("x".repeat(128 * 1024 + 1));
        operation[9].stdout = value.to_string().into_bytes();
    }
    let mut steps = initial();
    steps.extend(resource_operation(false));
    steps.extend(operation);
    let fixture = resource_configured(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files(&mut session, &fixture, mouse);
    let before = sql_state(&fixture);
    choose(&mut session, mouse, "[T Threads]", b"T");
    session.wait_for_command_count(&fixture, 58);
    owned(&mut session, &["OUTDATED", "UNRESOLVED", "NATIVE_THREAD_9"]);
    choose(&mut session, mouse, "[a All/file]", b"a");
    owned(&mut session, &["Filter: ALL PR", "NATIVE_THREAD_9"]);
    choose(&mut session, mouse, "[a All/file]", b"a");
    owned(&mut session, &["Filter: SELECTED FILE"]);
    if partial {
        assert!(!session.screen().contains(THREAD_BODY));
    } else {
        owned(&mut session, &[THREAD_BODY, "@native-thread-actor"]);
        choose(&mut session, mouse, "[Enter Expand]", b"\r");
        owned(&mut session, &["Collapsed"]);
        assert!(!session.screen().contains(THREAD_BODY));
        choose(&mut session, mouse, "[Enter Expand]", b"\r");
        owned(&mut session, &[THREAD_BODY]);
    }
    choose(&mut session, mouse, "[i Source]", b"i");
    owned(
        &mut session,
        &["numeric account Some(42)", "Original PR #7"],
    );
    session.resize_owned_header(
        if mouse { 120 } else { 80 },
        if mouse { 40 } else { 24 },
        THREAD_TITLE,
        &["[Esc Back]", "[q Quit]"],
    );
    owned(&mut session, &["Original head"]);
    choose(&mut session, mouse, "[Right]", b"\x1b[C");
    choose(&mut session, mouse, "[Home]", b"\x1b[H");
    choose(&mut session, mouse, "[i Source]", b"i");
    back(&mut session, mouse);
    exact_threads(&session, &fixture, &steps);
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_read_only_git(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
}
fn loss(mouse: bool) {
    let mut steps = initial();
    steps.extend(resource_operation(false));
    steps.extend(phase());
    steps.extend(principal(43));
    let fixture = resource_configured(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files(&mut session, &fixture, mouse);
    let before = sql_state(&fixture);
    choose(&mut session, mouse, "[T Threads]", b"T");
    session.wait_for_command_count(&fixture, 58);
    owned(&mut session, &[THREAD_BODY]);
    back(&mut session, mouse);
    choose(&mut session, mouse, "[T Threads]", b"T");
    session.wait_for_command_count(&fixture, 60);
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            text.contains("Original thread source unavailable")
                && !text.contains(THREAD_BODY)
                && !text.contains("native-thread-actor")
                && !text.contains(DETAIL_BODY)
        },
        "Actual private thread/PR/actor suppression after reused-login UID43",
        Duration::from_secs(8),
    );
    exact_threads(&session, &fixture, &steps);
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
}
fn cancel(mouse: bool) {
    let mut operation = phase();
    operation[9].delay_ms = 6000;
    operation.truncate(10);
    let mut steps = initial();
    steps.extend(resource_operation(false));
    steps.extend(operation);
    let fixture = resource_configured(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files(&mut session, &fixture, mouse);
    let before = sql_state(&fixture);
    choose(&mut session, mouse, "[T Threads]", b"T");
    session.wait_for_command_count(&fixture, 51);
    let pid = commands(&session, &fixture).last().unwrap()["pid"]
        .as_i64()
        .unwrap() as i32;
    let owned_session = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    let child = process_identity(pid)
        .unwrap()
        .expect("Actual six-second original child is live before Cancel");
    assert!(owned_session.owns(&child));
    assert!(!child.zombie);
    owned(&mut session, &["Loading original threads"]);
    back(&mut session, mouse);
    wait_original_child_gone(&mut session, &child);
    assert!(!session.screen().contains(THREAD_BODY));
    exact_threads(&session, &fixture, &steps);
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
}
fn attribution(mouse: bool) {
    let mut first = phase();
    let mut resolved = page();
    let thread = &mut resolved["data"]["node"]["reviewThreads"]["nodes"][0];
    thread["isResolved"] = json!(true);
    thread["resolvedBy"] = json!({"__typename":"User","databaseId":88,"login":"native-resolver","name":"Actual Resolver","avatarUrl":null});
    first[9].stdout = resolved.to_string().into_bytes();
    let mut second = phase();
    let mut unavailable = page();
    let thread = &mut unavailable["data"]["node"]["reviewThreads"]["nodes"][0];
    thread["isResolved"] = json!(true);
    thread["resolvedBy"] = Value::Null;
    thread["comments"]["nodes"][0]["createdAt"] = Value::Null;
    thread["comments"]["nodes"][0]["updatedAt"] = json!("invalid-timestamp");
    second[9].stdout = unavailable.to_string().into_bytes();
    let mut steps = initial();
    steps.extend(resource_operation(false));
    steps.extend(first);
    steps.extend(second);
    let fixture = resource_configured(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files(&mut session, &fixture, mouse);
    let before = sql_state(&fixture);
    choose(&mut session, mouse, "[T Threads]", b"T");
    session.wait_for_command_count(&fixture, 58);
    owned(&mut session, &["RESOLVED", "Resolver: @native-resolver"]);
    session.resize_owned_header(120, 40, THREAD_TITLE, &["[Esc Back]", "[q Quit]"]);
    owned(
        &mut session,
        &[
            "[AR]",
            "Resolver: @native-resolver",
            "@native-thread-actor",
            "Created (source UTC): 2026-10-01T00:00:00Z",
            "Updated (source UTC): 2026-10-02T00:00:00Z",
            THREAD_BODY,
        ],
    );
    back(&mut session, mouse);
    choose(&mut session, mouse, "[T Threads]", b"T");
    session.wait_for_command_count(&fixture, 75);
    owned(
        &mut session,
        &[
            "[?]",
            "Resolver: unavailable",
            "Created (source UTC): unavailable",
            "Updated (source UTC): unavailable",
            "@native-thread-actor",
            THREAD_BODY,
        ],
    );
    assert!(!session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .contains("@native-resolver"));
    assert!(!session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .contains("2026-10-01T00:00:00Z"));
    assert!(!session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .contains("2026-10-02T00:00:00Z"));
    back(&mut session, mouse);
    exact_threads(&session, &fixture, &steps);
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_read_only_git(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn review_threads_v1_keyboard_actual_resolver_timestamps_and_unavailable_are_distinct_from_author()
{
    attribution(false);
}
#[test]
fn review_threads_v1_mouse_actual_resolver_timestamps_and_unavailable_are_distinct_from_author() {
    attribution(true);
}
#[test]
fn review_threads_v1_keyboard_original_outdated_browse_expand_source_resize_back_read_only() {
    happy(false, false);
}
#[test]
fn review_threads_v1_mouse_original_outdated_browse_expand_source_resize_back_read_only() {
    happy(true, false);
}
#[test]
fn review_threads_v1_keyboard_partial_content_is_not_empty_or_old_cached_body() {
    happy(false, true);
}
#[test]
fn review_threads_v1_mouse_partial_content_is_not_empty_or_old_cached_body() {
    happy(true, true);
}
#[test]
fn review_threads_v1_keyboard_actual_private_body_then_uid_loss_hides_all_original_content() {
    loss(false);
}
#[test]
fn review_threads_v1_mouse_actual_private_body_then_uid_loss_hides_all_original_content() {
    loss(true);
}
#[test]
fn review_threads_v1_keyboard_actual_owned_live_child_cancel_restores_original_files_no_late_result(
) {
    cancel(false);
}
#[test]
fn review_threads_v1_mouse_actual_owned_live_child_cancel_restores_original_files_no_late_result() {
    cancel(true);
}
