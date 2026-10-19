//! Real remote-discovery terminal journeys, strict gh scopes and independently verified bytes.
use crate::support::{Fixture, GhStep, Session};
use base64::{engine::general_purpose::STANDARD, Engine};
use ghtui::{
    config::Config,
    storage::{Cache, Scope},
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs};

fn read(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &[
            "api",
            endpoint,
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ],
        &value.to_string(),
    )
}
fn identity() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"login":"alice","id":42})),
    ]
}
fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label)
    } else {
        session.send_key(key)
    };
}
fn query(session: &mut Session, mouse: bool, value: &str) {
    choose(session, mouse, "[/ Query]", b"/");
    session.send_key(b"\x01");
    session.paste(value);
    choose(session, mouse, "[Enter Search]", b"\r");
}
fn journal(session: &mut Session, fixture: &Fixture, count: usize) -> Vec<Value> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        let rows = session
            .inspect_command_journal(fixture)
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect::<Vec<_>>();
        if rows.len() == count {
            return rows;
        }
        assert!(
            rows.len() < count,
            "Unexpected gh request count: {} > {count}",
            rows.len()
        );
        assert!(
            std::time::Instant::now() < deadline,
            "Missing gh calls: {} of {count}",
            rows.len()
        );
        session.poll_output(std::time::Duration::from_millis(20));
    }
}
#[test]
fn query_modes_remote_keyboard_blob_fragments_exact_open_saved_scope_and_identity_loss() {
    journey(false)
}
#[test]
fn query_modes_remote_mouse_blob_fragments_exact_open_saved_scope_and_identity_loss() {
    journey(true)
}

#[test]
fn query_modes_remote_keyboard_discovered_pr_opens_exact_native_conversation_without_queue_fabrication(
) {
    discovered_pr_journey(false)
}

#[test]
fn query_modes_remote_mouse_discovered_pr_opens_exact_native_conversation_without_queue_fabrication(
) {
    discovered_pr_journey(true)
}

