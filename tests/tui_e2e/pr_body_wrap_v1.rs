//! PR summary bodies wrap to the pane instead of being clipped at its edge.
use super::*;
use std::time::Duration;
const REPO: &str = "example/repo0000";
const TAIL: &str = "WRAPPED_BODY_TAIL";

fn long_body() -> String {
    format!("{} {TAIL}", "words that keep going past the edge".repeat(8))
}
fn pr_value() -> Value {
    json!({"id":1007,"node_id":"PERSONAL_PR_1007","number":7,"title":"Long description",
        "body":long_body(),"url":format!("https://github.invalid/api/v3/repos/{REPO}/pulls/7"),
        "html_url":format!("https://github.invalid/{REPO}/pull/7"),
        "repository_url":format!("https://github.invalid/api/v3/repos/{REPO}"),
        "user":{"id":51,"node_id":"AUTHOR_51","login":"actual-author","type":"User"},
        "state":"open","draft":false,"merged":false,"changed_files":1,
        "head":{"sha":"a".repeat(40),"ref":"feature/wrap","repo":null},
        "base":{"sha":"b".repeat(40),"ref":"main","repo":{"id":10000,"node_id":"R_10000","full_name":REPO}}})
}
fn steps() -> Vec<GhStep> {
    let mut steps = identity("octocat");
    steps.extend(identity("octocat"));
    for queue in ["author", "assignee", "review-requested"] {
        steps.push(search("octocat", queue, vec![pr_value()], false));
    }
    steps.push(read(api(&format!("repos/{REPO}/pulls/7")), pr_value()));
    for endpoint in [
        "issues/7/comments",
        "pulls/7/commits",
        "pulls/7/files",
        "pulls/7/reviews",
        "issues/7/timeline",
    ] {
        let mut args = api(&format!("repos/{REPO}/{endpoint}?per_page=100"));
        args.extend(["--paginate".into(), "--slurp".into()]);
        steps.push(read(args, json!([[]])));
    }
    steps.push(read(api(&format!("repos/{REPO}/pulls/7")), pr_value()));
    steps
}
/// No row ends or starts inside a body word: a visible token never is a
/// strict fragment of one (e.g. `obs|erve`).
fn assert_word_boundaries(screen: &str) {
    let words = [
        "words",
        "that",
        "keep",
        "going",
        "past",
        "the",
        "edge",
        "edgewords",
        TAIL,
    ];
    for token in screen
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        // Key hints and short UI words are not body fragments.
        .filter(|token| token.len() >= 3)
    {
        assert!(
            words.contains(&token) || !words.iter().any(|word| word.contains(token)),
            "body split inside a word at {token:?}\n{screen}"
        );
    }
}
fn tail_visible(session: &mut Session) {
    if let Err(error) = session.try_wait_for_condition(
        |screen| screen.contains("Long description") && screen.contains(TAIL),
        "Wrapped PR body tail visible",
        Duration::from_secs(8),
    ) {
        session.save_artifacts();
        panic!("{error}\n{}", session.screen());
    }
}
fn journey(mouse: bool) {
    let fixture = configured();
    let steps = steps();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Inbox • focused view");
    refresh(&mut session, mouse);
    session.wait_for_screen("1 unique PRs across 3 queue matches");
    if mouse {
        session.click_visible_action("Long description");
    } else {
        session.send_key(b"\r");
    }
    tail_visible(&mut session);
    let body = long_body();
    assert!(
        !session.screen().lines().any(|line| line.contains(&body)),
        "the body is wider than the pane, so it must span rows"
    );
    assert_word_boundaries(&session.screen());
    session.resize(80, 24);
    // The small pane shows the body's beginning, then scrolls to its tail.
    session.wait_for_screen("words that keep going past the edge");
    for _ in 0..12 {
        if session.screen().contains(TAIL) {
            break;
        }
        if mouse {
            session.wheel(40, 18, true);
        } else {
            session.send_key(b"\x1b[6~");
        }
        session.poll_output(Duration::from_millis(100));
    }
    tail_visible(&mut session);
    assert_word_boundaries(&session.screen());
    journal(&mut session, &fixture, steps.len());
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(
        session.inspect_command_journal(&fixture).lines().count(),
        steps.len()
    );
}

#[test]
fn pr_body_wrap_keyboard_summary_shows_full_body_at_both_sizes() {
    journey(false)
}
#[test]
fn pr_body_wrap_mouse_summary_shows_full_body_at_both_sizes() {
    journey(true)
}
