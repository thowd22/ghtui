//! Strict offline fake-gh contracts for creating a PR from the current branch.
use super::*;
use crate::test_support::{Fixture, GhStep};
use serde_json::json;
use std::process::Command;

const HOST: &str = "github.invalid";

fn git(fixture: &Fixture, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(fixture.repo())
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
/// A checkout on `feature` with a GitHub origin; returns its HEAD.
fn checkout(fixture: &Fixture) -> String {
    git(fixture, &["init", "--initial-branch=main"]);
    git(fixture, &["config", "user.name", "Fixture"]);
    git(
        fixture,
        &["config", "user.email", "fixture@example.invalid"],
    );
    git(
        fixture,
        &[
            "remote",
            "add",
            "origin",
            "https://github.invalid/example/repo.git",
        ],
    );
    std::fs::write(fixture.repo().join("a.txt"), "a\n").unwrap();
    git(fixture, &["add", "a.txt"]);
    git(fixture, &["commit", "-m", "Base"]);
    git(fixture, &["switch", "-c", "feature"]);
    std::fs::write(fixture.repo().join("a.txt"), "b\n").unwrap();
    git(fixture, &["commit", "-am", "Change a"]);
    git(fixture, &["rev-parse", "HEAD"])
}
fn context(fixture: &Fixture) -> LocalContext {
    let workspace = crate::workspace::Workspace::load_with_cancellation(
        &fixture.repo(),
        crate::workspace::RepositoryMode::Overview,
        "HEAD",
        None,
        CancellationToken::default(),
    )
    .unwrap()
    .unwrap();
    LocalContext::from_workspace(&workspace, None, None)
}
fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &["api", endpoint, "--hostname", HOST, "--method", "GET"],
        &value.to_string(),
    )
}
fn identity(login: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (offline fixture)\n"),
        get("user", json!({"login": login, "id": 42})),
    ]
}
fn refs(sha: Option<&str>) -> GhStep {
    get(
        "repos/example/repo/git/matching-refs/heads/feature",
        match sha {
            Some(sha) => json!([
                {"ref": "refs/heads/feature-old", "object": {"sha": "0".repeat(40)}},
                {"ref": "refs/heads/feature", "object": {"sha": sha}}
            ]),
            None => json!([]),
        },
    )
}
fn open(prs: Value) -> GhStep {
    get(
        "repos/example/repo/pulls?state=open&head=example%3Afeature&per_page=10",
        prs,
    )
}
fn preview_steps(sha: &str) -> Vec<GhStep> {
    let mut steps = identity("octocat");
    steps.push(get(
        "repos/example/repo",
        json!({"full_name": "example/repo", "default_branch": "main", "archived": false}),
    ));
    steps.push(refs(Some(sha)));
    steps.push(open(json!([])));
    steps.push(get(
        "repos/example/repo/compare/main...feature",
        json!({
            "ahead_by": 1, "behind_by": 0,
            "commits": [{"sha": sha, "commit": {"message": "Change a\n\nbody"}}],
            "files": [{"filename": "a.txt", "additions": 1, "deletions": 1}]
        }),
    ));
    steps
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new(HOST)
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn values(title: &str, base: &str, draft: bool) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("title".into(), title.into()),
        ("body".into(), "Why this changes".into()),
        ("base".into(), base.into()),
        ("draft".into(), draft.to_string()),
    ])
}
fn journal(fixture: &Fixture) -> String {
    std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap_or_default()
}

