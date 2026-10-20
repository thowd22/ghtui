//! Merge from the PR actions menu: preview first, then exactly one pinned
//! `gh pr merge --match-head-commit` after confirmation.
use super::*;

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn merged_pr() -> Value {
    let mut value = pr();
    value["state"] = json!("closed");
    value["merged"] = json!(true);
    value
}
fn merge_command(method: &str) -> GhStep {
    GhStep::read(
        &[
            "pr",
            "merge",
            "7",
            "--repo",
            "github.invalid/example/repo",
            method,
            "--match-head-commit",
            HEAD,
        ],
        "",
    )
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
fn journey(mouse: bool) {
    let fixture = configured();
    let git = local_git_witness(&fixture);
    let mut steps = initial();
    steps.extend(details(vec![starter()]));
    let opened = steps.len();
    // Opening the form reads the repository's allowed merge methods.
    steps.push(repo_settings_step(repo_settings()));
    // Preview: fresh account and PR; no write.
    steps.extend(guard());
    let previewed = steps.len();
    // Confirm: fresh account/PR, provider guard, the pinned merge, readback.
    steps.extend(guard());
    steps.extend(guard());
    steps.push(merge_command("--squash"));
    steps.push(step(
        "repos/example/repo/pulls/7",
        "GET",
        merged_pr(),
        false,
    ));
    // Post-write refresh of the same PR, now merged.
    let mut refresh = inspected(vec![starter()]);
    for step in refresh.iter_mut() {
        if step
            .args
            .get(1)
            .is_some_and(|arg| arg == "repos/example/repo/pulls/7")
        {
            step.stdout = serde_json::to_vec(&merged_pr()).unwrap();
        }
    }
    steps.extend(refresh);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_summary(&mut session, mouse);
    records(&mut session, &fixture, opened);
    menu(&mut session, mouse, "Merge pull request");
    session.wait_for_screen("Merge method");
    if mouse {
        session.click_visible_action("[squash]");
    } else {
        session.send_key(b" ");
    }
    session.wait_for_screen("Merge method • squash");
    preview(&mut session, mouse);
    session.wait_for_screen("Method: squash • feature → main");
    session.wait_for_screen(&format!("GitHub merges only if the head is still {HEAD}"));
    let before = records(&mut session, &fixture, previewed);
    assert_eq!(mutation_count(&before), 0);
    assert!(!session
        .inspect_command_journal(&fixture)
        .contains("\"merge\""));
    session.resize(80, 24);
    if mouse {
        confirm(&mut session, mouse);
    } else {
        session.send_key(b"\x13");
    }
    session.wait_for_screen("example/repo#7 • Native collaboration • merged");
    session.wait_for_screen("Published • Merge pull request");
    let after = records(&mut session, &fixture, steps.len());
    let merges = after
        .iter()
        .filter(|record| record["args"][0] == "pr" && record["args"][1] == "merge")
        .count();
    assert_eq!(merges, 1, "exactly one pinned merge request");
    assert_eq!(local_git_witness(&fixture), git, "local checkout untouched");
    session.quit_and_assert_restored_terminal(mouse);
}

fn draft_journey(mouse: bool) {
    let fixture = configured();
    let mut draft = pr();
    draft["draft"] = json!(true);
    let mut steps = initial();
    for step in steps.iter_mut() {
        if step.args.iter().any(|arg| arg == "search/issues") {
            let text = String::from_utf8(step.stdout.clone()).unwrap();
            step.stdout = text
                .replace("\"draft\":false", "\"draft\":true")
                .into_bytes();
        }
    }
    let mut detail = details(vec![starter()]);
    for step in detail.iter_mut() {
        if step
            .args
            .get(1)
            .is_some_and(|arg| arg == "repos/example/repo/pulls/7")
        {
            step.stdout = serde_json::to_vec(&draft).unwrap();
        }
    }
    steps.extend(detail);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_summary(&mut session, mouse);
    menu(&mut session, mouse, "Merge pull request");
    session.wait_for_screen("Mark this draft PR ready for review before merging.");
    assert!(!session.screen().contains("Merge method"));
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, steps.len())),
        0
    );
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    closed_explanation(
        &mut session,
        "Mark this draft PR ready for review before merging.",
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn pr_merge_keyboard_previews_then_sends_one_pinned_squash_merge() {
    journey(false)
}
#[test]
fn pr_merge_mouse_previews_then_sends_one_pinned_squash_merge() {
    journey(true)
}
#[test]
fn pr_merge_keyboard_draft_pr_explains_instead_of_opening_a_form() {
    draft_journey(false)
}
#[test]
fn pr_merge_mouse_draft_pr_explains_instead_of_opening_a_form() {
    draft_journey(true)
}
