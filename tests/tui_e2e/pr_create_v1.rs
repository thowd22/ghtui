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
