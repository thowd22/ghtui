//! Create a pull request from the checkout's current branch: draft kept on
//! close, read-only preview, then exactly one POST after confirmation. The
//! checkout itself is never changed. Strict offline fake gh only.
use super::{close, preview};
use crate::support::{Fixture, GhStep, Session};
use ghtui::local_actions::LocalCommand;
use serde_json::{json, Value};

const HOST: &str = "github.invalid";

fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &["api", endpoint, "--hostname", HOST, "--method", "GET"],
        &value.to_string(),
    )
}
fn identity() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (offline fixture)\n"),
        get("user", json!({"login": "octocat", "id": 42})),
    ]
}
fn refs(sha: Option<&str>) -> GhStep {
    get(
        "repos/example/repo/git/matching-refs/heads/feature",
        match sha {
            Some(sha) => json!([{"ref": "refs/heads/feature", "object": {"sha": sha}}]),
            None => json!([]),
        },
    )
}
fn open() -> GhStep {
    get(
        "repos/example/repo/pulls?state=open&head=example%3Afeature&per_page=10",
        json!([]),
    )
}
fn repository() -> GhStep {
    get(
        "repos/example/repo",
        json!({"full_name": "example/repo", "default_branch": "main", "archived": false}),
    )
}
fn checkout() -> (Fixture, String) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.git(&[
        "remote",
        "add",
        "origin",
        "https://github.invalid/example/repo.git",
    ]);
    fixture.commit_file("a.txt", b"base\n", "Base version");
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file("a.txt", b"feature\n", "Add the feature");
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    (fixture, head)
}
fn witness(fixture: &Fixture) -> (String, String, String) {
    (
        fixture.git(&["rev-parse", "HEAD"]),
        fixture.git(&["show-ref"]),
        fixture.git(&["status", "--porcelain=v2", "--branch"]),
    )
}
fn launch(fixture: &Fixture) -> Session {
    let mut session =
        Session::launch_from_with_args(fixture, &fixture.repo(), 120, 40, &["--host", HOST]);
    session.wait_for_screen("Recent commits");
    session
}
fn open_form(session: &mut Session, mouse: bool) {
    crate::local_management::operation(session, mouse, LocalCommand::CreatePullRequest);
    session.wait_for_screen("Pull request title:");
}
fn type_title(session: &mut Session, mouse: bool, title: &str) {
    if mouse {
        let (column, row) = session.locate("Pull request title:").unwrap();
        session.click(column, row + 1);
    }
    session.send_key(b"\x15");
    session.paste(title);
    session.wait_for_screen(title);
}
fn journal(session: &Session, fixture: &Fixture) -> Vec<Value> {
    session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn posts(records: &[Value]) -> usize {
    records
        .iter()
        .filter(|record| {
            record["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST")
        })
        .count()
}

fn journey(mouse: bool) {
    let (fixture, head) = checkout();
    let mut steps = identity();
    steps.push(repository());
    steps.push(refs(Some(&head)));
    steps.push(open());
    steps.push(get(
        "repos/example/repo/compare/main...feature",
        json!({
            "ahead_by": 1, "behind_by": 0,
            "commits": [{"sha": head, "commit": {"message": "Add the feature"}}],
            "files": [{"filename": "a.txt", "additions": 1, "deletions": 1}]
        }),
    ));
    let previewed = steps.len();
    steps.extend(identity());
    steps.push(refs(Some(&head)));
    steps.push(open());
    let mut post = GhStep::read(
        &[
            "api",
            "repos/example/repo/pulls",
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &json!({"number": 12, "html_url": "https://github.invalid/example/repo/pull/12", "draft": false})
            .to_string(),
    );
    post.stdin = serde_json::to_vec(&json!({
        "title": "Ship the feature", "head": "feature", "base": "main", "body": "", "draft": false
    }))
    .unwrap();
    steps.push(post);
    fixture.scripted_gh(&steps);
    let before = witness(&fixture);
    let mut session = launch(&fixture);

    // Cancel keeps the typed title as a local draft; nothing is sent.
    open_form(&mut session, mouse);
    type_title(&mut session, mouse, "Ship the feature");
    close(&mut session, mouse);
    assert!(journal(&session, &fixture).is_empty());
    open_form(&mut session, mouse);
    session.wait_for_screen("Ship the feature");

    preview(&mut session, mouse);
    session.wait_for_screen("Proposed: example:feature → main • ready for review");
    session.wait_for_screen("matches your checkout; nothing needs pushing");
    session.wait_for_screen("a.txt +1 -1");
    session.wait_for_screen("Confirm action only creates the pull request on GitHub");
    let records = journal(&session, &fixture);
    assert_eq!(records.len(), previewed);
    assert_eq!(posts(&records), 0, "preview never writes");

    session.resize(80, 24);
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.send_key(b"\x13");
    }
    session.wait_for_screen("Created pull request example/repo#12");
    let records = journal(&session, &fixture);
    assert_eq!(records.len(), steps.len());
    assert_eq!(posts(&records), 1, "exactly one create request");
    assert!(records.iter().all(|record| record["matched"] == true));
    assert_eq!(witness(&fixture), before, "checkout unchanged");
    session.quit_and_assert_restored_terminal(mouse);
}

fn unpushed_journey(mouse: bool) {
    let (fixture, _) = checkout();
    let mut steps = identity();
    steps.push(repository());
    steps.push(refs(None));
    fixture.scripted_gh(&steps);
    let before = witness(&fixture);
    let mut session = launch(&fixture);
    open_form(&mut session, mouse);
    type_title(&mut session, mouse, "Not pushed yet");
    if mouse {
        session.click_visible_action("[Preview]");
    } else {
        session.send_key(b"\x10");
    }
    session.wait_for_screen("Branch feature is not on GitHub");
    assert!(!session.screen().contains("READ-ONLY PREVIEW"));
    let records = journal(&session, &fixture);
    assert_eq!(records.len(), steps.len());
    assert_eq!(posts(&records), 0);
    assert_eq!(witness(&fixture), before, "nothing was pushed");
    close(&mut session, mouse);
    session.quit_and_assert_restored_terminal(mouse);
}

fn create_post(repo: &str, stdin: Value, number: u64) -> GhStep {
    let endpoint = format!("repos/{repo}/pulls");
    let mut post = GhStep::read(
        &["api", &endpoint, "--hostname", HOST, "--method", "POST", "--input", "-"],
        &json!({"number": number, "html_url": format!("https://github.invalid/{repo}/pull/{number}"), "draft": false})
            .to_string(),
    );
    post.stdin = serde_json::to_vec(&stdin).unwrap();
    post
}
/// Reviewers and the target choice by keys, or by clicks. The multiline
/// Description fills the form, so F6 (key or button) pages one field down:
/// four presses from the title focus Reviewers at the top.
fn choose_reviewers_and_parent(session: &mut Session, mouse: bool, reviewers: &str) {
    for _ in 0..4 {
        if mouse {
            session.click_visible_action("[F6 Fields↓]");
        } else {
            session.send_key(b"\x1b[17~");
        }
    }
    session.wait_for_screen("Reviewers to request (comma-separated");
    if mouse {
        let (column, row) = session
            .locate("Reviewers to request (comma-separated")
            .unwrap();
        session.click(column, row + 1);
    }
    session.send_key(b"\x15");
    session.paste(reviewers);
    session.wait_for_screen(reviewers);
    if mouse {
        session.click_visible_action("[parent]");
    } else {
        session.send_key(b"\t ");
    }
}

/// A fork's PR targets GitHub's recorded parent, then requests reviewers once.
fn fork_journey(mouse: bool) {
    let (fixture, head) = checkout();
    let open_upstream = get(
        "repos/upstream/repo/pulls?state=open&head=example%3Afeature&per_page=10",
        json!([]),
    );
    let mut steps = identity();
    steps.push(get(
        "repos/example/repo",
        json!({"full_name": "example/repo", "default_branch": "main", "archived": false, "fork": true,
               "parent": {"full_name": "upstream/repo", "default_branch": "trunk", "archived": false}}),
    ));
    steps.push(refs(Some(&head)));
    steps.push(open_upstream.clone());
    steps.push(get(
        "repos/upstream/repo/compare/trunk...example:feature",
        json!({"ahead_by": 1, "behind_by": 0,
               "commits": [{"sha": head, "commit": {"message": "Add the feature"}}],
               "files": [{"filename": "a.txt", "additions": 1, "deletions": 1}]}),
    ));
    let previewed = steps.len();
    steps.extend(identity());
    steps.push(refs(Some(&head)));
    steps.push(open_upstream);
    steps.push(create_post(
        "upstream/repo",
        json!({"title": "Ship from fork", "head": "example:feature", "base": "trunk", "body": "", "draft": false}),
        7,
    ));
    let mut request = GhStep::read(
        &[
            "api",
            "repos/upstream/repo/pulls/7/requested_reviewers",
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &json!({"number": 7, "requested_reviewers": [{"login": "alice"}], "requested_teams": []})
            .to_string(),
    );
    request.stdin =
        serde_json::to_vec(&json!({"reviewers": ["alice"], "team_reviewers": []})).unwrap();
    steps.push(request);
    fixture.scripted_gh(&steps);
    let before = witness(&fixture);
    let mut session = launch(&fixture);
    open_form(&mut session, mouse);
    type_title(&mut session, mouse, "Ship from fork");
    choose_reviewers_and_parent(&mut session, mouse, "alice");
    preview(&mut session, mouse);
    session.wait_for_screen("(fork example/repo) → upstream/repo:trunk");
    session.wait_for_screen("Reviewers to request after creation: alice");
    let records = journal(&session, &fixture);
    assert_eq!(records.len(), previewed);
    assert_eq!(posts(&records), 0, "preview never writes");
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.send_key(b"\x13");
    }
    session.wait_for_screen("Created pull request upstream/repo#7");
    session.wait_for_command_count(&fixture, steps.len());
    let records = journal(&session, &fixture);
    assert_eq!(records.len(), steps.len());
    assert_eq!(posts(&records), 2, "one create, then one reviewer request");
    assert!(records.iter().all(|record| record["matched"] == true));
    assert_eq!(witness(&fixture), before, "checkout unchanged");
    session.quit_and_assert_restored_terminal(mouse);
}

/// GitHub refuses the create: the typed draft stays, and nothing is retried.
fn failed_post_journey(mouse: bool) {
    let (fixture, head) = checkout();
    let mut steps = identity();
    steps.push(repository());
    steps.push(refs(Some(&head)));
    steps.push(open());
    steps.push(get(
        "repos/example/repo/compare/main...feature",
        json!({"ahead_by": 1, "behind_by": 0, "commits": [], "files": []}),
    ));
    steps.extend(identity());
    steps.push(refs(Some(&head)));
    steps.push(open());
    let mut post = create_post(
        "example/repo",
        json!({"title": "Refused title", "head": "feature", "base": "main", "body": "", "draft": false}),
        1,
    );
    post.stdout = b"{\"message\":\"Validation Failed\"}".to_vec();
    post.stderr = b"gh: Validation Failed (HTTP 422)\n".to_vec();
    post.status = 1;
    steps.push(post);
    fixture.scripted_gh(&steps);
    let before = witness(&fixture);
    let mut session = launch(&fixture);
    open_form(&mut session, mouse);
    type_title(&mut session, mouse, "Refused title");
    preview(&mut session, mouse);
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.send_key(b"\x13");
    }
    session.wait_for_screen("Pull request not created • draft kept locally");
    let records = journal(&session, &fixture);
    assert_eq!(records.len(), steps.len());
    assert_eq!(posts(&records), 1, "the refused create is not retried");
    close(&mut session, mouse);
    open_form(&mut session, mouse);
    session.wait_for_screen("Refused title");
    assert_eq!(journal(&session, &fixture).len(), steps.len());
    assert_eq!(witness(&fixture), before, "checkout unchanged");
    close(&mut session, mouse);
    session.quit_and_assert_restored_terminal(mouse);
}