fn discovered_pr_journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("local.txt", b"saved local bytes\n", "Local fixture");
    fs::write(fixture.repo().join("local.txt"), b"dirty local bytes\n").unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    Config {
        host: "github.invalid".into(),
        avatars: false,
        active_set: Some("selected".into()),
        repository_sets: BTreeMap::from([("selected".into(), vec!["org/app".into()])]),
        local_repositories: BTreeMap::from([("org/app".into(), fixture.repo())]),
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    let summary = json!({"number":19,"node_id":"PR_discovered","title":"DISCUSS discovered change","body":"Full remote PR description","state":"open","repository_url":"https://api.github.invalid/repos/org/app","html_url":"https://github.invalid/org/app/pull/19","pull_request":{"url":"https://api.github.invalid/repos/org/app/pulls/19"},"user":{"login":"contributor","type":"User"},"head":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","ref":"feature"},"base":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","ref":"main"},"changed_files":0,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","text_matches":[{"fragment":"DISCUSS precise comment fragment","property":"body","object_type":"IssueComment","object_url":"https://api.github.invalid/repos/org/app/issues/comments/55","matches":[{"indices":[0,7]}]}]});
    let mut steps = identity();
    steps.extend(identity()); // Explicit GitHub mode scope.
    steps.extend(identity());
    steps.push(GhStep::read(
        &[
            "api",
            "search/issues",
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
            "--header",
            "Accept: application/vnd.github.text-match+json",
            "-f",
            "q=DISCUSS repo:org/app is:pr",
            "-f",
            "per_page=100",
            "-f",
            "page=1",
        ],
        &json!({"total_count":1,"incomplete_results":false,"items":[summary.clone()]}).to_string(),
    ));
    steps.extend(identity());
    steps.extend(identity()); // Exact native PR opening.
    steps.push(read("repos/org/app/pulls/19", summary.clone()));
    let paginated = |endpoint: &str, value: Value| {
        let mut step = read(endpoint, value);
        step.args.extend(["--paginate".into(), "--slurp".into()]);
        step
    };
    steps.push(paginated("repos/org/app/issues/19/comments?per_page=100",json!([[{"id":55,"node_id":"COMMENT_55","body":"DISCUSS complete selected comment\nSecond comment line","user":{"login":"reviewer","type":"User"},"updated_at":"2026-10-13T00:00:00Z"}]])));
    for resource in ["commits", "files", "reviews"] {
        steps.push(paginated(
            &format!("repos/org/app/pulls/19/{resource}?per_page=100"),
            json!([[]]),
        ));
    }
    steps.push(paginated(
        "repos/org/app/issues/19/timeline?per_page=100",
        json!([[]]),
    ));
    steps.push(read("repos/org/app/pulls/19", summary));
    steps.extend(identity());
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    choose(&mut session, mouse, "3 Search", b"3");
    session.wait_for_screen("Selected local search corpus ready");
    choose(&mut session, mouse, "h GitHub", b"h");
    session.wait_for_screen("GitHub scope verified");
    query(&mut session, mouse, "DISCUSS type:pr");
    session.wait_for_screen("1 results • GitHub");
    session.wait_for_screen("@contributor");
    session.wait_for_screen("DISCUSS precise comment fragment");
    journal(&mut session, &fixture, 9);
    choose(&mut session, mouse, "[o Open result]", b"o");
    session.wait_for_screen("Opened exact GitHub PR search destination");
    session.assert_focused_view("Inbox");
    session.wait_for_screen("org/app#19");
    session.wait_for_screen("Full remote PR description");
    journal(&mut session, &fixture, 20);
    session.resize(80, 24);
    choose(&mut session, mouse, "[c Conversation]", b"c");
    session.wait_for_screen("DISCUSS complete selected comment");
    session.wait_for_screen("Second comment line");
    session.wait_for_screen("@reviewer");
    // Discovery is not an authored/assigned personal queue: Back returns an empty scoped inbox.
    choose(&mut session, mouse, "[b Back]", b"b");
    session.wait_for_screen("No pull requests in this queue / scope.");
    choose(&mut session, mouse, "3 Search", b"3");
    session.wait_for_screen("1 results • GitHub");
    session.wait_for_screen("DISCUSS type:pr");
    let records = journal(&mut session, &fixture, 20);
    assert!(records.iter().all(|record| record["matched"] == true));
    assert!(records
        .iter()
        .all(|record| record["args"][0] == "--version" || record["args"][5] == "GET"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("local.txt")).unwrap(),
        b"dirty local bytes\n"
    );
    session.quit_and_assert_restored_terminal(mouse);
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    let content = (1..=70)
        .map(|number| format!("REMOTE LINE {number:02} • verified saved bytes\n"))
        .collect::<String>();
    fixture.commit_file("src/日本語.rs", content.as_bytes(), "Remote blob fixture");
    let sha = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let blob = fixture
        .git(&["rev-parse", "HEAD:src/日本語.rs"])
        .trim()
        .to_owned();
    fs::write(
        fixture.repo().join("src/日本語.rs"),
        b"local dirty contents must not be replaced\n",
    )
    .unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let dirty = fs::read(fixture.repo().join("src/日本語.rs")).unwrap();
    Config {
        host: "github.invalid".into(),
        avatars: false,
        active_set: Some("selected".into()),
        repository_sets: BTreeMap::from([("selected".into(), vec!["example/repo".into()])]),
        local_repositories: BTreeMap::from([("example/repo".into(), fixture.repo())]),
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    let mut steps = identity();
    steps.extend(identity()); // Mode loads verified scope/saved queries, no search.
    steps.extend(identity());
    steps.push(GhStep::read(&["api","search/code","--hostname","github.invalid","--method","GET","--header","Accept: application/vnd.github.text-match+json","-f","q=REMOTE repo:example/repo","-f","per_page=100","-f","page=1"],&json!({"total_count":1,"incomplete_results":false,"items":[{"name":"日本語.rs","path":"src/日本語.rs","sha":blob,"repository":{"full_name":"example/repo","owner":{"login":"repo-owner","type":"Organization"}},"text_matches":[{"fragment":"REMOTE LINE 19 • verified saved bytes","property":"content","object_type":"FileContent","matches":[{"indices":[0,6]}]}]}]}).to_string()));
    steps.extend(identity());
    steps.extend(identity());
    steps.extend([
        read("repos/example/repo",json!({"id":1,"full_name":"example/repo","default_branch":"main","permissions":{"pull":true}})),
        read("repos/example/repo/commits/main",json!({"sha":sha})),
        read(&format!("repos/example/repo/contents/src/%E6%97%A5%E6%9C%AC%E8%AA%9E.rs?ref={sha}"),json!({"type":"file","path":"src/日本語.rs","sha":blob,"size":content.len(),"encoding":"base64","content":STANDARD.encode(content.as_bytes())})),
    ]);
    steps.extend(identity());
    steps.extend(identity());
    steps.extend(identity()); // Saving is local, verified account boundary.
    steps.push(GhStep::read(&["--version"], "gh version 2.101.0\n"));
    let mut failure = read("user", Value::Null);
    failure.status = 1;
    failure.stderr = b"fixture auth expired\n".to_vec();
    steps.push(failure);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    choose(&mut session, mouse, "3 Search", b"3");
    session.wait_for_screen("Selected local search corpus ready");
    choose(&mut session, mouse, "h GitHub", b"h");
    session.wait_for_screen("GitHub scope verified");
    journal(&mut session, &fixture, 4);
    query(&mut session, mouse, "REMOTE type:file");
    session.wait_for_screen("1 results • GitHub");
    session.wait_for_screen("provider fragment");
    session.wait_for_screen("@repo-owner");
    session.wait_for_screen("Owner (not author)");
    assert!(!session.screen().contains(":0:0"));
    journal(&mut session, &fixture, 9);
    choose(&mut session, mouse, "[o Open result]", b"o");
    session.wait_for_screen("Opened verified GitHub saved file");
    session.wait_for_screen("REMOTE LINE 01");
    journal(&mut session, &fixture, 16);
    for _ in 0..8 {
        choose(&mut session, mouse, "[Ctrl↓ More]", b"\x1b[1;5B");
    }
    session.wait_for_screen("REMOTE LINE 25");
    choose(&mut session, mouse, "[z Name]", b"z");
    session.send_key(b"\x01");
    session.paste("remote exact scope");
    if !mouse {
        session.send_key(b"\t");
    }
    choose(&mut session, mouse, "[f Save query]", b"f");
    session.wait_for_screen("Saved query: remote exact scope");
    journal(&mut session, &fixture, 20);
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let saved_query = || {
        cache
            .saved_views(&scope)
            .unwrap()
            .into_iter()
            .find(|(name, _)| name == "search:remote exact scope")
            .unwrap()
            .1
    };
    let saved = saved_query();
    assert_eq!(saved["repositories"], json!(["example/repo"]));
    assert_eq!(
        saved["mode"],
        serde_json::to_value(ghtui::search::SearchMode::GitHub).unwrap()
    );
    cache
        .put_draft(&scope, "private-proof", "Retained private draft")
        .unwrap();
    session.resize(80, 24);
    // At 80 columns the Search key bar keeps its own actions; the global Refresh stays on its
    // key and in the command palette.
    if mouse {
        session.invoke_via_palette(true, "[r Refresh]");
    } else {
        session.send_key(b"r");
    }
    session.wait_for_screen("private search hidden");
    assert!(!session.screen().contains("REMOTE LINE"));
    assert!(!session.screen().contains("日本語.rs"));
    assert!(!session.screen().contains("remote exact scope"));
    let records = journal(&mut session, &fixture, 22);
    assert!(records
        .iter()
        .all(|record| record["args"].as_array().is_some_and(|args| !args
            .iter()
            .any(|arg| arg == "POST" || arg == "PATCH" || arg == "DELETE"))));
    assert_eq!(saved_query(), saved);
    assert_eq!(
        cache.draft(&scope, "private-proof").unwrap().as_deref(),
        Some("Retained private draft")
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("src/日本語.rs")).unwrap(),
        dirty
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn v1_search_keyboard_github_mode_defaults_to_checkout_origin_and_opens_issue() {
    origin_issue_journey(false)
}
#[test]
fn v1_search_mouse_github_mode_defaults_to_checkout_origin_and_opens_issue() {
    origin_issue_journey(true)
}

fn issue_search(query: &str, number: u64, repository: &str) -> GhStep {
    let item = json!({"id":number,"node_id":format!("I{number}"),"number":number,"title":"Crash on empty input","repository_url":format!("https://api.github.invalid/repos/{repository}"),"html_url":format!("https://github.invalid/{repository}/issues/{number}"),"state":"open","body":"Steps to reproduce\nFULL ISSUE BODY LINE","user":{"login":"reporter","type":"User"},"text_matches":[{"fragment":"BUG in the title","property":"title","object_type":"Issue","matches":[{"indices":[0,3]}]}]});
    GhStep::read(
        &[
            "api",
            "search/issues",
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
            "--header",
            "Accept: application/vnd.github.text-match+json",
            "-f",
            &format!("q={query}"),
            "-f",
            "per_page=100",
            "-f",
            "page=1",
        ],
        &json!({"total_count":1,"incomplete_results":false,"items":[item]}).to_string(),
    )
}

fn origin_issue_journey(mouse: bool) {
    // No --repo and no saved set: the launch checkout's origin is the default scope.
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("local.txt", b"saved local bytes\n", "Local fixture");
    fixture.git(&["remote", "add", "origin", "git@github.invalid:org/app.git"]);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    Config {
        host: "github.invalid".into(),
        avatars: false,
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    let mut steps = identity(); // Launch account check.
    steps.extend(identity()); // Explicit GitHub mode scope.
    steps.extend(identity());
    steps.push(issue_search("BUG repo:org/app is:issue", 3, "org/app"));
    steps.extend(identity()); // Account re-verified after the read.
    steps.extend(identity()); // Open keeps the issue in the preview, no extra read.
    steps.extend(identity());
    steps.push(issue_search("BUG repo:other/lib is:issue", 7, "other/lib"));
    steps.extend(identity());
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    choose(&mut session, mouse, "3 Search", b"3");
    session.wait_for_screen("Selected local search corpus ready");
    // SSH origin names the explicit mapping step instead of a dead end.
    session.wait_for_screen("origin org/app is not mapped");
    choose(&mut session, mouse, "h GitHub", b"h");
    session.wait_for_screen("GitHub scope verified");
    session.wait_for_screen("GitHub remote discovery / org/app");
    session.wait_for_screen("searches outside org/app");
    assert!(!session.screen().contains("is not mapped"));
    journal(&mut session, &fixture, 4);
    query(&mut session, mouse, "BUG type:issue");
    session.wait_for_screen("1 results • GitHub");
    session.wait_for_screen("#3 Crash on empty input");
    session.wait_for_screen("org/app#3 • open");
    session.wait_for_screen("https://github.invalid/org/app/issues/3");
    session.wait_for_screen("FULL ISSUE BODY LINE");
    journal(&mut session, &fixture, 9);
    choose(&mut session, mouse, "[o Open result]", b"o");
    session.wait_for_screen("Showing org/app#3 in the preview");
    session.assert_focused_view("Search");
    journal(&mut session, &fixture, 11);
    // A typed repo: qualifier is this query's explicit scope.
    query(&mut session, mouse, "BUG type:issue repo:other/lib");
    session.wait_for_screen("other/lib#7 • open");
    let records = journal(&mut session, &fixture, 16);
    session.resize(80, 24);
    session.wait_for_screen("#7 Crash on empty input");
    assert!(records.iter().all(|record| record["matched"] == true));
    assert!(records
        .iter()
        .all(|record| record["args"][0] == "--version" || record["args"][5] == "GET"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    session.quit_and_assert_restored_terminal(mouse);
}
