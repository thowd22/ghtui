#[path = "support/providers.rs"]
mod support;
use ghtui::{
    pr_progress,
    providers::{
        gh::{GithubProvider, HostIdentity, PrSummary},
        runner::CancellationToken,
    },
};
use serde_json::{json, Value};
use support::{Fixture, GhStep};

fn git(fixture: &Fixture, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(fixture.repo())
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn fixture() -> Fixture {
    let fixture = Fixture::new();
    git(&fixture, &["init", "--initial-branch=main"]);
    std::fs::write(
        fixture.repo().join("baseline.txt"),
        b"original saved bytes\n",
    )
    .unwrap();
    git(&fixture, &["add", "--", "baseline.txt"]);
    git(
        &fixture,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "Untouched baseline",
        ],
    );
    std::fs::write(fixture.repo().join("dirty.txt"), b"leave me alone\n").unwrap();
    fixture
}

const QUERY:&str="query($id:ID!,$cursor:String){node(id:$id){... on PullRequest{files(first:100,after:$cursor){nodes{path viewerViewedState} pageInfo{hasNextPage endCursor}}}}}";
fn identity(id: u64) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(id),
        gh_version: "fixture".into(),
    }
}
fn target() -> PrSummary {
    serde_json::from_value(json!({"pr":{"host":"github.invalid","id":"PR_7","repository":"example/repo","number":7,"title":"Private progress","body":"original","url":"https://github.invalid/example/repo/pull/7","author":{"login":"alice","actor_type":"User"},"state":"open","draft":false,"created_at":"2026-10-13T00:00:00Z","updated_at":"2026-10-13T00:01:00Z","queues":[]},"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean","additions":1,"deletions":1,"changed_files":1})).unwrap()
}
fn raw(head: &str) -> Value {
    json!({"node_id":"PR_7","number":7,"title":"Private progress","body":"original","repository_url":"https://api.github.invalid/repos/example/repo","html_url":"https://github.invalid/example/repo/pull/7","user":{"id":51,"login":"alice","type":"User"},"state":"open","draft":false,"head":{"sha":head,"ref":"feature"},"base":{"sha":"b".repeat(40),"ref":"main"},"changed_files":1,"additions":1,"deletions":1})
}
fn read_step(endpoint: &str, value: Value) -> GhStep {
    GhStep {
        args: [
            "api",
            endpoint,
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ]
        .map(str::to_owned)
        .to_vec(),
        stdin: vec![],
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
fn account(id: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read_step("user", json!({"id":id,"login":"octocat"})),
    ]
}
fn files() -> GhStep {
    GhStep {args:["api","graphql","--hostname","github.invalid","--method","POST","--input","-"].map(str::to_owned).to_vec(),stdin:serde_json::to_vec(&json!({"query":QUERY,"variables":{"id":"PR_7","cursor":null}})).unwrap(),stdout:serde_json::to_vec(&json!({"data":{"node":{"files":{"nodes":[{"path":"private/α file.rs","viewerViewedState":"DISMISSED"}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}})).unwrap(),stderr:vec![],status:0,delay_ms:0}
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn assert_commands(fixture: &Fixture, count: usize) {
    assert_eq!(
        std::fs::read(fixture.repo().join("dirty.txt")).unwrap(),
        b"leave me alone\n"
    );
    assert_eq!(
        git(fixture, &["show", "HEAD:baseline.txt"]),
        "original saved bytes\n"
    );
    assert_eq!(git(fixture, &["rev-list", "--count", "HEAD"]), "1\n");
    assert_eq!(git(fixture, &["diff", "--exit-code", "--staged"]), "");
    assert_eq!(
        git(fixture, &["status", "--porcelain=v1"]),
        "?? dirty.txt\n"
    );
    let records = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), count);
    assert!(records.iter().all(|row| row["matched"] == true));
    for row in records {
        if row["args"][1] == "graphql" {
            assert!(!row["stdin_hex"].as_str().unwrap().is_empty());
        } else {
            assert!(row["args"][5].is_null() || row["args"][5] == "GET");
        }
    }
    assert!(!fixture.root.join("config/ghtui/cache.sqlite3").exists());
}
fn successful() -> Vec<GhStep> {
    let mut steps = account(42);
    steps.push(read_step(
        "repos/example/repo/pulls/7",
        raw(&"a".repeat(40)),
    ));
    steps.push(files());
    steps.push(read_step(
        "repos/example/repo/pulls/7",
        raw(&"a".repeat(40)),
    ));
    steps.extend(account(42));
    steps
}

#[test]
fn viewed_files_progress_provider_reads_have_exact_pre_post_identity_and_pr_brackets() {
    let fixture = fixture();
    fixture.scripted_gh(&successful());
    let progress = pr_progress::read(
        &provider(&fixture),
        &identity(42),
        &target(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(progress.complete());
    assert_eq!(progress.files()[0].path, "private/α file.rs");
    assert_eq!(progress.files()[0].state, pr_progress::ViewedState::Changed);
    assert!(progress.rows()[2].0.contains("CHANGED SINCE VIEWED"));
    assert_commands(&fixture, 7);
}

#[test]
fn viewed_files_progress_provider_changed_head_before_read_refuses_without_file_query() {
    let fixture = fixture();
    let mut steps = account(42);
    steps.push(read_step(
        "repos/example/repo/pulls/7",
        raw(&"c".repeat(40)),
    ));
    steps.extend(account(42));
    fixture.scripted_gh(&steps);
    let error = pr_progress::read(
        &provider(&fixture),
        &identity(42),
        &target(),
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(error.contains("base/head changed"));
    assert!(!error.contains("private/α"));
    assert_commands(&fixture, 5);
}

#[test]
fn viewed_files_progress_provider_changed_head_after_read_never_installs_old_viewed_flags() {
    let fixture = fixture();
    let mut steps = successful();
    steps[4] = read_step("repos/example/repo/pulls/7", raw(&"c".repeat(40)));
    fixture.scripted_gh(&steps);
    let error = pr_progress::read(
        &provider(&fixture),
        &identity(42),
        &target(),
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(error.contains("base/head changed"));
    assert!(!error.contains("private/α"));
    assert_commands(&fixture, 7);
}

#[test]
fn viewed_files_progress_provider_final_reused_login_or_401_hides_private_result() {
    for unavailable in [false, true] {
        let fixture = fixture();
        let mut steps = successful();
        if unavailable {
            steps[6].stdout.clear();
            steps[6].stderr =
                b"gh: private progress diagnostic must stay hidden (HTTP 401)".to_vec();
            steps[6].status = 1;
        } else {
            steps[6] = read_step("user", json!({"id":43,"login":"octocat"}));
        }
        fixture.scripted_gh(&steps);
        let error = pr_progress::read(
            &provider(&fixture),
            &identity(42),
            &target(),
            &CancellationToken::default(),
        )
        .unwrap_err();
        assert!(error.contains("Account changed or identity unavailable"));
        assert!(!error.contains("private/α"));
        assert!(!error.contains("diagnostic"));
        assert_commands(&fixture, 7);
    }
}
