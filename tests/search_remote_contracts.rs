#[path = "git_support/mod.rs"]
mod git_support;
#[path = "support/providers.rs"]
#[allow(dead_code)] // This contract target uses only the relevant strict provider fixture helpers.
mod support;
use ghtui::{
    github_workspace::{GithubWorkspace, PrKey, PrTab},
    providers::{
        gh::{GithubProvider, HostIdentity},
        runner::CancellationToken,
    },
    search::{model::Destination, workspace::SearchWorkspace, Query, SearchMode},
    storage::{Cache, Scope},
};
use serde_json::{json, Value};
use support::{Fixture, GhStep};

fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "alice".into(),
        account_id: Some(7),
        gh_version: "test gh".into(),
    }
}

fn discovered_pr() -> ghtui::providers::gh::RemoteSearchItem {
    serde_json::from_value(json!({
        "host":"github.invalid","repository":"org/app","kind":"PullRequests",
        "id":"PR_discovered","number":19,"title":"Discovered change","body":"search fragment",
        "url":"https://github.invalid/org/app/pull/19","state":"open","author":null,
        "repository_owner":null,"path":null,"blob_sha":null,"created_at":"","updated_at":"",
        "text_matches":[]
    }))
    .unwrap()
}

fn pr_details_steps() -> Vec<GhStep> {
    let summary = json!({"number":19,"node_id":"PR_discovered","title":"Discovered change","body":"Full PR body","state":"open","html_url":"https://github.invalid/org/app/pull/19","user":{"login":"contributor","type":"User"},"head":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","ref":"feature"},"base":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","ref":"main"},"changed_files":0});
    let read = |endpoint: &str, value: Value, paginated: bool| {
        let mut args = vec![
            "api".into(),
            endpoint.into(),
            "--hostname".into(),
            "github.invalid".into(),
            "--method".into(),
            "GET".into(),
        ];
        if paginated {
            args.extend(["--paginate".into(), "--slurp".into()]);
        }
        GhStep {
            args,
            stdin: vec![],
            stdout: serde_json::to_vec(&value).unwrap(),
            stderr: vec![],
            status: 0,
            delay_ms: 0,
        }
    };
    let mut steps = vec![read("repos/org/app/pulls/19", summary.clone(), false)];
    steps.push(read("repos/org/app/issues/19/comments?per_page=100",json!([[{"id":55,"node_id":"COMMENT_55","body":"First line\nSecond line","user":{"login":"contributor"},"updated_at":"2026-10-13T00:00:00Z"}]]),true));
    for resource in ["commits", "files", "reviews"] {
        steps.push(read(
            &format!("repos/org/app/pulls/19/{resource}?per_page=100"),
            json!([[]]),
            true,
        ));
    }
    steps.push(read(
        "repos/org/app/issues/19/timeline?per_page=100",
        json!([[]]),
        true,
    ));
    steps.push(read("repos/org/app/pulls/19", summary, false));
    steps
}

#[test]
fn query_modes_remote_discovered_pr_scope_rejects_before_any_gh_request() {
    let fixture = Fixture::new();
    let provider = provider(&fixture);
    let candidate = discovered_pr();
    let scoped = || GithubWorkspace::empty(identity(), vec!["org/app".into()], Default::default());
    let cancel = CancellationToken::default();
    for invalid in [
        {
            let mut item = candidate.clone();
            item.host = "other.invalid".into();
            item
        },
        {
            let mut item = candidate.clone();
            item.repository = "other/app".into();
            item
        },
        {
            let mut item = candidate.clone();
            item.number = Some(0);
            item
        },
        {
            let mut item = candidate.clone();
            item.number = None;
            item
        },
        {
            let mut item = candidate.clone();
            item.id.clear();
            item
        },
        {
            let mut item = candidate.clone();
            item.kind = ghtui::providers::gh::RemoteSearchKind::Issues;
            item
        },
    ] {
        assert!(scoped()
            .open_search_result(&invalid, &provider, &cancel)
            .is_err());
    }
    let mut unverified = scoped();
    unverified.identity.account_id = None;
    assert!(unverified
        .open_search_result(&candidate, &provider, &cancel)
        .is_err());
    let mut excluded = scoped();
    excluded.exclusions.insert("ORG/APP".into());
    assert!(excluded
        .open_search_result(&candidate, &provider, &cancel)
        .is_err());
    assert!(
        GithubWorkspace::empty(identity(), vec![], Default::default())
            .open_search_result(&candidate, &provider, &cancel)
            .is_err()
    );
    // Ordinary inbox opening is deliberately not broadened by this discovery entry point.
    assert!(scoped()
        .open(
            &PrKey {
                host: "github.invalid".into(),
                repository: "org/app".into(),
                number: 19
            },
            &provider,
            &cancel
        )
        .is_err());
    assert!(journal(&fixture).is_empty());
}

