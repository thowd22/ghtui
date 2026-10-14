use crate::support::{Fixture, GhStep, Session};
use ghtui::{
    config::Config,
    github_workspace::GithubWorkspace,
    providers::gh::{HostIdentity, Queue},
    storage::{Cache, Scope},
};
use serde_json::{json, Value};

fn configured() -> Fixture {
    let fixture = Fixture::new();
    let config = Config {
        host: "github.invalid".into(),
        avatars: false,
        ..Config::default()
    };
    config
        .save(&fixture.root.join("config/ghtui/config.toml"))
        .unwrap();
    fixture
}
fn api(endpoint: &str) -> Vec<String> {
    vec![
        "api".into(),
        endpoint.into(),
        "--hostname".into(),
        "github.invalid".into(),
        "--method".into(),
        "GET".into(),
    ]
}
fn read(args: Vec<String>, value: Value) -> GhStep {
    GhStep {
        args,
        stdin: vec![],
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
fn identity(login: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read(
            api("user"),
            json!({"login":login,"id":if login == "bob" {43}else{42}}),
        ),
    ]
}
fn pr(repo: &str, n: u64, title: &str, updated: &str) -> Value {
    json!({"node_id":format!("PR_{repo}_{n}"),"number":n,"title":title,"body":"A useful change\nSecond paragraph","repository_url":format!("https://api.github.invalid/repos/{repo}"),"html_url":format!("https://github.invalid/{repo}/pull/{n}"),"user":{"login":"alice","type":"User"},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":updated,"head":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","ref":"feature"},"base":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","ref":"main"},"changed_files":1,"additions":1,"deletions":1,"mergeable":true,"mergeable_state":"clean"})
}
fn search(login: &str, queue: &str, items: Vec<Value>, partial: bool) -> GhStep {
    let mut args = api("search/issues");
    args.extend([
        "-f".into(),
        format!("q=is:pr is:open {queue}:{login}"),
        "-f".into(),
        "per_page=100".into(),
        "-f".into(),
        "page=1".into(),
    ]);
    read(
        args,
        json!({"total_count":items.len(),"incomplete_results":partial,"items":items}),
    )
}
fn queues(login: &str, partial: bool) -> Vec<GhStep> {
    let a = pr("org/alpha", 7, "Improve auth", "2026-10-12T14:00:00Z");
    let b = pr("org/beta", 7, "Repair parser", "2026-10-12T12:00:00Z");
    let c = pr("org/gamma", 9, "Update docs", "2026-10-12T10:00:00Z");
    vec![
        search(login, "author", vec![a.clone(), c], partial),
        search(login, "assignee", vec![a, b.clone()], false),
        search(login, "review-requested", vec![b], false),
    ]
}
fn initial(partial: bool) -> Vec<GhStep> {
    let mut steps = identity("octocat");
    steps.extend(identity("octocat"));
    steps.extend(queues("octocat", partial));
    steps
}
fn refresh(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[r Refresh]")
    } else {
        session.send_key(b"r")
    }
}
fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label)
    } else {
        session.send_key(key)
    }
}
fn journal(session: &mut Session, fixture: &Fixture, count: usize) {
    session.wait_for_command_count(fixture, count);
    let records: Vec<Value> = session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), count);
    assert!(records.iter().all(|record| record["matched"] == true));
    assert!(records.iter().all(|record| record["args"][0] == "api"
        || record["args"][0] == "--version"
        || record["args"][0] == "pr"));
}

#[test]
fn personal_inbox_keyboard_overlap_scope_selection_and_partial_warning() {
    inbox(false)
}

