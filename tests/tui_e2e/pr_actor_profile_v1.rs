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

/// The profile lines every surface must paint for one actor, in order. The verification
/// line names its context (`#7` or `run 123`) after this shared prefix, so only the prefix is
/// compared; every other line is identical text on every surface.
const OCTOCAT: &[&str] = &[
    "[OC] @octocat",
    "Username: @octocat",
    "Kind: User account",
    "Verified: github.invalid reported this account on example/repo",
    "Signed-in account: @octocat (same login)",
    "Identity comes from GitHub, never from a name or email match",
    "Avatar: none reported • initials shown",
];
/// A bot: verified by the host, never the signed-in account (the block has no such line).
const BOT: &[&str] = &[
    "[DE] @dependabot[bot] [bot]",
    "Username: @dependabot[bot]",
    "Kind: Bot (automation account)",
    "Verified: github.invalid reported this account on example/repo",
    "Identity comes from GitHub, never from a name or email match",
    "Avatar: none reported • initials shown",
];
/// A deleted account: honest, with no verified live account and no identity claim.
const GHOST: &[&str] = &[
    "[×] @ghost [deleted]",
    "Username: @ghost",
    "Kind: Deleted account (GitHub shows it as ghost)",
    "Not verified: github.invalid reported no live account here",
    "Avatar: none reported • initials shown",
];
const NOTICE_TITLE: &str = "Actor profile • read-only";
const NOTICE_END: &str = "Read-only • reads nothing from GitHub or Git";

/// Whether `lines` are painted in order on consecutive screen rows. One extra row is allowed
/// after each line for a wrapped tail (a long verification context at 68 inner columns).
fn block_at(screen: &str, lines: &[&str]) -> bool {
    let rows: Vec<&str> = screen.lines().collect();
    (0..rows.len()).any(|start| {
        let mut row = start;
        for (index, line) in lines.iter().enumerate() {
            let window = if index == 0 { 1 } else { 2 };
            match (row..(row + window).min(rows.len())).find(|r| rows[*r].contains(line)) {
                Some(found) => row = found + 1,
                None => return false,
            }
        }
        true
    })
}

/// Scroll the open profile notice (Down by key, the wheel over the notice by mouse) until one
/// actor's whole block is painted, then return the completed frame's text. Panics if the
/// notice ends first, so a missing or reordered line can never pass.
fn show_block(session: &mut Session, mouse: bool, lines: &[&str]) -> String {
    session.wait_for_cursorless_condition(
        |frame| frame.contents().contains(NOTICE_TITLE),
        "completed actor profile notice",
        Duration::from_secs(8),
    );
    for _ in 0..80 {
        let text = session.completed_cursorless_screen().unwrap().contents();
        if block_at(&text, lines) {
            return text;
        }
        assert!(
            !text.contains(NOTICE_END),
            "profile ended without the block {lines:?}\n{text}"
        );
        let end = session.completed_cursorless_end();
        if mouse {
            let (column, row) = session.locate(NOTICE_TITLE).unwrap();
            session.wheel(column + 2, row + 4, true);
        } else {
            session.send_key(b"\x1b[B");
        }
        session.wait_for_new_cursorless_condition(
            end,
            |frame| frame.contents().contains(NOTICE_TITLE),
            "one notice scroll step",
            Duration::from_secs(8),
        );
    }
    panic!("block {lines:?} never painted\n{}", session.screen());
}

fn open_pr_profile(session: &mut Session, mouse: bool, palette: bool) {
    if !mouse {
        session.send_key(b"@");
    } else if palette {
        session.invoke_via_palette(true, "[@ Actor profile]");
    } else {
        session.click_visible_action("[@ Actor profile]");
    }
    session.wait_for_screen(NOTICE_TITLE);
}

/// A painted row holding every part (a chip, its `@login` label and the row's own label).
fn row_with(screen: &str, parts: &[&str]) -> bool {
    screen
        .lines()
        .any(|line| parts.iter().all(|part| line.contains(part)))
}

fn cross_surface_commits() -> Value {
    json!([[
        {
            "sha": "c".repeat(40),
            "commit": {
                "message": "Bump the lockfile",
                "author": {"name": "Octo Cat", "email": "octocat@example.invalid", "date": "2026-10-12T12:00:00Z"},
                "committer": {"name": "dependabot[bot]", "email": "bot@example.invalid", "date": "2026-10-12T12:00:00Z"}
            },
            "author": {"id": 42, "login": "octocat", "type": "User"},
            "committer": {"id": 49699333, "login": "dependabot[bot]", "type": "Bot"},
            "parents": [{"sha": "b".repeat(40)}]
        },
        {
            "sha": "d".repeat(40),
            "commit": {
                "message": "Remove the stale note",
                "author": {"name": "Former Contributor", "email": "former@example.invalid", "date": "2026-10-12T13:00:00Z"},
                "committer": {"name": "Local Committer", "email": "committer@example.invalid", "date": "2026-10-12T13:00:00Z"}
            },
            "author": {"id": 10137, "login": "ghost", "type": "User"},
            "committer": null,
            "parents": [{"sha": "c".repeat(40)}]
        }
    ]])
}

