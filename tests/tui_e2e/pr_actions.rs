use crate::support::{Fixture, GhStep, Session};
use ghtui::{
    config::Config,
    storage::{Cache, Scope},
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    fs,
    time::{Duration, Instant},
};

#[path = "pr_checks_bridge.rs"]
mod pr_checks_bridge;
#[path = "pr_metadata.rs"]
mod pr_metadata;
#[path = "pr_progress.rs"]
mod pr_progress;

fn configured() -> Fixture {
    let fixture = Fixture::new();
    Config {
        host: "github.invalid".into(),
        avatars: false,
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    fs::write(
        fixture.repo().join("unrelated.txt"),
        b"unchanged local bytes\n",
    )
    .unwrap();
    fixture
}
fn args(endpoint: &str, method: &str, paged: bool) -> Vec<String> {
    let mut args = vec![
        "api".into(),
        endpoint.into(),
        "--hostname".into(),
        "github.invalid".into(),
        "--method".into(),
        method.into(),
    ];
    if paged {
        args.extend(["--paginate".into(), "--slurp".into()]);
    }
    args
}
fn step(endpoint: &str, method: &str, value: Value, paged: bool) -> GhStep {
    GhStep {
        args: args(endpoint, method, paged),
        stdin: vec![],
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
fn identity() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step("user", "GET", json!({"id":42,"login":"octocat"}), false),
    ]
}
fn pr() -> Value {
    json!({"node_id":"PR_example/repo_7","number":7,"title":"Native collaboration","body":"A fixture description","repository_url":"https://api.github.invalid/repos/example/repo","html_url":"https://github.invalid/example/repo/pull/7","user":{"id":42,"login":"octocat","type":"User"},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","head":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","ref":"feature"},"base":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","ref":"main"},"changed_files":0,"additions":0,"deletions":0,"mergeable":true,"mergeable_state":"clean"})
}
fn comment(id: u64, body: &str, updated: &str) -> Value {
    json!({"id":id,"node_id":format!("IC_{id}"),"body":body,"user":{"id":42,"login":"octocat","type":"User"},"created_at":"2026-10-13T00:00:00Z","updated_at":updated,"issue_url":"https://api.github.invalid/repos/example/repo/issues/7","html_url":format!("https://github.invalid/example/repo/pull/7#issuecomment-{id}")})
}
fn starter() -> Value {
    comment(9, "Existing starter comment", "2026-10-13T00:00:00Z")
}
fn initial() -> Vec<GhStep> {
    let mut steps = identity();
    steps.extend(identity());
    for queue in ["author", "assignee", "review-requested"] {
        let items = if queue == "review-requested" {
            vec![]
        } else {
            vec![pr()]
        };
        let mut read = step(
            "search/issues",
            "GET",
            json!({"total_count":items.len(),"incomplete_results":false,"items":items}),
            false,
        );
        read.args.extend([
            "-f".into(),
            format!("q=is:pr is:open {queue}:octocat"),
            "-f".into(),
            "per_page=100".into(),
            "-f".into(),
            "page=1".into(),
        ]);
        steps.push(read);
    }
    steps
}
fn guard() -> Vec<GhStep> {
    let mut steps = identity();
    steps.push(step("repos/example/repo/pulls/7", "GET", pr(), false));
    steps
}
fn details(comments: Vec<Value>) -> Vec<GhStep> {
    let mut steps = vec![
        step("repos/example/repo/pulls/7", "GET", pr(), false),
        step(
            "repos/example/repo/issues/7/comments?per_page=100",
            "GET",
            json!([comments]),
            true,
        ),
    ];
    for endpoint in [
        "repos/example/repo/pulls/7/commits?per_page=100",
        "repos/example/repo/pulls/7/files?per_page=100",
        "repos/example/repo/pulls/7/reviews?per_page=100",
        "repos/example/repo/issues/7/timeline?per_page=100",
    ] {
        steps.push(step(endpoint, "GET", json!([[]]), true));
    }
    steps.push(step("repos/example/repo/pulls/7", "GET", pr(), false));
    steps
}
fn inspected(comments: Vec<Value>) -> Vec<GhStep> {
    let mut steps = identity();
    steps.extend(details(comments));
    steps.extend(identity());
    steps
}
fn create(body: &str, id: u64) -> Vec<GhStep> {
    let mut steps = guard();
    let mut write = step(
        "repos/example/repo/issues/7/comments",
        "POST",
        comment(id, body, "2026-10-13T00:01:00Z"),
        false,
    );
    write.args.extend(["--input".into(), "-".into()]);
    write.stdin = serde_json::to_vec(&json!({"body":body})).unwrap();
    steps.push(write);
    steps.push(step(
        &format!("repos/example/repo/issues/comments/{id}"),
        "GET",
        comment(id, body, "2026-10-13T00:01:00Z"),
        false,
    ));
    steps
}
fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}
fn open(session: &mut Session, mouse: bool) {
    session.wait_for_screen("ghtui");
    choose(session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("Native collaboration");
    session.wait_for_screen("Fresh • fetched");
    choose(session, mouse, "Native collaboration", b"\r");
    session.wait_for_screen("1 comments");
    choose(session, mouse, "[c Conversation]", b"c");
    session.wait_for_screen("Existing starter comment");
}
fn menu(session: &mut Session, mouse: bool, label: &str) {
    choose(session, mouse, "[p PR actions]", b"p");
    session.wait_for_screen("[PgDn More]");
    if mouse {
        for _ in 0..8 {
            session.poll_output(Duration::from_millis(10));
            if let Some((column, row)) = session
                .locate(&format!("> {label}"))
                .or_else(|| session.locate(&format!("  {label}")))
            {
                session.click(column + 2, row);
                return;
            }
            session.click_visible_action("[PgDn More]");
            session.poll_output(Duration::from_millis(20));
        }
        panic!(
            "Exact native menu row did not become visible: {label}\n{}",
            session.screen()
        );
    } else {
        let selected_label = crate::support::selected_pr_menu_row;
        session
            .try_wait_for_condition(
                |screen| selected_label(screen).is_some(),
                "actual selected PR action row",
                Duration::from_secs(8),
            )
            .unwrap();
        for _ in 0..64 {
            let selected = selected_label(&session.screen()).unwrap();
            if selected == label || selected == format!("{label} [disabled]") {
                session.send_key(b"\r");
                return;
            }
            session.send_key(b"\x1b[B");
            // Exactly one outstanding movement. A stale selected row cannot
            // authorize Enter while later Down events are still queued.
            session
                .try_wait_for_condition(
                    |screen| selected_label(screen).is_some_and(|next| next != selected),
                    "one actual PR menu selection movement",
                    Duration::from_secs(8),
                )
                .unwrap();
        }
        panic!(
            "Exact native action never gained visible menu selection: {label}\n{}",
            session.screen()
        );
    }
}

#[test]
fn conversation_comments_keyboard_uncertain_restart_inspect_and_ack_do_not_resend() {
    restart_uncertain(false);
}
#[test]
fn conversation_comments_mouse_uncertain_restart_inspect_and_ack_do_not_resend() {
    restart_uncertain(true);
}
fn restart_uncertain(mouse: bool) {
    let fixture = configured();
    let first_body = "Uncertain original café draft 雪";
    let retry_body = "Deliberate new text after inspection";
    let mut steps = initial();
    steps.extend(details(vec![starter()]));
    steps.extend(guard());
    steps.extend(guard());
    let mut failed = create(first_body, 10);
    failed.pop();
    let post = failed.last_mut().unwrap();
    post.stdout.clear();
    post.stderr = b"fixture write outcome could not be confirmed".to_vec();
    post.status = 1;
    steps.extend(failed);
    let failed_count = steps.len();
    steps.extend(initial());
    steps.extend(details(vec![starter()]));
    let restart_count = steps.len();
    steps.extend(identity());
    steps.extend(details(vec![starter()]));
    let refresh_count = steps.len();
    steps.extend(inspected(vec![starter()]));
    let inspect_count = steps.len();
    steps.extend(identity());
    let ack_count = steps.len();
    steps.extend(guard());
    steps.extend(guard());
    steps.extend(create(retry_body, 11));
    steps.extend(inspected(vec![
        starter(),
        comment(11, retry_body, "2026-10-13T00:01:00Z"),
    ]));
    let final_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open(&mut session, mouse);
    menu(&mut session, mouse, "New conversation comment");
    session.wait_for_screen("Comment text");
    session.paste(first_body);
    preview(&mut session, mouse);
    confirm(&mut session, mouse);
    session.wait_for_screen("Uncertain publication");
    let journal = records(&mut session, &fixture, failed_count);
    assert_eq!(mutation_count(&journal), 1);
    let database = Connection::open(cache_path(&fixture)).unwrap();
    let marker: (i64, String, String) = database
        .query_row(
            "SELECT attempt_id,state,payload_sha256 FROM pr_write_attempts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(marker.1, "uncertain");
    assert_eq!(marker.2.len(), 64);
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_overlay_close("┌ New conversation comment ");
    session.quit_and_assert_restored_terminal(mouse);
    let cache = Cache::open(&cache_path(&fixture)).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let resource = "example/repo/pull/7/action/new-comment/pr";
    let durable: Value =
        serde_json::from_str(&cache.draft(&scope, resource).unwrap().unwrap()).unwrap();
    assert_eq!(durable["version"], 2);
    assert_eq!(durable["values"]["body"], first_body);
    assert_eq!(durable["publication"]["attempt_id"], marker.0);
    assert_eq!(durable["publication"]["payload_sha256"], marker.2);
    assert!(
        cache
            .sync_frame(&scope, "pr-write-projection:PR_example/repo_7")
            .unwrap()
            .is_none(),
        "an uncertain write must never fabricate confirmed discussion records"
    );
    let mut session = Session::launch(&fixture, 80, 24);
    open(&mut session, mouse);
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, restart_count)),
        1
    );
    choose(&mut session, mouse, "[r Refresh]", b"r");
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, refresh_count)),
        1,
        "ordinary refresh must only inspect, never resend"
    );
    session.wait_for_screen("Existing starter comment");
    assert_eq!(
        database
            .query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [marker.0],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "uncertain",
        "ordinary refresh must not acknowledge the durable barrier"
    );
    menu(&mut session, mouse, "New conversation comment");
    session.wait_for_screen("Unresolved PR publication");
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, refresh_count)),
        1
    );
    if mouse {
        session.close_overlay_by_mouse("Action unavailable • explanation only");
    } else {
        session.close_overlay_by_key("Action unavailable • explanation only");
    }
    menu(&mut session, mouse, "Inspect pending result");
    session.wait_for_screen("Inspected exact PR result");
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, inspect_count)),
        1
    );
    assert_eq!(
        database
            .query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [marker.0],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "uncertain",
        "inspection alone must not unlock or resend"
    );
    menu(&mut session, mouse, "Acknowledge inspected result");
    session.wait_for_screen("Acknowledged inspected PR result");
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, ack_count)),
        1
    );
    assert_eq!(
        database
            .query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [marker.0],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "acknowledged"
    );
    menu(&mut session, mouse, "New conversation comment");
    session.wait_for_screen(first_body);
    session.send_key(b"\x01");
    session.paste(retry_body);
    complete(
        &mut session,
        mouse,
        "New conversation comment",
        &fixture,
        final_count,
    );
    session.wait_for_screen(retry_body);
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, final_count)),
        2
    );
    let outcomes: Vec<(i64, String)> = database
        .prepare("SELECT attempt_id,state FROM pr_write_attempts ORDER BY attempt_id")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[0], (marker.0, "acknowledged".into()));
    assert_eq!(outcomes[1].1, "confirmed");
    assert_ne!(outcomes[1].0, marker.0);
    assert_projection(&fixture, Some(retry_body), 11);
    assert_eq!(
        fs::read(fixture.repo().join("unrelated.txt")).unwrap(),
        b"unchanged local bytes\n"
    );
    session.quit_and_assert_restored_terminal(mouse);
}
fn preview(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("no publishing until confirmation");
}
fn confirm(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.focus_visible_form_action("[Confirm action]");
        session.send_key(b"\r");
    }
}
fn records(session: &mut Session, fixture: &Fixture, count: usize) -> Vec<Value> {
    session.wait_for_command_count(fixture, count);
    let records: Vec<Value> = session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), count);
    assert!(records.iter().all(|record| record["matched"] == true));
    records
}
fn mutation_count(records: &[Value]) -> usize {
    records.iter().filter(|record| is_mutation(record)).count()
}
fn is_mutation(record: &Value) -> bool {
    let args = record["args"].as_array().unwrap();
    let Some(index) = args.iter().position(|arg| arg == "--method") else {
        return false;
    };
    let method = args[index + 1].as_str().unwrap();
    if !["POST", "PATCH", "DELETE", "PUT"].contains(&method) {
        return false;
    }
    if args.get(1).is_some_and(|endpoint| endpoint == "graphql") {
        let hex = record["stdin_hex"].as_str().unwrap();
        let bytes = (0..hex.len())
            .step_by(2)
            .map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap())
            .collect::<Vec<_>>();
        let payload: Value = serde_json::from_slice(&bytes).unwrap();
        if payload["query"]
            .as_str()
            .is_some_and(|query| query.trim_start().starts_with("query"))
        {
            return false;
        }
    }
    true
}
fn cache_path(fixture: &Fixture) -> std::path::PathBuf {
    fixture.root.join("config/ghtui/cache.sqlite3")
}