fn inbox_pr(number: u64, title: &str, updated: &str) -> Value {
    json!({"node_id": format!("PR_example_repo_{number}"), "number": number, "title": title,
           "body": "", "repository_url": "https://api.github.invalid/repos/example/repo",
           "html_url": format!("https://github.invalid/example/repo/pull/{number}"),
           "user": {"login": "octocat", "type": "User"}, "state": "open", "draft": false,
           "created_at": "2026-10-01T00:00:00Z", "updated_at": updated})
}
fn search(queue: &str, items: Vec<Value>) -> GhStep {
    GhStep::read(
        &[
            "api",
            "search/issues",
            "--hostname",
            HOST,
            "--method",
            "GET",
            "-f",
            &format!("q=is:pr is:open {queue}:octocat"),
            "-f",
            "per_page=100",
            "-f",
            "page=1",
        ],
        &json!({"total_count": items.len(), "incomplete_results": false, "items": items})
            .to_string(),
    )
}
/// One Inbox load: the first adds the startup identity read before the
/// refresh identity; a reload re-reads only the refresh identity.
fn inbox_load(first: bool, authored: Vec<Value>) -> Vec<GhStep> {
    let mut steps = if first { identity() } else { Vec::new() };
    steps.extend(identity());
    steps.push(search("author", authored));
    steps.push(search("assignee", vec![]));
    steps.push(search("review-requested", vec![]));
    steps
}
fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label)
    } else {
        session.send_key(key)
    }
}