/// Run 123: dependabot started the first attempt; octocat (the signed-in account) reran it.
fn cross_surface_run() -> Value {
    json!({"id":123,"workflow_id":7,"name":"CI matrix","display_title":"Native collaboration","head_sha":"a".repeat(40),"head_branch":"feature","event":"pull_request","status":"completed","conclusion":"failure","run_attempt":2,"created_at":"2026-10-13T12:00:00Z","updated_at":"2026-10-13T12:10:00Z","actor":{"id":49699333,"login":"dependabot[bot]","type":"Bot"},"triggering_actor":{"id":42,"login":"octocat","type":"User"}})
}

/// The generic Actions view after the Inbox: its scope is the Inbox's repository, so it
/// verifies the account, lists that repository's runs, then opens run 123 attempt 2.
fn cross_surface_actions_steps() -> Vec<crate::support::GhStep> {
    let mut steps = super::identity();
    steps.push(super::step(
        "repos/example/repo/actions/runs?per_page=100",
        "GET",
        json!({"total_count":1,"workflow_runs":[cross_surface_run()]}),
        false,
    ));
    steps.push(super::step(
        "repos/example/repo/actions/runs/123/attempts/2",
        "GET",
        cross_surface_run(),
        false,
    ));
    steps.push(super::step(
        "repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100",
        "GET",
        json!([{"jobs":[{"id":501,"run_id":123,"run_attempt":2,"head_sha":"a".repeat(40),"name":"matrix test linux","status":"completed","conclusion":"failure","runner_name":"hosted","labels":["ubuntu-latest"],"steps":[{"number":3,"name":"Run tests","status":"completed","conclusion":"failure"}]}]}]),
        true,
    ));
    steps
}

