//! Actual Native PR -> fresh Threads -> pure Back. No synthetic Images page,
//! no parser bypass; raw Git/source/full protocol/SQL/config/TTY independent.
use super::*;
#[path = "review_thread_code_v4.rs"]
mod current_code_v4;
use ghtui::providers::gh::METRICS_REVIEW_THREADS_QUERY;
const THREADS: &str = "PR review threads • read only";
const BODY: &str = "NATIVE_CURRENT_SOURCE_THREAD_PRIVATE_BODY 界";
fn thread_phase(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut parent = parent_resource();
    h.bind_parent(&mut parent);
    let repository_node =
        json!({"id":"REPO_catalogue_10000","databaseId":10000,"nameWithOwner":FIRST});
    let pr = json!({"__typename":"PullRequest","id":"GENUINE_CANONICAL_PR_NODE_1007","fullDatabaseId":"1007","number":7,"headRefOid":h.h,"repository":repository_node});
    let comment = json!({"id":"ACTUAL_NATIVE_ENTRY_COMMENT_9","fullDatabaseId":"9009","body":BODY,
        "createdAt":"2026-10-01T00:00:00Z","updatedAt":"2026-10-02T00:00:00Z","path":history::PATH,
        "line":null,"startLine":null,"originalLine":2,"originalStartLine":null,
        "commit":{"oid":h.m},"originalCommit":{"oid":h.m},"replyTo":null,
        "author":{"__typename":"User","databaseId":77,"login":"native-thread-actor","name":"Actual native author","avatarUrl":null},
        "pullRequest":pr,"repository":repository_node});
    let comments =
        json!({"totalCount":1,"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[comment]});
    let thread = json!({"id":"ACTUAL_NATIVE_ENTRY_THREAD_9","subjectType":"LINE","isResolved":false,"isOutdated":true,
        "path":history::PATH,"diffSide":"LEFT","startDiffSide":null,"line":null,"startLine":null,"originalLine":2,"originalStartLine":null,
        "viewerCanReply":false,"viewerCanResolve":false,"viewerCanUnresolve":false,"resolvedBy":null,
        "pullRequest":pr,"repository":repository_node,"comments":comments});
    let mut graph_parent = pr;
    graph_parent["reviewThreads"] =
        json!({"totalCount":1,"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[thread]});
    let mut graph = GhStep::read(
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
        &json!({"data":{"node":graph_parent}}).to_string(),
    );
    graph.stdin=serde_json::to_vec(&json!({"query":METRICS_REVIEW_THREADS_QUERY,"variables":{"id":"GENUINE_CANONICAL_PR_NODE_1007","first":25,"after":null,"commentsFirst":20,"includeComments":true}})).unwrap();
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent.clone()));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent.clone()));
    steps.push(get("user", json!({"id":42,"login":"octocat"})));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent.clone()));
    steps.push(graph);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent.clone()));
    steps.push(get("user", json!({"id":42,"login":"octocat"})));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 17);
    steps
}
fn actual_native_body(session: &mut Session) -> String {
    session.wait_for_overlay_body("Native PR diff •")
}
fn exact_native_thread_reads(session: &Session, fixture: &Fixture, steps: &[GhStep]) {
    let rows = commands(session, fixture);
    assert_eq!(rows.len(), steps.len());
    for (ordinal, (actual, step)) in rows.iter().zip(steps).enumerate() {
        assert_eq!(actual["step"], ordinal);
        assert_eq!(actual["args"], json!(step.args));
        assert_eq!(actual["matched"], true);
        let expected = step
            .stdin
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            actual["stdin_hex"], expected,
            "Every real child stdin must equal its exact original fixture bytes"
        );
        assert!(!step
            .args
            .iter()
            .any(|arg| matches!(arg.as_str(), "PATCH" | "DELETE" | "PUT")));
        if step
            .args
            .iter()
            .any(|arg| arg == "POST" || arg == "graphql")
        {
            assert_eq!(step.args.get(1).map(String::as_str), Some("graphql"));
            let query: Value = serde_json::from_slice(&step.stdin).unwrap();
            assert_eq!(query["query"], METRICS_REVIEW_THREADS_QUERY);
            assert_eq!(
                query["variables"],
                json!({"id":"GENUINE_CANONICAL_PR_NODE_1007","first":25,"after":null,"commentsFirst":20,"includeComments":true})
            );
            assert!(query["query"].as_str().unwrap().starts_with("query("));
        } else {
            assert!(step.stdin.is_empty());
        }
    }
    assert_eq!(
        rows.iter()
            .filter(|row| row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "--paginate"))
            .count(),
        steps
            .iter()
            .filter(|step| step.args.iter().any(|arg| arg == "--paginate"))
            .count(),
        "actual bounded pagination unchanged"
    );
}
fn journey(mouse: bool) {
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    steps.extend(thread_phase(&h));
    fixture.scripted_gh(&steps);
    let git = readonly_git(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    let sql = sql_state(&fixture);
    let git_trace = git_commands(&fixture);
    choose(&mut session, mouse, "[v Native diff]", b"v");
    session.wait_for_command_count(&fixture, 58);
    nready(&mut session, "new-no-newline");
    focus(&mut session, mouse, "new-no-newline");
    act(&mut session, mouse, NATIVE, "[o Options]", b"o");
    act(&mut session, mouse, NATIVE, "[w Wrap]", b"w");
    act(&mut session, mouse, NATIVE, "[o Back to diff]", b"o");
    nready(&mut session, "new-no-newline");
    let native = actual_native_body(&mut session);
    let count = commands(&session, &fixture).len();
    assert_eq!(count, 58);
    act(&mut session, mouse, NATIVE, "[T Threads]", b"T");
    session.wait_for_command_count(&fixture, 75);
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, THREADS).is_some_and(|body| {
                body.contains("[Esc Back]")
                    && body.contains("native-thread-actor")
                    && body.contains("OUTDATED")
            })
        },
        "genuine native entry fresh completed thread source/actor/outdated observation",
        Duration::from_secs(8),
    );
    let after = session.completed_cursorless_end();
    act(&mut session, mouse, THREADS, "[Esc Back]", b"\x1b");
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            crate::support::overlay_body(frame, "Native PR diff •")
                .is_some_and(|body| body == native)
                && crate::support::overlay_body(frame, THREADS).is_none()
        },
        "pure Back retains exact original Native query/options/selection/viewport",
        Duration::from_secs(8),
    );
    assert_eq!(commands(&session, &fixture).len(), 75);
    assert_eq!(sql_state(&fixture), sql);
    assert_eq!(git_commands(&fixture), git_trace);
    assert_eq!(readonly_git(&fixture), git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_eq!(sql_state(&fixture), sql);
    no_attempts(&fixture);
    exact_native_thread_reads(&session, &fixture, &steps);
    assert_read_only_git(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn review_threads_v3_native_keyboard_original_patch_threads_observed_actor_outdated_and_exact_back()
{
    journey(false);
}
#[test]
fn review_threads_v3_native_mouse_original_patch_threads_observed_actor_outdated_and_exact_back() {
    journey(true);
}
