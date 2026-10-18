//! Daily review decisions from the PR actions menu: Approve, Request changes
//! and Submit comment review open a local draft, preview the exact effect,
//! publish exactly one review after confirmation and refresh to show it.
use super::*;

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// The fixture PR authored by someone else, so the viewer may approve it.
fn other_author(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if map.get("number") == Some(&json!(7)) {
                map.insert("user".into(), json!({"id":7,"login":"hubot","type":"User"}));
            }
            map.values_mut().for_each(other_author);
        }
        Value::Array(items) => items.iter_mut().for_each(other_author),
        _ => {}
    }
}
fn rewrite(steps: &mut [GhStep], reviews: Option<&Value>) {
    for step in steps.iter_mut() {
        let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) else {
            continue;
        };
        other_author(&mut value);
        if let Some(reviews) = reviews {
            if step
                .args
                .get(1)
                .is_some_and(|arg| arg == "repos/example/repo/pulls/7/reviews?per_page=100")
            {
                value = json!([[reviews]]);
            }
        }
        step.stdout = serde_json::to_vec(&value).unwrap();
    }
}
fn review(state: &str, body: &str) -> Value {
    json!({"id":900,"node_id":"PRR_900","state":state,"body":body,"commit_id":HEAD,
        "pull_request_url":"https://api.github.invalid/repos/example/repo/pulls/7",
        "html_url":"https://github.invalid/example/repo/pull/7#pullrequestreview-900",
        "user":{"id":42,"login":"octocat","type":"User"},
        "submitted_at":"2026-10-17T00:00:00Z"})
}
fn open_summary(session: &mut Session, mouse: bool) {
    session.wait_for_screen("ghtui");
    choose(session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Inbox • focused view");
    choose(session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("Native collaboration");
    choose(session, mouse, "Native collaboration", b"\r");
    session.wait_for_screen("1 comments");
}
fn journey(mouse: bool, label: &str, event: &str, state: &str, body: &str, shown: &str) {
    let fixture = configured();
    let git = local_git_witness(&fixture);
    let mut steps = initial();
    steps.extend(details(vec![starter()]));
    let opened = steps.len();
    // Confirm: fresh account and PR, provider guard, ONE POST, readback.
    steps.extend(guard());
    steps.extend(guard());
    let mut post = write(
        "repos/example/repo/pulls/7/reviews",
        "POST",
        Some(json!({"body":body,"commit_id":HEAD,"event":event})),
        review(state, body),
    );
    post.args.truncate(6);
    post.args.extend(["--input".into(), "-".into()]);
    steps.push(post);
    steps.push(step(
        "repos/example/repo/pulls/7/reviews/900",
        "GET",
        review(state, body),
        false,
    ));
    let published = steps.len();
    let mut refresh = inspected(vec![starter()]);
    rewrite(&mut refresh, Some(&review(state, body)));
    steps.extend(refresh);
    rewrite(&mut steps[..published], None);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_summary(&mut session, mouse);
    records(&mut session, &fixture, opened);
    menu(&mut session, mouse, label);
    session.wait_for_screen(&format!("{label} • example/repo#7"));
    session.wait_for_screen("Review summary:");
    if !body.is_empty() {
        session.paste(body);
        session.wait_for_screen(body);
    }
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen(shown);
    session.wait_for_screen("Nothing published");
    let before = records(&mut session, &fixture, opened);
    assert_eq!(mutation_count(&before), 0, "preview sends nothing");
    if mouse {
        session.click_visible_action("[Submit]");
    } else {
        session.send_key(b"\x13");
    }
    session.wait_for_screen("Published");
    let after = records(&mut session, &fixture, steps.len());
    assert_eq!(mutation_count(&after), 1, "exactly one review POST");
    session.wait_for_screen(&format!("@octocat {}", state_word(state)));
    assert_eq!(local_git_witness(&fixture), git, "local checkout untouched");
    session.quit_and_assert_restored_terminal(mouse);
}
fn state_word(state: &str) -> &'static str {
    match state {
        "APPROVED" => "approved",
        "CHANGES_REQUESTED" => "requested changes",
        _ => "commented",
    }
}
fn own_pr_journey(mouse: bool) {
    let fixture = configured();
    let mut steps = initial();
    steps.extend(details(vec![starter()]));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_summary(&mut session, mouse);
    menu(&mut session, mouse, "Approve review");
    session.wait_for_screen("You cannot approve or request changes on your own PR.");
    assert!(!session.screen().contains("Approve review • example/repo#7"));
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, steps.len())),
        0
    );
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_screen("Native collaboration • open");
    // Pending reviews and dismissal are discoverable from the same menu.
    menu(
        &mut session,
        mouse,
        "Pending reviews, dismissal, re-request",
    );
    session.wait_for_screen("then F10 Reviews");
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_screen("Native collaboration • open");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn v1_pr_reviews_keyboard_approve_previews_publishes_and_refreshes() {
    journey(
        false,
        "Approve review",
        "APPROVE",
        "APPROVED",
        "",
        "Approve example/repo#7 by @hubot",
    )
}
#[test]
fn v1_pr_reviews_mouse_approve_previews_publishes_and_refreshes() {
    journey(
        true,
        "Approve review",
        "APPROVE",
        "APPROVED",
        "",
        "Approve example/repo#7 by @hubot",
    )
}
#[test]
fn v1_pr_reviews_keyboard_request_changes_with_summary() {
    journey(
        false,
        "Request changes",
        "REQUEST_CHANGES",
        "CHANGES_REQUESTED",
        "Please add a test",
        "Request changes on example/repo#7 by @hubot",
    )
}
#[test]
fn v1_pr_reviews_mouse_comment_review_with_summary() {
    journey(
        true,
        "Submit comment review",
        "COMMENT",
        "COMMENTED",
        "Looks reasonable so far",
        "Comment on example/repo#7 by @hubot",
    )
}
#[test]
fn v1_pr_reviews_keyboard_own_pr_explains_instead_of_opening_a_form() {
    own_pr_journey(false)
}
#[test]
fn v1_pr_reviews_mouse_own_pr_explains_instead_of_opening_a_form() {
    own_pr_journey(true)
}