#[test]
fn query_modes_remote_discovered_pr_keeps_personal_queues_empty_and_exact_rows_refreshable() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&pr_details_steps());
    let provider = provider(&fixture);
    let mut workspace =
        GithubWorkspace::empty(identity(), vec!["org/app".into()], Default::default())
            .open_search_result(&discovered_pr(), &provider, &CancellationToken::default())
            .unwrap();
    assert!(workspace.visible().is_empty());
    assert!(workspace.inbox.pull_requests.is_empty());
    assert!(!workspace.needs_refresh);
    assert_eq!(workspace.selected.as_ref().unwrap().number, 19);
    workspace = workspace
        .load_tab(
            PrTab::Conversation,
            &provider,
            &CancellationToken::default(),
        )
        .unwrap();
    let page = workspace.pr.as_ref().unwrap();
    let selected = page
        .rows
        .iter()
        .filter_map(|row| row.comment.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(
        selected.len(),
        3,
        "Header and every body line retain the same exact target"
    );
    assert!(selected.iter().all(|selection| selection.observed.id == 55
        && selection.observed.node_id == "COMMENT_55"
        && selection.observed.body == "First line\nSecond line"));
    assert!(page
        .rows
        .iter()
        .filter(|row| row.label.contains("Full PR body"))
        .all(|row| row.comment.is_none()));
    let mut fresh = page.details.clone();
    fresh.comments[0].body = "Verified updated comment".into();
    fresh.summary.head_sha = "cccccccccccccccccccccccccccccccccccccccc".into();
    workspace.replace_pr_details(fresh.clone()).unwrap();
    let page = workspace.pr.as_ref().unwrap();
    assert_eq!(page.tab, PrTab::Conversation);
    assert!(page
        .rows
        .iter()
        .any(|row| row.label == "Verified updated comment"));
    assert_eq!(
        page.rows
            .iter()
            .filter_map(|row| row.comment.as_ref())
            .next()
            .unwrap()
            .observed
            .body,
        "Verified updated comment"
    );
    assert!(workspace.inbox.pull_requests.is_empty());
    fresh.summary.pr.id = "DIFFERENT_PR".into();
    assert!(workspace.replace_pr_details(fresh).is_err());
    assert_eq!(
        workspace.pr.as_ref().unwrap().details.summary.pr.id,
        "PR_discovered"
    );
    let records = journal(&fixture);
    assert_eq!(records.len(), 7);
    assert!(records
        .iter()
        .all(|record| record["matched"] == true && record["args"][5] == "GET"));
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn remote_workspace() -> SearchWorkspace {
    let mut workspace =
        SearchWorkspace::new(Scope::github("github.invalid", Some(7)).unwrap(), vec![]);
    workspace.set_mode(SearchMode::GitHub);
    workspace
}
fn step(endpoint: &str, query: &str, items: Vec<Value>, total: u64, incomplete: bool) -> GhStep {
    GhStep {
        args: vec![
            "api".into(),
            endpoint.into(),
            "--hostname".into(),
            "github.invalid".into(),
            "--method".into(),
            "GET".into(),
            "--header".into(),
            "Accept: application/vnd.github.text-match+json".into(),
            "-f".into(),
            format!("q={query}"),
            "-f".into(),
            "per_page=100".into(),
            "-f".into(),
            "page=1".into(),
        ],
        stdin: vec![],
        stdout: serde_json::to_vec(
            &json!({"total_count":total,"incomplete_results":incomplete,"items":items}),
        )
        .unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
fn journal(fixture: &Fixture) -> Vec<Value> {
    std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn query_modes_remote_adapter_rejects_unverified_empty_wrong_and_unsupported_scopes_before_gh() {
    let fixture = Fixture::new();
    let provider = provider(&fixture);
    let mut workspace = remote_workspace();
    let cancel = CancellationToken::default();
    for query in [
        "needle type:comment",
        "needle type:pr type:file",
        "needle rev:main",
    ] {
        assert!(
            workspace
                .search_github(query, &provider, &identity(), &["org/app".into()], &cancel)
                .is_err(),
            "{query}"
        );
    }
    assert!(workspace
        .search_github("needle", &provider, &identity(), &[], &cancel)
        .is_err());
    assert!(workspace
        .search_github(
            "needle type:repository",
            &provider,
            &identity(),
            &[],
            &cancel
        )
        .is_err());
    let mut missing = identity();
    missing.account_id = None;
    assert!(workspace
        .search_github("needle", &provider, &missing, &["org/app".into()], &cancel)
        .is_err());
    workspace.scope = Scope::github("github.invalid", Some(8)).unwrap();
    assert!(workspace
        .search_github(
            "needle",
            &provider,
            &identity(),
            &["org/app".into()],
            &cancel
        )
        .is_err());
    assert!(journal(&fixture).is_empty());
    assert!(workspace.page.is_none());
}

#[test]
fn query_modes_remote_code_preserves_blob_candidate_fragment_provenance_and_saved_scope() {
    let fixture = Fixture::new();
    let content = b"fn needle() {}\nexact remote bytes\n";
    let blob = String::from_utf8(
        git_support::git_input(&fixture.repo(), ["hash-object", "--stdin"], content).stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    let item = json!({"name":"日本語.rs","path":"src/日本語.rs","sha":blob,"html_url":"https://github.invalid/org/app/blob/main/src/file.rs","repository":{"full_name":"org/app","owner":{"login":"repo-owner","type":"Organization"}},"text_matches":[{"fragment":"fn needle() {}","property":"content","object_type":"FileContent","matches":[{"indices":[3,9]}]}]});
    fixture.scripted_gh(&[step(
        "search/code",
        "needle repo:org/app",
        vec![item],
        1001,
        true,
    )]);
    let mut workspace = remote_workspace();
    workspace
        .search_github(
            "needle type:file",
            &provider(&fixture),
            &identity(),
            &["org/app".into()],
            &CancellationToken::default(),
        )
        .unwrap();
    let hit = &workspace.page.as_ref().unwrap().hits[0];
    assert_eq!(hit.document.key.revision, format!("indexed-blob:{blob}"));
    assert_eq!((hit.line, hit.column), (0, 0));
    assert_eq!(hit.highlights, std::iter::once(3..9).collect::<Vec<_>>());
    assert!(
        hit.document.metadata.actors.is_empty(),
        "Repository ownership does not establish a file author"
    );
    assert!(
        matches!(hit.destination(), Destination::RemoteCodeCandidate { path, blob_sha, .. } if path.0 == "src/日本語.rs".as_bytes() && blob_sha == blob)
    );
    let mut falsely_resolved = (*hit.document).clone();
    falsely_resolved.destination = Destination::RemoteFile {
        host: "github.invalid".into(),
        repository: "org/app".into(),
        path: falsely_resolved.key.path.clone().unwrap(),
        revision: blob.clone(),
    };
    assert!(falsely_resolved.validate(&workspace.scope).is_err());
    assert!(!workspace.page.as_ref().unwrap().coverage.complete);
    assert!(workspace
        .page
        .as_ref()
        .unwrap()
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("1,000")));
    assert_eq!(
        workspace
            .remote_item(0)
            .unwrap()
            .repository_owner
            .as_ref()
            .unwrap()
            .login,
        "repo-owner"
    );
    assert_eq!(
        workspace.result_actors(0).unwrap()[0].0,
        "repository owner (not file author)"
    );
    assert_eq!(workspace.actors()[0].login, "repo-owner");
    workspace.preview(0).unwrap();
    let preview = workspace.preview.as_ref().unwrap();
    assert!(preview.context.contains("full-file location unknown"));
    assert!(preview.context.contains("not file author"));
    assert!(preview
        .lines
        .iter()
        .all(|line| line.number == 0 && !line.selected));
    assert!(preview
        .lines
        .iter()
        .any(|line| line.text.contains("fn needle()")));
    assert_eq!(
        journal(&fixture).len(),
        1,
        "Preview never resolves/fetches code automatically"
    );
    let resolved = ghtui::providers::gh::ResolvedRemoteFile {
        host: "github.invalid".into(),
        repository: "org/app".into(),
        path: "src/日本語.rs".into(),
        commit_sha: "a".repeat(40),
        blob_sha: blob.clone(),
    };
    assert!(workspace
        .open_remote_file(0, &resolved, b"substituted bytes")
        .is_err());
    workspace.open_remote_file(0, &resolved, content).unwrap();
    let exact = workspace.preview.as_ref().unwrap();
    assert!(
        matches!(&exact.destination, Destination::RemoteFile { revision, .. } if revision == &resolved.commit_sha)
    );
    assert_eq!(exact.lines[1].number, 2);
    assert_eq!(exact.lines[1].text, "exact remote bytes");
    assert!(exact.context.contains("immutable remote file"));
    assert!(
        matches!(
            workspace.destination(0).unwrap(),
            Destination::RemoteCodeCandidate { .. }
        ),
        "Opening retains the original indexed candidate for safe retry"
    );
    let mut other = resolved.clone();
    other.path = "other.rs".into();
    assert!(workspace.open_remote_file(0, &other, content).is_err());
    workspace.corpus_limits.max_file_bytes = 1;
    assert!(workspace
        .open_remote_file(0, &resolved, content)
        .unwrap_err()
        .0
        .contains("budget"));
    workspace.corpus_limits.max_file_bytes = 2 * 1024 * 1024;
    let cache = Cache::memory().unwrap();
    workspace.save(&cache, "Remote discovery", true).unwrap();
    let mut fresh = remote_workspace();
    fresh.load_saved(&cache).unwrap();
    fresh.restore_saved("Remote discovery").unwrap();
    assert_eq!(fresh.remote_repositories, ["org/app"]);
    assert_eq!(fresh.mode, SearchMode::GitHub);
    assert!(fresh.selections.is_empty() && fresh.warnings.is_empty());
    assert_eq!(fresh.saved[0].revision, None);
    assert!(fresh.saved[0].sources.is_empty());
}

#[test]
fn query_modes_remote_pr_comment_fragments_open_parent_item_and_keep_provider_order() {
    let fixture = Fixture::new();
    let item = |number: u64| json!({"id":number,"node_id":format!("PR{number}"),"number":number,"title":format!("PR {number}"),"repository_url":"https://api.github.invalid/repos/org/app","pull_request":{"url":"https://api.github.invalid/repos/org/app/pulls/1"},"state":"open","body":"parent body","user":{"login":"author","type":"User"},"text_matches":[{"fragment":"你好 needle comment","property":"body","object_type":"IssueComment","object_url":"https://api.github.invalid/repos/org/app/issues/comments/9","matches":[{"indices":[3,9]}]}]});
    fixture.scripted_gh(&[step(
        "search/issues",
        "needle repo:org/app is:pr",
        vec![item(8), item(2)],
        2,
        false,
    )]);
    let mut workspace = remote_workspace();
    workspace
        .search_github(
            "needle type:pr",
            &provider(&fixture),
            &identity(),
            &["org/app".into()],
            &CancellationToken::default(),
        )
        .unwrap();
    let hits = &workspace.page.as_ref().unwrap().hits;
    assert_eq!(
        hits.iter()
            .map(|hit| hit.document.title.as_str())
            .collect::<Vec<_>>(),
        ["PR 8", "PR 2"]
    );
    assert!(matches!(
        hits[0].destination(),
        Destination::PullRequest {
            number: 8,
            comment_id: None,
            thread_id: None,
            ..
        }
    ));
    assert_eq!(hits[0].document.metadata.actors[0].login, "author");
    assert!(
        hits[0].highlights.is_empty(),
        "Unspecified Unicode offset units must not become guessed byte highlights"
    );
    assert!(hits[0].matched_fields[0].contains("IssueComment"));
    assert!(workspace.remote_item(0).unwrap().text_matches[0]
        .object_url
        .ends_with("/comments/9"));
    assert_eq!(journal(&fixture).len(), 1);
    assert_eq!(workspace.actors().len(), 1);
    assert_eq!(workspace.result_actors(0).unwrap()[0].0, "author");
    workspace.scope = Scope::github("github.invalid", Some(8)).unwrap();
    assert!(
        workspace.actors().is_empty(),
        "Mismatched account never contributes private avatar actors"
    );
    assert!(workspace.result_actors(0).is_err());
}

#[test]
fn query_modes_remote_repository_discovery_requires_explicit_org_and_retains_owner_not_author() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[step("search/repositories", "needle org:org", vec![json!({"id":9,"node_id":"R9","full_name":"org/app","description":"needle repository","owner":{"login":"repo-owner","type":"Organization"}})], 1, false)]);
    let mut workspace = remote_workspace();
    workspace
        .search_github(
            "needle org:org type:repository",
            &provider(&fixture),
            &identity(),
            &[],
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(
        matches!(workspace.destination(0).unwrap(), Destination::Repository { repository, .. } if repository == "org/app")
    );
    assert!(workspace.page.as_ref().unwrap().hits[0]
        .document
        .metadata
        .actors
        .is_empty());
    assert_eq!(
        workspace
            .remote_item(0)
            .unwrap()
            .repository_owner
            .as_ref()
            .unwrap()
            .login,
        "repo-owner"
    );
    assert_eq!(journal(&fixture).len(), 1);
    assert!(Query::parse("needle org:org type:repository", SearchMode::GitHub).is_ok());
}

fn v1_search_issue(number: u64, repository: &str) -> Value {
    json!({"id":number,"node_id":format!("I{number}"),"number":number,"title":"Crash on empty input","repository_url":format!("https://api.github.invalid/repos/{repository}"),"html_url":format!("https://github.invalid/{repository}/issues/{number}"),"state":"open","body":"Steps to reproduce\nFULL ISSUE BODY LINE","user":{"login":"reporter","type":"User"},"text_matches":[{"fragment":"needle in the title","property":"title","object_type":"Issue","matches":[{"indices":[0,6]}]}]})
}

#[test]
fn v1_search_github_typed_repo_and_org_qualifiers_replace_default_scope_once() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        step(
            "search/issues",
            "needle repo:other/lib is:issue",
            vec![v1_search_issue(7, "other/lib")],
            1,
            false,
        ),
        step("search/issues", "needle org:acme is:pr", vec![], 0, false),
        step(
            "search/issues",
            "needle repo:org/app is:issue",
            vec![v1_search_issue(3, "org/app")],
            1,
            false,
        ),
    ]);
    let provider = provider(&fixture);
    let cancel = CancellationToken::default();
    let mut workspace = remote_workspace();
    workspace.remote_repositories = vec!["org/app".into()];
    let default = vec!["org/app".to_string()];
    workspace
        .search_github(
            "needle type:issue repo:other/lib",
            &provider,
            &identity(),
            &default,
            &cancel,
        )
        .unwrap();
    assert_eq!(workspace.page.as_ref().unwrap().hits.len(), 1);
    assert_eq!(
        workspace.remote_repositories, default,
        "Typed scope is per query"
    );
    workspace
        .search_github(
            "needle type:pr org:acme",
            &provider,
            &identity(),
            &default,
            &cancel,
        )
        .unwrap();
    assert!(workspace.page.as_ref().unwrap().hits.is_empty());
    workspace
        .search_github(
            "needle type:issue",
            &provider,
            &identity(),
            &default,
            &cancel,
        )
        .unwrap();
    assert_eq!(workspace.remote_repositories, default);
    let args = journal(&fixture)
        .iter()
        .map(|record| record["args"][9].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        args,
        [
            "q=needle repo:other/lib is:issue",
            "q=needle org:acme is:pr",
            "q=needle repo:org/app is:issue"
        ]
    );
}

