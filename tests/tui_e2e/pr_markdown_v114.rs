//! TASK-114/115 on the real binary: a worklappy-style PR body (Devin badge in HTML comments,
//! a linked `<picture>`, `---` rules and `##` headings) reads as GitHub renders it in the Inbox
//! preview, on Summary and in Conversation, and the Conversation shows separated comment cards
//! (author chip, role badge, time, reactions, buttons) between its PR action bar and the
//! persistent Add a comment box. Keyboard and mouse, 140x40 then 80x24. Read only: every
//! scripted read runs, nothing is written and the checkout is untouched.
use super::{configured, identity, local_git_witness, mutation_count, records, step};
use crate::support::{GhStep, Session};
use serde_json::{json, Value};
use std::time::Duration;

const TITLE: &str = "Badge body cards";
const BODY: &str = "<!-- devin-review-badge-begin -->\n<a href=\"https://app.devin.ai/review/acme/infra/pull/7\" target=\"_blank\">\n  <picture>\n    <source media=\"(prefers-color-scheme: dark)\" srcset=\"https://static.devin.ai/assets/gh-open-in-devin-review-dark.svg?v=1\">\n    <img src=\"https://static.devin.ai/assets/gh-open-in-devin-review-light.svg?v=1\" alt=\"Open with Devin\">\n  </picture>\n</a>\n<!-- devin-review-badge-end -->\n\n---\n\n## Summary\n\nMoves the **ingest** workers to `europe-west2`.\n\n## Test plan\n\n- [x] terraform plan is clean\n- [ ] Apply in staging\n\n---\n";
const COMMENT: &str =
    "Thanks! **LGTM** with one nit:\n\n```rust\nlet region = 2;\n```\n\n- [ ] follow-up ticket";
/// Raw markup that must never reach the screen.
const RAW: [&str; 9] = [
    "<!--",
    "devin-review-badge",
    "<a ",
    "href=",
    "<picture",
    "<source",
    "<img",
    "**",
    "```",
];

fn pr() -> Value {
    json!({"node_id":"PR_example/repo_7","number":7,"title":TITLE,"body":BODY,"repository_url":"https://api.github.invalid/repos/example/repo","html_url":"https://github.invalid/example/repo/pull/7","user":{"id":42,"login":"octocat","type":"User"},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","head":{"sha":"a".repeat(40),"ref":"feature"},"base":{"sha":"b".repeat(40),"ref":"main"},"changed_files":0,"additions":0,"deletions":0,"mergeable":true,"mergeable_state":"clean"})
}

fn comment() -> Value {
    json!({"id":9,"node_id":"IC_9","body":COMMENT,"user":{"id":42,"login":"octocat","type":"User"},"author_association":"OWNER","created_at":"2026-10-13T00:00:00Z","updated_at":"2026-10-14T00:00:00Z","issue_url":"https://api.github.invalid/repos/example/repo/issues/7","html_url":"https://github.invalid/example/repo/pull/7#issuecomment-9","reactions":{"url":"https://api.github.invalid/repos/example/repo/issues/comments/9/reactions","total_count":3,"+1":2,"-1":0,"laugh":0,"hooray":1,"confused":0,"heart":0,"rocket":0,"eyes":0}})
}

/// The Inbox load (identity, three queue searches, the rollup read) with this PR.
fn initial() -> Vec<GhStep> {
    let mut steps = identity();
    steps.extend(identity());
    for queue in ["author", "assignee", "review-requested"] {
        let items = if queue == "review-requested" {
            vec![]
        } else {
            vec![pr()]
        };
        let mut read = step(
            "search/issues",
            "GET",
            json!({"total_count":items.len(),"incomplete_results":false,"items":items}),
            false,
        );
        read.args.extend([
            "-f".into(),
            format!("q=is:pr is:open {queue}:octocat"),
            "-f".into(),
            "per_page=100".into(),
            "-f".into(),
            "page=1".into(),
        ]);
        steps.push(read);
    }
    steps.extend(crate::support::inbox_rollup_unknown(
        "github.invalid",
        &[("example/repo", 7)],
    ));
    steps
}

fn details() -> Vec<GhStep> {
    let mut steps = vec![
        step("repos/example/repo/pulls/7", "GET", pr(), false),
        step(
            "repos/example/repo/issues/7/comments?per_page=100",
            "GET",
            json!([[comment()]]),
            true,
        ),
    ];
    for endpoint in [
        "repos/example/repo/pulls/7/commits?per_page=100",
        "repos/example/repo/pulls/7/files?per_page=100",
        "repos/example/repo/pulls/7/reviews?per_page=100",
        "repos/example/repo/issues/7/timeline?per_page=100",
    ] {
        steps.push(step(endpoint, "GET", json!([[]]), true));
    }
    steps.push(step("repos/example/repo/pulls/7", "GET", pr(), false));
    steps
}

fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}

fn no_raw(text: &str) -> bool {
    RAW.iter().all(|raw| !text.contains(raw))
}

fn wait(session: &mut Session, what: &str, condition: impl Fn(&str) -> bool) {
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            crate::support::main_footer_visible(frame) && no_raw(&text) && condition(&text)
        },
        what,
        Duration::from_secs(10),
    );
}

