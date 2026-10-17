//! Real-binary journeys through owner-bound selected PR Open and cached Back.
use super::*;
#[path = "repository_catalogue_scoped_pr_observed_tabs_v1.rs"]
mod observed_tabs;
#[path = "repository_catalogue_scoped_pr_resources_v1.rs"]
pub(crate) mod resources;
use rusqlite::{Connection, OpenFlags};

const ROW_TITLE: &str = "Previous Issue-search observation";
const DETAIL_TITLE: &str = "Fresh exact canonical PR";
const DETAIL_BODY: &str = "EXACT_PR_BODY_CANARY • café 界🦀";
const SEARCH_BODY: &str = "PREVIOUS_SEARCH_BODY_NOT_DETAIL_AUTHORITY";
const PRIVATE_DRAFT: &str = "ORIGINAL42_PRIVATE_DRAFT_UNTOUCHED";

fn paged(endpoint: &str, rows: Value) -> GhStep {
    GhStep::read(
        &[
            "api",
            endpoint,
            "--hostname",
            HOST,
            "--method",
            "GET",
            "--paginate",
            "--slurp",
        ],
        &json!([rows]).to_string(),
    )
}
fn search_row() -> Value {
    json!({"id":2007,"node_id":"GENUINE_SEARCH_ISSUE_NODE_2007","number":7,
        "title":ROW_TITLE,"body":SEARCH_BODY,"state":"open","draft":false,
        "repository_url":format!("https://{HOST}/api/v3/repos/{FIRST}"),
        "html_url":format!("https://{HOST}/{FIRST}/pull/7"),
        "pull_request":{"url":format!("https://{HOST}/api/v3/repos/{FIRST}/pulls/7"),"html_url":format!("https://{HOST}/{FIRST}/pull/7")},
        "created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-14T00:00:00Z",
        "user":{"id":51,"node_id":"U_author_51","login":"search-author","type":"User"}})
}
fn open_inbox() -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.extend(principal(42));
    for qualifier in ["author", "assignee", "review-requested"] {
        steps.push(GhStep::read(
            &[
                "api",
                "search/issues",
                "--hostname",
                HOST,
                "--method",
                "GET",
                "-f",
                &format!("q=is:pr is:open {qualifier}:octocat repo:{FIRST}"),
                "-f",
                "per_page=100",
                "-f",
                "page=1",
            ],
            &json!({"total_count":1,"incomplete_results":false,"items":[search_row()]}).to_string(),
        ));
    }
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 11);
    steps
}
fn summary() -> Value {
    let body = std::iter::once(DETAIL_BODY.to_owned())
        .chain((0..32).map(|i| format!("Observed detail line {i:02} • 界🦀")))
        .collect::<Vec<_>>()
        .join("\n");
    json!({"id":1007,"node_id":"GENUINE_CANONICAL_PR_NODE_1007","number":7,"title":DETAIL_TITLE,"body":body,
        "state":"open","draft":false,"merged":false,"html_url":format!("https://{HOST}/{FIRST}/pull/7"),
        "repository_url":format!("https://{HOST}/repos/{FIRST}"),
        "user":{"id":61,"node_id":"U_pr_author_61","login":"actual-pr-author","type":"User"},
        "created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-14T00:00:00Z",
        "head":{"sha":"a".repeat(40),"ref":"feature/scoped-open","repo":{"id":920,"node_id":"FORK_920","full_name":"contributor/source"}},
        "base":{"sha":"b".repeat(40),"ref":"main","repo":{"id":10000,"node_id":"REPO_catalogue_10000","full_name":FIRST}},
        "additions":2,"deletions":1,"changed_files":1,"mergeable":null,"mergeable_state":"unknown"})
}
fn open_pr() -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), summary()));
    steps.push(paged(&format!("repos/{FIRST}/issues/7/comments?per_page=100"),json!([{"id":301,"node_id":"COMMENT_301","body":"Actual conversation not auto-selected","user":{"id":62,"login":"comment-author","type":"User"}}])));
    steps.push(paged(
        &format!("repos/{FIRST}/pulls/7/commits?per_page=100"),
        json!([{"sha":"c".repeat(40),"commit":{"message":"Observed scoped commit"}}]),
    ));
    steps.push(paged(&format!("repos/{FIRST}/pulls/7/files?per_page=100"),json!([{"filename":"src/界.rs","status":"modified","additions":2,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new"}])));
    steps.push(paged(&format!("repos/{FIRST}/pulls/7/reviews?per_page=100"),json!([{"id":601,"node_id":"REVIEW_601","state":"COMMENTED","body":"Observed review","user":{"id":63,"login":"review-author","type":"User"}}])));
    steps.push(paged(
        &format!("repos/{FIRST}/issues/7/timeline?per_page=100"),
        json!([{"id":801,"event":"review_requested","created_at":"2026-10-02T00:00:00Z"}]),
    ));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), summary()));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 13);
    steps
}
fn exact_reads(session: &Session, fixture: &Fixture, steps: &[GhStep]) {
    let rows = commands(session, fixture);
    assert_eq!(rows.len(), steps.len());
    for (ordinal, (actual, step)) in rows.iter().zip(steps).enumerate() {
        assert_eq!(actual["step"], ordinal);
        assert_eq!(actual["args"], json!(step.args));
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["stdin_hex"], "");
        assert!(!step.args.iter().any(|arg| matches!(
            arg.as_str(),
            "POST" | "PATCH" | "DELETE" | "PUT" | "graphql"
        )));
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
        "actual bounded paginated subprocesses match the exact original fixture, including explicit reopen"
    );
}
fn raw(fixture: &Fixture) -> Connection {
    Connection::open_with_flags(database(fixture), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}
fn sql_state(fixture: &Fixture) -> Vec<String> {
    let db = raw(fixture);
    ["SELECT json_array(host,login,resource,body) FROM drafts ORDER BY host,login,resource",
     "SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name",
     "SELECT json_array(host,login,source,generation,complete) FROM checkpoints ORDER BY host,login,source",
     "SELECT json_array(host,login,kind,id,payload) FROM records ORDER BY host,login,kind,id",
     "SELECT json_array(attempt_id,state) FROM pr_write_attempts ORDER BY attempt_id",
     "SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id"]
        .iter().flat_map(|query|db.prepare(query).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn no_attempts(fixture: &Fixture) {
    let db = raw(fixture);
    for table in ["pr_write_attempts", "actions_write_attempts"] {
        let count: u64 = db
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "read cannot create publication intent");
    }
    let foreign:u64=db.query_row("SELECT (SELECT count(*) FROM records WHERE login='user-id:43')+(SELECT count(*) FROM drafts WHERE login='user-id:43')+(SELECT count(*) FROM saved_views WHERE login='user-id:43')",[],|row|row.get(0)).unwrap();
    assert_eq!(foreign, 0);
}
fn journey(mouse: bool) {
    let mut steps = collection(vec![json!([repository(0)])], 42);
    steps.extend(open_inbox());
    steps.extend(open_pr());
    assert_eq!(steps.len(), 31);
    let fixture = configured(&steps);
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&database(&fixture)).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&database(&fixture)).unwrap());
    let cache = Cache::open(&database(&fixture)).unwrap();
    let scope = Scope::github(HOST, Some(42)).unwrap();
    cache
        .put_draft(&scope, "scoped-original-private-draft", PRIVATE_DRAFT)
        .unwrap();
    cache
        .save_view(
            &scope,
            "independent-original-set",
            &json!({"ids":[10000,19991],"names":[FIRST,"example/unseen-original"]}),
        )
        .unwrap();
    drop(cache);
    let git_before = git_state(&fixture);
    let config_path = fixture.root.join("config/ghtui/config.toml");
    let config_before = fs::read(&config_path).unwrap();
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    session.wait_for_command_count(&fixture, 7);
    ready(&mut session, &[TITLE, FIRST, "Repository ID: 10000"]);
    if !mouse {
        session.send_key(b"\t");
    }
    choose(&mut session, mouse, "[Open My PRs]", b"\r");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_command_count(&fixture, 18);
    ready(
        &mut session,
        &[
            "Inbox • focused view",
            ROW_TITLE,
            "1 unique PRs across 3 queue matches • complete",
        ],
    );
    let before_open = sql_state(&fixture);
    assert!(!session.screen().contains(PRIVATE_DRAFT));
    assert!(!session.screen().contains(DETAIL_BODY));
    // Actual View action establishes content focus; it must NOT launch a
    // default query merely because an explicit scoped Inbox already exists.
    choose(&mut session, mouse, "1 Inbox", b"1");
    ready(&mut session, &[ROW_TITLE]);
    choose(&mut session, mouse, ROW_TITLE, b"\r");
    session.wait_for_command_count(&fixture, 31);
    ready(
        &mut session,
        &[
            DETAIL_TITLE,
            "actual-pr-author",
            "Head aaaaaaaaa",
            "Base bbbbbbbbb",
            DETAIL_BODY,
            "[b Back]",
        ],
    );
    assert!(!session.screen().contains(SEARCH_BODY));
    assert!(!session.screen().contains(PRIVATE_DRAFT));
    assert!(!session.screen().contains("GENUINE_SEARCH_ISSUE_NODE"));
    exact_reads(&session, &fixture, &steps);
    assert_eq!(
        sql_state(&fixture),
        before_open,
        "detail OPEN does not commit/prune inbox, drafts, original sets or attempts"
    );
    no_attempts(&fixture);
    let gh_before = session.inspect_command_journal(&fixture);
    let git_commands_before = git_commands(&fixture);
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    ready(&mut session, &[DETAIL_TITLE, "[b Back]"]);
    if mouse {
        let (column, row) = session.locate("Head aaaaaaaaa").unwrap();
        session.wheel(column, row, true);
    } else {
        session.send_key(b"\x1b[6~");
    }
    ready(&mut session, &["Observed detail line"]);
    if mouse {
        let (column, row) = session.locate("Observed detail line").unwrap();
        session.wheel(column, row, false);
    } else {
        session.send_key(b"\x1b[5~");
    }
    ready(&mut session, &[DETAIL_TITLE, "[b Back]"]);
    choose(&mut session, mouse, "[? Help]", b"?");
    overlay_ready(
        &mut session,
        "Help • keyboard and mouse",
        &["Inbox • help for this view", "[Esc Close]"],
    );
    if mouse {
        session.close_overlay_by_mouse("Help • keyboard and mouse");
    } else {
        session.close_overlay_by_key("Help • keyboard and mouse");
    }
    ready(&mut session, &[DETAIL_TITLE, "[b Back]"]);
    choose(&mut session, mouse, "[b Back]", b"b");
    ready(
        &mut session,
        &[
            ROW_TITLE,
            "Cached • fetched",
            "1 unique PRs across 3 queue matches • PARTIAL",
            "Previous inbox observation",
        ],
    );
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains(ROW_TITLE)
                    && screen.contains("Previous inbox observation")
                    && !screen.contains(DETAIL_TITLE)
                    && !screen.contains(DETAIL_BODY)
            },
            "Complete cached Back frame has erased the previous detail body",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(!session.screen().contains(DETAIL_TITLE));
    assert!(!session.screen().contains(DETAIL_BODY));
    // Cached Back is PURE and retains the selected original row. Filter and
    // repeat-view are navigation, not hidden scope/API refresh or approval.
    choose(&mut session, mouse, "[c Created by me]", b"c");
    ready(&mut session, &[ROW_TITLE, "Queue: Created by me"]);
    choose(&mut session, mouse, "[a All]", b"a");
    ready(&mut session, &[ROW_TITLE, "Queue: All"]);
    assert_eq!(session.inspect_command_journal(&fixture), gh_before);
    assert_eq!(git_commands(&fixture), git_commands_before);
    assert_eq!(sql_state(&fixture), before_open);
    choose(&mut session, mouse, "2 Repository", b"2");
    ready(
        &mut session,
        &["Repository • focused view", "Recent commits"],
    );
    choose(&mut session, mouse, "1 Inbox", b"1");
    ready(
        &mut session,
        &[ROW_TITLE, "Cached • fetched", "Previous inbox observation"],
    );
    exact_reads(&session, &fixture, &steps);
    assert_eq!(git_state(&fixture), git_before);
    assert_eq!(fs::read(&config_path).unwrap(), config_before);
    assert_read_only_git(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
    exact_reads(&session, &fixture, &steps);
    assert_eq!(sql_state(&fixture), before_open);
    no_attempts(&fixture);
    assert_eq!(git_state(&fixture), git_before);
}
#[test]
fn repo_catalog_scoped_pr_read_keyboard_source_exact_details_cached_back_no_publication() {
    journey(false);
}
#[test]
fn repo_catalog_scoped_pr_read_mouse_source_exact_details_cached_back_no_publication() {
    journey(true);
}