#[test]
fn v1_pr_create_preview_reads_pushed_branch_without_writing() {
    let fixture = Fixture::new();
    let head = checkout(&fixture);
    fixture.scripted_gh(&preview_steps(&head));
    let plan = PrCreatePlanV1::prepare(
        &context(&fixture),
        &values("Add a", "", true),
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(plan.repository, "example/repo");
    assert_eq!(
        (plan.head.as_str(), plan.base.as_str()),
        ("feature", "main")
    );
    assert!(plan.draft);
    let text = plan.preview.join("\n");
    assert!(
        text.contains("Account: octocat on github.invalid"),
        "{text}"
    );
    assert!(text.contains("Proposed: example:feature → main • draft"));
    assert!(text.contains("matches your checkout; nothing needs pushing"));
    assert!(text.contains("Change a"));
    assert!(text.contains("a.txt +1 -1"));
    assert!(!journal(&fixture).contains("POST"));
    assert!(journal(&fixture)
        .lines()
        .all(|line| line.contains("\"matched\":true")));
}

#[test]
fn v1_pr_create_unpushed_branch_explains_push_instead_of_guessing() {
    let fixture = Fixture::new();
    checkout(&fixture);
    let mut steps = preview_steps("x");
    steps.truncate(3);
    steps.push(refs(None));
    fixture.scripted_gh(&steps);
    let error = PrCreatePlanV1::prepare(
        &context(&fixture),
        &values("Add a", "", false),
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(error.contains("not on GitHub"), "{error}");
    assert!(error.contains("Push branch"), "{error}");
    assert!(error.contains("Nothing was pushed or created"), "{error}");
}

#[test]
fn v1_pr_create_duplicate_open_pr_and_missing_title_are_refused() {
    let fixture = Fixture::new();
    let head = checkout(&fixture);
    assert_eq!(
        PrCreatePlanV1::prepare(
            &context(&fixture),
            &values(" ", "", false),
            &provider(&fixture),
            &CancellationToken::default(),
        )
        .unwrap_err(),
        "Title is required."
    );
    let mut steps = preview_steps(&head);
    steps.truncate(4);
    steps.push(open(
        json!([{"number": 5, "html_url": "https://github.invalid/example/repo/pull/5"}]),
    ));
    fixture.scripted_gh(&steps);
    let error = PrCreatePlanV1::prepare(
        &context(&fixture),
        &values("Add a", "", false),
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(error.contains("#5 is already open"), "{error}");
}

#[test]
fn v1_pr_create_submit_rechecks_then_posts_once() {
    let fixture = Fixture::new();
    let head = checkout(&fixture);
    let mut steps = preview_steps(&head);
    steps.extend(identity("octocat"));
    steps.push(refs(Some(&head)));
    steps.push(open(json!([])));
    let payload = json!({"title": "Add a", "head": "feature", "base": "main", "body": "Why this changes", "draft": false});
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
        &json!({"number": 12, "html_url": "https://github.invalid/example/repo/pull/12", "draft": false}).to_string(),
    );
    post.stdin = serde_json::to_vec(&payload).unwrap();
    steps.push(post);
    fixture.scripted_gh(&steps);
    let provider = provider(&fixture);
    let cancel = CancellationToken::default();
    let plan = PrCreatePlanV1::prepare(
        &context(&fixture),
        &values("Add a", "", false),
        &provider,
        &cancel,
    )
    .unwrap();
    let created = plan.submit(&provider, &cancel).unwrap();
    assert_eq!(created.number, 12);
    assert_eq!(created.url, "https://github.invalid/example/repo/pull/12");
    let journal = journal(&fixture);
    assert_eq!(journal.matches("POST").count(), 1);
    assert!(journal
        .lines()
        .all(|line| line.contains("\"matched\":true")));
}

#[test]
fn v1_pr_create_submit_refuses_when_github_branch_moved() {
    let fixture = Fixture::new();
    let head = checkout(&fixture);
    let mut steps = preview_steps(&head);
    steps.extend(identity("octocat"));
    steps.push(refs(Some(&"f".repeat(40))));
    fixture.scripted_gh(&steps);
    let provider = provider(&fixture);
    let cancel = CancellationToken::default();
    let plan = PrCreatePlanV1::prepare(
        &context(&fixture),
        &values("Add a", "", false),
        &provider,
        &cancel,
    )
    .unwrap();
    let error = plan.submit(&provider, &cancel).unwrap_err();
    assert!(error.contains("moved from"), "{error}");
    assert!(!journal(&fixture).contains("POST"));
}
