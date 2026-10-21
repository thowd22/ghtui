//! TASK-34 actor profiles on a real PR: the PR author (from Summary, which has no comment
//! targets), a bot commenter and a deleted (ghost) commenter each open one read-only profile
//! by `@` and by clicking `[@ Actor profile]` or its Commands entry. Profiles open only from
//! rows that were already targets (a selected comment); no heading becomes selectable. The
//! profile shows the same initials and `@login` the Conversation row paints, the account kind
//! and whether the host verified it; opening, closing and resizing reads nothing more from gh
//! and leaves the checkout untouched. On Commits the same control lists every commit author
//! and committer, including an honest "No GitHub account" profile for an unlinked committer.
use super::{
    choose, closed_explanation, configured, details, initial, local_git_witness, mutation_count,
    records, select_comment,
};
use crate::support::Session;
use serde_json::{json, Value};
use std::time::Duration;

fn comment(id: u64, login: &str, kind: &str, body: &str) -> Value {
    json!({"id":id,"node_id":format!("IC_{id}"),"body":body,"user":{"id":id,"login":login,"type":kind},"created_at":"2026-10-13T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","issue_url":"https://api.github.invalid/repos/example/repo/issues/7","html_url":format!("https://github.invalid/example/repo/pull/7#issuecomment-{id}")})
}

fn open_summary(session: &mut Session, mouse: bool) {
    session.wait_for_screen("ghtui");
    choose(session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Inbox • focused view");
    choose(session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("Native collaboration");
    session.wait_for_screen("Fresh • fetched");
    choose(session, mouse, "Native collaboration", b"\r");
    session.wait_for_screen("3 comments");
}

fn open_conversation(session: &mut Session, mouse: bool) {
    let before = session.completed_cursorless_end();
    choose(session, mouse, "[c Conversation]", b"c");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            crate::support::main_footer_visible(frame)
                && text.contains("@octocat PR description")
                && text.contains("Removed account note")
                && !text.contains("Verifying original Conversation")
                && !text.contains("Loading PR Metadata")
        },
        "completed idle Conversation with all three comments",
        Duration::from_secs(8),
    );
}

/// The text of the Conversation row carrying the `▌` selection bar.
fn selected_row(screen: &str) -> Option<String> {
    screen
        .lines()
        .find_map(|line| line.split_once('▌').map(|(_, row)| row.trim().to_owned()))
}

/// Select the row showing `text`. A click selects it directly; keys go to the top and then
/// move one row at a time, each Down waiting for its repaint so queued keys never overshoot.
fn select_row(session: &mut Session, mouse: bool, text: &str) {
    if mouse {
        select_comment(session, mouse, text);
        return;
    }
    session.wait_for_screen(text);
    session.send_key(&b"\x1b[A".repeat(30));
    session
        .try_wait_for_condition(
            |screen| selected_row(screen).is_some_and(|row| row.contains("comment #9")),
            "selection at the first comment, the first existing target",
            Duration::from_secs(8),
        )
        .unwrap();
    for _ in 0..16 {
        let current = selected_row(&session.screen()).unwrap_or_default();
        if current.contains(text) {
            return;
        }
        session.send_key(b"\x1b[B");
        session
            .try_wait_for_condition(
                |screen| selected_row(screen).is_some_and(|row| row != current),
                "one selection movement",
                Duration::from_secs(8),
            )
            .unwrap();
    }
    panic!(
        "{text} never gained the selection bar\n{}",
        session.screen()
    );
}

/// Select `body`, open its author's profile, and check every expected line is painted.
fn profile(session: &mut Session, mouse: bool, body: &str, expected: &[&str]) {
    select_row(session, mouse, body);
    choose(session, mouse, "[@ Actor profile]", b"@");
    session.wait_for_screen("Actor profile • read-only");
    for text in expected {
        session.wait_for_screen(text);
    }
}

fn close(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[Esc Close]", b"\x1b");
    closed_explanation(session, "Actor profile • read-only");
}

