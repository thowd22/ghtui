//! SOURCE ONLY. Actual ordinary Personal/Catalogue Summary/Conversation,
//! same binary/queue/Core/journal. Exact offline transport, SQL and raw Git
//! observers are independent of the visible success/uncertainty witnesses.
use super::*;
use crate::support::completed_navigation_controls_v1::FocusAttributeV1;
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession};
use ghtui::pr_actions::PrCommand;
use rusqlite::OptionalExtension;
#[path = "pr_conversation_close_v20.rs"]
mod close_v20;
#[path = "pr_lifecycle_v68.rs"]
mod lifecycle_v68;
#[path = "pr_conversation_lineage_v18.rs"]
mod lineage_v18;
#[path = "pr_conversation_local_values_v21.rs"]
mod local_values_v21;
#[path = "pr_metadata_base_v32.rs"]
mod metadata_base_v32;
#[path = "pr_metadata_original_v25.rs"]
mod metadata_original_v25;
#[path = "pr_conversation_refreshed_v19.rs"]
pub(crate) mod refreshed_v19;

const BODY: &str = "TASK17_NEW_EXACT_BODY_界";
const OLD: &str = "TASK17_ORIGINAL_COMMENT_43";
const COMMENT_QUERY: &str = "query($id:ID!){node(id:$id){__typename ... on IssueComment{id fullDatabaseId body createdAt updatedAt viewerCanUpdate viewerCanDelete viewerCanReact viewerDidAuthor repository{id nameWithOwner} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id nameWithOwner}}}}}";
const ABSENT_QUERY: &str = "query($id:ID!){node(id:$id){__typename ... on IssueComment{id}}}";
const SUB_MUTATION: &str = "mutation($input:UpdateSubscriptionInput!){updateSubscription(input:$input){subscribable{id viewerSubscription}}}";
const EDITABLE: &[&str] = &[
    "[Preview]",
    "[External editor]",
    "[Confirm action]",
    "[Keep draft / Close]",
];
const BODYLESS: &[&str] = &["[Preview]", "[Confirm action]", "[Keep draft / Close]"];
const HISTORY: &[&str] = &["[Inspect current]", "[Acknowledge local]", "[Esc Close]"];
const SIX: [PrCommand; 6] = [
    PrCommand::NewComment,
    PrCommand::EditComment,
    PrCommand::DeleteComment,
    PrCommand::AddReaction,
    PrCommand::RemoveReaction,
    PrCommand::Subscription,
];

