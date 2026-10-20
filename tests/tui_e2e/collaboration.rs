//! Actual binary journeys for explicit discussion and original commit-message sources.
use crate::hydration_fixture as gh_fixture;
use crate::support::{Fixture, Session};
use ghtui::{
    config::Config,
    storage::{Cache, Scope},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, Instant},
};

fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}
fn query(session: &mut Session, mouse: bool, input: &str, expected: &str) {
    choose(session, mouse, "[/ Query]", b"/");
    session.send_key(b"\x01");
    session.paste(input);
    choose(session, mouse, "[Enter Search]", b"\r");
    session.wait_for_screen(expected);
}
/// `S Save query` leaves the 80-column Search key bar; Enter in the Name field and the
/// palette's save entry run the same Save action.
fn save_query_at_80(session: &mut Session, mouse: bool) {
    if mouse {
        session.invoke_via_palette(true, "Save current search intent");
    } else {
        session.send_key(b"\r");
    }
}
fn sources(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[H Sources]", b"H");
    session.wait_for_screen("Search sources • explicit read-only hydration");
}
fn open_search(session: &mut Session, mouse: bool) {
    session.wait_for_screen("Recent commits");
    choose(session, mouse, "3 Search", b"3");
    // Controls exist in the loading shell too; wait for the prepared corpus so
    // opening the source editor can choose the actual configured repository.
    session.wait_for_screen("Selected local search corpus ready");
    session.wait_for_control("[H Sources]");
}
fn source_field(session: &mut Session, mouse: bool, label: &str, value: &str) {
    session.wait_for_screen(label);
    let (left, row) = session.locate(label).unwrap();
    if mouse {
        session.click(left + 24, row);
    } else {
        // Caps have no magic hotkey: follow actual Tab focus in the displayed
        // field, then Enter. The terminal attribute proves keyboard navigation.
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            session.poll_output(Duration::from_millis(1));
            if session.cell_underlined(row, left + 24) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Source field never focused: {label}\n{}",
                session.screen()
            );
            session.send_key(b"\t");
            session.poll_output(Duration::from_millis(50));
        }
        session.send_key(b"\r");
    }
    session.send_key(b"\x01");
    session.paste(value);
    session.send_key(b"\r");
    session.wait_for_screen(value);
}
fn close_sources(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_control("[H Sources]");
}
fn saved_sources(fixture: &Fixture) -> Value {
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    cache
        .saved_view(
            &Scope::new("local", "workspace").unwrap(),
            "search-source-selection:v1",
        )
        .unwrap()
        .unwrap()
}