fn journey(mouse: bool) {
    let fixture = configured();
    let git = local_git_witness(&fixture);
    let mut steps = initial();
    steps.extend(details(vec![
        comment(9, "octocat", "User", "Existing starter comment"),
        comment(12, "dependabot[bot]", "Bot", "Automated bump note"),
        comment(13, "ghost", "User", "Removed account note"),
    ]));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_summary(&mut session, mouse);
    let reads = steps.len();
    assert_eq!(mutation_count(&records(&mut session, &fixture, reads)), 0);

    // Summary has no comment targets, so the explicit control names the PR author. A narrow
    // Summary bar may drop it, so the mouse path uses its Commands entry.
    if mouse {
        session.invoke_via_palette(true, "[@ Actor profile]");
    } else {
        session.send_key(b"@");
    }
    session.wait_for_screen("Actor profile • read-only");
    for text in [
        "Actor profile — PR author",
        "[OC] @octocat",
        "Username: @octocat",
        "Kind: User account",
        "Verified: github.invalid reported this account on example/repo #7",
        "Signed-in account: @octocat (same login)",
        "Avatar: none reported • initials shown",
    ] {
        session.wait_for_screen(text);
    }
    close(&mut session, mouse);

    open_conversation(&mut session, mouse);
    // The rows the profiles must match: initials chip, one gap, then the shared label.
    let screen = session.screen();
    for row in [
        "OC @octocat PR description",
        "DE @dependabot[bot] [bot] comment #12",
        "@ghost [deleted] comment #13",
    ] {
        assert!(screen.contains(row), "missing row {row}\n{screen}");
    }
    // The PR author heading stays a read-only row: it never carries the selection bar.
    assert!(!selected_row(&screen).is_some_and(|row| row.contains("PR description")));

    // A bot's comment body opens the bot, never the PR author or the signed-in account.
    profile(
        &mut session,
        mouse,
        "Automated bump note",
        &[
            "Actor profile — Comment author",
            "[DE] @dependabot[bot] [bot]",
            "Kind: Bot (automation account)",
            "Verified: github.invalid reported",
        ],
    );
    assert!(!session.screen().contains("Signed-in account"));
    close(&mut session, mouse);

    // A deleted account is honest: no verified account behind it.
    profile(
        &mut session,
        mouse,
        "Removed account note",
        &[
            "Actor profile — Comment author",
            "[×] @ghost [deleted]",
            "Kind: Deleted account",
            "Not verified: github.invalid reported no live account here",
        ],
    );
    // Resizing keeps the username and the state visible.
    session.resize(80, 24);
    session.wait_for_screen("Actor profile • read-only");
    session.wait_for_screen("[×] @ghost [deleted]");
    session.wait_for_screen("Kind: Deleted account");
    close(&mut session, mouse);
    // At 80 columns the control may leave the bar; `@` and its Commands entry still open the
    // selected comment's author, and the selection stays on that comment.
    if mouse {
        session.invoke_via_palette(true, "[@ Actor profile]");
    } else {
        session.send_key(b"@");
    }
    session.wait_for_screen("Actor profile — Comment author");
    session.wait_for_screen("[×] @ghost [deleted]");
    close(&mut session, mouse);
    assert!(selected_row(&session.screen()).is_some_and(|row| row.contains("Removed account note")));

    // Profiles are read-only: no gh call beyond the scripted reads, no mutation, no Git change.
    assert_eq!(mutation_count(&records(&mut session, &fixture, reads)), 0);
    assert_eq!(local_git_witness(&fixture), git);
    assert!(!fixture.root.join("config/ghtui/avatars").exists());
    session.quit_and_assert_restored_terminal(mouse);
}

/// One PR commit: GitHub linked its author to @alice; its committer has no GitHub account.
fn commit_value() -> Value {
    json!({
        "sha": "c".repeat(40),
        "commit": {
            "message": "Tighten profile wording\n\nBody",
            "author": {"name": "Alice Author", "email": "alice@example.invalid", "date": "2026-10-12T12:00:00Z"},
            "committer": {"name": "Local Committer", "email": "committer@example.invalid", "date": "2026-10-12T12:00:00Z"}
        },
        "author": {"id": 77, "login": "alice", "type": "User"},
        "committer": null,
        "parents": [{"sha": "b".repeat(40)}]
    })
}

