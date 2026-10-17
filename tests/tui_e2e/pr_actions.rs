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
    fixture.init_git();
    fixture.commit_file(
        "unrelated.txt",
        b"unchanged local bytes\n",
        "Fixture checkout",
    );
    fixture
}
fn local_git_witness(fixture: &Fixture) -> (String, String, Vec<u8>) {
    (
        fixture.git(&["rev-parse", "HEAD"]),
        fixture.git(&["status", "--porcelain=v2", "-z"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
    )
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
fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}
fn open(session: &mut Session, mouse: bool) {
    session.wait_for_screen("ghtui");
    // Actual navigation owns focus before Refresh. Local checkout discovery
    // may finish asynchronously and must not choose this test's remote view.
    choose(session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Inbox • focused view");
    choose(session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("Native collaboration");
    session.wait_for_screen("Fresh • fetched");
    choose(session, mouse, "Native collaboration", b"\r");
    session.wait_for_screen("1 comments");
    let before_conversation = session.completed_cursorless_end();
    choose(session, mouse, "[c Conversation]", b"c");
    session.wait_for_new_cursorless_condition(
        before_conversation,
        |frame| {
            let text = frame.contents();
            crate::support::main_footer_visible(frame)
                && text.contains("[c Conversation]")
                && text.contains("@octocat PR description")
                && text.contains("Existing starter comment")
                && !text.contains("Verifying original Conversation")
                && !text.contains("[Esc Cancel]")
        },
        "NEW completed idle Conversation after one input, not starter text on prior Summary",
        Duration::from_secs(8),
    );
}
pub(crate) fn menu(session: &mut Session, mouse: bool, label: &str) {
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
    crate::repository_catalogue_native_v1::scoped_pr_read_v1::resources::pr_conversation_v17::refreshed_v19::uncertain_restart_inspect_ack_new(mouse);
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
    let git = local_git_witness(&fixture);
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
    assert_eq!(local_git_witness(&fixture), git);
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
        let git = local_git_witness(&fixture);
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
        assert_eq!(local_git_witness(&fixture), git);
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
    crate::repository_catalogue_native_v1::scoped_pr_read_v1::resources::pr_conversation_v17::refreshed_v19::nullable_subscription_denied(mouse);
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
    crate::repository_catalogue_native_v1::scoped_pr_read_v1::resources::pr_conversation_v17::refreshed_v19::sequential_six(mouse);
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
    crate::repository_catalogue_native_v1::scoped_pr_read_v1::resources::pr_conversation_v17::refreshed_v19::create_cancel_receipt(mouse);
}