#[test]
fn v1_search_github_issue_open_shows_full_description_and_url_without_failing() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[step(
        "search/issues",
        "needle repo:org/app is:issue",
        vec![v1_search_issue(3, "org/app")],
        1,
        false,
    )]);
    let mut workspace = remote_workspace();
    workspace
        .search_github(
            "needle type:issue",
            &provider(&fixture),
            &identity(),
            &["org/app".into()],
            &CancellationToken::default(),
        )
        .unwrap();
    workspace.preview(0).unwrap();
    let lines = workspace
        .preview
        .as_ref()
        .unwrap()
        .lines
        .iter()
        .map(|line| line.text.clone())
        .collect::<Vec<_>>();
    assert_eq!(lines[0], "org/app#3 • open");
    assert_eq!(lines[1], "https://github.invalid/org/app/issues/3");
    assert!(lines.iter().any(|line| line == "FULL ISSUE BODY LINE"));
    assert!(lines
        .iter()
        .any(|line| line.contains("needle in the title")));
    let status = workspace.open_remote_overview(0).unwrap().unwrap();
    assert!(
        status.starts_with("Showing org/app#3 in the preview"),
        "{status}"
    );
    assert!(status.contains("https://github.invalid/org/app/issues/3"));
    assert_eq!(
        journal(&fixture).len(),
        1,
        "Open makes no further gh request"
    );
}
