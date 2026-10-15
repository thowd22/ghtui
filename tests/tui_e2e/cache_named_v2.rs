//! Actual rollback-journal lifecycle; no fabricated sidecars or automatic retry.
use super::*;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn cache_storage_named_zero_v2_keyboard_actual_sqlite_churn_clear_restart_and_account_isolation() {
    journey(false);
}
#[test]
fn cache_storage_named_zero_v2_mouse_actual_sqlite_churn_clear_restart_and_account_isolation() {
    journey(true);
}

fn readonly_git(fixture: &Fixture, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .args(["--no-optional-locks", "-c", "diff.autoRefreshIndex=false"])
        .args(args)
        .current_dir(fixture.repo())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    output.stdout
}

fn direct_draft(path: &Path, account: u64) -> String {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .query_row(
            "SELECT body FROM drafts WHERE host='github.invalid' AND login=?1 AND resource='review'",
            [format!("user-id:{account}")],
            |row| row.get(0),
        )
        .unwrap()
}

fn journey(mouse: bool) {
    let fixture = configured();
    fixture.init_git();
    fixture.commit_file(
        "tracked.txt",
        b"original saved bytes\n",
        "private cache local canary",
    );
    fs::write(
        fixture.repo().join("tracked.txt"),
        b"dirty original bytes\n",
    )
    .unwrap();
    fs::write(
        fixture.repo().join("untracked.txt"),
        b"untracked original bytes\n",
    )
    .unwrap();
    let git = (
        readonly_git(&fixture, &["rev-parse", "HEAD"]),
        readonly_git(&fixture, &["show-ref"]),
        readonly_git(&fixture, &["status", "--porcelain=v2", "-z"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fs::read(fixture.repo().join(".git/config")).unwrap(),
    );
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let path = fixture.root.join("config/ghtui/cache.sqlite3");
    let journal_path = fixture.root.join("config/ghtui/cache.sqlite3-journal");
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let other_scope = Scope::github("github.invalid", Some(43)).unwrap();
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
    let source = workspace.source();
    let payload = ghtui::providers::gh::PullRequest {
        host: "github.invalid".into(),
        id: "PR_named_journal".into(),
        repository: "org/alpha".into(),
        number: 7,
        title: "Named journal original private change".into(),
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
        .put_draft(&scope, "review", "original account42 persistent draft")
        .unwrap();
    cache
        .put_draft(
            &other_scope,
            "review",
            "separate account43 persistent draft",
        )
        .unwrap();
    cache
        .save_view(&scope, "Original view", &json!({"query":"original"}))
        .unwrap();
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
    let reservation = cache
        .reserve_sync_generation(&scope, "named-journal-canary")
        .unwrap();
    drop(cache);
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.busy_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        sql.query_row("PRAGMA journal_mode=DELETE", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "delete"
    );
    sql.execute_batch("CREATE TABLE named_journal_probe(value INTEGER NOT NULL); BEGIN IMMEDIATE; INSERT INTO named_journal_probe VALUES(0);").unwrap();
    let original_journal = fs::symlink_metadata(&journal_path).unwrap();
    assert!(
        original_journal.is_file() && original_journal.len() > 0 && original_journal.nlink() == 1
    );
    for name in [&path, &journal_path] {
        fs::set_permissions(name, fs::Permissions::from_mode(0o644)).unwrap();
    }
    let mut steps = identity("octocat");
    steps.extend(identity("octocat"));
    let mut offline = search("octocat", "author", vec![], false);
    offline.status = 1;
    offline.stderr = b"named journal fixture offline\n".to_vec();
    steps.push(offline);
    steps.extend(identity("bob"));
    steps.extend(identity("bob"));
    for queue in ["author", "assignee", "review-requested"] {
        steps.push(search("bob", queue, vec![], false));
    }
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    // The actual binary must repair its original held nonempty journal before
    // this test commits. An inspector Cache::open cannot provide false proof.
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let main = fs::symlink_metadata(&path).unwrap();
        let journal = fs::symlink_metadata(&journal_path).unwrap();
        assert_eq!(
            (journal.dev(), journal.ino()),
            (original_journal.dev(), original_journal.ino())
        );
        if main.mode() & 0o777 == 0o600 && journal.mode() & 0o777 == 0o600 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "actual startup did not repair original live journal; {}",
            session.screen()
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    sql.execute_batch("COMMIT;").unwrap();
    assert!(
        !journal_path.exists(),
        "SQLite itself must remove its rollback journal"
    );
    drop(sql);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "1 Inbox", b"1");
    refresh(&mut session, mouse);
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            text.contains("Named journal original private change")
                && text.contains("Offline/stale")
                && text.contains("named journal fixture offline")
                && !text.contains("[Esc Cancel]")
        },
        "completed original account cached/offline frame",
        Duration::from_secs(8),
    );
    assert_eq!(
        direct_draft(&path, 42),
        "original account42 persistent draft"
    );
    assert_eq!(
        direct_draft(&path, 43),
        "separate account43 persistent draft"
    );
    let mut cache = Cache::open(&path).unwrap();
    session.resize(80, 24);
    choose(&mut session, mouse, "[x Clear cache]", b"x");
    session.wait_for_screen("No pull requests");
    assert!(cache.list(&scope, None).unwrap().is_empty());
    assert!(!cache
        .commit_reserved_sync(
            &scope,
            "named-journal-canary",
            reservation,
            &[ghtui::storage::Record::new(
                "comment",
                "late",
                "org/alpha",
                json!({"body":"must remain fenced"})
            )],
            true
        )
        .unwrap());
    assert_eq!(
        cache.draft(&scope, "review").unwrap().as_deref(),
        Some("original account42 persistent draft")
    );
    assert_eq!(cache.saved_views(&scope).unwrap().len(), 1);
    drop(cache);
    session.quit_and_assert_restored_terminal(mouse);
    // Sixteen genuine SQL journal create/commit/unlink cycles after ALL actual
    // application/DraftWriter connections close. No sidecar is manually deleted.
    let sql = rusqlite::Connection::open(&path).unwrap();
    for value in 1..=16 {
        sql.execute_batch("BEGIN IMMEDIATE;").unwrap();
        sql.execute("INSERT INTO named_journal_probe VALUES(?1)", [value])
            .unwrap();
        let journal = fs::symlink_metadata(&journal_path).unwrap();
        assert!(journal.is_file() && journal.len() > 0 && journal.nlink() == 1);
        assert_eq!(journal.mode() & 0o777, 0o600);
        sql.execute_batch("COMMIT;").unwrap();
        assert!(!journal_path.exists());
    }
    assert_eq!(
        sql.query_row("SELECT count(*) FROM named_journal_probe", [], |row| row
            .get::<_, u64>(0))
            .unwrap(),
        17
    );
    drop(sql);
    let mut restarted = Session::launch(&fixture, 80, 24);
    restarted.wait_for_screen("ghtui");
    choose(&mut restarted, mouse, "1 Inbox", b"1");
    refresh(&mut restarted, mouse);
    restarted.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            text.contains("@bob")
                && text.contains("Fresh • fetched")
                && text.contains("No pull requests")
                && !text.contains("[Esc Cancel]")
        },
        "completed isolated account43 after genuine rollback-journal churn",
        Duration::from_secs(8),
    );
    assert!(!restarted
        .screen()
        .contains("Named journal original private change"));
    assert_eq!(
        direct_draft(&path, 42),
        "original account42 persistent draft"
    );
    assert_eq!(
        direct_draft(&path, 43),
        "separate account43 persistent draft"
    );
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_eq!(readonly_git(&fixture, &["rev-parse", "HEAD"]), git.0);
    assert_eq!(readonly_git(&fixture, &["show-ref"]), git.1);
    assert_eq!(
        readonly_git(&fixture, &["status", "--porcelain=v2", "-z"]),
        git.2
    );
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), git.3);
    assert_eq!(fs::read(fixture.repo().join(".git/config")).unwrap(), git.4);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        b"dirty original bytes\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("untracked.txt")).unwrap(),
        b"untracked original bytes\n"
    );
    readonly_journal(&mut restarted, &fixture, steps.len());
    restarted.quit_and_assert_restored_terminal(mouse);
}