#[test]
fn collaboration_index_keyboard_local_commit_optin_exact_regex_ranked_original_context_restart_and_remove(
) {
    local_commit(false);
}
#[test]
fn collaboration_index_mouse_local_commit_optin_exact_regex_ranked_original_context_restart_and_remove(
) {
    local_commit(true);
}
fn local_commit(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "tracked.txt",
        b"saved file bytes\n",
        "Base summary\n\nEarlierHistoryOnlyNeedle",
    );
    let message = format!(
        "Current provenance\n\nCommitOnlyNeedle α\n{}\nExactMessageTail 日本語",
        (0..80)
            .map(|line| format!("Original message detail {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    fixture.commit_file("other.txt", b"other saved bytes\n", &message);
    fs::write(
        fixture.repo().join("tracked.txt"),
        b"dirty user bytes preserved\n",
    )
    .unwrap();
    Config {
        avatars: false,
        active_set: Some("chosen".into()),
        repository_sets: BTreeMap::from([("chosen".into(), vec!["org/app".into()])]),
        local_repositories: BTreeMap::from([("org/app".into(), fixture.repo())]),
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let mut session = Session::launch(&fixture, 120, 40);
    open_search(&mut session, mouse);
    query(&mut session, mouse, "CommitOnlyNeedle", "0 results • Exact");
    sources(&mut session, mouse);
    choose(&mut session, mouse, "[g Local commit messages]", b"g");
    session.wait_for_screen("n/a • local Git only");
    source_field(&mut session, mouse, "Max documents/commits", "1");
    source_field(&mut session, mouse, "Max bytes/category", "65536");
    choose(&mut session, mouse, "[l Load source]", b"l");
    session.wait_for_screen("Loaded 1 original commit messages");
    let persisted = saved_sources(&fixture);
    assert_eq!(persisted["sources"]["commits"][0]["revision"], "HEAD");
    assert_eq!(persisted["sources"]["commits"][0]["max_commits"], 1);
    assert_eq!(persisted["max_bytes"], 65536);
    close_sources(&mut session, mouse);
    query(
        &mut session,
        mouse,
        "EarlierHistoryOnlyNeedle type:commit",
        "0 results • Exact",
    );
    sources(&mut session, mouse);
    choose(&mut session, mouse, "[g Local commit messages]", b"g");
    source_field(&mut session, mouse, "Max documents/commits", "2");
    choose(&mut session, mouse, "[l Load source]", b"l");
    session.wait_for_screen("Loaded 2 original commit messages");
    close_sources(&mut session, mouse);
    query(
        &mut session,
        mouse,
        "CommitOnlyNeedle type:commit",
        "1 results • Exact",
    );
    choose(&mut session, mouse, "x Regex", b"x");
    query(
        &mut session,
        mouse,
        "CommitOnlyN.*α type:commit",
        "1 results • Regex",
    );
    session.resize(80, 24);
    choose(&mut session, mouse, "n Ranked", b"n");
    query(
        &mut session,
        mouse,
        "CommitOnlyNeedle type:commit",
        "1 results • Ranked",
    );
    assert!(fixture.root.join("config/ghtui/index").exists());
    choose(&mut session, mouse, "[o Open result]", b"o");
    session.wait_for_screen("Exact search source • read-only");
    session.wait_for_screen(head.trim());
    session.wait_for_screen("author Fixture Author");
    session.wait_for_screen("committer Fixture Author");
    for _ in 0..20 {
        if session.screen().contains("ExactMessageTail 日本語") {
            break;
        }
        choose(&mut session, mouse, "[PgDn More]", b"\x1b[6~");
        session.poll_output(Duration::from_millis(50));
    }
    session.wait_for_screen("ExactMessageTail 日本語");
    choose(&mut session, mouse, "[PgUp Previous]", b"\x1b[5~");
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_control("[H Sources]");
    choose(&mut session, mouse, "[z Name]", b"z");
    session.send_key(b"\x01");
    session.paste("original message scope");
    save_query_at_80(&mut session, mouse);
    session.wait_for_screen("Saved query: original message scope");
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    let view = cache
        .saved_view(
            &Scope::new("local", "workspace").unwrap(),
            "search:original message scope",
        )
        .unwrap()
        .unwrap();
    assert_eq!(view["metadata_sources"]["commits"][0]["max_commits"], 2);
    drop(cache);
    let mut restarted = session.restart_with_same_cache(&fixture, 80, 24);
    restarted.wait_for_screen("Recent commits");
    choose(&mut restarted, mouse, "3 Search", b"3");
    restarted.wait_for_screen("[u original message scope]");
    choose(&mut restarted, mouse, "[u original message scope]", b"u");
    restarted.wait_for_screen("1 results • Ranked");
    sources(&mut restarted, mouse);
    choose(&mut restarted, mouse, "[g Local commit messages]", b"g");
    restarted.wait_for_screen("Selected: 0 discussion categories, 1 commit histories");
    choose(&mut restarted, mouse, "[d Remove source]", b"d");
    restarted.wait_for_screen("Removed 1 selected sources");
    assert!(saved_sources(&fixture)["sources"]["commits"]
        .as_array()
        .unwrap()
        .is_empty());
    close_sources(&mut restarted, mouse);
    query(
        &mut restarted,
        mouse,
        "CommitOnlyNeedle type:commit",
        "0 results • Ranked",
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        b"dirty user bytes preserved\n"
    );
    assert!(
        restarted.inspect_command_journal(&fixture).is_empty(),
        "Opted-in original local commits need no gh, GitHub login or network"
    );
    restarted.quit_and_assert_restored_terminal(mouse);
}

fn remote_fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "tracked.txt",
        b"independent local bytes\n",
        "Local unrelated history",
    );
    fs::write(
        fixture.repo().join("tracked.txt"),
        b"dirty private checkout bytes\n",
    )
    .unwrap();
    Config {
        host: gh_fixture::HOST.into(),
        avatars: false,
        active_set: Some("chosen".into()),
        repository_sets: BTreeMap::from([("chosen".into(), vec![gh_fixture::REPOSITORY.into()])]),
        local_repositories: BTreeMap::from([(gh_fixture::REPOSITORY.into(), fixture.repo())]),
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    fixture
}
fn remote_cache(fixture: &Fixture) -> Cache {
    Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap()
}
fn private_scope() -> Scope {
    Scope::github(gh_fixture::HOST, Some(42)).unwrap()
}
fn private_sources(fixture: &Fixture) -> ghtui::search::sources::MetadataSources {
    let saved = remote_cache(fixture)
        .saved_view(&private_scope(), "search-source-selection:v1")
        .unwrap()
        .unwrap();
    serde_json::from_value(saved["sources"].clone()).unwrap()
}
fn exact_journal(session: &mut Session, fixture: &Fixture, count: usize) {
    session.wait_for_command_count(fixture, count);
    let journal = session.inspect_command_journal(fixture);
    let rows = journal
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    assert!(rows.iter().all(|row| row["matched"] == true));
    assert!(
        rows.iter().all(|row| row["args"][0] == "--version"
            || row["args"][5] == "GET"
            || row["args"][1] == "graphql" && row["args"][5] == "POST"),
        "Only exact fixture constant read queries are authorized"
    );
}
fn private_query(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    input: &str,
    expected: &str,
) {
    private_query_reads(session, fixture, mouse, input, expected, 6);
}
fn private_query_reads(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    input: &str,
    expected: &str,
    reads: usize,
) {
    let before = session.inspect_command_journal(fixture).lines().count();
    query(session, mouse, input, expected);
    // Scope-specific current repo access is independent of the two numeric
    // principal brackets; callers explicitly use4 only with no private sources.
    session.wait_for_command_count(fixture, before + reads);
    session.wait_for_screen("Search complete");
    session.wait_for_screen(expected);
    if expected.starts_with("1 results") {
        let needle = input.split_whitespace().next().unwrap();
        session
            .try_wait_for_condition(
                |screen| {
                    screen
                        .lines()
                        .filter(|line| !line.contains("[/ Query]"))
                        .any(|line| line.contains(needle))
                },
                needle,
                Duration::from_secs(8),
            )
            .unwrap();
    }
}
fn denied_cached_read() -> Vec<crate::support::GhStep> {
    let mut steps = gh_fixture::account(42);
    steps.push(gh_fixture::read_failure(
        "repos/example/repo",
        1,
        "gh: Forbidden (HTTP 403)",
    ));
    steps.extend(gh_fixture::account(42));
    steps
}
fn denied_private_query(session: &mut Session, fixture: &Fixture, mouse: bool, input: &str) {
    let before = session.inspect_command_journal(fixture).lines().count();
    query(session, mouse, input, "prior results hidden");
    exact_journal(session, fixture, before + 5);
    session.wait_for_screen("Current repository access unavailable");
    assert!(
        !session.screen().contains("0 results •"),
        "Current denied access is unavailable, not complete empty membership"
    );
}
fn context(session: &mut Session, mouse: bool, expected: &[&str]) {
    choose(session, mouse, "[o Open result]", b"o");
    session.wait_for_screen("Exact search source • read-only");
    for text in expected {
        session.wait_for_screen(text);
    }
    choose(session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_control("[H Sources]");
}

#[test]
fn collaboration_index_keyboard_pr_general_review_inline_exact_context_edit_delete_refresh_restart_and_remove(
) {
    pr_sources(false);
}
#[test]
fn collaboration_index_mouse_pr_general_review_inline_exact_context_edit_delete_refresh_restart_and_remove(
) {
    pr_sources(true);
}
fn pr_sources(mouse: bool) {
    let fixture = remote_fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let parent = gh_fixture::pr("PrBodyOnlyNeedle");
    let mut steps = gh_fixture::pr_load(
        &parent,
        vec![gh_fixture::comment(501, "OriginalCommentOnlyNeedle", false)],
        vec![gh_fixture::review(502, "ReviewOnlyNeedle")],
        vec![gh_fixture::thread(
            vec![gh_fixture::inline(503, "InlineOnlyNeedle")],
            false,
        )],
        42,
    );
    for _ in 0..6 {
        steps.extend(gh_fixture::private_source_read(42));
    } // Three exact queries + three native resource opens.
    steps.extend(gh_fixture::pr_load(
        &parent,
        vec![gh_fixture::comment(501, "FreshCommentOnlyNeedle", false)],
        vec![],
        vec![gh_fixture::thread(
            vec![gh_fixture::inline(503, "InlineOnlyNeedle")],
            false,
        )],
        42,
    ));
    for _ in 0..3 {
        steps.extend(gh_fixture::private_source_read(42));
    } // Two deleted/edited old searches and fresh rank.
    steps.extend(gh_fixture::private_read(42)); // Save changes only local intent.
    steps.extend(gh_fixture::private_read(42)); // Restart explicit host/account verification, no remote search.
    steps.extend(gh_fixture::private_source_read(42)); // Restore saved cached ranked query.
    steps.extend(gh_fixture::account(42)); // Exact Source Remove identity, local-only effect.
    steps.extend(gh_fixture::private_source_read(42)); // Restore old saved intent after fence cannot revive its text.
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_search(&mut session, mouse);
    query(&mut session, mouse, "ReviewOnlyNeedle", "0 results • Exact");
    sources(&mut session, mouse);
    choose(&mut session, mouse, "[p PR]", b"p");
    source_field(&mut session, mouse, "[v PR/issue number]", "7");
    choose(&mut session, mouse, "[l Load source]", b"l");
    session.wait_for_screen("Loaded 4 cached discussion records");
    let targets = private_sources(&fixture);
    assert_eq!(targets.discussions.len(), 4);
    for target in &targets.discussions {
        let cache = remote_cache(&fixture);
        let generation = cache
            .sync_checkpoint(&private_scope(), &target.source())
            .unwrap()
            .unwrap()
            .0;
        assert_eq!(
            cache
                .binding_status(&private_scope(), &target.source())
                .unwrap()
                .unwrap()
                .visible_generation,
            Some(generation)
        );
    }
    close_sources(&mut session, mouse);
    private_query(
        &mut session,
        &fixture,
        mouse,
        "OriginalCommentOnlyNeedle",
        "1 results • Exact",
    );
    context(
        &mut session,
        mouse,
        &["COMMENT_501", gh_fixture::PR_NODE, "discussion-author"],
    );
    private_query(
        &mut session,
        &fixture,
        mouse,
        "ReviewOnlyNeedle",
        "1 results • Exact",
    );
    context(
        &mut session,
        mouse,
        &[
            "REVIEW_502",
            "review-author",
            "cccccccccccccccccccccccccccccccccccccccc",
        ],
    );
    session.resize(80, 24);
    private_query(
        &mut session,
        &fixture,
        mouse,
        "InlineOnlyNeedle",
        "1 results • Exact",
    );
    context(&mut session, mouse, &["INLINE_503", "THREAD_1", "ci-bot"]);
    sources(&mut session, mouse);
    choose(&mut session, mouse, "[l Load source]", b"l");
    session.wait_for_screen("Loaded 3 cached discussion records");
    let bodies = remote_cache(&fixture)
        .list(&private_scope(), Some("collaboration-document-v1"))
        .unwrap();
    assert!(bodies
        .iter()
        .any(|record| record.payload["body"] == "FreshCommentOnlyNeedle"));
    assert!(!bodies.iter().any(
        |record| record.payload["body"] == "OriginalCommentOnlyNeedle"
            || record.payload["body"] == "ReviewOnlyNeedle"
    ));
    close_sources(&mut session, mouse);
    private_query(
        &mut session,
        &fixture,
        mouse,
        "OriginalCommentOnlyNeedle",
        "0 results • Exact",
    );
    private_query(
        &mut session,
        &fixture,
        mouse,
        "ReviewOnlyNeedle",
        "0 results • Exact",
    );
    choose(&mut session, mouse, "n Ranked", b"n");
    private_query(
        &mut session,
        &fixture,
        mouse,
        "FreshCommentOnlyNeedle",
        "1 results • Ranked",
    );
    choose(&mut session, mouse, "[z Name]", b"z");
    session.send_key(b"\x01");
    session.paste("private discussion scope");
    save_query_at_80(&mut session, mouse);
    session.wait_for_screen("Saved query: private discussion scope");
    let mut restarted = session.restart_with_same_cache(&fixture, 80, 24);
    // The loading shell already paints the mode controls; choosing GitHub
    // before the prepared corpus lands is superseded by its ready status.
    open_search(&mut restarted, mouse);
    choose(&mut restarted, mouse, "h GitHub", b"h");
    restarted.wait_for_screen("GitHub scope verified");
    restarted.wait_for_screen("[u private discussion scope]");
    choose(&mut restarted, mouse, "[u private discussion scope]", b"u");
    restarted.wait_for_screen("1 results • Ranked");
    sources(&mut restarted, mouse);
    source_field(&mut restarted, mouse, "[v PR/issue number]", "7");
    choose(&mut restarted, mouse, "[d Remove source]", b"d");
    restarted.wait_for_screen("Removed 4 selected sources");
    assert!(private_sources(&fixture).discussions.is_empty());
    for target in targets.discussions {
        assert!(remote_cache(&fixture)
            .binding_status(&private_scope(), &target.source())
            .unwrap()
            .unwrap()
            .visible_generation
            .is_none());
    }
    assert!(
        remote_cache(&fixture)
            .list(&private_scope(), Some("collaboration-document-v1"))
            .unwrap()
            .iter()
            .any(|record| record.payload["body"] == "FreshCommentOnlyNeedle"),
        "Remove hides but preserves canonical body bytes and saved intent"
    );
    close_sources(&mut restarted, mouse);
    choose(&mut restarted, mouse, "[u private discussion scope]", b"u");
    restarted.wait_for_screen("0 results • Ranked");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        b"dirty private checkout bytes\n"
    );
    exact_journal(&mut restarted, &fixture, steps.len());
    restarted.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn collaboration_index_keyboard_issue_body_comment_exact_context_edits_deletions_and_remove() {
    issue_sources(false);
}
#[test]
fn collaboration_index_mouse_issue_body_comment_exact_context_edits_deletions_and_remove() {
    issue_sources(true);
}
fn issue_sources(mouse: bool) {
    let fixture = remote_fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let parent = gh_fixture::issue("IssueBodyOnlyNeedle");
    let fresh = gh_fixture::issue("FreshIssueBodyOnlyNeedle");
    let mut steps = gh_fixture::issue_load(
        &parent,
        vec![gh_fixture::comment(601, "IssueCommentOnlyNeedle", true)],
        42,
    );
    for _ in 0..4 {
        steps.extend(gh_fixture::private_source_read(42));
    } // Two exact issue destinations.
    steps.extend(gh_fixture::issue_load(&fresh, vec![], 42));
    for _ in 0..2 {
        steps.extend(gh_fixture::private_source_read(42));
    }
    steps.extend(gh_fixture::account(42)); // Source Remove is a local visibility effect.
    steps.extend(gh_fixture::private_read(42));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_search(&mut session, mouse);
    sources(&mut session, mouse);
    choose(&mut session, mouse, "[i Issue]", b"i");
    source_field(&mut session, mouse, "[v PR/issue number]", "8");
    choose(&mut session, mouse, "[l Load source]", b"l");
    session.wait_for_screen("Loaded 2 cached discussion records");
    assert_eq!(private_sources(&fixture).discussions.len(), 2);
    remote_cache(&fixture)
        .put_draft(&private_scope(), "untouched-issue-draft", "Retained draft")
        .unwrap();
    close_sources(&mut session, mouse);
    private_query(
        &mut session,
        &fixture,
        mouse,
        "IssueBodyOnlyNeedle type:issue",
        "1 results • Exact",
    );
    context(
        &mut session,
        mouse,
        &[
            "ISSUE_8",
            "Native hydration issue",
            "unknown actor [unknown]",
        ],
    );
    session.resize(80, 24);
    private_query(
        &mut session,
        &fixture,
        mouse,
        "IssueCommentOnlyNeedle",
        "1 results • Exact",
    );
    context(
        &mut session,
        mouse,
        &["COMMENT_601", "ISSUE_8", "discussion-author"],
    );
    sources(&mut session, mouse);
    choose(&mut session, mouse, "[l Load source]", b"l");
    session.wait_for_screen("Loaded 1 cached discussion records");
    let records = remote_cache(&fixture)
        .list(&private_scope(), Some("collaboration-document-v1"))
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].payload["body"], "FreshIssueBodyOnlyNeedle");
    close_sources(&mut session, mouse);
    private_query(
        &mut session,
        &fixture,
        mouse,
        "IssueCommentOnlyNeedle",
        "0 results • Exact",
    );
    private_query(
        &mut session,
        &fixture,
        mouse,
        "FreshIssueBodyOnlyNeedle type:issue",
        "1 results • Exact",
    );
    sources(&mut session, mouse);
    choose(&mut session, mouse, "[d Remove source]", b"d");
    session.wait_for_screen("Removed 2 selected sources");
    assert!(private_sources(&fixture).discussions.is_empty());
    close_sources(&mut session, mouse);
    private_query_reads(
        &mut session,
        &fixture,
        mouse,
        "FreshIssueBodyOnlyNeedle type:issue",
        "0 results • Exact",
        4,
    );
    assert_eq!(
        remote_cache(&fixture)
            .draft(&private_scope(), "untouched-issue-draft")
            .unwrap()
            .as_deref(),
        Some("Retained draft")
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        b"dirty private checkout bytes\n"
    );
    exact_journal(&mut session, &fixture, steps.len());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn collaboration_index_keyboard_permission_revocation_hides_cached_ranked_bodies_preserves_drafts_and_views(
) {
    revoked_source(false);
}
#[test]
fn collaboration_index_mouse_permission_revocation_hides_cached_ranked_bodies_preserves_drafts_and_views(
) {
    revoked_source(true);
}
fn revoked_source(mouse: bool) {
    let fixture = remote_fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let mut steps = gh_fixture::pr_load(
        &gh_fixture::pr("Private parent body"),
        vec![gh_fixture::comment(701, "RevokedPrivateOnlyNeedle", false)],
        vec![],
        vec![],
        42,
    );
    steps.extend(gh_fixture::private_source_read(42));
    steps.extend(gh_fixture::private_source_read(42));
    steps.extend(gh_fixture::repository_denied(403, 42));
    steps.extend(denied_cached_read());
    steps.extend(denied_cached_read());
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_search(&mut session, mouse);
    sources(&mut session, mouse);
    source_field(&mut session, mouse, "[v PR/issue number]", "7");
    choose(&mut session, mouse, "[l Load source]", b"l");
    session.wait_for_screen("Loaded 2 cached discussion records");
    let targets = private_sources(&fixture).discussions;
    let cache = remote_cache(&fixture);
    cache
        .put_draft(&private_scope(), "private-draft", "Preserved user intent")
        .unwrap();
    cache
        .save_view(
            &private_scope(),
            "independent-view",
            &serde_json::json!({"preserve":true}),
        )
        .unwrap();
    assert_eq!(
        cache
            .list(&private_scope(), Some("collaboration-document-v1"))
            .unwrap()
            .len(),
        2
    );
    drop(cache);
    close_sources(&mut session, mouse);
    private_query(
        &mut session,
        &fixture,
        mouse,
        "RevokedPrivateOnlyNeedle",
        "1 results • Exact",
    );
    choose(&mut session, mouse, "n Ranked", b"n");
    private_query(
        &mut session,
        &fixture,
        mouse,
        "RevokedPrivateOnlyNeedle",
        "1 results • Ranked",
    );
    session.resize(80, 24);
    sources(&mut session, mouse);
    choose(&mut session, mouse, "[l Load source]", b"l");
    session.wait_for_screen("Repository access revoked");
    let cache = remote_cache(&fixture);
    assert!(cache
        .list(&private_scope(), Some("collaboration-document-v1"))
        .unwrap()
        .is_empty());
    for target in targets {
        assert!(cache
            .binding_status(&private_scope(), &target.source())
            .unwrap()
            .unwrap()
            .visible_generation
            .is_none());
    }
    assert_eq!(
        cache
            .draft(&private_scope(), "private-draft")
            .unwrap()
            .as_deref(),
        Some("Preserved user intent")
    );
    assert_eq!(
        cache
            .saved_view(&private_scope(), "independent-view")
            .unwrap()
            .unwrap(),
        serde_json::json!({"preserve":true})
    );
    drop(cache);
    close_sources(&mut session, mouse);
    denied_private_query(&mut session, &fixture, mouse, "RevokedPrivateOnlyNeedle");
    choose(&mut session, mouse, "e Exact", b"e");
    denied_private_query(&mut session, &fixture, mouse, "RevokedPrivateOnlyNeedle");
    assert!(
        !session
            .screen()
            .lines()
            .filter(|line| !line.contains("[/ Query]"))
            .any(|line| line.contains("RevokedPrivateOnlyNeedle")),
        "Revoked private bodies must disappear beyond the user's editable query"
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        b"dirty private checkout bytes\n"
    );
    exact_journal(&mut session, &fixture, steps.len());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn collaboration_index_keyboard_reused_username_postfetch_numeric_account_switch_cannot_reuse_private_sources(
) {
    account_switch(false);
}
#[test]
fn collaboration_index_mouse_reused_username_postfetch_numeric_account_switch_cannot_reuse_private_sources(
) {
    account_switch(true);
}
fn account_switch(mouse: bool) {
    let fixture = remote_fixture();
    let parent = gh_fixture::pr("Prior principal private parent");
    let mut steps = gh_fixture::pr_load(
        &parent,
        vec![gh_fixture::comment(801, "PriorPrincipalOnlyNeedle", false)],
        vec![],
        vec![],
        42,
    );
    steps.extend(gh_fixture::private_source_read(42));
    let mut reassigned = gh_fixture::pr_load(
        &parent,
        vec![gh_fixture::comment(
            801,
            "NeverAdmitReassignedPrivateText",
            false,
        )],
        vec![],
        vec![],
        43,
    );
    // The first post-fetch identity is sufficient to reject publication; a
    // second successful-read verification is not invoked on the failure path.
    reassigned.truncate(reassigned.len() - 2);
    steps.extend(reassigned);
    steps.extend(gh_fixture::private_read(43)); // Explicit new-principal host verification.
    steps.extend(gh_fixture::private_read(43)); // Local Exact underUID43 cannot importUID42cache.
    fixture.scripted_gh(&steps);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    open_search(&mut session, mouse);
    sources(&mut session, mouse);
    source_field(&mut session, mouse, "[v PR/issue number]", "7");
    choose(&mut session, mouse, "[l Load source]", b"l");
    session.wait_for_screen("Loaded 2 cached discussion records");
    close_sources(&mut session, mouse);
    private_query(
        &mut session,
        &fixture,
        mouse,
        "PriorPrincipalOnlyNeedle",
        "1 results • Exact",
    );
    session.resize(80, 24);
    sources(&mut session, mouse);
    choose(&mut session, mouse, "[l Load source]", b"l");
    session.wait_for_screen("Source account unavailable");
    assert!(!session.screen().contains("Search sources • explicit"));
    assert!(!session.screen().contains("PriorPrincipalOnlyNeedle"));
    choose(&mut session, mouse, "h GitHub", b"h");
    session.wait_for_screen("GitHub scope verified");
    choose(&mut session, mouse, "e Exact", b"e");
    private_query_reads(
        &mut session,
        &fixture,
        mouse,
        "PriorPrincipalOnlyNeedle",
        "0 results • Exact",
        4,
    );
    let new_scope = Scope::github(gh_fixture::HOST, Some(43)).unwrap();
    let cache = remote_cache(&fixture);
    assert!(cache
        .saved_view(&new_scope, "search-source-selection:v1")
        .unwrap()
        .is_none());
    assert!(cache
        .list(&new_scope, Some("collaboration-document-v1"))
        .unwrap()
        .is_empty());
    assert!(cache
        .list(&private_scope(), Some("collaboration-document-v1"))
        .unwrap()
        .iter()
        .any(|record| record.payload["body"] == "PriorPrincipalOnlyNeedle"));
    assert!(!cache
        .list(&private_scope(), Some("collaboration-document-v1"))
        .unwrap()
        .iter()
        .any(|record| record.payload["body"] == "NeverAdmitReassignedPrivateText"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        b"dirty private checkout bytes\n"
    );
    exact_journal(&mut session, &fixture, steps.len());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn collaboration_index_keyboard_transport_unavailable_hides_old_ranked_text_without_deleting_then_explicit_hydration_recovers(
) {
    unavailable_source(false);
}
#[test]
fn collaboration_index_mouse_transport_unavailable_hides_old_ranked_text_without_deleting_then_explicit_hydration_recovers(
) {
    unavailable_source(true);
}
fn unavailable_source(mouse: bool) {
    let fixture = remote_fixture();
    let parent = gh_fixture::pr("Unavailable parent body");
    let mut steps = gh_fixture::pr_load(
        &parent,
        vec![gh_fixture::comment(
            901,
            "UnavailablePriorOnlyNeedle",
            false,
        )],
        vec![],
        vec![],
        42,
    );
    steps.extend(gh_fixture::private_source_read(42));
    steps.extend(gh_fixture::account(42));
    steps.push(gh_fixture::read_failure(
        "repos/example/repo",
        1,
        "gh: dial tcp github.invalid: network is unreachable",
    ));
    steps.extend(gh_fixture::account(42));
    steps.extend(gh_fixture::private_source_read(42));
    steps.extend(gh_fixture::pr_load(
        &parent,
        vec![gh_fixture::comment(
            901,
            "RecoveredPrivateOnlyNeedle",
            false,
        )],
        vec![],
        vec![],
        42,
    ));
    steps.extend(gh_fixture::private_source_read(42));
    fixture.scripted_gh(&steps);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    open_search(&mut session, mouse);
    sources(&mut session, mouse);
    source_field(&mut session, mouse, "[v PR/issue number]", "7");
    choose(&mut session, mouse, "[l Load source]", b"l");
    session.wait_for_screen("Loaded 2 cached discussion records");
    let targets = private_sources(&fixture).discussions;
    close_sources(&mut session, mouse);
    choose(&mut session, mouse, "n Ranked", b"n");
    private_query(
        &mut session,
        &fixture,
        mouse,
        "UnavailablePriorOnlyNeedle",
        "1 results • Ranked",
    );
    session.resize(80, 24);
    sources(&mut session, mouse);
    choose(&mut session, mouse, "[l Load source]", b"l");
    session.wait_for_screen("Source unavailable");
    let cache = remote_cache(&fixture);
    assert!(cache.list(&private_scope(), Some("collaboration-document-v1")).unwrap().iter().any(|record| record.payload["body"] == "UnavailablePriorOnlyNeedle"), "An offline transport failure is not proof of permission revocation and cannot erase canonical old bodies");
    for target in targets {
        assert!(cache
            .binding_status(&private_scope(), &target.source())
            .unwrap()
            .unwrap()
            .visible_generation
            .is_none());
    }
    drop(cache);
    assert!(!session.screen().contains("Repository access revoked"));
    close_sources(&mut session, mouse);
    private_query(
        &mut session,
        &fixture,
        mouse,
        "UnavailablePriorOnlyNeedle",
        "0 results • Ranked",
    );
    sources(&mut session, mouse);
    choose(&mut session, mouse, "[l Load source]", b"l");
    session.wait_for_screen("Loaded 2 cached discussion records");
    close_sources(&mut session, mouse);
    private_query(
        &mut session,
        &fixture,
        mouse,
        "RecoveredPrivateOnlyNeedle",
        "1 results • Ranked",
    );
    assert!(remote_cache(&fixture)
        .list(&private_scope(), Some("collaboration-document-v1"))
        .unwrap()
        .iter()
        .any(|record| record.payload["body"] == "RecoveredPrivateOnlyNeedle"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        b"dirty private checkout bytes\n"
    );
    exact_journal(&mut session, &fixture, steps.len());
    session.quit_and_assert_restored_terminal(mouse);
}