fn assert_projection(fixture: &Fixture, expected_body: Option<&str>, comment_id: u64) {
    let cache = Cache::open(&cache_path(fixture)).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let frame = cache
        .sync_frame(&scope, "pr-write-projection:PR_example/repo_7")
        .unwrap()
        .expect("confirmed fresh result must publish its canonical derived source");
    let generation = Connection::open(cache_path(fixture))
        .unwrap()
        .query_row(
            "SELECT MAX(attempt_id) FROM pr_write_attempts WHERE state='confirmed' AND pr_node='PR_example/repo_7'",
            [],
            |row| row.get::<_, u64>(0),
        )
        .unwrap();
    assert_eq!(frame.generation, generation);
    assert!(frame.complete, "fixture discussion coverage is complete");
    assert!(frame
        .records
        .iter()
        .any(|record| record.kind == "pr-details" && record.id == "PR_example/repo_7"));
    let node = format!("IC_{comment_id}");
    let comment = cache.get(&scope, "pr-conversation-comment", &node).unwrap();
    let details = cache
        .get(&scope, "pr-details", "PR_example/repo_7")
        .unwrap()
        .unwrap();
    let saved_comment = details.payload["comments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|comment| comment["id"] == comment_id);
    if let Some(expected) = expected_body {
        let record = comment.expect("exact confirmed conversation node must be cached");
        assert_eq!(record.payload["version"], 1);
        assert_eq!(record.payload["parent_pr_node"], "PR_example/repo_7");
        assert_eq!(record.payload["repository"], "example/repo");
        assert_eq!(record.payload["number"], 7);
        assert_eq!(record.payload["head_sha"], "a".repeat(40));
        assert_eq!(record.payload["comment_id"], comment_id);
        assert_eq!(record.payload["comment_node"], node);
        assert_eq!(record.payload["body"], expected);
        assert_eq!(record.payload["actor"]["login"], "octocat");
        assert_eq!(saved_comment.unwrap()["body"], expected);
        assert_eq!(
            record.source_updated_at.as_deref(),
            record.payload["updated_at"].as_str()
        );
        assert!(frame.records.iter().any(|record| record.id == node));
    } else {
        assert!(comment.is_none(), "deleted exact comment record survived");
        assert!(saved_comment.is_none());
        assert!(!frame.records.iter().any(|record| record.id == node));
    }
}

fn write(endpoint: &str, method: &str, payload: Option<Value>, response: Value) -> GhStep {
    let mut mutation = step(endpoint, method, response, false);
    if let Some(payload) = payload {
        mutation.args.extend(["--input".into(), "-".into()]);
        mutation.stdin = serde_json::to_vec(&payload).unwrap();
    }
    mutation
}
fn select_comment(session: &mut Session, mouse: bool, body: &str) {
    session.wait_for_screen(body);
    if mouse {
        session.click_visible_action(body);
    } else {
        session.send_key(&b"\x1b[A".repeat(30));
    }
    for _ in 0..35 {
        session.poll_output(Duration::from_millis(10));
        if session.screen().contains(&format!("> {body}")) {
            return;
        }
        if !mouse {
            session.send_key(b"\x1b[B");
        }
    }
    panic!(
        "Exact comment body never gained visible selection: {body}\n{}",
        session.screen()
    );
}
fn selected_preview(observed: Value) -> Vec<GhStep> {
    let mut steps = guard();
    steps.push(step(
        "repos/example/repo/issues/comments/10",
        "GET",
        observed,
        false,
    ));
    steps
}
fn edit_steps(observed: Value, updated: Value, new_body: &str) -> Vec<GhStep> {
    let mut steps = guard();
    steps.push(step(
        "repos/example/repo/issues/comments/10",
        "GET",
        observed,
        false,
    ));
    steps.push(write(
        "repos/example/repo/issues/comments/10",
        "PATCH",
        Some(json!({"body":new_body})),
        updated.clone(),
    ));
    steps.push(step(
        "repos/example/repo/issues/comments/10",
        "GET",
        updated,
        false,
    ));
    steps
}
fn reaction(id: u64, account: u64, content: &str) -> Value {
    json!({"id":id,"node_id":format!("REACT_{id}"),"content":content,"user":{"id":account,"login":"octocat","type":"User"},"created_at":"2026-10-13T00:02:00Z"})
}
fn reaction_catalog(observed: Value, reactions: Vec<Value>) -> Vec<GhStep> {
    let id = observed["id"].as_u64().unwrap();
    vec![
        step(
            &format!("repos/example/repo/issues/comments/{id}"),
            "GET",
            observed,
            false,
        ),
        step(
            &format!("repos/example/repo/issues/comments/{id}/reactions?per_page=100"),
            "GET",
            json!([reactions]),
            true,
        ),
    ]
}
fn loaded_reactions(observed: Value, reactions: Vec<Value>) -> Vec<GhStep> {
    let id = observed["id"].as_u64().unwrap();
    let mut steps = guard();
    steps.push(step(
        &format!("repos/example/repo/issues/comments/{id}"),
        "GET",
        observed.clone(),
        false,
    ));
    steps.extend(reaction_catalog(observed.clone(), reactions));
    steps.push(step(
        &format!("repos/example/repo/issues/comments/{id}"),
        "GET",
        observed,
        false,
    ));
    steps.push(step("repos/example/repo/pulls/7", "GET", pr(), false));
    steps.extend(identity());
    steps
}

fn current_subscription(state: Value, permission: bool) -> Vec<GhStep> {
    let mut steps = guard();
    steps.push(write("graphql","POST",Some(json!({"query":ghtui::providers::gh::PR_CURRENT_SUBSCRIPTION_QUERY,"variables":{"id":"PR_example/repo_7"}})),json!({"data":{"node":{"__typename":"PullRequest","id":"PR_example/repo_7","number":7,"headRefOid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","baseRefOid":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","repository":{"nameWithOwner":"example/repo"},"viewerSubscription":state,"viewerCanSubscribe":permission}}})));
    steps.push(step("repos/example/repo/pulls/7", "GET", pr(), false));
    steps.extend(identity());
    steps
}

fn pending_marker(fixture: &Fixture) -> i64 {
    use ghtui::{
        pr_write_journal::{PrWriteIntent, PrWriteJournal, PrWriteTarget},
        providers::gh::{HostIdentity, PrSummary},
    };
    use std::collections::BTreeMap;
    let target:PrSummary=serde_json::from_value(json!({"pr":{"host":"github.invalid","id":"PR_example/repo_7","repository":"example/repo","number":7,"title":"Native collaboration","body":"A fixture description","url":"https://github.invalid/example/repo/pull/7","author":{"login":"octocat","actor_type":"User"},"state":"open","draft":false,"created_at":"t0","updated_at":"t0","queues":[]},"head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","base_sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0})).unwrap();
    let identity = HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "2.101.0".into(),
    };
    let target = PrWriteTarget::new(&identity, &target).unwrap();
    let mut journal = PrWriteJournal::open(&cache_path(fixture)).unwrap();
    journal
        .begin(
            &target,
            &PrWriteIntent {
                command: "new-comment".into(),
                selected_resource: "pr".into(),
                observed_head: "a".repeat(40),
                values: BTreeMap::from([("body".into(), "Unresolved fixture publication".into())]),
            },
        )
        .unwrap()
        .marker()
        .attempt_id
}
fn actor_reactions() -> Vec<Value> {
    let mut items = vec![
        reaction(501, 42, "heart"),
        reaction(502, 43, "heart"),
        json!({"id":503,"node_id":"REACT_503","content":"rocket","user":{"id":99,"login":"ci-bot","type":"Bot"},"created_at":"2026-10-13T00:02:00Z"}),
        json!({"id":504,"node_id":"REACT_504","content":"eyes","user":null,"created_at":""}),
    ];
    for id in 505..=524 {
        items.push(json!({"id":id,"node_id":format!("REACT_{id}"),"content":"hooray","user":{"id":1000+id,"login":format!("reviewer-{id}"),"type":"User"},"created_at":"2026-10-13T00:03:00Z"}));
    }
    items
}
fn paged_reactions() -> Vec<GhStep> {
    let items = actor_reactions();
    let mut steps = loaded_reactions(starter(), items.clone());
    steps[5].stdout = serde_json::to_vec(&json!([&items[..12], &items[12..]])).unwrap();
    steps
}
fn page_to(session: &mut Session, mouse: bool, text: &str) {
    for _ in 0..24 {
        session.poll_output(Duration::from_millis(15));
        if session.screen().contains(text) {
            return;
        }
        choose(session, mouse, "[PgDn More]", b"\x1b[6~");
    }
    panic!(
        "Read-only reaction row never became visible: {text}\n{}",
        session.screen()
    );
}

fn wait_absent(session: &mut Session, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while session.screen().contains(text) {
        assert!(
            Instant::now() < deadline,
            "Expected observed terminal text to disappear: {text}\n{}",
            session.screen()
        );
        session.poll_output(Duration::from_millis(20));
    }
}

/// Read exact visible text across wrapped terminal rows, ignoring only layout
/// whitespace within the independently observed overlay border.
fn wait_wrapped_text(session: &mut Session, text: &str) {
    let compact = |value: &str| {
        value
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>()
    };
    let expected = compact(text);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if session
            .visible_overlay_body("Action unavailable • explanation only")
            .is_some_and(|body| compact(&body).contains(&expected))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Exact wrapped terminal text missing: {text}\n{}",
            session.screen()
        );
        session.poll_output(Duration::from_millis(20));
    }
}

fn wait_field_value(session: &mut Session, label: &str, value: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some((_, row)) = session.locate(label) {
            if session
                .screen()
                .lines()
                .nth(usize::from(row) + 1)
                .is_some_and(|line| line.contains(value))
            {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "Visible field {label:?} never contained {value:?}\n{}",
            session.screen()
        );
        session.poll_output(Duration::from_millis(20));
    }
}

#[test]
fn conversation_comments_keyboard_browse_all_reaction_actors_and_pages_preserves_pending() {
    browse_reactions(false);
}
#[test]
fn conversation_comments_mouse_browse_all_reaction_actors_and_pages_preserves_pending() {
    browse_reactions(true);
}
fn browse_reactions(mouse: bool) {
    let fixture = configured();
    let pending = pending_marker(&fixture);
    let mut steps = initial();
    steps.extend(details(vec![starter()]));
    steps.extend(paged_reactions());
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open(&mut session, mouse);
    select_comment(&mut session, mouse, "Existing starter comment");
    let cache = Cache::open(&cache_path(&fixture)).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let before = cache.list(&scope, None).unwrap();
    menu(&mut session, mouse, "Browse selected comment reactions");
    session.wait_for_screen("Comment reactions • read-only actors");
    session.wait_for_screen("Loaded 24 reactions");
    session.wait_for_screen("all fetched pages");
    session.wait_for_screen("account #42 • yours");
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, steps.len())),
        0
    );
    page_to(&mut session, mouse, "account #43 • another actor");
    page_to(&mut session, mouse, "ci-bot");
    page_to(&mut session, mouse, "ownership unverified");
    page_to(&mut session, mouse, "Created: unavailable");
    session.resize(80, 24);
    page_to(&mut session, mouse, "reaction #524");
    if mouse {
        session.wheel(20, 12, false);
        session.poll_output(Duration::from_millis(20));
        session.click_visible_action("[PgUp Previous]");
    } else {
        session.send_key(b"\x1b[A\x1b[5~");
    }
    wait_absent(&mut session, "reaction #524");
    for _ in 0..24 {
        session.poll_output(Duration::from_millis(15));
        if session.screen().contains("Loaded 24 reactions") {
            break;
        }
        choose(&mut session, mouse, "[PgUp Previous]", b"\x1b[5~");
    }
    session.wait_for_screen("Loaded 24 reactions");
    assert!(!session.screen().contains("reaction #524"));
    if mouse {
        session.wheel(20, 12, true);
    } else {
        session.send_key(b"\x1b[B");
    }
    wait_absent(&mut session, "Exact PR: example/repo#7");
    if mouse {
        session.close_overlay_by_mouse("Comment reactions • read-only actors");
    } else {
        session.focus_visible_action("[Esc Close]");
        session.send_key(b"\r");
        session.wait_for_overlay_close("Comment reactions • read-only actors");
    }
    assert_eq!(
        cache.list(&scope, None).unwrap(),
        before,
        "browsing must not rewrite canonical records"
    );
    let database = Connection::open(cache_path(&fixture)).unwrap();
    assert_eq!(
        database
            .query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [pending],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "pending"
    );
    assert_eq!(
        database
            .query_row("SELECT generation FROM pr_write_scopes", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, steps.len())),
        0
    );
    assert_eq!(
        fs::read(fixture.repo().join("unrelated.txt")).unwrap(),
        b"unchanged local bytes\n"
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn conversation_comments_keyboard_private_reaction_read_hides_authloss_and_reassigned_principal() {
    reaction_privacy(false);
}
#[test]
fn conversation_comments_mouse_private_reaction_read_hides_authloss_and_reassigned_principal() {
    reaction_privacy(true);
}
fn reaction_privacy(mouse: bool) {
    for auth_loss in [true, false] {
        let fixture = configured();
        let pending = pending_marker(&fixture);
        let mut steps = initial();
        steps.extend(details(vec![starter()]));
        steps.extend(paged_reactions());
        let first_count = steps.len();
        let mut failed = paged_reactions();
        let identity = failed.last_mut().unwrap();
        if auth_loss {
            identity.stdout.clear();
            identity.stderr = b"HTTP 401: authentication failed".to_vec();
            identity.status = 1;
        } else {
            identity.stdout = serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
        }
        steps.extend(failed);
        fixture.scripted_gh(&steps);
        let mut session = Session::launch(&fixture, 80, 24);
        open(&mut session, mouse);
        select_comment(&mut session, mouse, "Existing starter comment");
        menu(&mut session, mouse, "Browse selected comment reactions");
        session.wait_for_screen("Loaded 24 reactions");
        assert_eq!(
            mutation_count(&records(&mut session, &fixture, first_count)),
            0
        );
        if mouse {
            session.close_overlay_by_mouse("Comment reactions • read-only actors");
        } else {
            session.close_overlay_by_key("Comment reactions • read-only actors");
        }
        menu(&mut session, mouse, "Browse selected comment reactions");
        session.wait_for_screen("private pages hidden");
        assert_eq!(
            mutation_count(&records(&mut session, &fixture, steps.len())),
            0
        );
        assert!(!session.screen().contains("Existing starter comment"));
        assert!(!session.screen().contains("Loaded 24 reactions"));
        assert!(!session
            .screen()
            .contains("Comment reactions • read-only actors"));
        let database = Connection::open(cache_path(&fixture)).unwrap();
        assert_eq!(
            database
                .query_row(
                    "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                    [pending],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "pending"
        );
        assert_eq!(
            database
                .query_row("SELECT generation FROM pr_write_scopes", [], |row| row
                    .get::<_, u64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            fs::read(fixture.repo().join("unrelated.txt")).unwrap(),
            b"unchanged local bytes\n"
        );
        session.quit_and_assert_restored_terminal(mouse);
    }
}

#[test]
fn conversation_comments_keyboard_nullable_subscription_requires_choice_and_denied_current_is_honest(
) {
    subscription_unknown_denied(false);
}
#[test]
fn conversation_comments_mouse_nullable_subscription_requires_choice_and_denied_current_is_honest()
{
    subscription_unknown_denied(true);
}
fn subscription_unknown_denied(mouse: bool) {
    let fixture = configured();
    let mut steps = initial();
    steps.extend(details(vec![starter()]));
    steps.extend(current_subscription(Value::Null, true));
    let empty_count = steps.len();
    steps.extend(current_subscription(Value::Null, true));
    let preview_count = steps.len();
    steps.extend(current_subscription(json!("UNSUBSCRIBED"), false));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open(&mut session, mouse);
    menu(&mut session, mouse, "Change PR subscription");
    session.wait_for_screen("Current subscription: unavailable");
    session.wait_for_screen("Requested subscription state");
    let (column, row) = session.locate("Requested subscription state").unwrap();
    session.assert_cells(row + 1, column, "            ");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("Choose a supported subscription state");
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, empty_count)),
        0
    );
    choose(&mut session, mouse, "[Space Next]", b" ");
    wait_field_value(&mut session, "Requested subscription state", "subscribed");
    choose(&mut session, mouse, "[Space Next]", b" ");
    wait_field_value(&mut session, "Requested subscription state", "unsubscribed");
    preview(&mut session, mouse);
    session.wait_for_screen("Requested subscription state: unsubscribed");
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, preview_count)),
        0
    );
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_overlay_close("┌ Change PR subscription ");
    session.resize(80, 24);
    menu(&mut session, mouse, "Change PR subscription");
    session.wait_for_screen("Current verified subscription: unsubscribed");
    wait_wrapped_text(&mut session, "viewerCanSubscribe=false");
    assert!(!session.screen().contains("[Confirm action]"));
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, steps.len())),
        0
    );
    if mouse {
        session.close_overlay_by_mouse("Action unavailable • explanation only");
    } else {
        session.close_overlay_by_key("Action unavailable • explanation only");
    }
    assert_eq!(
        Connection::open(cache_path(&fixture))
            .unwrap()
            .query_row("SELECT COUNT(*) FROM pr_write_attempts", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read(fixture.repo().join("unrelated.txt")).unwrap(),
        b"unchanged local bytes\n"
    );
    session.quit_and_assert_restored_terminal(mouse);
}
fn complete(
    session: &mut Session,
    mouse: bool,
    label: &str,
    fixture: &Fixture,
    count: usize,
) -> Vec<Value> {
    session.wait_for_screen(&format!("┌ {label} "));
    preview(session, mouse);
    confirm(session, mouse);
    session.wait_for_screen("Published");
    session.wait_for_overlay_close(&format!("┌ {label} "));
    records(session, fixture, count)
}