/// TASK-34 testing step 2: ONE session shows the same account (@octocat, the signed-in user)
/// as PR author, comment author, linked commit author and Actions triggering actor, plus a
/// bot (comment author, commit committer, run actor) and a deleted account (comment author,
/// commit author). Every surface's profile paints the identical chip, login, kind,
/// verification and avatar lines for that actor, and each list row paints the same chip and
/// label. Unlinked Git identities stay honest. Profiles read nothing from gh, write no cache
/// or avatar file and leave the checkout untouched, at 120x40 and again at 80x24.
fn cross_surface_journey(mouse: bool) {
    let fixture = configured();
    let git = local_git_witness(&fixture);
    let mut steps = initial();
    // The comment authors carry the same numeric account ids as the other surfaces.
    let mut detail = details(
        [
            (9, 42, "octocat", "User", "Existing starter comment"),
            (
                12,
                49699333,
                "dependabot[bot]",
                "Bot",
                "Automated bump note",
            ),
            (13, 10137, "ghost", "User", "Removed account note"),
        ]
        .map(|(id, account, login, kind, body)| {
            let mut value = comment(id, login, kind, body);
            value["user"]["id"] = json!(account);
            value
        })
        .to_vec(),
    );
    let commits = detail
        .iter_mut()
        .find(|step| {
            step.args
                .iter()
                .any(|arg| arg == "repos/example/repo/pulls/7/commits?per_page=100")
        })
        .unwrap();
    commits.stdout = serde_json::to_vec(&cross_surface_commits()).unwrap();
    steps.extend(detail);
    let pr_reads = steps.len();
    steps.extend(cross_surface_actions_steps());
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_summary(&mut session, mouse);
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, pr_reads)),
        0
    );

    // Surface 1: the PR author, from Summary (no comment target selected).
    open_pr_profile(&mut session, mouse, true);
    session.wait_for_screen("Actor profile — PR author");
    show_block(&mut session, mouse, OCTOCAT);
    close(&mut session, mouse);

    // Surface 2: comment authors. The rows paint the same chip and label as the profiles.
    open_conversation(&mut session, mouse);
    let screen = session.screen();
    for parts in [
        &["OC @octocat", "PR description"][..],
        &["OC @octocat", "comment #9"],
        &["DE @dependabot[bot] [bot]", "comment #12"],
        &["@ghost [deleted]", "comment #13"],
    ] {
        assert!(row_with(&screen, parts), "missing row {parts:?}\n{screen}");
    }
    for (body, block) in [
        ("Existing starter comment", OCTOCAT),
        ("Automated bump note", BOT),
        ("Removed account note", GHOST),
    ] {
        select_row(&mut session, mouse, body);
        open_pr_profile(&mut session, mouse, false);
        session.wait_for_screen("Actor profile — Comment author");
        let text = show_block(&mut session, mouse, block);
        if block != OCTOCAT {
            assert!(!text.contains("Signed-in account"), "{text}");
        }
        close(&mut session, mouse);
    }

    // Surface 3: commit authors and committers. Linked roles reuse the GitHub profile; the
    // unlinked committer keeps only its Git name and email, never a guessed username.
    choose(&mut session, mouse, "[h Commits]", b"h");
    session.wait_for_screen("Commit = a saved version");
    session.wait_for_screen("Remove the stale note");
    let screen = session.screen();
    for parts in [
        &["OC @octocat", "author Octo Cat"][..],
        &["DE @dependabot[bot] [bot]", "committer dependabot[bot]"],
        &["@ghost [deleted]", "author Former Contributor"],
        &["committer Local Committer [local Git; no associated username]"],
    ] {
        assert!(row_with(&screen, parts), "missing row {parts:?}\n{screen}");
    }
    open_pr_profile(&mut session, mouse, true);
    session.wait_for_screen("Actor profiles — commit authors and committers");
    for block in [
        &["Commit 1 of 2: ccccccc Bump the lockfile"][..],
        &["Commit author (GitHub account)"],
        OCTOCAT,
        &["Git records: Octo Cat <octocat@example.invalid>"],
        &["Commit committer (GitHub account)"],
        BOT,
        &["Commit 2 of 2: ddddddd Remove the stale note"],
        &["Commit author (GitHub account)"],
        GHOST,
        &[
            "Commit committer — no GitHub account",
            "[LC] committer Local Committer <committer@example.invalid>",
            "No GitHub account: the host linked none to this Git identity",
            "Git name: Local Committer",
            "Git email: committer@example.invalid",
            "Never guessed from a Git name or email",
        ],
    ] {
        show_block(&mut session, mouse, block);
    }
    assert!(!session.screen().contains("@Local"), "{}", session.screen());
    close(&mut session, mouse);
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, pr_reads)),
        0
    );

    // Surface 4: the Actions run. The generic Actions scope is the Inbox's repository.
    choose(&mut session, mouse, "4 Actions", b"4");
    session.wait_for_screen("123 #2");
    if mouse {
        session.click_actions_control("123 #2");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("matrix test linux");
    session.wait_for_screen("triggering actor");
    // Actions draws these rows with the shared table actor cell (`ui_table::actor_text`):
    // the same chip, login and kind marker, without the `@` every table cell drops.
    let screen = session.screen();
    for parts in [
        &["OC octocat", "triggering actor"][..],
        &["DE dependabot[bot] [bot]", "run actor"],
    ] {
        assert!(row_with(&screen, parts), "missing row {parts:?}\n{screen}");
    }
    let all = steps.len();
    assert_eq!(mutation_count(&records(&mut session, &fixture, all)), 0);
    for (cols, rows) in [(120, 40), (80, 24)] {
        if cols == 80 {
            session.resize(cols, rows);
            session.wait_for_screen("triggering actor");
        }
        // At 80 columns the control may leave the bar; its key and Commands entry still work.
        if mouse {
            session.click_actions_control("[@ Actor profile]");
        } else {
            session.send_key(b"@");
        }
        session.wait_for_screen("Actor profile — Actions run actors");
        session.wait_for_screen("Triggering actor (started attempt 2)");
        show_block(&mut session, mouse, OCTOCAT);
        show_block(
            &mut session,
            mouse,
            &["Run actor (started the first attempt)"],
        );
        show_block(&mut session, mouse, BOT);
        if mouse {
            session.click_visible_action("[Esc Close]");
        } else {
            session.send_key(b"\x1b");
        }
        session.wait_for_overlay_close(NOTICE_TITLE);
    }

    // Read-only everywhere: no gh call beyond the scripted reads, no mutation, no avatar
    // file, and the checkout's HEAD, index and status are unchanged.
    assert_eq!(mutation_count(&records(&mut session, &fixture, all)), 0);
    assert_eq!(local_git_witness(&fixture), git);
    assert!(!fixture.root.join("config/ghtui/avatars").exists());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actor_identity_keyboard_same_actor_across_pr_comment_commit_and_run_profiles() {
    cross_surface_journey(false);
}

#[test]
fn actor_identity_mouse_same_actor_across_pr_comment_commit_and_run_profiles() {
    cross_surface_journey(true);
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
