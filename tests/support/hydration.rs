//! Exact offline gh fragments for native opt-in discussion hydration journeys.
//! No default response and no mutation acceptance; GraphQL POST is a constant read.
#![allow(dead_code)]
use crate::support::GhStep;
use serde_json::{json, Value};

pub const HOST: &str = "github.invalid";
pub const REPOSITORY: &str = "example/repo";
pub const PR_NODE: &str = "PR_example/repo_7";
pub const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub const BASE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
pub const UPDATED: &str = "2026-10-13T00:00:00Z";

pub fn api_args(endpoint: &str) -> Vec<String> {
    ["api", endpoint, "--hostname", HOST, "--method", "GET"]
        .map(str::to_owned)
        .to_vec()
}
pub fn read(endpoint: &str, output: Value) -> GhStep {
    GhStep {
        args: api_args(endpoint),
        stdin: vec![],
        stdout: serde_json::to_vec(&output).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
pub fn account(id: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(
            &["--version"],
            "gh version 2.101.0 (offline hydration fixture)\n",
        ),
        read("user", json!({"id":id,"login":"octocat"})),
    ]
}
/// Identity before and after cached search/open. No new remote data is fabricated.
pub fn private_read(id: u64) -> Vec<GhStep> {
    let mut steps = account(id);
    steps.extend(account(id));
    steps
}
/// Current cached discussion admission/Open checks positive repository identity
/// and nullable read access independently before and after local collection.
pub fn private_source_read(id: u64) -> Vec<GhStep> {
    let mut steps = account(id);
    steps.push(read("repos/example/repo", repository()));
    steps.push(read("repos/example/repo", repository()));
    steps.extend(account(id));
    steps
}
pub fn repository() -> Value {
    json!({"id":410,"full_name":REPOSITORY,"private":true,"default_branch":"main","archived":false,
        "description":"Explicit selected hydration repository","permissions":{"pull":true,"push":true,"maintain":false,"admin":false}})
}
pub fn pr(body: &str) -> Value {
    json!({"id":700,"node_id":PR_NODE,"number":7,"title":"Native hydration PR","body":body,
        "url":format!("https://{HOST}/api/v3/repos/{REPOSITORY}/pulls/7"),
        "repository_url":format!("https://{HOST}/api/v3/repos/{REPOSITORY}"),
        "html_url":format!("https://{HOST}/{REPOSITORY}/pull/7"),
        "user":{"id":42,"login":"octocat","type":"User","avatar_url":null},"state":"open","draft":false,
        "created_at":"2026-10-01T00:00:00Z","updated_at":UPDATED,"labels":[{"name":"search"}],
        "head":{"sha":HEAD,"ref":"feature"},"base":{"sha":BASE,"ref":"main","repo":{"id":410,"full_name":REPOSITORY}},
        "changed_files":1,"additions":3,"deletions":1,"mergeable":true,"mergeable_state":"clean"})
}
pub fn issue(body: &str) -> Value {
    json!({"id":800,"node_id":"ISSUE_8","number":8,"title":"Native hydration issue","body":body,"state":"open",
        "url":format!("https://{HOST}/api/v3/repos/{REPOSITORY}/issues/8"),
        "repository_url":format!("https://{HOST}/api/v3/repos/{REPOSITORY}"),
        "html_url":format!("https://{HOST}/{REPOSITORY}/issues/8"),"user":null,"labels":[{"name":"search"}],
        "created_at":"2026-10-02T00:00:00Z","updated_at":UPDATED})
}
pub fn comment(id: u64, body: &str, issue: bool) -> Value {
    let number = if issue { 8 } else { 7 };
    let html_kind = if issue { "issues" } else { "pull" };
    json!({"id":id,"node_id":format!("COMMENT_{id}"),"body":body,
        "user":{"id":51,"login":"discussion-author","type":"User","avatar_url":null},
        "created_at":"2026-10-11T12:00:00Z","updated_at":"2026-10-12T12:00:00Z",
        "issue_url":format!("https://{HOST}/api/v3/repos/{REPOSITORY}/issues/{number}"),
        "html_url":format!("https://{HOST}/{REPOSITORY}/{html_kind}/{number}#issuecomment-{id}")})
}
pub fn review(id: u64, body: &str) -> Value {
    json!({"id":id,"node_id":format!("REVIEW_{id}"),"body":body,"state":"COMMENTED","commit_id":"cccccccccccccccccccccccccccccccccccccccc",
        "user":{"id":52,"login":"review-author","type":"User","avatar_url":null},
        "submitted_at":"2026-10-12T10:00:00Z","updated_at":"2026-10-12T12:00:00Z",
        "pull_request_url":format!("https://{HOST}/api/v3/repos/{REPOSITORY}/pulls/7"),
        "html_url":format!("https://{HOST}/{REPOSITORY}/pull/7#pullrequestreview-{id}")})
}
pub fn inline(id: u64, body: &str) -> Value {
    json!({"id":format!("INLINE_{id}"),"fullDatabaseId":id.to_string(),"body":body,
        "createdAt":"2026-10-11T12:00:00Z","updatedAt":"2026-10-12T12:00:00Z",
        "url":format!("https://{HOST}/{REPOSITORY}/pull/7#discussion_r{id}"),
        "author":{"__typename":"Bot","login":"ci-bot","avatarUrl":null},"commit":{"oid":HEAD},"pullRequest":{"id":PR_NODE}})
}
pub fn thread(comments: Vec<Value>, more_comments: bool) -> Value {
    json!({"id":"THREAD_1","path":"src/lib.rs","diffSide":"RIGHT","line":7,"startLine":null,
        "isResolved":false,"isOutdated":false,"viewerCanResolve":true,"viewerCanUnresolve":false,
        "pullRequest":{"id":PR_NODE},"repository":{"nameWithOwner":REPOSITORY},
        "comments":{"totalCount":comments.len()+usize::from(more_comments),"nodes":comments,
        "pageInfo":{"hasNextPage":more_comments,"endCursor":if more_comments {Some("INNER_NEXT")} else {None}}}})
}
pub fn thread_page(nodes: Vec<Value>, total: u64, more: bool, cursor: Option<&str>) -> Value {
    json!({"data":{"node":{"__typename":"PullRequest","id":PR_NODE,"number":7,"headRefOid":HEAD,"baseRefOid":BASE,
        "updatedAt":UPDATED,"repository":{"nameWithOwner":REPOSITORY},
        "reviewThreads":{"totalCount":total,"nodes":nodes,"pageInfo":{"hasNextPage":more,"endCursor":cursor}}}}})
}
pub fn graphql(cursor: Option<&str>, first: usize, inner: usize, output: Value) -> GhStep {
    let mut step = read("graphql", output);
    step.args[5] = "POST".into();
    step.args.extend(["--input".into(), "-".into()]);
    step.stdin = serde_json::to_vec(
        &json!({"query":ghtui::providers::gh::DISCUSSION_THREADS_QUERY,
        "variables":{"id":PR_NODE,"cursor":cursor,"first":first,"commentsFirst":inner}}),
    )
    .unwrap();
    step
}
pub fn pr_category(parent: &Value, category_reads: Vec<GhStep>, final_account: u64) -> Vec<GhStep> {
    let mut steps = account(42);
    steps.push(read("repos/example/repo/pulls/7", parent.clone()));
    steps.extend(category_reads);
    steps.push(read("repos/example/repo/pulls/7", parent.clone()));
    steps.extend(account(final_account));
    steps
}
/// Entire root Load happy path (29 calls with one page per category).
/// Caller supplies exact snapshot nodes, including observed excluded edits.
pub fn pr_load(
    parent: &Value,
    comments: Vec<Value>,
    reviews: Vec<Value>,
    threads: Vec<Value>,
    final_account: u64,
) -> Vec<GhStep> {
    let mut steps = account(42);
    steps.extend([
        read("repos/example/repo", repository()),
        read("repos/example/repo/pulls/7", parent.clone()),
    ]);
    steps.extend(pr_category(
        parent,
        vec![read(
            "repos/example/repo/issues/7/comments?per_page=50&page=1",
            json!(comments),
        )],
        42,
    ));
    steps.extend(pr_category(
        parent,
        vec![read(
            "repos/example/repo/pulls/7/reviews?per_page=50&page=1",
            json!(reviews),
        )],
        42,
    ));
    let total = threads.len() as u64;
    steps.extend(pr_category(
        parent,
        vec![graphql(
            None,
            50,
            20,
            thread_page(threads, total, false, None),
        )],
        42,
    ));
    steps.extend(private_read(final_account));
    steps
}
/// Entire root Load issue happy path: initial3 + preflight7 + details9 + final4.
pub fn issue_load(parent: &Value, comments: Vec<Value>, final_account: u64) -> Vec<GhStep> {
    let mut steps = account(42);
    steps.push(read("repos/example/repo", repository()));
    steps.extend(account(42));
    steps.extend([
        read("repos/example/repo", repository()),
        read("repos/example/repo/issues/8", parent.clone()),
        read("repos/example/repo", repository()),
    ]);
    steps.extend(account(42));
    steps.extend(account(42));
    steps.extend([
        read("repos/example/repo", repository()),
        read("repos/example/repo/issues/8", parent.clone()),
        read(
            "repos/example/repo/issues/8/comments?per_page=50&page=1",
            json!(comments),
        ),
        read("repos/example/repo/issues/8", parent.clone()),
        read("repos/example/repo", repository()),
    ]);
    steps.extend(account(42));
    steps.extend(private_read(final_account));
    steps
}
pub fn read_failure(endpoint: &str, status: i32, diagnostic: &str) -> GhStep {
    let mut step = read(endpoint, Value::Null);
    step.status = status;
    step.stderr = diagnostic.as_bytes().to_vec();
    step
}
/// Account is deliberately fresh after the selected repository read was denied.
pub fn repository_denied(http_status: u16, final_account: u64) -> Vec<GhStep> {
    let mut steps = account(42);
    steps.push(read_failure(
        "repos/example/repo",
        1,
        &format!("gh: Selected repository inaccessible (HTTP {http_status})"),
    ));
    steps.extend(account(final_account));
    steps
}