/// The created PR shows up in the already loaded authored queue: returning to
/// the Inbox reloads it once, without a manual Refresh.
fn authored_queue_journey(mouse: bool) {
    let (fixture, head) = checkout();
    ghtui::config::Config {
        host: HOST.into(),
        avatars: false,
        refresh_seconds: 3600,
        ..Default::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    let earlier = inbox_pr(3, "Earlier work", "2026-10-10T00:00:00Z");
    let mut steps = inbox_load(true, vec![earlier.clone()]);
    let loaded = steps.len();
    steps.extend(identity());
    steps.push(repository());
    steps.push(refs(Some(&head)));
    steps.push(open());
    steps.push(get(
        "repos/example/repo/compare/main...feature",
        json!({"ahead_by": 1, "behind_by": 0,
               "commits": [{"sha": head, "commit": {"message": "Add the feature"}}],
               "files": [{"filename": "a.txt", "additions": 1, "deletions": 1}]}),
    ));
    steps.extend(identity());
    steps.push(refs(Some(&head)));
    steps.push(open());
    steps.push(create_post(
        "example/repo",
        json!({"title": "Ship the feature", "head": "feature", "base": "main", "body": "", "draft": false}),
        12,
    ));
    let created = steps.len();
    steps.extend(inbox_load(
        false,
        vec![
            inbox_pr(12, "Ship the feature", "2026-10-20T00:00:00Z"),
            earlier,
        ],
    ));
    fixture.scripted_gh(&steps);
    let before = witness(&fixture);
    let mut session = launch(&fixture);

    choose(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Inbox • focused view");
    choose(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("1 unique PRs across 1 queue matches");
    session.wait_for_screen("Earlier work");
    session.wait_for_command_count(&fixture, loaded);

    choose(&mut session, mouse, "2 Repository", b"2");
    session.wait_for_screen("Recent commits");
    open_form(&mut session, mouse);
    type_title(&mut session, mouse, "Ship the feature");
    preview(&mut session, mouse);
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.send_key(b"\x13");
    }
    session.wait_for_screen("Created pull request example/repo#12");
    session.wait_for_command_count(&fixture, created);
    // A successful create closes the form by itself.
    assert!(!session.screen().contains("Pull request title:"));

    // No Refresh: the stale authored queue reloads once when the Inbox is shown.
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("2 unique PRs across 2 queue matches");
    choose(&mut session, mouse, "[c Created by me]", b"c");
    session.wait_for_screen("Queue: Created by me");
    session.wait_for_screen("Ship the feature");
    session.wait_for_screen("Earlier work");
    session.wait_for_command_count(&fixture, steps.len());
    let records = journal(&session, &fixture);
    assert_eq!(
        records.len(),
        steps.len(),
        "the Inbox reloaded exactly once"
    );
    assert_eq!(posts(&records), 1, "exactly one create request");
    assert!(records.iter().all(|record| record["matched"] == true));
    assert_eq!(witness(&fixture), before, "checkout unchanged");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn v1_pr_create_keyboard_new_pr_appears_in_authored_queue() {
    authored_queue_journey(false)
}
#[test]
fn v1_pr_create_mouse_new_pr_appears_in_authored_queue() {
    authored_queue_journey(true)
}
#[test]
fn v1_pr_create_keyboard_fork_parent_with_reviewers() {
    fork_journey(false)
}
#[test]
fn v1_pr_create_mouse_fork_parent_with_reviewers() {
    fork_journey(true)
}
#[test]
fn v1_pr_create_keyboard_failed_post_keeps_draft() {
    failed_post_journey(false)
}
#[test]
fn v1_pr_create_mouse_failed_post_keeps_draft() {
    failed_post_journey(true)
}
#[test]
fn v1_pr_create_keyboard_previews_then_creates_exactly_one_pr() {
    journey(false)
}
#[test]
fn v1_pr_create_mouse_previews_then_creates_exactly_one_pr() {
    journey(true)
}
#[test]
fn v1_pr_create_keyboard_unpushed_branch_explains_push() {
    unpushed_journey(false)
}
#[test]
fn v1_pr_create_mouse_unpushed_branch_explains_push() {
    unpushed_journey(true)
}
