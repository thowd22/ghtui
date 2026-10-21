//! TASK-13: a Timeline item opens its own comment, commit or head checks in that tab, and
//! the Timeline tab returns to the same item, by keys and by clicks. Only the Checks tab
//! reads (its check and required-check reads); Git, config and private storage stay unchanged.
use super::*;
use std::fs;

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OLDER: &str = "cccccccccccccccccccccccccccccccccccccccc";

fn steps() -> Vec<GhStep> {
    let mut steps = initial(false);
    let mut old = pages();
    // Check runs, statuses, then the base branch's protection and rulesets (TASK-14).
    let checks = old[13..17].to_vec();
    old.truncate(7);
    // Two commits: an older one and the current head.
    old[2].stdout = serde_json::to_vec(&json!([[
        {"sha":OLDER,"commit":{"message":"Add parser guard","author":{"name":"Alice","date":"2026-10-11T12:00:00Z"},"committer":{"name":"Alice"}},"author":{"login":"alice"},"committer":{"login":"alice"},"parents":[]},
        {"sha":HEAD,"commit":{"message":"Fix auth boundary","author":{"name":"Alice","date":"2026-10-12T12:00:00Z"},"committer":{"name":"Alice"}},"author":{"login":"alice"},"committer":{"login":"alice"},"parents":[{"sha":OLDER}]}
    ]]))
    .unwrap();
    // A label first, so the first cursor rests on no item.
    old[5].stdout = serde_json::to_vec(&json!([[
        {"event":"labeled","label":{"name":"security"},"actor":{"login":"alice"},"created_at":"2026-10-12T11:00:00Z"},
        {"event":"commented","id":10,"user":{"login":"reviewer"},"created_at":"2026-10-12T12:00:00Z","body":"Please keep auth strict"},
        {"event":"committed","sha":OLDER,"message":"Add parser guard","author":{"name":"Alice","date":"2026-10-11T12:00:00Z"}},
        {"event":"committed","sha":HEAD,"message":"Fix auth boundary","author":{"name":"Alice","date":"2026-10-12T12:00:00Z"}}
    ]]))
    .unwrap();
    steps.extend(old);
    steps.extend(checks);
    steps
}

fn sql(fixture: &Fixture) -> Vec<Vec<String>> {
    let db = rusqlite::Connection::open_with_flags(
        fixture.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    [
        "SELECT json_array(host,login,resource,body) FROM drafts ORDER BY host,login,resource",
        "SELECT json_array(host,login,kind,id,payload) FROM records ORDER BY host,login,kind,id",
    ]
    .iter()
    .map(|query| {
        db.prepare(query)
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
    })
    .collect()
}

/// The screen line that contains `text`, which must carry the `▌` selection bar.
fn assert_selected(session: &Session, text: &str) {
    let screen = session.screen();
    let line = screen
        .lines()
        .find(|line| line.contains(text))
        .unwrap_or_else(|| panic!("{text} not painted:\n{screen}"));
    assert!(line.contains('▌'), "{text} is not selected:\n{screen}");
}

fn select(session: &mut Session, mouse: bool, row_text: &str, status: &str) {
    if mouse {
        session.click_visible_action(row_text);
    } else {
        session.send_key(b"\x1b[B");
    }
    session.wait_for_screen(status);
    assert_selected(session, row_text);
}

fn go(session: &mut Session, mouse: bool, status: &str) {
    choose(session, mouse, "[Enter Go to item]", b"\r");
    session.wait_for_screen(status);
}

fn back_to_timeline(session: &mut Session, mouse: bool, item: &str, row_text: &str) {
    choose(session, mouse, "[t Timeline]", b"t");
    session.wait_for_screen(&format!("Back at the Timeline {item}"));
    assert_selected(session, row_text);
}

fn journey(mouse: bool) {
    let fixture = configured();
    fixture.init_git();
    fixture.commit_file("tracked", b"saved\n", "Timeline links canary");
    fs::write(fixture.repo().join("tracked"), b"dirty\n").unwrap();
    let git = |fixture: &Fixture| {
        (
            fixture.git(&["show-ref", "--head"]),
            fs::read(fixture.repo().join(".git/index")).unwrap(),
            fixture.git(&["diff", "--binary"]),
        )
    };
    let original_git = git(&fixture);
    let config_path = fixture.root.join("config/ghtui/config.toml");
    let config = fs::read(&config_path).unwrap();
    let steps = steps();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.assert_focused_view("Inbox");
    refresh(&mut session, mouse);
    session.wait_for_screen("3 unique PRs across 5 queue matches");
    choose(&mut session, mouse, "Improve auth", b"\r");
    session.wait_for_screen("Head aaaaaaaaa");
    journal(&mut session, &fixture, 15);
    let original_sql = sql(&fixture);
    choose(&mut session, mouse, "[t Timeline]", b"t");
    session.wait_for_screen("Label: security");
    session.wait_for_screen("comment author • commented");

    // A comment opens in Conversation on its own row, and t comes back to it.
    select(
        &mut session,
        mouse,
        "comment author • commented",
        "Selected timeline comment #10 • Enter shows it in Conversation",
    );
    go(
        &mut session,
        mouse,
        "Timeline comment #10 → Conversation • t returns to the Timeline item",
    );
    assert_selected(&session, "comment #10");
    back_to_timeline(
        &mut session,
        mouse,
        "comment #10",
        "comment author • commented",
    );

    // An older commit opens in Commits, scrolled to its row.
    select(
        &mut session,
        mouse,
        "Add parser guard",
        "Selected timeline commit ccccccc • Enter shows it in Commits",
    );
    go(&mut session, mouse, "Timeline commit ccccccc → Commits");
    session.wait_for_screen(&format!("Commit {OLDER}"));
    back_to_timeline(&mut session, mouse, "commit ccccccc", "Add parser guard");
    journal(&mut session, &fixture, 15);
    assert_eq!(sql(&fixture), original_sql);

    // The head commit opens the checks that ran on it: the Checks tab's own four reads.
    select(
        &mut session,
        mouse,
        "Fix auth boundary",
        "Selected timeline commit aaaaaaa • Enter shows it in Checks",
    );
    go(&mut session, mouse, "Timeline commit aaaaaaa → Checks");
    session.wait_for_screen("Rust tests");
    assert_selected(&session, "Rust tests");
    journal(&mut session, &fixture, 19);

    // At 80x24 the item control stays on the bar, and the return still works.
    session.resize(80, 24);
    back_to_timeline(&mut session, mouse, "commit aaaaaaa", "Fix auth boundary");
    assert!(
        session.screen().contains("Enter Go to"),
        "{}",
        session.screen()
    );
    choose(&mut session, mouse, "[b Back]", b"b");
    session.wait_for_screen("3 unique PRs across 5 queue matches");
    assert_selected(&session, "Improve auth");
    journal(&mut session, &fixture, 19);
    assert_eq!(git(&fixture), original_git);
    assert_eq!(fs::read(&config_path).unwrap(), config);
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(sql(&fixture)[0], original_sql[0], "drafts unchanged");
}

#[test]
fn pr_timeline_links_v13_keyboard_opens_comment_commit_and_head_checks_and_returns() {
    journey(false)
}
#[test]
fn pr_timeline_links_v13_mouse_opens_comment_commit_and_head_checks_and_returns() {
    journey(true)
}