fn comment(id: u64, body: &str) -> Value {
    json!({"id":id,"node_id":format!("IC_{id}"),"body":body,
        "issue_url":format!("https://{HOST}/api/v3/repos/{FIRST}/issues/7"),
        "url":format!("https://{HOST}/api/v3/repos/{FIRST}/issues/comments/{id}"),
        "user":{"id":if id==501 {43}else{42},"login":"octocat","type":"User"},
        "created_at":"2026-10-15T00:00:00Z","updated_at":"2026-10-15T00:00:00Z"})
}
fn graphql(query: &str, variables: Value, response: Value) -> GhStep {
    let mut step = get("graphql", response);
    *step.args.iter_mut().find(|v| v.as_str() == "GET").unwrap() = "POST".into();
    step.args.extend(["--input".into(), "-".into()]);
    step.stdin = serde_json::to_vec(&json!({"query":query,"variables":variables})).unwrap();
    step
}
fn capabilities(allowed: Option<bool>) -> Value {
    json!({"data":{"node":{"__typename":"IssueComment","id":"IC_501","fullDatabaseId":"501",
        "body":OLD,"createdAt":"2026-10-15T00:00:00Z","updatedAt":"2026-10-15T00:00:00Z",
        "viewerCanUpdate":allowed,"viewerCanDelete":allowed,"viewerCanReact":allowed,"viewerDidAuthor":false,
        "repository":{"id":"REPO_catalogue_10000","nameWithOwner":FIRST},
        "pullRequest":{"id":"GENUINE_CANONICAL_PR_NODE_1007","fullDatabaseId":"1007","number":7,
            "headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),
            "repository":{"id":"REPO_catalogue_10000","nameWithOwner":FIRST}}}}})
}
fn reaction(id: u64, content: &str, uid: u64) -> Value {
    json!({"id":id,"node_id":format!("REACTION_{id}"),"content":content,
        "user":{"id":uid,"login":"octocat","type":"User"},"created_at":"2026-10-15T00:00:00Z"})
}
fn reactions() -> Value {
    json!([reaction(601, "heart", 42), reaction(602, "eyes", 43)])
}
fn subscription(state: &str) -> Value {
    json!({"data":{"node":{"__typename":"PullRequest","id":"GENUINE_CANONICAL_PR_NODE_1007","number":7,
        "headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"repository":{"nameWithOwner":FIRST},
        "viewerSubscription":state,"viewerCanSubscribe":true}}})
}
fn observed(command: PrCommand, opening: bool, allowed: Option<bool>) -> Vec<GhStep> {
    match command {
        PrCommand::NewComment => vec![],
        PrCommand::Subscription => vec![graphql(
            ghtui::providers::gh::PR_CURRENT_SUBSCRIPTION_QUERY,
            json!({"id":"GENUINE_CANONICAL_PR_NODE_1007"}),
            subscription("UNSUBSCRIBED"),
        )],
        _ => {
            let mut result = vec![
                get(
                    &format!("repos/{FIRST}/issues/comments/501"),
                    comment(501, OLD),
                ),
                graphql(COMMENT_QUERY, json!({"id":"IC_501"}), capabilities(allowed)),
            ];
            if command == PrCommand::RemoveReaction || opening && command == PrCommand::AddReaction
            {
                result.push(get(
                    &format!("repos/{FIRST}/issues/comments/501/reactions?per_page=100&page=1"),
                    reactions(),
                ));
            }
            result
        }
    }
}
fn phase(command: PrCommand, opening: bool, allowed: Option<bool>) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.extend(observed(command, opening, allowed));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.extend(principal(42));
    steps
}
fn initial_steps(personal: bool) -> Vec<GhStep> {
    let mut steps = if personal {
        let mut calls = principal(42);
        calls.extend(principal(42));
        for qualifier in ["author", "assignee", "review-requested"] {
            calls.push(GhStep::read(
                &[
                    "api",
                    "search/issues",
                    "--hostname",
                    HOST,
                    "--method",
                    "GET",
                    "-f",
                    &format!("q=is:pr is:open {qualifier}:octocat"),
                    "-f",
                    "per_page=100",
                    "-f",
                    "page=1",
                ],
                &json!({"total_count":1,"incomplete_results":false,"items":[search_row()]})
                    .to_string(),
            ));
        }
        calls.extend(open_pr()[3..10].iter().cloned());
        assert_eq!(calls.len(), 14);
        calls
    } else {
        initial()
    };
    let comments = steps
        .iter_mut()
        .find(|s| s.args.iter().any(|a| a.contains("issues/7/comments?")))
        .unwrap();
    comments.stdout = serde_json::to_vec(&json!([[comment(501, OLD)]])).unwrap();
    steps
}
fn write(endpoint: &str, method: &str, payload: Option<Value>, response: Value) -> GhStep {
    let mut step = get(endpoint, response);
    *step.args.iter_mut().find(|v| v.as_str() == "GET").unwrap() = method.into();
    if let Some(payload) = payload {
        step.args.extend(["--input".into(), "-".into()]);
        step.stdin = serde_json::to_vec(&payload).unwrap();
    }
    step
}
fn http(
    endpoint: &str,
    method: &str,
    payload: Option<Value>,
    status: u16,
    response: Option<Value>,
) -> GhStep {
    let mut step = write(endpoint, method, None, Value::Null);
    step.args.push("--include".into());
    if let Some(payload) = payload {
        step.args.extend(["--input".into(), "-".into()]);
        step.stdin = serde_json::to_vec(&payload).unwrap();
    }
    let body = response.map(|v| v.to_string()).unwrap_or_default();
    step.stdout = format!(
        "HTTP/2.0 {status} {}\nContent-Length: {}\r\n\r\n{body}",
        if status == 204 {
            "No Content"
        } else {
            "Created"
        },
        body.len()
    )
    .into_bytes();
    step
}
fn receipt(command: PrCommand) -> Vec<GhStep> {
    let endpoint = format!("repos/{FIRST}/issues/comments/501");
    match command {
        PrCommand::NewComment => vec![
            write(
                &format!("repos/{FIRST}/issues/7/comments"),
                "POST",
                Some(json!({"body":BODY})),
                comment(901, BODY),
            ),
            get(
                &format!("repos/{FIRST}/issues/comments/901"),
                comment(901, BODY),
            ),
        ],
        PrCommand::EditComment => vec![
            write(
                &endpoint,
                "PATCH",
                Some(json!({"body":BODY})),
                comment(501, BODY),
            ),
            get(&endpoint, comment(501, BODY)),
        ],
        PrCommand::DeleteComment => vec![
            http(&endpoint, "DELETE", None, 204, None),
            graphql(
                ABSENT_QUERY,
                json!({"id":"IC_501"}),
                json!({"data":{"node":null}}),
            ),
        ],
        PrCommand::AddReaction => vec![
            http(
                &format!("{endpoint}/reactions"),
                "POST",
                Some(json!({"content":"heart"})),
                201,
                Some(reaction(901, "heart", 42)),
            ),
            get(
                &format!("{endpoint}/reactions?per_page=100&page=1"),
                json!([
                    reaction(601, "heart", 42),
                    reaction(602, "eyes", 43),
                    reaction(901, "heart", 42)
                ]),
            ),
        ],
        PrCommand::RemoveReaction => vec![
            http(
                &format!("{endpoint}/reactions/601"),
                "DELETE",
                None,
                204,
                None,
            ),
            get(
                &format!("{endpoint}/reactions?per_page=100&page=1"),
                json!([reaction(602, "eyes", 43)]),
            ),
        ],
        PrCommand::Subscription => vec![
            graphql(
                SUB_MUTATION,
                json!({"input":{"subscribableId":"GENUINE_CANONICAL_PR_NODE_1007","state":"IGNORED"}}),
                json!({"data":{"updateSubscription":{"subscribable":{"id":"GENUINE_CANONICAL_PR_NODE_1007","viewerSubscription":"IGNORED"}}}}),
            ),
            graphql(
                ghtui::providers::gh::PR_CURRENT_SUBSCRIPTION_QUERY,
                json!({"id":"GENUINE_CANONICAL_PR_NODE_1007"}),
                subscription("IGNORED"),
            ),
        ],
        _ => unreachable!(),
    }
}
fn visibility() -> Vec<GhStep> {
    visibility_for_v19(PrCommand::NewComment)
}
fn visibility_for_v19(command: PrCommand) -> Vec<GhStep> {
    let comments = match command {
        PrCommand::DeleteComment => json!([]),
        PrCommand::NewComment => json!([comment(501, OLD), comment(901, BODY)]),
        PrCommand::EditComment => json!([comment(501, BODY)]),
        _ => json!([comment(501, OLD)]),
    };
    visibility_with_comments_v19(comments)
}
fn visibility_with_comments_v19(comments: Value) -> Vec<GhStep> {
    let mut calls = vec![
        get("user", json!({"id":42,"login":"octocat"})),
        get(&format!("repos/{FIRST}"), repository(0)),
        get(&format!("repos/{FIRST}/pulls/7"), parent_resource()),
    ];
    for (endpoint, rows) in [
        ("issues/7/comments", comments),
        ("pulls/7/commits", json!([])),
        (
            "pulls/7/files",
            json!([{"filename":"src/界.rs","status":"modified","additions":2,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new"}]),
        ),
        ("pulls/7/reviews", json!([])),
        ("issues/7/timeline", json!([])),
    ] {
        let mut child = get(
            &format!("repos/{FIRST}/{endpoint}?per_page=100"),
            json!([rows]),
        );
        child.args.extend(["--paginate".into(), "--slurp".into()]);
        calls.push(child);
    }
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    calls.push(get("user", json!({"id":42,"login":"octocat"})));
    assert_eq!(calls.len(), 10);
    calls
}
fn enter(
    s: &mut Session,
    f: &Fixture,
    personal: bool,
    mouse: bool,
    count: usize,
    selected_comment: bool,
    expected: &[GhStep],
) {
    assert_eq!(expected.len(), count);
    if personal {
        s.wait_for_main_frame();
        let before = s.completed_cursorless_end();
        choose(s, mouse, "1 Inbox", b"1");
        s.wait_for_new_cursorless_condition(
            before,
            |screen| {
                crate::support::main_footer_visible(screen)
                    && screen.contents().contains("Inbox • focused view")
            },
            "Actual explicit Personal Inbox frame",
            Duration::from_secs(8),
        );
        let before_refresh = s.completed_cursorless_end();
        choose(s, mouse, "[r Refresh]", b"r");
        // A cached row is not refresh completion. The original queue phase is
        // seven children; only the subsequent explicit Open consumes seven more.
        s.wait_for_command_count(f, count - 7);
        s.wait_for_new_cursorless_condition(
            before_refresh,
            |screen| {
                let text = screen.contents();
                crate::support::main_footer_visible(screen)
                    && text.contains("Fresh")
                    && text.contains(ROW_TITLE)
                    && !text.contains("[Esc Cancel]")
                    && !text.contains(DETAIL_BODY)
            },
            "NEW completed Fresh Personal queue before explicit row Open",
            Duration::from_secs(8),
        );
        choose(s, mouse, ROW_TITLE, b"\r");
        s.wait_for_command_count(f, count);
        ready(s, &[DETAIL_TITLE, DETAIL_BODY]);
        s.wait_for_control("[b Back]");
    } else {
        // A restarted fixture has already consumed its first session's strict
        // prefix. Keep the same actual catalogue route, not initial-only counts.
        let base = count - initial_steps(false).len();
        // The shared read-only catalogue helper correctly requires EMPTY
        // stdin for its whole journal. This Task17 restart has an earlier real
        // POST, so take the SAME actual Commands route and validate its exact
        // known cumulative argv AND stdin prefix instead; never waive that POST.
        choose(s, mouse, "[: Commands]", b":");
        overlay_ready(
            s,
            "Commands · current context",
            &["Filter focused:", "[Enter Choose]"],
        );
        s.send_key(b"\x01");
        s.paste("Discover repositories");
        overlay_ready(
            s,
            "Commands · current context",
            &["> Discover repositories"],
        );
        choose(s, mouse, "> Discover repositories", b"\r");
        s.wait_for_overlay_close("Commands · current context");
        ready(s, &[TITLE, "[s Source]", "[Esc Close]"]);
        exact(s, f, &expected[..base]);
        discover(s, mouse);
        s.wait_for_command_count(f, base + 7);
        ready(s, &[TITLE, FIRST]);
        if !mouse {
            s.send_key(b"\t");
        }
        let before_queue = s.completed_cursorless_end();
        choose(s, mouse, "[Open My PRs]", b"\r");
        s.wait_for_overlay_close(TITLE);
        s.wait_for_command_count(f, base + 18);
        s.wait_for_new_cursorless_condition(
            before_queue,
            |screen| {
                let text = screen.contents();
                crate::support::main_footer_visible(screen)
                    && text.contains("Fresh")
                    && text.contains(ROW_TITLE)
                    && !text.contains("[Esc Cancel]")
                    && !text.contains(DETAIL_BODY)
            },
            "NEW completed actual Catalogue PR queue before explicit row Open",
            Duration::from_secs(8),
        );
        choose(s, mouse, "1 Inbox", b"1");
        choose(s, mouse, ROW_TITLE, b"\r");
        s.wait_for_command_count(f, count);
        ready(s, &[DETAIL_TITLE, DETAIL_BODY]);
        s.wait_for_control("[b Back]");
    }
    exact(s, f, expected);
    if selected_comment {
        let before = s.completed_cursorless_end();
        choose(s, mouse, "[c Conversation]", b"c");
        s.wait_for_new_cursorless_condition(
            before,
            |screen| {
                crate::support::controls::control_painted(screen, "[c Conversation]")
                    && crate::support::main_footer_visible(screen)
            },
            "Actual cached Conversation tab",
            Duration::from_secs(8),
        );
        if !mouse {
            s.send_key(b"\x1b[F");
        }
        for _ in 0..40 {
            s.poll_output(Duration::from_millis(10));
            if s.screen().contains(OLD) {
                break;
            }
            if mouse {
                s.wheel(25, 12, true)
            } else {
                s.send_key(b"\x1b[B")
            }
        }
        s.wait_for_screen(OLD);
        if mouse {
            s.click_visible_action(OLD);
        }
        s.try_wait_for_condition(
            |screen| {
                screen
                    .lines()
                    .any(|line| line.contains(OLD) && line.contains('▌'))
            },
            "Exact real comment row selected, not a ReviewComment",
            Duration::from_secs(8),
        )
        .unwrap();
    }
}
fn form_frame(s: &mut Session, after: usize, title: &str, labels: &[&str]) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(2));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|screen| {
                crate::support::overlay_body(screen, title).is_some_and(|body| {
                    labels
                        .iter()
                        .all(|label| crate::support::controls::body_has(screen, &body, label))
                })
            })
        {
            return;
        }
        if Instant::now() >= until {
            s.save_artifacts();
            panic!(
                "Actual NEW owned centered Form missing {title}/{labels:?}\n{}",
                s.screen()
            );
        }
    }
}
fn control(s: &mut Session, mouse: bool, title: &str, label: &str, controls: &[&str]) {
    if label == "[Esc Cancel]" {
        // Pending Forms deliberately ignore Tab. Witness the genuine owned
        // Close control, then use its actual Esc binding or exact mouse hit.
        let p = s.wait_for_owned_completed_action(title, label, &[]);
        if mouse {
            s.click(p.0, p.1);
        } else {
            s.send_key(b"\x1b");
        }
    } else if mouse {
        let p = s.wait_for_owned_completed_action(title, label, &[]);
        s.click(p.0, p.1);
    } else {
        s.focus_completed_owned_action_v1(title, label, FocusAttributeV1::Inverse, controls, &[]);
        s.send_key(b"\r");
    }
}
fn command_key(command: PrCommand) -> &'static str {
    match command {
        PrCommand::NewComment => "conversation-create-v17",
        PrCommand::EditComment => "conversation-edit-v17",
        PrCommand::DeleteComment => "conversation-delete-v17",
        PrCommand::AddReaction => "conversation-reaction-add-v17",
        PrCommand::RemoveReaction => "conversation-reaction-remove-v17",
        PrCommand::Subscription => "conversation-subscription-v17",
        _ => unreachable!(),
    }
}
fn title(command: PrCommand) -> String {
    format!("{} • {FIRST}#7", command.label())
}
fn controls(command: PrCommand) -> &'static [&'static str] {
    if matches!(command, PrCommand::NewComment | PrCommand::EditComment) {
        EDITABLE
    } else {
        BODYLESS
    }
}
fn prepare(
    s: &mut Session,
    f: &Fixture,
    mouse: bool,
    command: PrCommand,
    opened: usize,
    prepared: usize,
) {
    let title = title(command);
    let before = s.completed_cells_end();
    crate::pr_actions::menu(
        s,
        mouse,
        if command == PrCommand::NewComment {
            "New comment • eligibility UNKNOWN/server-enforced"
        } else {
            command.label()
        },
    );
    s.wait_for_command_count(f, opened);
    form_frame(s, before, &title, controls(command));
    let compose =
        crate::support::overlay_body(s.completed_cells_screen().unwrap(), &title).unwrap();
    assert!(
        compose.contains("account #42")
            && !compose.contains("Some(")
            && !compose.contains("numeric UID")
    );
    if command == PrCommand::NewComment {
        s.capture_completed_terminal_v1("task67-readable-pr-compose");
    }
    assert!(!s.screen().contains("LOCAL DRAFT • Submit"));
    match command {
        PrCommand::NewComment | PrCommand::EditComment => {
            if mouse {
                s.click_owned_form_field("PR INTENT • Preview first", "Comment text");
            }
            s.send_key(b"\x01");
            s.paste(BODY);
            s.wait_for_screen(BODY);
        }
        PrCommand::AddReaction => {
            if mouse {
                s.click_visible_action("[heart]");
            } else {
                for _ in 0..4 {
                    s.send_key(b" ");
                }
            }
        }
        PrCommand::Subscription => {
            if mouse {
                s.click_visible_action("[ignored]");
            } else {
                s.send_key(b" ");
            }
        }
        PrCommand::RemoveReaction => {
            assert!(!s.screen().contains("reaction 602"));
        }
        _ => {}
    }
    assert_eq!(
        raw(f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    let before = s.completed_cells_end();
    control(s, mouse, &title, "[Preview]", controls(command));
    s.wait_for_command_count(f, prepared);
    form_frame(
        s,
        before,
        &title,
        &["READ-ONLY PREVIEW", "[Confirm action]"],
    );
    let shown = crate::support::overlay_body(s.completed_cells_screen().unwrap(), &title).unwrap();
    assert!(
        shown.contains("account #42") && !shown.contains("Some(") && !shown.contains("numeric UID")
    );
    if command == PrCommand::NewComment {
        s.capture_completed_terminal_v1("task67-readable-pr-preview");
    }
    s.paste("POISON_AT_PREPARED_PREVIEW");
    assert!(!s.screen().contains("POISON_AT_PREPARED_PREVIEW"));
    assert_eq!(
        raw(f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
}
fn stable_sql(f: &Fixture) -> Vec<String> {
    let connection = raw(f);
    ["SELECT json_array(host,login,resource,body) FROM drafts WHERE resource NOT LIKE 'conversation-draft-v17/%' ORDER BY host,login,resource",
        "SELECT json_array(host,login,kind,id,payload) FROM records ORDER BY host,login,kind,id",
        "SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name",
        "SELECT json_array(host,login,source,generation,complete) FROM checkpoints ORDER BY host,login,source",
        "SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id"]
        .iter().flat_map(|sql|connection.prepare(sql).unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn exact(s: &Session, f: &Fixture, expected: &[GhStep]) {
    let rows = commands(s, f);
    assert_eq!(rows.len(), expected.len());
    for (n, (actual, step)) in rows.iter().zip(expected).enumerate() {
        assert_eq!(actual["step"], n);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(step.args));
        assert_eq!(
            actual["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
}
fn complete(mouse: bool, personal: bool) {
    for command in SIX {
        let mut steps = initial_steps(personal);
        let original = steps.len();
        steps.extend(phase(command, true, Some(true)));
        let opened = steps.len();
        steps.extend(phase(command, false, Some(true)));
        let prepared = steps.len();
        steps.extend(phase(command, false, Some(true)));
        let before_write = steps.len();
        steps.extend(receipt(command));
        steps.extend(visibility_for_v19(command));
        let f = resource_configured(&steps);
        for uid in [42, 43] {
            Cache::open(&database(&f))
                .unwrap()
                .put_draft(
                    &Scope::github(HOST, Some(uid)).unwrap(),
                    "independent-kept",
                    &format!("KEPT_{uid}"),
                )
                .unwrap();
        }
        let git = git_state(&f);
        let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
        let mut s = launch(&f, mouse);
        enter(
            &mut s,
            &f,
            personal,
            mouse,
            original,
            !matches!(command, PrCommand::NewComment | PrCommand::Subscription),
            &steps[..original],
        );
        let stable = stable_sql(&f);
        prepare(&mut s, &f, mouse, command, opened, prepared);
        control(
            &mut s,
            mouse,
            &title(command),
            "[Confirm action]",
            controls(command),
        );
        s.wait_for_command_count(&f, steps.len());
        refreshed_v19::wait_fresh_page_v19(&mut s);
        let connection = raw(&f);
        let (state, purpose, selected, effect): (String, String, String, String) = connection
            .query_row(
                "SELECT state,command,selected_resource,confirmed_json FROM pr_write_attempts",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(state, "confirmed");
        assert_eq!(purpose, command_key(command));
        assert!(selected
            .strip_prefix("conversation-v17/")
            .is_some_and(|s| s.len() == 64));
        let effect: Value = serde_json::from_str(&effect).unwrap();
        assert_eq!(
            effect["resource_id"],
            if command == PrCommand::Subscription {
                "ignored"
            } else if matches!(command, PrCommand::NewComment | PrCommand::AddReaction) {
                "901"
            } else if command == PrCommand::RemoveReaction {
                "601"
            } else {
                "501"
            }
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM pr_write_conversation_origins_v17",
                    [],
                    |r| r.get::<_, u64>(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'",
                    [],
                    |r| r.get::<_, u64>(0)
                )
                .unwrap(),
            0
        );
        assert_eq!(commands(&s, &f)[before_write]["matched"], true);
        refreshed_v19::assert_published_transition_v19(&f, &stable, command, 1);
        assert_eq!(git_state(&f), git);
        assert_eq!(
            fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        exact(&s, &f, &steps);
        assert_read_only_git(&f);
        s.quit_and_assert_restored_terminal(mouse);
    }
}

#[test]
fn conversation_comments_v17_catalogue_six_effects_keyboard() {
    complete(false, false)
}
#[test]
fn conversation_comments_v17_catalogue_six_effects_mouse() {
    complete(true, false)
}
#[test]
fn conversation_comments_v17_personal_six_effects_keyboard() {
    complete(false, true)
}
#[test]
fn conversation_comments_v17_personal_six_effects_mouse() {
    complete(true, true)
}

fn canceled(mouse: bool, personal: bool, restart: bool) {
    let command = PrCommand::NewComment;
    let mut steps = initial_steps(personal);
    let original = steps.len();
    steps.extend(phase(command, true, Some(true)));
    let opened = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let prepared = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let receipts = receipt(command);
    steps.push(receipts[0].clone());
    let held = steps.len();
    let mut reading = receipts[1].clone();
    reading.delay_ms = 30_000;
    steps.push(reading);
    let cancel_count = steps.len();
    if restart {
        steps.extend(initial_steps(personal));
        steps.extend(phase(command, false, Some(true)));
        let mut inspecting = phase(command, false, Some(true));
        // Create attribution was never observed by the receipt. COMPLETE
        // current catalogue is a fresh observation, NOT a guessed effect ID.
        inspecting.insert(
            4,
            get(
                &format!("repos/{FIRST}/issues/7/comments?per_page=100&page=1"),
                json!([comment(901, BODY)]),
            ),
        );
        steps.extend(inspecting);
    }
    let f = resource_configured(&steps);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    enter(
        &mut s,
        &f,
        personal,
        mouse,
        original,
        false,
        &steps[..original],
    );
    let stable = stable_sql(&f);
    // BOTH routes use the real Inbox cache namespace. Its exact fixture scope
    // is empty for ordinary Personal, or the one actually selected Catalogue
    // repository. Never discover an expected source by accepting poststate.
    let inbox_source = format!(
        "personal-inbox:{}",
        json!({"excluded":[],"repositories":if personal { vec![] } else { vec![FIRST] }})
    );
    let checkpoint: (u64, i64) = raw(&f).query_row(
            "SELECT generation,complete FROM checkpoints WHERE host=?1 AND login='user-id:42' AND source=?2",
            [HOST, inbox_source.as_str()], |r| Ok((r.get(0)?,r.get(1)?)),
        ).unwrap();
    assert_eq!(
        checkpoint,
        (1, 1),
        "ONLY the actual first explicit original-scope Inbox refresh occurred"
    );
    prepare(&mut s, &f, mouse, command, opened, prepared);
    control(
        &mut s,
        mouse,
        &title(command),
        "[Confirm action]",
        controls(command),
    );
    s.wait_for_command_count(&f, cancel_count);
    let rows = commands(&s, &f);
    let pid = i32::try_from(rows[held]["pid"].as_u64().unwrap()).unwrap();
    let identity = process_identity(pid).unwrap().unwrap();
    let owned = OwnedPtySession::capture(s.owned_application_pid() as u32).unwrap();
    assert!(owned.owns(&identity));
    assert!(!identity.zombie);
    let connection = raw(&f);
    assert_eq!(
        connection
            .query_row("SELECT state FROM pr_write_attempts", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "pending"
    );
    let encoded: String = connection
        .query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let draft: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(draft["values"]["body"], BODY);
    assert!(draft["publication"].is_object());
    assert!(draft.get("approval").is_none());
    let original_id = draft["publication"]["attempt_id"].as_i64().unwrap();
    let marker_sql = "SELECT json_array(attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at) FROM pr_write_attempts WHERE attempt_id=?1";
    let pending_marker: Value = serde_json::from_str(
        &connection
            .query_row(marker_sql, [original_id], |r| r.get::<_, String>(0))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(pending_marker[1], HOST);
    assert_eq!(pending_marker[2], "user-id:42");
    assert_eq!(pending_marker[3], draft["publication"]["pr_node_id"]);
    assert_eq!(pending_marker[4], FIRST);
    assert_eq!(pending_marker[5], 7);
    assert_eq!(pending_marker[6], command_key(command));
    assert_eq!(pending_marker[8], draft["publication"]["observed_head"]);
    assert_eq!(pending_marker[9], draft["publication"]["payload_sha256"]);
    assert_eq!(pending_marker[10], "pending");
    assert!(pending_marker[11].is_null());
    let mut uncertain_marker = pending_marker.clone();
    uncertain_marker[10] = json!("uncertain");
    control(
        &mut s,
        mouse,
        &title(command),
        "[Esc Cancel]",
        &["[Esc Cancel]"],
    );
    s.wait_for_cursorless_condition(
        |screen| {
            crate::support::main_footer_visible(screen)
                && !screen.contents().contains(BODY)
                && !screen.contents().contains(DETAIL_BODY)
                && !screen.contents().contains(&title(command))
        },
        "Actual canceled publication private retirement",
        Duration::from_secs(8),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while let Some(current) = process_identity(pid).unwrap() {
        assert_eq!(
            (current.uid, current.birth, current.session),
            (identity.uid, identity.birth, identity.session)
        );
        assert!(
            Instant::now() < deadline,
            "Exact held readback child must be reaped"
        );
        s.poll_output(Duration::from_millis(10));
    }
    // Paint retirement/reaped child precede asynchronous worker journal finish.
    // Share the ORIGINAL five-second reap deadline; do not add another delay or
    // accept pending as terminal. Every observation preserves exact ref/body.
    loop {
        assert!(
            Instant::now() < deadline,
            "Exact canceled worker did not persist uncertain within ORIGINAL reap deadline"
        );
        let row: Value = serde_json::from_str(
            &connection
                .query_row(marker_sql, [original_id], |r| r.get::<_, String>(0))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .unwrap(),
            encoded
        );
        if row[10] == "uncertain" {
            assert_eq!(
                row, uncertain_marker,
                "ONLY exact original pending→uncertain state changes"
            );
            break;
        }
        assert_eq!(row, pending_marker, "No other row/ref/receipt/created metadata may drift while awaiting actual worker completion");
        s.poll_output(Duration::from_millis(2));
    }
    assert_eq!(
        connection
            .query_row(
                "SELECT body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        encoded
    );
    assert_eq!(stable_sql(&f), stable);
    assert_eq!(git_state(&f), git);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact(&s, &f, &steps[..cancel_count]);
    s.quit_and_assert_restored_terminal(mouse);
    if restart {
        let mut s = launch(&f, mouse);
        let reopened = cancel_count + initial_steps(personal).len();
        enter(
            &mut s,
            &f,
            personal,
            mouse,
            reopened,
            false,
            &steps[..reopened],
        );
        // Independently derive the ONE authorized second refresh effect.
        // Every other full stable SQL row must remain byte-for-byte identical;
        // never adopt observed post-inspection/ACK state as a baseline.
        let reopened_stable = {
            let checkpoint: (u64, i64) = connection.query_row(
                "SELECT generation,complete FROM checkpoints WHERE host=?1 AND login='user-id:42' AND source=?2",
                [HOST, inbox_source.as_str()], |r| Ok((r.get(0)?,r.get(1)?)),
            ).unwrap();
            assert_eq!(checkpoint, (2, 1));
            let old = json!([HOST, "user-id:42", inbox_source.as_str(), 1, 1]).to_string();
            let new = json!([HOST, "user-id:42", inbox_source.as_str(), 2, 1]).to_string();
            assert_eq!(stable.iter().filter(|row| *row == &old).count(), 1);
            stable
                .iter()
                .map(|row| {
                    if row == &old {
                        new.clone()
                    } else {
                        row.clone()
                    }
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            stable_sql(&f),
            reopened_stable,
            "Exact reopened queue SQL BEFORE Inspect/ACK"
        );
        let history_title = format!("Original Conversation history • {FIRST}#7");
        let before = s.completed_cells_end();
        crate::pr_actions::menu(&mut s, mouse, "Inspect pending result");
        s.wait_for_command_count(&f, reopened + phase(command, false, Some(true)).len());
        form_frame(&mut s, before, &history_title, HISTORY);
        assert_eq!(
            connection
                .query_row("SELECT state FROM pr_write_attempts", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "uncertain"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            encoded
        );
        let before = s.completed_cells_end();
        control(&mut s, mouse, &history_title, "[Inspect current]", HISTORY);
        s.wait_for_command_count(&f, steps.len());
        form_frame(
            &mut s,
            before,
            &history_title,
            &["[Acknowledge local]", "[Esc Close]"],
        );
        // Match the FULL static attribution sentence from actual owned completed
        // document rows. Paragraph wrapping may split inside 'attribution'; only
        // whitespace is removed, never a character, identity or substantive word.
        let expected_attribution = "Original created comment identity was never observed; UNKNOWN attribution. No first/newest comment is chosen.";
        let compact = |text: &str| {
            text.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        };
        let expected_attribution = compact(expected_attribution);
        let attribution_visible = |session: &Session| {
            session.completed_cells_screen().is_some_and(|screen| {
                crate::support::overlay_body(screen, &history_title)
                    .is_some_and(|body| compact(&body).contains(&expected_attribution))
            })
        };
        let until = Instant::now() + Duration::from_secs(8);
        let mut scroll_frame = None;
        loop {
            s.poll_output(Duration::from_millis(2));
            assert!(
                Instant::now() < until,
                "Fresh inspected unknown attribution must be inspectable\n{}",
                s.screen()
            );
            if scroll_frame.is_some_and(|at| s.completed_cells_end() <= at) {
                continue;
            }
            if attribution_visible(&s) {
                break;
            }
            // At most ONE actual event until a NEW completed cells frame.
            if mouse {
                let p = s.wait_for_owned_completed_action(&history_title, "[Esc Close]", &[]);
                if attribution_visible(&s) {
                    break;
                }
                let at = s.completed_cells_end();
                s.wheel(p.0, p.1.saturating_sub(3), true);
                scroll_frame = Some(at);
            } else {
                let at = s.completed_cells_end();
                s.send_key(b"\x1b[6;5~");
                scroll_frame = Some(at);
            }
        }
        s.capture_completed_terminal_v1("conversation-history");
        assert_eq!(
            stable_sql(&f),
            reopened_stable,
            "Read-only exact Inspect changes no pinned stable SQL"
        );
        control(
            &mut s,
            mouse,
            &history_title,
            "[Acknowledge local]",
            HISTORY,
        );
        s.wait_for_cursorless_condition(
            |screen| {
                crate::support::main_footer_visible(screen)
                    && screen.contents().contains("Local ACK #")
                    && screen.contents().contains("no resend")
                    && !screen.contents().contains(BODY)
                    && !screen.contents().contains(DETAIL_BODY)
            },
            "Actual consume-once LOCAL ACK terminal outcome, never new publication permission",
            Duration::from_secs(8),
        );
        assert_eq!(
            connection
                .query_row("SELECT state FROM pr_write_attempts", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "acknowledged"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            encoded
        );
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            1
        );
        assert_eq!(stable_sql(&f), reopened_stable);
        assert_eq!(git_state(&f), git);
        assert_eq!(
            fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        exact(&s, &f, &steps);
        s.quit_and_assert_restored_terminal(mouse);
    }
}
#[test]
fn conversation_comments_v17_catalogue_held_post_readback_cancel_keyboard() {
    canceled(false, false, false)
}
#[test]
fn conversation_comments_v17_catalogue_held_post_readback_cancel_mouse() {
    canceled(true, false, false)
}
#[test]
fn conversation_comments_v17_personal_held_post_readback_cancel_keyboard() {
    canceled(false, true, false)
}
#[test]
fn conversation_comments_v17_personal_held_post_readback_cancel_mouse() {
    canceled(true, true, false)
}
#[test]
fn conversation_comments_v17_catalogue_restart_inspect_explicit_ack_keyboard() {
    canceled(false, false, true)
}
#[test]
fn conversation_comments_v17_catalogue_restart_inspect_explicit_ack_mouse() {
    canceled(true, false, true)
}
#[test]
fn conversation_comments_v17_personal_restart_inspect_explicit_ack_keyboard() {
    canceled(false, true, true)
}
#[test]
fn conversation_comments_v17_personal_restart_inspect_explicit_ack_mouse() {
    canceled(true, true, true)
}

fn refused_capability(mouse: bool, personal: bool) {
    let command = PrCommand::EditComment;
    let mut steps = initial_steps(personal);
    let original = steps.len();
    let mut denied = phase(command, true, None);
    denied.truncate(6);
    steps.extend(denied);
    let fixture = resource_configured(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    enter(
        &mut session,
        &fixture,
        personal,
        mouse,
        original,
        true,
        &steps[..original],
    );
    let sql = sql_state(&fixture);
    crate::pr_actions::menu(&mut session, mouse, command.label());
    session.wait_for_command_count(&fixture, steps.len());
    session.wait_for_cursorless_condition(
        |screen| {
            crate::support::main_footer_visible(screen)
                && screen.contents().contains("Actual capability refused")
                && !screen.contents().contains(&title(command))
        },
        "Missing actual capability is a genuine disabled explanation, not login authorization",
        Duration::from_secs(8),
    );
    assert_eq!(sql_state(&fixture), sql);
    no_attempts(&fixture);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact(&session, &fixture, &steps);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn conversation_comments_v17_catalogue_unknown_capability_keyboard() {
    refused_capability(false, false)
}
#[test]
fn conversation_comments_v17_catalogue_unknown_capability_mouse() {
    refused_capability(true, false)
}
#[test]
fn conversation_comments_v17_personal_unknown_capability_keyboard() {
    refused_capability(false, true)
}
#[test]
fn conversation_comments_v17_personal_unknown_capability_mouse() {
    refused_capability(true, true)
}

fn preview_focus(s: &mut Session, after: usize, title: &str, label: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(2));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|screen| {
                crate::support::completed_navigation_controls_v1::owned_focus_v1(
                    screen,
                    title,
                    label,
                    FocusAttributeV1::Inverse,
                    EDITABLE,
                    &[],
                )
                .is_some_and(|focus| focus.target_focused)
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "Actual completed Preview footer focus missing {title}/{label}\n{}",
            s.screen()
        );
    }
}

fn preview_close(mouse: bool) {
    for personal in [false, true] {
        let command = PrCommand::NewComment;
        let mut steps = initial_steps(personal);
        let original = steps.len();
        steps.extend(phase(command, true, Some(true)));
        let opened = steps.len();
        steps.extend(phase(command, false, Some(true)));
        let prepared = steps.len();
        let f = resource_configured(&steps);
        let git = git_state(&f);
        let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
        let mut s = launch(&f, mouse);
        enter(
            &mut s,
            &f,
            personal,
            mouse,
            original,
            false,
            &steps[..original],
        );
        let stable = stable_sql(&f);
        let title = title(command);
        let at = s.completed_cells_end();
        crate::pr_actions::menu(
            &mut s,
            mouse,
            "New comment • eligibility UNKNOWN/server-enforced",
        );
        s.wait_for_command_count(&f, opened);
        form_frame(&mut s, at, &title, EDITABLE);
        s.capture_completed_terminal_v1("conversation-compose");
        if mouse {
            s.click_owned_form_field("PR INTENT • Preview first", "Comment text");
        }
        s.send_key(b"\x01");
        s.paste(BODY);
        s.wait_for_screen(BODY);
        let at = s.completed_cells_end();
        if mouse {
            control(&mut s, true, &title, "[Preview]", EDITABLE);
        } else {
            // Ctrl-P opens Preview from actual Body focus, rather than first
            // focusing its footer through a helper and masking the old bug.
            s.send_key(b"\x10");
        }
        s.wait_for_command_count(&f, prepared);
        form_frame(
            &mut s,
            at,
            &title,
            &["READ-ONLY PREVIEW", "[Confirm action]"],
        );
        s.capture_completed_terminal_v1("conversation-preview");
        s.paste("POISON_AT_PREPARED_PREVIEW");
        assert!(!s.screen().contains("POISON_AT_PREPARED_PREVIEW"));
        // No Tab/helper input is allowed before this INITIAL paint assertion.
        // Hidden Body cannot be the currently painted focus at Preview entry.
        assert!(
            crate::support::completed_navigation_controls_v1::owned_focus_v1(
                s.completed_cells_screen().unwrap(),
                &title,
                "[Preview]",
                FocusAttributeV1::Inverse,
                EDITABLE,
                &[],
            )
            .is_some_and(|focus| focus.target_focused)
        );
        let at = s.completed_cells_end();
        s.send_key(b"\t");
        preview_focus(&mut s, at, &title, "[External editor]");
        let at = s.completed_cells_end();
        s.send_key(b"\x1b[Z");
        preview_focus(&mut s, at, &title, "[Preview]");
        let at = s.completed_cursorless_end();
        control(&mut s, mouse, &title, "[Keep draft / Close]", EDITABLE);
        s.wait_for_new_cursorless_condition(
            at,
            |screen| {
                let text = screen.contents();
                crate::support::main_footer_visible(screen)
                    && text.contains(DETAIL_BODY)
                    && !text.contains(&title)
                    && !text.contains(BODY)
            },
            "Actual Preview Close returns the original private page without publication",
            Duration::from_secs(8),
        );
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            let body: Option<String> = raw(&f)
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .unwrap();
            if let Some(body) = body {
                let value: Value = serde_json::from_str(&body).unwrap();
                if value["values"]["body"] == BODY {
                    assert!(value["publication"].is_null());
                    assert!(value.get("approved").is_none());
                    break;
                }
            }
            assert!(
                Instant::now() < until,
                "Actual ordered draft save did not preserve the unapproved body"
            );
            s.poll_output(Duration::from_millis(2));
        }
        no_attempts(&f);
        assert_eq!(
            raw(&f)
                .query_row(
                    "SELECT count(*) FROM pr_write_conversation_origins_v17",
                    [],
                    |r| r.get::<_, usize>(0)
                )
                .unwrap(),
            0
        );
        assert_eq!(stable_sql(&f), stable);
        assert_eq!(git_state(&f), git);
        assert_eq!(
            fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        exact(&s, &f, &steps);
        assert_read_only_git(&f);
        s.quit_and_assert_restored_terminal(mouse);
    }
}

#[test]
fn conversation_comments_v18_actual_preview_initial_footer_tab_backtab_close_keyboard() {
    preview_close(false)
}
#[test]
fn conversation_comments_v18_actual_preview_initial_footer_tab_backtab_close_mouse() {
    preview_close(true)
}