#[test]
fn conversation_comments_keyboard_native_edit_react_remove_delete_and_subscription() {
    all_actions(false);
}
#[test]
fn conversation_comments_mouse_native_edit_react_remove_delete_and_subscription() {
    all_actions(true);
}
fn all_actions(mouse: bool) {
    let fixture = configured();
    let initial_body = "Comment before native edit";
    let edited_body = "Edited exact comment 雪";
    let created = comment(10, initial_body, "2026-10-13T00:01:00Z");
    let edited = comment(10, edited_body, "2026-10-13T00:02:00Z");
    let owned = reaction(501, 42, "heart");
    let foreign = reaction(502, 43, "heart");
    let mut steps = initial();
    steps.extend(details(vec![starter()]));
    steps.extend(guard());
    steps.extend(guard());
    steps.extend(create(initial_body, 10));
    steps.extend(inspected(vec![starter(), created.clone()]));
    let create_count = steps.len();
    steps.extend(selected_preview(created.clone()));
    steps.extend(guard());
    steps.extend(edit_steps(created, edited.clone(), edited_body));
    steps.extend(inspected(vec![starter(), edited.clone()]));
    let edit_count = steps.len();
    steps.extend(selected_preview(edited.clone()));
    steps.extend(guard());
    steps.extend(guard());
    steps.push(step(
        "repos/example/repo/issues/comments/10",
        "GET",
        edited.clone(),
        false,
    ));
    steps.push(write(
        "repos/example/repo/issues/comments/10/reactions",
        "POST",
        Some(json!({"content":"heart"})),
        owned.clone(),
    ));
    steps.push(step(
        "repos/example/repo/issues/comments/10/reactions?per_page=100",
        "GET",
        json!([[owned.clone(), foreign.clone()]]),
        true,
    ));
    steps.extend(inspected(vec![starter(), edited.clone()]));
    let react_count = steps.len();
    steps.extend(loaded_reactions(
        edited.clone(),
        vec![owned.clone(), foreign.clone()],
    ));
    steps.extend(selected_preview(edited.clone()));
    steps.extend(reaction_catalog(
        edited.clone(),
        vec![owned.clone(), foreign.clone()],
    ));
    steps.extend(guard());
    steps.extend(guard());
    steps.push(step(
        "repos/example/repo/issues/comments/10",
        "GET",
        edited.clone(),
        false,
    ));
    steps.push(step(
        "repos/example/repo/issues/comments/10/reactions?per_page=100",
        "GET",
        json!([[owned, foreign.clone()]]),
        true,
    ));
    steps.push(write(
        "repos/example/repo/issues/comments/10/reactions/501",
        "DELETE",
        None,
        Value::Null,
    ));
    steps.push(step(
        "repos/example/repo/issues/comments/10/reactions?per_page=100",
        "GET",
        json!([[foreign.clone()]]),
        true,
    ));
    steps.extend(inspected(vec![starter(), edited.clone()]));
    let remove_count = steps.len();
    steps.extend(selected_preview(edited.clone()));
    steps.extend(guard());
    steps.extend(guard());
    steps.push(step(
        "repos/example/repo/issues/comments/10",
        "GET",
        edited,
        false,
    ));
    steps.push(write(
        "repos/example/repo/issues/comments/10",
        "DELETE",
        None,
        Value::Null,
    ));
    steps.push(step(
        "repos/example/repo/issues/7/comments?per_page=100",
        "GET",
        json!([[starter()]]),
        true,
    ));
    steps.extend(inspected(vec![starter()]));
    let delete_count = steps.len();
    steps.extend(current_subscription(json!("SUBSCRIBED"), true));
    steps.extend(current_subscription(json!("SUBSCRIBED"), true));
    steps.extend(current_subscription(json!("SUBSCRIBED"), true));
    steps.extend(guard());
    steps.push(write("graphql","POST",Some(json!({"query":"mutation($input:UpdateSubscriptionInput!){updateSubscription(input:$input){subscribable{id viewerSubscription}}}","variables":{"input":{"subscribableId":"PR_example/repo_7","state":"UNSUBSCRIBED"}}})),json!({"data":{"updateSubscription":{"subscribable":{"id":"PR_example/repo_7","viewerSubscription":"UNSUBSCRIBED"}}}})));
    steps.extend(inspected(vec![starter()]));
    let subscription_count = steps.len();
    steps.extend(current_subscription(json!("UNSUBSCRIBED"), true));
    let reopened_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open(&mut session, mouse);
    menu(&mut session, mouse, "New conversation comment");
    session.wait_for_screen("Comment text");
    session.paste(initial_body);
    complete(
        &mut session,
        mouse,
        "New conversation comment",
        &fixture,
        create_count,
    );
    assert_projection(&fixture, Some(initial_body), 10);
    session.wait_for_screen(initial_body);
    select_comment(&mut session, mouse, initial_body);
    menu(&mut session, mouse, "Edit selected comment");
    session.wait_for_screen("┌ Edit selected comment ");
    session.wait_for_screen(initial_body);
    session.send_key(b"\x01");
    session.paste(edited_body);
    complete(
        &mut session,
        mouse,
        "Edit selected comment",
        &fixture,
        edit_count,
    );
    assert_projection(&fixture, Some(edited_body), 10);
    session.wait_for_screen(edited_body);
    assert!(!session.screen().contains(initial_body));
    session.resize(80, 24);
    select_comment(&mut session, mouse, edited_body);
    menu(&mut session, mouse, "React to selected comment");
    session.wait_for_screen("Reaction");
    for _ in 0..4 {
        choose(&mut session, mouse, "[Space Next]", b" ");
        session.poll_output(Duration::from_millis(20));
    }
    session.wait_for_screen("heart");
    complete(
        &mut session,
        mouse,
        "React to selected comment",
        &fixture,
        react_count,
    );
    select_comment(&mut session, mouse, edited_body);
    menu(&mut session, mouse, "Remove my selected reaction");
    session.wait_for_screen("heart • @octocat • reaction 501");
    assert!(!session.screen().contains("reaction 502"));
    complete(
        &mut session,
        mouse,
        "Remove my selected reaction",
        &fixture,
        remove_count,
    );
    select_comment(&mut session, mouse, edited_body);
    menu(&mut session, mouse, "Delete selected comment");
    complete(
        &mut session,
        mouse,
        "Delete selected comment",
        &fixture,
        delete_count,
    );
    assert_projection(&fixture, None, 10);
    session.wait_for_screen("Existing starter comment");
    assert!(!session.screen().contains(edited_body));
    menu(&mut session, mouse, "Change PR subscription");
    session.wait_for_screen("Current subscription: subscribed");
    session.wait_for_screen("Requested subscription state");
    choose(&mut session, mouse, "[Space Next]", b" ");
    session.wait_for_screen("unsubscribed");
    let journal = complete(
        &mut session,
        mouse,
        "Change PR subscription",
        &fixture,
        subscription_count,
    );
    assert_eq!(mutation_count(&journal), 6);
    assert_projection(&fixture, None, 10);
    menu(&mut session, mouse, "Change PR subscription");
    session.wait_for_screen("Current subscription: unsubscribed");
    session.wait_for_screen("Requested subscription state");
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, reopened_count)),
        6
    );
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_overlay_close("┌ Change PR subscription ");
    let database = Connection::open(cache_path(&fixture)).unwrap();
    let mut query = database
        .prepare("SELECT command,state,confirmed_json FROM pr_write_attempts ORDER BY attempt_id")
        .unwrap();
    let receipts: Vec<(String, String, Value)> = query
        .query_map([], |row| {
            let effect: String = row.get(2)?;
            Ok((
                row.get(0)?,
                row.get(1)?,
                serde_json::from_str(&effect).unwrap(),
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(receipts.len(), 6);
    assert!(receipts.iter().all(|(_, state, _)| state == "confirmed"));
    assert_eq!(
        receipts
            .iter()
            .map(|(_, _, receipt)| receipt["resource_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["10", "10", "501", "501", "10", "unsubscribed"]
    );
    let methods: Vec<(&str, &str)> = journal
        .iter()
        .filter(|record| is_mutation(record))
        .filter_map(|record| {
            let args = record["args"].as_array().unwrap();
            let method = args
                .iter()
                .position(|arg| arg == "--method")
                .map(|index| args[index + 1].as_str().unwrap())?;
            (["POST", "PATCH", "DELETE"].contains(&method))
                .then(|| (args[1].as_str().unwrap(), method))
        })
        .collect();
    assert_eq!(
        methods,
        vec![
            ("repos/example/repo/issues/7/comments", "POST"),
            ("repos/example/repo/issues/comments/10", "PATCH"),
            ("repos/example/repo/issues/comments/10/reactions", "POST"),
            (
                "repos/example/repo/issues/comments/10/reactions/501",
                "DELETE"
            ),
            ("repos/example/repo/issues/comments/10", "DELETE"),
            ("graphql", "POST")
        ]
    );
    assert_eq!(
        fs::read(fixture.repo().join("unrelated.txt")).unwrap(),
        b"unchanged local bytes\n"
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn conversation_comments_keyboard_native_create_preview_cancel_confirm_and_receipt() {
    create_journey(false);
}
#[test]
fn conversation_comments_mouse_native_create_preview_cancel_confirm_and_receipt() {
    create_journey(true);
}
fn create_journey(mouse: bool) {
    let fixture = configured();
    let body = "Native café comment 雪\nSecond line";
    let mut steps = initial();
    steps.extend(details(vec![starter()]));
    steps.extend(guard()); // cancelled preview
    steps.extend(guard()); // restored preview
    steps.extend(guard()); // publication preflight
    steps.extend(create(body, 10));
    steps.extend(inspected(vec![
        starter(),
        comment(10, body, "2026-10-13T00:01:00Z"),
    ]));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open(&mut session, mouse);
    assert_eq!(mutation_count(&records(&mut session, &fixture, 14)), 0);
    menu(&mut session, mouse, "New conversation comment");
    session.wait_for_screen("Comment text");
    session.paste(body);
    preview(&mut session, mouse);
    assert_eq!(mutation_count(&records(&mut session, &fixture, 17)), 0);
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_overlay_close("┌ New conversation comment ");
    session.wait_for_screen("Existing starter comment");
    menu(&mut session, mouse, "New conversation comment");
    session.wait_for_screen("Native café comment 雪");
    session.resize(80, 24);
    preview(&mut session, mouse);
    assert_eq!(mutation_count(&records(&mut session, &fixture, 20)), 0);
    confirm(&mut session, mouse);
    session.wait_for_screen("Published");
    session.wait_for_overlay_close("┌ New conversation comment ");
    session.wait_for_screen("Native café comment 雪");
    let journal = records(&mut session, &fixture, steps.len());
    assert_eq!(mutation_count(&journal), 1);
    assert_projection(&fixture, Some(body), 10);
    let write = journal
        .iter()
        .find(|record| {
            record["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST")
        })
        .unwrap();
    assert!(write["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "repos/example/repo/issues/7/comments"));
    assert_eq!(
        write["stdin_hex"],
        serde_json::to_vec(&json!({"body":body}))
            .unwrap()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let database = Connection::open(cache_path(&fixture)).unwrap();
    let receipt: (String, String, String) = database
        .query_row(
            "SELECT state,command,confirmed_json FROM pr_write_attempts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(receipt.0, "confirmed");
    assert_eq!(receipt.1, "new-comment");
    assert_eq!(
        serde_json::from_str::<Value>(&receipt.2).unwrap()["resource_id"],
        "10"
    );
    let cache = Cache::open(&cache_path(&fixture)).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    while cache
        .draft(&scope, "example/repo/pull/7/action/new-comment/pr")
        .unwrap()
        .is_some()
    {
        assert!(
            Instant::now() < deadline,
            "confirmed native draft was not removed"
        );
        session.poll_output(Duration::from_millis(10));
    }
    assert_eq!(
        fs::read(fixture.repo().join("unrelated.txt")).unwrap(),
        b"unchanged local bytes\n"
    );
    session.quit_and_assert_restored_terminal(mouse);
}