/// TASK-34: the Commits tab's commit rows stay read-only, so `@ Actor profile` (key, or its
/// Commands entry by click) lists every commit's author and committer. The linked author
/// shows the shared GitHub profile; the committer without an account shows an honest "No
/// GitHub account" profile with the Git name and email, and no username. Reads nothing.
fn commits_journey(mouse: bool) {
    let fixture = configured();
    let git = local_git_witness(&fixture);
    let mut steps = initial();
    let mut detail = details(vec![comment(
        9,
        "octocat",
        "User",
        "Existing starter comment",
    )]);
    let commits = detail
        .iter_mut()
        .find(|step| {
            step.args
                .iter()
                .any(|arg| arg == "repos/example/repo/pulls/7/commits?per_page=100")
        })
        .unwrap();
    commits.stdout = serde_json::to_vec(&json!([[commit_value()]])).unwrap();
    steps.extend(detail);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Inbox • focused view");
    choose(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("Native collaboration");
    session.wait_for_screen("Fresh • fetched");
    choose(&mut session, mouse, "Native collaboration", b"\r");
    session.wait_for_screen("1 comments");
    let reads = steps.len();
    records(&mut session, &fixture, reads);

    choose(&mut session, mouse, "[h Commits]", b"h");
    session.wait_for_screen("Commit = a saved version");
    session.wait_for_screen("Tighten profile wording");
    session.wait_for_screen("committer Local Committer [local Git; no associated username]");
    let before = session.screen();
    if mouse {
        session.invoke_via_palette(true, "[@ Actor profile]");
    } else {
        session.send_key(b"@");
    }
    session.wait_for_screen("Actor profile • read-only");
    for text in [
        "Actor profiles — commit authors and committers",
        "1 commits on example/repo #7",
        "Commit 1 of 1: ccccccc Tighten profile wording",
        "Commit author (GitHub account)",
        "[AL] @alice",
        "Username: @alice",
        "Verified: github.invalid reported this account on example/repo #7",
    ] {
        session.wait_for_screen(text);
    }
    // The committer section sits below the first page of the notice.
    if mouse {
        session.click_visible_action("[PgDn More]");
    } else {
        session.send_key(b"\x1b[6~");
    }
    for text in [
        "Commit committer — no GitHub account",
        "[LC] committer Local Committer <committer@example.invalid>",
        "No GitHub account: the host linked none to this Git identity",
        "Git name: Local Committer",
        "Git email: committer@example.invalid",
        "Never guessed from a Git name or email",
    ] {
        session.wait_for_screen(text);
    }
    let screen = session.screen();
    assert!(!screen.contains("@Local"), "no guessed username\n{screen}");
    // Resizing keeps the profile readable.
    session.resize(80, 24);
    session.wait_for_screen("Actor profile • read-only");
    close(&mut session, mouse);
    // The commit rows were never selection targets, before or after the profile.
    let after = session.screen();
    for screen in [&before, &after] {
        assert!(
            !screen
                .lines()
                .any(|line| line.contains('▌') && line.contains("Tighten")),
            "{screen}"
        );
    }
    assert!(after.contains("Commit = a saved"), "{after}");

    assert_eq!(mutation_count(&records(&mut session, &fixture, reads)), 0);
    assert_eq!(local_git_witness(&fixture), git);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actor_identity_keyboard_pr_commit_author_and_no_account_committer_profiles() {
    commits_journey(false);
}

#[test]
fn actor_identity_mouse_pr_commit_author_and_no_account_committer_profiles() {
    commits_journey(true);
}

#[test]
fn actor_identity_keyboard_pr_author_bot_and_deleted_commenter_profiles() {
    journey(false);
}

#[test]
fn actor_identity_mouse_pr_author_bot_and_deleted_commenter_profiles() {
    journey(true);
}
