//! TASK-62 Dashboard attention widgets on the actual executable: Stale PRs is
//! derived from the loaded inventory (no gh read), Unresolved threads reads the
//! loaded open PRs' review threads only on request, and every row of both opens
//! its exact PR by key and by click. 220 columns shows both controls on the key
//! bar; at 80x24 they overflow into Commands.
use super::*;
use ghtui::providers::gh::METRICS_REVIEW_THREADS_QUERY;
use rusqlite::Connection;

const THREAD_PATH: &str = "src/auth.rs:20";

fn graph_repository() -> Value {
    json!({"id":"R_dashboard_410","databaseId":410,"nameWithOwner":APP})
}
fn graph_parent() -> Value {
    json!({"__typename":"PullRequest","id":"PR_dashboard_1","fullDatabaseId":"1001","number":1,"headRefOid":HEAD,"repository":graph_repository()})
}
fn graph_actor(id: u64, login: &str) -> Value {
    json!({"__typename":"User","databaseId":id,"login":login,"name":login,"avatarUrl":null})
}
fn graph_comment(id: &str, database_id: u64, author: Value) -> Value {
    json!({"id":id,"fullDatabaseId":database_id.to_string(),"body":"Thread body stays out of the Dashboard","createdAt":"2026-10-12T00:00:00Z","updatedAt":"2026-10-12T00:00:00Z","path":"src/auth.rs","line":20,"startLine":null,"originalLine":20,"originalStartLine":null,"originalCommit":{"oid":HEAD},"commit":{"oid":HEAD},"replyTo":null,"author":author,"pullRequest":graph_parent(),"repository":graph_repository()})
}
fn graph_thread(id: &str, resolved: bool, path: &str, author: Value) -> Value {
    // Each comment keeps its own database ID; a repeated one is a contradiction.
    let mut comment = graph_comment(
        &format!("C_{id}"),
        if resolved { 9002 } else { 9001 },
        author,
    );
    comment["path"] = json!(path);
    json!({"__typename":"PullRequestReviewThread","id":id,"isResolved":resolved,"isOutdated":false,"path":path,"diffSide":"RIGHT","startDiffSide":null,"line":20,"startLine":null,"originalLine":20,"originalStartLine":null,"viewerCanReply":true,"viewerCanResolve":!resolved,"viewerCanUnresolve":resolved,"resolvedBy":if resolved {graph_actor(53,"dana")} else {Value::Null},"pullRequest":graph_parent(),"repository":graph_repository(),
        "comments":{"nodes":[comment],"totalCount":1,"pageInfo":{"hasNextPage":false,"endCursor":null}}})
}
fn threads_page() -> Value {
    let mut parent = graph_parent();
    parent["reviewThreads"] = json!({"nodes":[
        graph_thread("T_open", false, "src/auth.rs", graph_actor(52, "carol")),
        graph_thread("T_done", true, "src/done.rs", graph_actor(53, "dana")),
    ],"totalCount":2,"pageInfo":{"hasNextPage":false,"endCursor":null}});
    json!({"data":{"node":parent}})
}
/// The worker's principal bracket around one PR's bracketed thread read.
fn thread_steps() -> Vec<GhStep> {
    let user = || get("user", json!({"id":42,"login":"octocat"}));
    let mut steps = gh_fixture::account(42);
    steps.push(user());
    steps.push(get(&format!("repos/{APP}"), repository(APP)));
    steps.push(get(&format!("repos/{APP}/pulls/1"), alpha()));
    let mut graphql = GhStep::read(
        &[
            "api",
            "graphql",
            "--hostname",
            gh_fixture::HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &threads_page().to_string(),
    );
    graphql.stdin = serde_json::to_vec(&json!({"query":METRICS_REVIEW_THREADS_QUERY,"variables":{"id":"PR_dashboard_1","first":25,"after":null,"commentsFirst":20,"includeComments":true}})).unwrap();
    steps.push(graphql);
    steps.push(get(&format!("repos/{APP}"), repository(APP)));
    steps.push(get(&format!("repos/{APP}/pulls/1"), alpha()));
    steps.push(user());
    steps.extend(gh_fixture::account(42));
    assert_eq!(steps.len(), 11);
    steps
}
/// Every scripted command matched, reads are GET and only the thread query posts.
fn journal_v1(session: &mut Session, fixture: &Fixture, count: usize) {
    session.wait_for_command_count(fixture, count);
    let rows = session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    for row in &rows {
        assert_eq!(row["matched"], true);
        if row["args"][1] == "graphql" {
            assert_eq!(row["args"][5], "POST");
        } else {
            assert!(row["args"][0] == "--version" || row["args"][5] == "GET");
            assert_eq!(row["stdin_hex"], "");
        }
    }
}
fn trend_points(fixture: &Fixture) -> i64 {
    Connection::open(fixture.root.join("config/ghtui/cache.sqlite3"))
        .unwrap()
        .query_row("SELECT COUNT(*) FROM metric_trend_points_v1", [], |row| {
            row.get(0)
        })
        .unwrap()
}
fn pr_opened(session: &mut Session) {
    session.wait_for_screen("Inbox • focused view");
    session.wait_for_screen(BODY);
    session.wait_for_screen("@alice");
}

#[test]
fn dashboard_widgets_keyboard_stale_prs_and_unresolved_threads_open_exact_prs() {
    widgets(false);
}
#[test]
fn dashboard_widgets_mouse_stale_prs_and_unresolved_threads_open_exact_prs() {
    widgets(true);
}
fn widgets(mouse: bool) {
    let fixture = configured(false);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    // The inventory's fixed updated_at (2026-10-13) is more than 7 days old.
    let mut steps = refresh(&[APP]);
    steps.extend(open_steps());
    steps.extend(thread_steps());
    steps.extend(open_steps());
    assert_eq!(steps.len(), 52);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 220, 42);
    start(&mut session, mouse);
    journal_v1(&mut session, &fixture, 7);
    cards_ready(&mut session, 1, 1);
    let points = trend_points(&fixture);
    assert_eq!(points, 8, "one retained point per card from the load");

    // Stale PRs: derived from the loaded inventory, no gh read.
    session.wait_for_control("[z Stale PRs]");
    session.wait_for_control("[u Unresolved threads]");
    choose(&mut session, mouse, "[z Stale PRs]", b"z");
    session.wait_for_screen("Stale open PRs");
    session.wait_for_screen("1 open PRs not updated for 7+ days");
    session
        .try_wait_for_condition(
            |screen| {
                screen.lines().any(|line| {
                    line.contains("example/app#1")
                        && line.contains(TITLE)
                        && line.contains("alice")
                        && line.contains("idle ")
                })
            },
            "stale PR row with its author and idle days",
            Duration::from_secs(8),
        )
        .unwrap();
    selected(&mut session, TITLE);
    journal_v1(&mut session, &fixture, 7);
    if mouse {
        session.click_visible_action(TITLE);
    } else {
        session.send_key(b"\r");
    }
    journal_v1(&mut session, &fixture, 24);
    pr_opened(&mut session);
    // Native Back returns to the widget it left, without another read.
    choose(&mut session, mouse, "[b Back]", b"b");
    session.wait_for_screen("Dashboard • focused view");
    session.wait_for_screen("Stale open PRs");
    selected(&mut session, TITLE);
    back(&mut session, mouse);
    cards_ready(&mut session, 1, 1);
    journal_v1(&mut session, &fixture, 24);

    // Unresolved threads at 80x24: the control overflows into Commands for the mouse.
    session.resize(80, 24);
    session.wait_for_screen("Needs my review");
    if mouse {
        session.invoke_via_palette(true, "[u Unresolved threads]");
    } else {
        session.send_key(b"u");
    }
    journal_v1(&mut session, &fixture, 35);
    session.wait_for_screen("Unresolved review threads");
    session.wait_for_screen("1 unresolved review threads on 1 of 1 open PRs");
    session
        .try_wait_for_condition(
            |screen| {
                // At 80 columns the TITLE column (the thread place) always stays.
                screen.lines().any(|line| line.contains(THREAD_PATH))
            },
            "unresolved thread row at 80x24",
            Duration::from_secs(8),
        )
        .unwrap();
    let screen = session.screen();
    assert!(
        !screen.contains("src/done.rs"),
        "resolved threads are not listed"
    );
    assert!(
        !screen.contains("Thread body stays out"),
        "thread bodies stay out"
    );
    assert_eq!(
        trend_points(&fixture),
        points,
        "a thread read retains nothing"
    );
    selected(&mut session, THREAD_PATH);
    if mouse {
        session.click_visible_action(THREAD_PATH);
    } else {
        session.send_key(b"\r");
    }
    journal_v1(&mut session, &fixture, 52);
    pr_opened(&mut session);
    session.resize(220, 42);
    session.wait_for_control("[b Back]");
    choose(&mut session, mouse, "[b Back]", b"b");
    session.wait_for_screen("Unresolved review threads");
    selected(&mut session, THREAD_PATH);
    back(&mut session, mouse);
    session.wait_for_screen("Needs my review");
    journal_v1(&mut session, &fixture, 52);
    session.quit_and_assert_restored_terminal(mouse);
    journal_v1(&mut session, &fixture, 52);
    assert_eq!(trend_points(&fixture), points);
    metadata_only(&fixture);
    preserve(&fixture, &head, &index, &status);
}