fn journey(mouse: bool) {
    let fixture = configured();
    let git = local_git_witness(&fixture);
    let mut steps = initial();
    steps.extend(details());
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 140, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Inbox • focused view");
    choose(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen(TITLE);
    session.wait_for_screen("Fresh • fetched");
    // Inbox preview: the badge is a readable image link, headings and task lists render.
    wait(&mut session, "rendered Inbox preview description", |text| {
        text.contains("[image: Open with Devin][1]")
            && text.contains("Test plan")
            && text.contains("☑ terraform plan is clean")
            && text.contains("☐ Apply in staging")
            && text.contains("Moves the ingest workers to europe-west2.")
            && text.contains("[1] https://app.devin.ai/review/acme/infra/pull/7")
            && !text.contains("## Summary")
    });
    // Summary: the same rendered description.
    choose(&mut session, mouse, TITLE, b"\r");
    wait(&mut session, "rendered Summary description", |text| {
        text.contains("1 comments")
            && text.contains("[image: Open with Devin][1]")
            && text.contains("Test plan")
            && text.contains("☑ terraform plan is clean")
    });
    // Conversation: action bar, description card, Add a comment box.
    choose(&mut session, mouse, "[c Conversation]", b"c");
    wait(&mut session, "Conversation action bar and box", |text| {
        text.contains("@octocat PR description")
            && text.contains("[n Comment]")
            && text.contains("[A Approve]")
            && text.contains("[M Merge]")
            && text.contains("[Z Close]")
            && text.contains("✎ Add a comment…")
            && text.contains("[image: Open with Devin][1]")
    });
    // The comment card is below the description: End (keys) or the wheel (mouse) reaches it.
    for _ in 0..12 {
        if session.screen().contains("[Q Quote reply]") {
            break;
        }
        if mouse {
            session.wheel(30, 20, true);
        } else {
            session.send_key(b"\x1b[F");
        }
        session.poll_output(Duration::from_millis(50));
    }
    wait(&mut session, "rendered comment card", |text| {
        text.contains("OC @octocat comment #9 • 2026-10-13T00:00:00Z")
            && text.contains("Author • Owner • edited")
            && text.contains("Thanks! LGTM with one nit:")
            && text.contains("┃ let region = 2;")
            && text.contains("☐ follow-up ticket")
            && text.contains("👍 2  🎉 1")
            && text.contains("[Q Quote reply]")
            && text.contains("[+ React]")
            && text.contains("[E Edit]")
            && text.contains("[D Delete]")
            && text.contains("✎ Add a comment…")
    });
    // The bar and the box stay while the cards scroll; both survive 80 columns.
    session.resize(80, 24);
    wait(&mut session, "80x24 Conversation", |text| {
        text.contains("[n Comment]")
            && text.contains("[Y Ready]")
            && text.contains("✎ Add a comment…")
    });
    // Read only: every scripted read matched, nothing was written, the checkout is untouched.
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, steps.len())),
        0
    );
    assert_eq!(local_git_witness(&fixture), git);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn markdown_v114_keyboard_badge_body_renders_in_preview_summary_and_conversation_cards() {
    journey(false);
}

#[test]
fn markdown_v114_mouse_badge_body_renders_in_preview_summary_and_conversation_cards() {
    journey(true);
}