#[test]
fn cache_storage_keyboard_reassigned_username_never_unlocks_original_private_records() {
    principal_restart(false)
}
#[test]
fn cache_storage_mouse_reassigned_username_never_unlocks_original_private_records() {
    principal_restart(true)
}
fn principal_restart(mouse: bool) {
    let fixture = configured();
    let mut steps = initial(false);
    for _ in 0..2 {
        let mut identity = identity("octocat");
        identity[1].stdout = serde_json::to_vec(&json!({"login":"octocat","id":43})).unwrap();
        steps.extend(identity);
    }
    for queue in ["author", "assignee", "review-requested"] {
        steps.push(search("octocat", queue, vec![], false));
    }
    let mut unknown = identity("octocat");
    unknown[1].stdout = serde_json::to_vec(&json!({"login":"octocat"})).unwrap();
    steps.extend(unknown);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("Improve auth");
    journal(&mut session, &fixture, 7);
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    let original = Scope::github("github.invalid", Some(42)).unwrap();
    cache
        .put_draft(&original, "private", "Original private draft")
        .unwrap();
    let original_records = cache.list(&original, Some("pr")).unwrap();
    assert_eq!(original_records.len(), 3);
    session.quit_and_assert_restored_terminal(mouse);
    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("Fresh • fetched");
    session.wait_for_screen("No pull requests");
    assert!(!session.screen().contains("Improve auth"));
    journal(&mut session, &fixture, 14);
    let reassigned = Scope::github("github.invalid", Some(43)).unwrap();
    assert!(cache.list(&reassigned, Some("pr")).unwrap().is_empty());
    assert!(cache.draft(&reassigned, "private").unwrap().is_none());
    assert_eq!(cache.list(&original, Some("pr")).unwrap(), original_records);
    assert_eq!(
        cache.draft(&original, "private").unwrap().as_deref(),
        Some("Original private draft")
    );
    session.quit_and_assert_restored_terminal(mouse);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("private cache remains hidden");
    assert!(!session.screen().contains("Improve auth"));
    assert!(!session.screen().contains("Original private draft"));
    journal(&mut session, &fixture, 16);
    assert_eq!(cache.list(&original, Some("pr")).unwrap(), original_records);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn cache_storage_keyboard_live_identity_loss_hides_private_pages_preserving_cache() {
    live_identity_loss(false)
}
#[test]
fn cache_storage_mouse_live_identity_loss_hides_private_pages_preserving_cache() {
    live_identity_loss(true)
}
fn live_identity_loss(mouse: bool) {
    let fixture = configured();
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    assert!(
        !database.exists(),
        "Only the real app may create this cache"
    );
    let mut steps = initial(false);
    steps.push(GhStep::read(&["--version"], "gh version 2.101.0\n"));
    let mut failure = read(api("user"), Value::Null);
    failure.status = 1;
    failure.stderr = b"fixture authentication expired\n".to_vec();
    steps.push(failure);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("Improve auth");
    journal(&mut session, &fixture, 7);
    // Check BEFORE any inspector Cache::open: actual binary publication must
    // create one0600 inode and remove only its unpublished temporary name.
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::symlink_metadata(&database).unwrap();
        assert!(metadata.is_file());
        assert_eq!((metadata.mode() & 0o777, metadata.nlink()), (0o600, 1));
        assert!(std::fs::read_dir(database.parent().unwrap())
            .unwrap()
            .all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .as_encoded_bytes()
                    .starts_with(b".ghtui-new-cache-")
            }));
    }
    let cache = Cache::open(&database).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let records = cache.list(&scope, Some("pr")).unwrap();
    cache
        .put_draft(&scope, "private", "Retain private draft")
        .unwrap();
    refresh(&mut session, mouse);
    session.wait_for_screen("private data hidden");
    session.wait_for_screen("fixture authentication");
    assert!(!session.screen().contains("Improve auth"));
    assert!(!session.screen().contains("Repair parser"));
    assert!(!session.screen().contains("Retain private draft"));
    journal(&mut session, &fixture, 9);
    assert_eq!(cache.list(&scope, Some("pr")).unwrap(), records);
    assert_eq!(
        cache.draft(&scope, "private").unwrap().as_deref(),
        Some("Retain private draft")
    );
    session.resize(80, 24);
    choose(&mut session, mouse, "4 Actions", b"4");
    session.wait_for_screen("Actions scope needed");
    assert!(!session.screen().contains("Improve auth"));
    journal(&mut session, &fixture, 9);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn personal_inbox_mouse_overlap_scope_selection_and_partial_warning() {
    inbox(true)
}
fn inbox(mouse: bool) {
    let fixture = configured();
    fixture.scripted_gh(&initial(true));
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("3 unique PRs across 5 queue matches");
    session.wait_for_screen("PARTIAL");
    session.wait_for_screen("@alice");
    journal(&mut session, &fixture, 7);
    choose(&mut session, mouse, "[m Assigned to me]", b"m");
    session.wait_for_screen("Queue: Assigned to me");
    session.wait_for_screen("Repair parser");
    assert!(!session.screen().contains("Update docs"));
    choose(&mut session, mouse, "[v Needs my review]", b"v");
    session.wait_for_screen("Queue: Needs my review");
    session.wait_for_screen("Repair parser");
    assert!(!session.screen().contains("Improve auth"));
    choose(&mut session, mouse, "[c Created by me]", b"c");
    session.wait_for_screen("Queue: Created by me");
    session.wait_for_screen("Update docs");
    assert!(!session.screen().contains("Repair parser"));
    session.resize(80, 24);
    choose(&mut session, mouse, "[a All]", b"a");
    session.wait_for_screen("Queue: All");
    session.wait_for_screen("Improve auth");
    session.wait_for_screen("Repair parser");
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let records = cache.list(&scope, Some("pr")).unwrap();
    assert_eq!(records.len(), 3);
    let overlap = records
        .iter()
        .find(|record| record.id == "org/alpha#7")
        .unwrap();
    let saved: ghtui::providers::gh::PullRequest =
        serde_json::from_value(overlap.payload.clone()).unwrap();
    assert!(saved.queues.contains(&Queue::Assigned) && saved.queues.contains(&Queue::Authored));
    session.quit_and_assert_restored_terminal(mouse);
}

fn pages() -> Vec<GhStep> {
    let summary = pr("org/alpha", 7, "Improve auth", "2026-10-12T14:00:00Z");
    let mut steps = vec![read(api("repos/org/alpha/pulls/7"), summary.clone())];
    for (endpoint, value) in [
        (
            "repos/org/alpha/issues/7/comments?per_page=100",
            json!([[{"id":10,"node_id":"IC10","body":"Please keep auth strict","user":{"login":"reviewer","type":"User"},"created_at":"2026-10-12T12:00:00Z","updated_at":"2026-10-12T12:00:00Z"}]]),
        ),
        (
            "repos/org/alpha/pulls/7/commits?per_page=100",
            json!([[{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","commit":{"message":"Fix auth boundary","author":{"name":"Alice","date":"2026-10-12T12:00:00Z"},"committer":{"name":"Build Bot"}},"author":{"login":"alice"},"committer":{"login":"build-bot","type":"Bot"},"parents":[{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}]}]]),
        ),
        (
            "repos/org/alpha/pulls/7/files?per_page=100",
            json!([[{"filename":"src/auth.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new"}]]),
        ),
        (
            "repos/org/alpha/pulls/7/reviews?per_page=100",
            json!([[{"id":12,"body":"Needs a guard","state":"CHANGES_REQUESTED","user":{"login":"reviewer"}}]]),
        ),
        (
            "repos/org/alpha/issues/7/timeline?per_page=100",
            json!([[{"event":"labeled","label":{"name":"security"},"actor":{"login":"alice"},"created_at":"2026-10-12T13:00:00Z"}]]),
        ),
    ] {
        let mut args = api(endpoint);
        args.extend(["--paginate".into(), "--slurp".into()]);
        steps.push(read(args, value));
    }
    steps.push(read(api("repos/org/alpha/pulls/7"), summary.clone()));
    let mut metadata_summary = summary.clone();
    metadata_summary["head"]["repo"] =
        json!({"id":2,"full_name":"contributor/alpha","default_branch":"main"});
    metadata_summary["requested_reviewers"] = json!([{"login":"requested-reviewer","type":"User"}]);
    metadata_summary["requested_teams"] =
        json!([{"id":10,"name":"Security team","slug":"security"}]);
    metadata_summary["maintainer_can_modify"] = json!(true);
    steps.push(read(api("repos/org/alpha/pulls/7"), metadata_summary));
    steps.push(read(api("repos/org/alpha/issues/7"),json!({"labels":[{"name":"security"}],"assignees":[{"login":"assignee"}],"milestone":{"number":1,"title":"Launch"},"locked":false})));
    steps.push(read(api("repos/org/alpha"),json!({"id":1,"full_name":"org/alpha","default_branch":"main","permissions":{"pull":true,"push":false,"maintain":false,"admin":false}})));
    steps.push(read(api("repos/org/alpha/pulls/7"), summary.clone()));
    steps.push(GhStep::read(&["pr","diff","7","--repo","github.invalid/org/alpha","--color","never"],"diff --git a/src/auth.rs b/src/auth.rs\n--- a/src/auth.rs\n+++ b/src/auth.rs\n@@ -1 +1 @@\n-old\n+new\n"));
    steps.push(read(api("repos/org/alpha/pulls/7"), summary));
    let mut checks = api(
        "repos/org/alpha/commits/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/check-runs?per_page=100",
    );
    checks.extend(["--paginate".into(), "--slurp".into()]);
    steps.push(read(checks,json!([{"check_runs":[{"id":50,"name":"Rust tests","status":"completed","conclusion":"success","head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]}])));
    let mut statuses = api(
        "repos/org/alpha/commits/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/statuses?per_page=100",
    );
    statuses.extend(["--paginate".into(), "--slurp".into()]);
    steps.push(read(statuses, json!([[]])));
    steps
}

#[test]
fn pr_details_pr_diff_pr_checks_keyboard_native_tabs() {
    detail(false)
}
#[test]
fn pr_details_pr_diff_pr_checks_mouse_native_tabs() {
    detail(true)
}
fn detail(mouse: bool) {
    let fixture = configured();
    let mut steps = initial(false);
    steps.extend(pages());
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("3 unique PRs across 5 queue matches");
    if mouse {
        session.click_visible_action("Improve auth")
    } else {
        session.send_key(b"\r")
    }
    session.wait_for_screen("1 comments");
    session.wait_for_screen("Head aaaaaaaaa");
    choose(&mut session, mouse, "[u Metadata]", b"u");
    session.wait_for_screen("Your repository permissions");
    session.wait_for_screen("Fork origin: contributor/alpha");
    session.wait_for_screen("@requested-reviewer");
    session.wait_for_screen("Milestone: Launch");
    choose(&mut session, mouse, "[c Conversation]", b"c");
    session.wait_for_screen("Please keep auth strict");
    session.wait_for_screen("@reviewer");
    choose(&mut session, mouse, "[h Commits]", b"h");
    session.wait_for_screen("Fix auth boundary");
    session.wait_for_screen("@build-bot [bot]");
    choose(&mut session, mouse, "[f Files]", b"f");
    session.wait_for_screen("Canonical diff");
    session.wait_for_screen("1 +new");
    session.wait_for_screen("1       -old");
    choose(&mut session, mouse, "[k Checks]", b"k");
    session.wait_for_screen("Rust tests");
    session.wait_for_screen("success");
    session.resize(80, 24);
    choose(&mut session, mouse, "[t Timeline]", b"t");
    session.wait_for_screen("Label: security");
    choose(&mut session, mouse, "[b Back]", b"b");
    session.wait_for_screen("3 unique PRs across 5 queue matches");
    journal(&mut session, &fixture, steps.len());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn cache_storage_keyboard_cache_before_delayed_refresh_clear_restart_and_account_switch() {
    cache_journey(false, false)
}
#[test]
fn cache_storage_mouse_cache_before_delayed_refresh_clear_restart_and_account_switch() {
    cache_journey(true, false)
}
#[test]
fn cache_storage_keyboard_optional_journal_last_close_reopen_restart_preserves_original_effects() {
    cache_journey(false, true)
}
#[test]
fn cache_storage_mouse_optional_journal_last_close_reopen_restart_preserves_original_effects() {
    cache_journey(true, true)
}
fn cache_journey(mouse: bool, cycle_journals: bool) {
    use std::os::unix::fs::PermissionsExt;
    let fixture = configured();
    let original_git = cycle_journals.then(|| {
        fixture.init_git();
        fixture.commit_file(
            "preserved.txt",
            b"original saved bytes\n",
            "preserve local source",
        );
        std::fs::write(
            fixture.repo().join("preserved.txt"),
            b"dirty original bytes\n",
        )
        .unwrap();
        std::fs::write(
            fixture.repo().join("untracked.txt"),
            b"untracked original bytes\n",
        )
        .unwrap();
        (
            fixture.git(&["rev-parse", "HEAD"]),
            fixture.git(&["show-ref"]),
            std::fs::read(fixture.repo().join(".git/index")).unwrap(),
            fixture.git(&["status", "--porcelain=v2", "-z"]),
        )
    });
    let original_config = std::fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let path = fixture.root.join("config/ghtui/cache.sqlite3");
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let workspace = GithubWorkspace::empty(
        HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            gh_version: "gh version 2.101.0".into(),
            account_id: Some(42),
        },
        vec![],
        Default::default(),
    );
    let payload = ghtui::providers::gh::PullRequest {
        host: "github.invalid".into(),
        id: "PRcached".into(),
        repository: "org/alpha".into(),
        number: 7,
        title: "Cached private change".into(),
        body: String::new(),
        url: String::new(),
        author: Default::default(),
        state: "open".into(),
        draft: false,
        created_at: "2026-10-01T00:00:00Z".into(),
        updated_at: "2026-10-12T00:00:00Z".into(),
        queues: std::collections::BTreeSet::from([Queue::Authored]),
    };
    let mut cache = Cache::open(&path).unwrap();
    cache
        .put_draft(&scope, "review", "Keep draft after clear")
        .unwrap();
    cache
        .save_view(&scope, "Mine", &json!({"query":"auth"}))
        .unwrap();
    let source = workspace.source();
    cache
        .commit_sync(
            &scope,
            &source,
            1,
            &[
                ghtui::storage::Record::new(
                    "pr",
                    "org/alpha#7",
                    "org/alpha",
                    serde_json::to_value(payload).unwrap(),
                ),
                ghtui::storage::Record::new(
                    "inbox-meta",
                    &source,
                    "",
                    json!({"coverage":{"loaded":1,"total_matches":1,"complete":true,"reasons":[]}}),
                ),
            ],
            true,
        )
        .unwrap();
    // Keep a real SQLite connection alive so its nonempty legacy WAL and SHM
    // survive until the child opens them; this is not a synthetic sidecar file.
    let legacy = rusqlite::Connection::open(&path).unwrap();
    legacy.pragma_update(None, "journal_mode", "WAL").unwrap();
    let pending_source = "discussion:PR_cached";
    let pending_generation = cache
        .reserve_sync_generation(&scope, pending_source)
        .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o644
    );
    let sidecars = [
        fixture.root.join("config/ghtui/cache.sqlite3-wal"),
        fixture.root.join("config/ghtui/cache.sqlite3-shm"),
    ];
    assert!(std::fs::metadata(&sidecars[0]).unwrap().len() > 0);
    for sidecar in &sidecars {
        std::fs::set_permissions(sidecar, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            std::fs::metadata(sidecar).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }
    let mut steps = identity("octocat");
    steps.extend(identity("octocat"));
    let mut offline = search("octocat", "author", vec![], false);
    offline.status = 1;
    offline.stderr = b"fixture offline\n".to_vec();
    offline.delay_ms = 500;
    steps.push(offline.clone());
    if cycle_journals {
        steps.extend(identity("octocat"));
        steps.extend(identity("octocat"));
        steps.push(offline);
    }
    steps.extend(identity("bob"));
    steps.extend(identity("bob"));
    for queue in ["author", "assignee", "review-requested"] {
        steps.push(search("bob", queue, vec![], false));
    }
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    if cycle_journals {
        choose(&mut session, mouse, "1 Inbox", b"1");
        session.assert_focused_view("Inbox");
    }
    assert!(!session.screen().contains("Cached private change"));
    refresh(&mut session, mouse);
    session.wait_for_screen("Cached private change");
    session.wait_for_screen("Cached • fetched");
    session.wait_for_screen("Offline/stale");
    session.wait_for_screen("fixture offline");
    // The real child, not a later Cache::open in this test, repaired permissions.
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    for sidecar in &sidecars {
        assert_eq!(
            std::fs::metadata(sidecar).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let (mut cache, _legacy) = if cycle_journals {
        session.wait_for_cursorless_condition(
            |frame| {
                let text = frame.contents();
                text.contains("Cached private change")
                    && text.contains("Offline/stale")
                    && !text.contains("[Esc Cancel]")
            },
            "Actual completed cached/offline frame before journal lifecycle",
            std::time::Duration::from_secs(8),
        );
        drop(legacy);
        drop(cache);
        // DraftWriter deliberately owns a live Cache for this entire actual
        // app lifetime. Its graceful shutdown is required for a genuine LAST
        // SQLite close; sidecars must not disappear while that owner is alive.
        session.quit_and_assert_restored_terminal(mouse);
        for sidecar in &sidecars {
            assert!(
                !sidecar.exists(),
                "actual last SQLite close including real DraftWriter did not clean journals"
            );
        }
        for _ in 0..3 {
            let connection = rusqlite::Connection::open(&path).unwrap();
            let draft: String = connection
                .query_row("SELECT body FROM drafts WHERE host='github.invalid' AND login='user-id:42' AND resource='review'", [], |row| row.get(0))
                .unwrap();
            assert_eq!(draft, "Keep draft after clear");
            for sidecar in &sidecars {
                assert!(
                    sidecar.is_file(),
                    "actual WAL read did not open its journals"
                );
                assert_eq!(
                    std::fs::metadata(sidecar).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            drop(connection); // SQLite itself unlinks, never manual sidecar removal.
            for sidecar in &sidecars {
                assert!(!sidecar.exists());
            }
        }
        session = Session::launch(&fixture, 120, 40);
        session.wait_for_screen("ghtui");
        choose(&mut session, mouse, "1 Inbox", b"1");
        session.assert_focused_view("Inbox");
        refresh(&mut session, mouse);
        session.wait_for_cursorless_condition(
            |frame| {
                let text = frame.contents();
                text.contains("Cached private change")
                    && text.contains("Offline/stale")
                    && text.contains("fixture offline")
                    && !text.contains("[Esc Cancel]")
            },
            "Actual restarted cached/offline frame after three real journal cleanup cycles",
            std::time::Duration::from_secs(8),
        );
        assert!(!session.screen().contains("owned regular file"));
        (Cache::open(&path).unwrap(), None)
    } else {
        (cache, Some(legacy))
    };
    session.resize(80, 24);
    choose(&mut session, mouse, "[x Clear cache]", b"x");
    session.wait_for_screen("No pull requests");
    assert!(!session.screen().contains("Cached private change"));
    assert!(cache.list(&scope, None).unwrap().is_empty());
    assert_eq!(
        cache.sync_reservation(&scope, pending_source).unwrap(),
        None
    );
    assert!(!cache
        .commit_reserved_sync(
            &scope,
            pending_source,
            pending_generation,
            &[ghtui::storage::Record::new(
                "comment",
                "late",
                "org/alpha",
                json!({"body":"must not repopulate after clear"})
            )],
            true
        )
        .unwrap());
    assert!(cache.list(&scope, None).unwrap().is_empty());
    assert_eq!(
        cache.draft(&scope, "review").unwrap().as_deref(),
        Some("Keep draft after clear")
    );
    assert_eq!(cache.saved_views(&scope).unwrap().len(), 1);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    for sidecar in &sidecars {
        std::fs::set_permissions(sidecar, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    let mut restarted = session.restart_with_same_cache(&fixture, 80, 24);
    if cycle_journals {
        choose(&mut restarted, mouse, "1 Inbox", b"1");
        restarted.assert_focused_view("Inbox");
    }
    refresh(&mut restarted, mouse);
    restarted.wait_for_screen("@bob");
    restarted.wait_for_screen("Fresh • fetched");
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    for sidecar in &sidecars {
        assert_eq!(
            std::fs::metadata(sidecar).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert!(!restarted.screen().contains("Cached private change"));
    assert_eq!(
        std::fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        original_config
    );
    if let Some((head, refs, index, status)) = original_git {
        assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
        assert_eq!(fixture.git(&["show-ref"]), refs);
        assert_eq!(
            std::fs::read(fixture.repo().join(".git/index")).unwrap(),
            index
        );
        assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
        assert_eq!(
            std::fs::read(fixture.repo().join("preserved.txt")).unwrap(),
            b"dirty original bytes\n"
        );
        assert_eq!(
            std::fs::read(fixture.repo().join("untracked.txt")).unwrap(),
            b"untracked original bytes\n"
        );
    }
    journal(&mut restarted, &fixture, steps.len());
    restarted.quit_and_assert_restored_terminal(mouse);
}

fn open_comment(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[p PR actions]", b"p");
    session.wait_for_screen("PR actions • native workflows");
    choose(session, mouse, "Add conversation comment", b"\r");
    session.wait_for_screen("LOCAL DRAFT • Submit is the only publishing action");
}
fn readonly_journal(session: &mut Session, fixture: &Fixture, count: usize) {
    journal(session, fixture, count);
    let records: Vec<Value> = session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(records.iter().all(
        |record| record["args"]
            .as_array()
            .unwrap()
            .iter()
            .all(|arg| arg != "POST" && arg != "PATCH" && arg != "DELETE" && arg != "PUT")
    ));
}
#[test]
fn composer_keyboard_paste_preview_selection_draft_restart_and_external_editor() {
    composer(false)
}
#[test]
fn composer_mouse_paste_preview_selection_draft_restart_and_external_editor() {
    composer(true)
}
fn composer(mouse: bool) {
    let fixture = configured();
    let mut steps = initial(false);
    steps.extend(pages().into_iter().take(7));
    steps.extend(initial(false));
    steps.extend(pages().into_iter().take(7));
    fixture.scripted_gh(&steps);
    let editor = fixture.root.join("fixture editor.sh");
    std::fs::write(&editor,"#!/bin/sh\nstty -a > \"$GH_FIXTURE_DIR/editor-termios.txt\"\nprintf 'Editor retained draft café\\nsecond edited line\\n' > \"$1\"\n").unwrap();
    let editor_command = format!("sh '{}'", editor.display());
    let mut session = Session::launch_binary(
        std::path::Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        120,
        40,
        &[("VISUAL", ""), ("EDITOR", &editor_command)],
    );
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("Improve auth");
    choose(&mut session, mouse, "Improve auth", b"\r");
    session.wait_for_screen("1 comments");
    open_comment(&mut session, mouse);
    let body = "Draft alpha\nUnicode café 👩🏽‍💻 and 界";
    session.paste(body);
    session.wait_for_screen("Draft alpha");
    session.wait_for_screen("Draft saved locally");
    if mouse {
        let (x, y) = session.locate("Draft alpha").unwrap();
        session.drag((x, y), (x + 5, y));
        session.paste("Saved");
    } else {
        session.send_key(b"\x1b[1;5H\x1b[1;2C\x1b[1;2C\x1b[1;2C\x1b[1;2C\x1b[1;2C");
        session.paste("Saved");
    }
    session.wait_for_screen("Saved alpha");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("PREVIEW • Nothing published");
    session.wait_for_screen("Saved alpha");
    readonly_journal(&mut session, &fixture, 14);
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_screen("[p PR actions]");
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let resource = "org/alpha/pull/7/composer/comment";
    session.quit_and_assert_restored_terminal(mouse);
    let draft = cache.draft(&scope, resource).unwrap().unwrap();
    let saved: Value = serde_json::from_str(&draft).unwrap();
    assert_eq!(saved["version"], 2);
    assert!(saved["publication"].is_null());
    assert_eq!(
        saved["values"]["body"],
        "Saved alpha\nUnicode café 👩🏽‍💻 and 界"
    );
    let mut restarted = Session::launch_binary(
        std::path::Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        80,
        24,
        &[("VISUAL", ""), ("EDITOR", &editor_command)],
    );
    restarted.wait_for_screen("ghtui");
    refresh(&mut restarted, mouse);
    restarted.wait_for_screen("Fresh • fetched");
    choose(&mut restarted, mouse, "Improve auth", b"\r");
    restarted.wait_for_screen("1 comments");
    open_comment(&mut restarted, mouse);
    restarted.wait_for_screen("Saved alpha");
    choose(&mut restarted, mouse, "[External editor]", b"\x05");
    restarted.wait_for_screen("Editor retained draft café");
    restarted.wait_for_screen("second edited line");
    restarted.wait_for_screen("Draft saved locally");
    let editor_termios =
        std::fs::read_to_string(fixture.root.join("gh-fixture/editor-termios.txt")).unwrap();
    assert!(editor_termios
        .split_whitespace()
        .any(|flag| flag == "icanon" || flag == "icanon;"));
    assert!(!editor_termios.contains("-icanon"));
    choose(&mut restarted, mouse, "[Keep draft / Close]", b"\x1b");
    restarted.wait_for_screen("[p PR actions]");
    readonly_journal(&mut restarted, &fixture, steps.len());
    restarted.quit_and_assert_restored_terminal(mouse);
    let saved: Value =
        serde_json::from_str(&cache.draft(&scope, resource).unwrap().unwrap()).unwrap();
    assert_eq!(
        saved["values"]["body"],
        "Editor retained draft café\nsecond edited line\n"
    );
}

#[test]
fn composer_comment_mutation_keyboard_explicit_submit_and_confirmed_timeline() {
    publish(false)
}
#[test]
fn composer_comment_mutation_mouse_explicit_submit_and_confirmed_timeline() {
    publish(true)
}
fn publish(mouse: bool) {
    let fixture = configured();
    let body = "A published café comment\nSecond line";
    let mut steps = initial(false);
    steps.extend(pages().into_iter().take(7));
    for _ in 0..3 {
        steps.extend(legacy_guard());
    }
    let mut args = api("repos/org/alpha/issues/7/comments");
    args[5] = "POST".into();
    args.extend(["--input".into(), "-".into()]);
    let created = json!({"id":20,"node_id":"IC20","body":body,"user":{"id":42,"login":"octocat","type":"User"},"issue_url":"https://api.github.invalid/repos/org/alpha/issues/7","html_url":"https://github.invalid/org/alpha/pull/7#issuecomment-20","created_at":"2026-10-13T00:00:00Z","updated_at":"2026-10-13T00:00:00Z"});
    let mut write = read(args, created.clone());
    write.stdin = serde_json::to_vec(&json!({"body":body})).unwrap();
    steps.push(write);
    steps.push(read(
        api("repos/org/alpha/issues/comments/20"),
        created.clone(),
    ));
    steps.extend(identity("octocat"));
    let mut refreshed = pages().into_iter().take(7).collect::<Vec<_>>();
    refreshed[1].stdout = serde_json::to_vec(&json!([[created]])).unwrap();
    steps.extend(refreshed);
    steps.extend(identity("octocat"));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("Improve auth");
    choose(&mut session, mouse, "Improve auth", b"\r");
    session.wait_for_screen("1 comments");
    open_comment(&mut session, mouse);
    session.paste(body);
    session.wait_for_screen("A published café comment");
    readonly_journal(&mut session, &fixture, 14);
    if mouse {
        session.click_visible_action("[Submit]")
    } else {
        session.send_key(b"\t\t\t\r")
    }
    session.wait_for_screen("Published • Add conversation comment");
    choose(&mut session, mouse, "[c Conversation]", b"c");
    session.wait_for_screen("@octocat");
    session.wait_for_screen("A published café comment");
    journal(&mut session, &fixture, steps.len());
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    assert!(cache
        .draft(
            &Scope::github("github.invalid", Some(42)).unwrap(),
            "org/alpha/pull/7/composer/comment"
        )
        .unwrap()
        .is_none());
    let commands: Vec<Value> = session
        .inspect_command_journal(&fixture)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        commands
            .iter()
            .filter(|record| record["args"][5] == "POST")
            .count(),
        1
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn composer_keyboard_editor_failure_retains_draft_and_publishes_nothing() {
    editor_failure(false)
}
#[test]
fn composer_mouse_editor_failure_retains_draft_and_publishes_nothing() {
    editor_failure(true)
}
fn editor_failure(mouse: bool) {
    let fixture = configured();
    let mut steps = initial(false);
    steps.extend(pages().into_iter().take(7));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch_binary(
        std::path::Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        120,
        40,
        &[("VISUAL", ""), ("EDITOR", "false")],
    );
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("Improve auth");
    choose(&mut session, mouse, "Improve auth", b"\r");
    session.wait_for_screen("1 comments");
    open_comment(&mut session, mouse);
    session.paste("Keep my café draft\nSecond line");
    session.wait_for_screen("Keep my café draft");
    if mouse {
        session.click_visible_action("[External editor]")
    } else {
        session.send_key(b"\t\t\r")
    }
    session.wait_for_screen("Editor error:");
    session.wait_for_screen("draft retained");
    session.wait_for_screen("Keep my café draft");
    session.resize(80, 24);
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_screen("[p PR actions]");
    readonly_journal(&mut session, &fixture, steps.len());
    session.quit_and_assert_restored_terminal(mouse);
    let saved = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3"))
        .unwrap()
        .draft(
            &Scope::github("github.invalid", Some(42)).unwrap(),
            "org/alpha/pull/7/composer/comment",
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&saved).unwrap()["values"]["body"],
        "Keep my café draft\nSecond line"
    );
}

#[test]
fn composer_keyboard_failed_publish_keeps_draft_and_blocks_duplicate_write() {
    ambiguous(false)
}
#[test]
fn composer_mouse_failed_publish_keeps_draft_and_blocks_duplicate_write() {
    ambiguous(true)
}
fn ambiguous(mouse: bool) {
    let fixture = configured();
    let body = "Retain after ambiguous failure";
    let mut steps = initial(false);
    steps.extend(pages().into_iter().take(7));
    for _ in 0..3 {
        steps.extend(legacy_guard());
    }
    let mut args = api("repos/org/alpha/issues/7/comments");
    args[5] = "POST".into();
    args.extend(["--input".into(), "-".into()]);
    let mut write = read(args, Value::Null);
    write.stdin = serde_json::to_vec(&json!({"body":body})).unwrap();
    write.status = 1;
    write.stderr = b"network disconnected after write launch\n".to_vec();
    steps.push(write);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("Improve auth");
    choose(&mut session, mouse, "Improve auth", b"\r");
    session.wait_for_screen("1 comments");
    open_comment(&mut session, mouse);
    session.paste(body);
    session.wait_for_screen(body);
    if mouse {
        session.click_visible_action("[Submit]")
    } else {
        session.send_key(b"\t\t\t\r")
    }
    session.wait_for_screen("network disconnected");
    session.wait_for_screen(body);
    if mouse {
        session.click_visible_action("[Submit]")
    } else {
        session.send_key(b"\r")
    }
    session.wait_for_screen("unresolved publication");
    journal(&mut session, &fixture, steps.len());
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    // PTY reads may stop halfway through a redraw: the upper title can be
    // cleared before the lower foreground controls. Require the whole native
    // PR frame before another key so Esc+q cannot become Alt+q.
    session
        .try_wait_for_condition(
            |screen| {
                !screen.contains("Add conversation comment • org/alpha#7")
                    && !screen.contains("[Submit]")
                    && !screen.contains("[Keep draft / Close]")
                    && screen.contains("org/alpha#7 • Improve auth • open")
                    && screen.contains("[p PR actions]")
            },
            "complete dismissed composer/native PR frame before quit",
            std::time::Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(!session.screen().contains("[Submit]"));
    session.wait_for_screen("[p PR actions]");
    session.quit_and_assert_restored_terminal(mouse);
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    let saved = cache
        .draft(
            &Scope::github("github.invalid", Some(42)).unwrap(),
            "org/alpha/pull/7/composer/comment",
        )
        .unwrap()
        .unwrap();
    let saved: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(saved["values"]["body"], body);
    assert!(saved["publication"]["attempt_id"]
        .as_i64()
        .is_some_and(|id| id > 0));
    assert_eq!(saved["publication"]["pr_node_id"], "PR_org/alpha_7");
    let records = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let writes = records
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|record| record["args"][5] == "POST")
        .count();
    assert_eq!(writes, 1);
}

fn legacy_guard() -> Vec<GhStep> {
    let mut steps = identity("octocat");
    steps.push(read(
        api("repos/org/alpha/pulls/7"),
        pr("org/alpha", 7, "Improve auth", "2026-10-12T14:00:00Z"),
    ));
    steps
}
