use super::*;
use crate::app::View;
use crate::github_workspace::{GithubWorkspace, PrPage};
use crate::providers::gh::{
    Actor, Coverage, HostIdentity, PrDetails, PrSummary, PullRequest, Queue,
};
use ratatui::{backend::TestBackend, Terminal};
use std::collections::BTreeSet;

fn actor(login: &str) -> Actor {
    Actor {
        login: login.into(),
        display_name: None,
        avatar_url: None,
        actor_type: "User".into(),
    }
}

fn comment(id: u64, login: &str, body: &str) -> ConversationComment {
    ConversationComment {
        id,
        node_id: format!("IC_{id}"),
        body: body.into(),
        author: actor(login),
        created_at: "2026-10-13T00:00:00Z".into(),
        updated_at: "2026-10-13T00:00:00Z".into(),
        url: format!("https://github.invalid/example/repo/pull/7#issuecomment-{id}"),
        card_v115: None,
    }
}

/// An open PR by `octocat` viewed by `current-user` on Conversation.
fn conversation(comments: Vec<ConversationComment>, draft: bool) -> App {
    let mut github = GithubWorkspace::empty(
        HostIdentity {
            host: "github.invalid".into(),
            login: "current-user".into(),
            account_id: Some(42),
            gh_version: "2.99.0".into(),
        },
        vec![],
        BTreeSet::new(),
    );
    let pr = PullRequest {
        host: "github.invalid".into(),
        id: "PR_7".into(),
        repository: "example/repo".into(),
        number: 7,
        title: "Cards".into(),
        body: "## Plan\n\n- [x] **done**".into(),
        url: String::new(),
        author: actor("octocat"),
        state: "open".into(),
        draft,
        created_at: "2026-10-01T00:00:00Z".into(),
        updated_at: String::new(),
        queues: BTreeSet::from([Queue::Authored]),
    };
    github.inbox.pull_requests = vec![pr.clone()];
    github.selected = Some((&pr).into());
    let mut page = PrPage {
        details: PrDetails {
            summary: PrSummary {
                pr,
                head_sha: "a".repeat(40),
                base_sha: "b".repeat(40),
                head_ref: "feature".into(),
                base_ref: "main".into(),
                merged: false,
                auto_merge: false,
                merge_commit_sha: None,
                mergeable: Some(true),
                mergeable_state: "clean".into(),
                additions: 0,
                deletions: 0,
                changed_files: 0,
            },
            comments,
            commits: vec![],
            files: vec![],
            reviews: vec![serde_json::json!({
                "id": 810, "state": "APPROVED", "body": "Looks **good**",
                "user": {"login": "reviewer", "type": "User"},
                "author_association": "MEMBER",
                "submitted_at": "2026-10-14T00:00:00Z"
            })],
            timeline: vec![],
            coverage: Coverage {
                complete: true,
                ..Default::default()
            },
        },
        tab: PrTab::Conversation,
        diff: None,
        checks: None,
        required_checks: None,
        rows: vec![],
        context: None,
        metadata_display_v51: None,
        review_progress: None,
    };
    page.prepare_rows().unwrap();
    github.pr = Some(page);
    App {
        view: View::Inbox,
        github: Some(github),
        github_content_focused: true,
        ..Default::default()
    }
}

fn paint(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    app.hits.clear();
    terminal
        .draw(|frame| {
            crate::github_ui::draw(frame, app, Rect::new(0, 0, width, height));
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

fn hit_at(app: &App, rows: &[String], text: &str) -> Action {
    let (y, row) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains(text))
        .unwrap_or_else(|| panic!("{text} not painted:\n{}", rows.join("\n")));
    let column = row[..row.find(text).unwrap()].chars().count() as u16;
    app.hits
        .iter()
        .rev()
        .find(|hit| hit.rect.contains((column + 1, y as u16).into()))
        .map(|hit| hit.action.clone())
        .unwrap_or_else(|| panic!("no hit on {text}"))
}

#[test]
fn cards_v115_headers_keep_id_and_utc_and_add_age_badges_and_edited() {
    let mut owned = comment(9, "octocat", "x");
    owned.card_v115 = Some(Box::new(crate::providers::gh::CommentCardFactsV115 {
        author_association: "OWNER".into(),
        reactions: vec![],
    }));
    owned.updated_at = "2026-10-14T00:00:00Z".into();
    let header = comment_header(&owned, "octocat");
    assert!(
        header.starts_with("comment #9 • 2026-10-13T00:00:00Z • "),
        "{header}"
    );
    assert!(
        header.ends_with(" ago • Author • Owner • edited"),
        "{header}"
    );
    let plain = comment_header(&comment(10, "someone", "x"), "octocat");
    assert!(
        !plain.contains("Author") && !plain.contains("edited"),
        "{plain}"
    );
    let review = serde_json::json!({"id": 810, "state": "CHANGES_REQUESTED", "user": {"login": "octocat"}, "author_association": "COLLABORATOR"});
    assert_eq!(
        review_header(&review, "octocat"),
        "review 810 • requested changes • Author • Collaborator"
    );
    assert_eq!(
        reaction_summary(&[("+1".into(), 2), ("heart".into(), 1), ("eyes".into(), 0)]),
        Some("👍 2  ❤ 1".into())
    );
    assert_eq!(reaction_summary(&[]), None);
    assert_eq!(
        quote("octocat", "first\n\nsecond"),
        "> @octocat wrote:\n> first\n>\n> second\n\n"
    );
}

#[test]
fn cards_v115_conversation_rows_are_separated_cards_with_chrome_outside_the_list_stops() {
    let mut reacted = comment(9, "octocat", "Existing **starter** comment");
    reacted.card_v115 = Some(Box::new(crate::providers::gh::CommentCardFactsV115 {
        author_association: String::new(),
        reactions: vec![("hooray".into(), 3)],
    }));
    let app = conversation(vec![reacted], false);
    let github = app.github.as_ref().unwrap();
    let page = github.pr.as_ref().unwrap();
    let labels = page
        .rows
        .iter()
        .map(|row| row.label.as_str())
        .collect::<Vec<_>>();
    let header = labels
        .iter()
        .position(|label| label.starts_with("comment #9 • "))
        .unwrap();
    assert!(matches!(page.rows[header - 1].card, Some(PrCardV115::Top)));
    assert_eq!(labels[header + 1], "Existing starter comment");
    assert_eq!(labels[header + 2], "🎉 3");
    assert!(matches!(
        page.rows[header + 3].card,
        Some(PrCardV115::Buttons)
    ));
    assert!(labels.contains(&"Plan"), "description rendered: {labels:?}");
    assert!(labels.contains(&"• ☑ done"));
    assert!(labels.contains(&"Looks good"));
    assert!(labels
        .iter()
        .any(|label| label.starts_with("review 810 • approved • 2026-10-14T00:00:00Z")));
    // Header and body are list stops; the rule, reactions and buttons are not.
    assert!(github.pr_row_is_interactive(header));
    assert!(github.pr_row_is_interactive(header + 1));
    for chrome in [header - 1, header + 2, header + 3] {
        assert!(!github.pr_row_is_interactive(chrome), "{}", labels[chrome]);
    }
    // Down from the comment's last body line stays there: chrome is never a stop.
    assert_eq!(
        github.pr_row_navigation_target(header + 1, true),
        Some(header + 1)
    );
    assert!(page.rows[header + 2].comment.is_none() && page.rows[header + 3].comment.is_none());
}

#[test]
fn cards_v115_buttons_and_bar_hold_the_menu_actions_by_key_and_click() {
    let mut app = conversation(
        vec![
            comment(9, "octocat", "Existing starter comment"),
            comment(10, "current-user", "Mine"),
        ],
        true,
    );
    for width in [80u16, 140] {
        let rows = paint(&mut app, width, 40);
        let all = rows.join("\n");
        for text in [
            "[n Comment]",
            "[A Approve]",
            "[X Request changes]",
            "[M Merge]",
            "[Z Close]",
            "[Y Ready]",
            "[Q Quote reply]",
            "[+ React]",
            "[E Edit]",
            "[D Delete]",
            "✎ Add a comment…",
        ] {
            assert!(all.contains(text), "{width}: {text} missing\n{all}");
        }
        assert!(!all.contains("**"), "Markdown syntax leaked:\n{all}");
        // The Add a comment box is the last painted row; the bar sits under the heading.
        assert!(rows.last().unwrap().contains("✎ Add a comment…"));
        assert!(rows[2].contains("[n Comment]"), "{}", rows[2]);
    }
    let rows = paint(&mut app, 140, 40);
    // The bar's enabled buttons carry the bar action; the menu action matches.
    assert_eq!(
        hit_at(&app, &rows, "[n Comment]"),
        Action::PrCardsV115(CardsActionV115::Bar(BarControlV115::Comment))
    );
    assert!(matches!(
        bar_action(&app, BarControlV115::Comment),
        Action::Pr(crate::app::PrUiAction::Command(
            crate::pr_actions::PrCommand::NewComment
        ))
    ));
    // A draft cannot merge: the button is dim and its click says why; Ready is offered.
    assert_eq!(
        hit_at(&app, &rows, "[M Merge]"),
        Action::Explain("Mark this draft PR ready for review before merging.".into())
    );
    assert!(matches!(
        bar_action(&app, BarControlV115::Ready),
        Action::Pr(crate::app::PrUiAction::Command(
            crate::pr_actions::PrCommand::Ready
        ))
    ));
    // Each card's buttons act on their own comment.
    let quote_hits = app
        .hits
        .iter()
        .filter_map(|hit| match &hit.action {
            Action::PrCardsV115(CardsActionV115::Card {
                row,
                control: CardControlV115::QuoteReply,
            }) => Some(*row),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(quote_hits.len(), 2, "one Quote reply per comment card");
    let page = app.github.as_ref().unwrap().pr.as_ref().unwrap();
    let ids = quote_hits
        .iter()
        .map(|row| page.rows[*row].comment.as_ref().unwrap().observed.id)
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![9, 10]);
    // Keys resolve to the same controls; per-comment keys explain until a comment is selected.
    let keys = crate::github_ui::footer_keys(&app);
    let key = |text: &str| keys.iter().find(|key| key.key == text).unwrap();
    assert_eq!(
        key("n").action,
        Action::PrCardsV115(CardsActionV115::Bar(BarControlV115::Comment))
    );
    assert!(key("M").disabled.is_some());
    app.inbox_list.selected = 0;
    let keys = crate::github_ui::footer_keys(&app);
    assert!(keys
        .iter()
        .find(|key| key.key == "E")
        .unwrap()
        .disabled
        .is_some());
    // Clicking a card button selects that comment first, as the menu requires.
    let edit_row = quote_hits[1];
    dispatch(
        &mut app,
        CardsActionV115::Card {
            row: edit_row,
            control: CardControlV115::Edit,
        },
    );
    let page = app.github.as_ref().unwrap().pr.as_ref().unwrap();
    let selected = &page.rows[app.inbox_list.selected];
    assert!(selected.actor.is_some());
    assert_eq!(selected.comment.as_ref().unwrap().observed.id, 10);
    // Every card button is the menu's own entry for that comment, disabled reason included.
    let header = app.inbox_list.selected;
    let menu = app.pr_controls_at(header);
    for (control, label) in [
        (CardControlV115::React, "React to selected comment"),
        (CardControlV115::Edit, "Edit selected comment"),
        (CardControlV115::Delete, "Delete selected comment"),
        (
            CardControlV115::Reactions,
            "Browse selected comment reactions",
        ),
    ] {
        let expected = menu
            .iter()
            .find(|(text, _)| text == label)
            .unwrap()
            .1
            .clone();
        assert_eq!(card_action_at(&app, header, control), expected, "{label}");
    }
    assert_eq!(
        app.pr_action,
        Some(crate::app::PrUiAction::Command(
            crate::pr_actions::PrCommand::EditComment
        ))
    );
}

#[test]
fn cards_v115_quote_reply_pastes_only_into_an_empty_new_comment_body() {
    let mut app = conversation(vec![comment(9, "octocat", "Line one\nLine two")], false);
    let header = app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|row| row.label.starts_with("comment #9"))
        .unwrap();
    dispatch(
        &mut app,
        CardsActionV115::Card {
            row: header,
            control: CardControlV115::QuoteReply,
        },
    );
    assert_eq!(
        app.quote_reply_v115.as_deref(),
        Some("> @octocat wrote:\n> Line one\n> Line two\n\n")
    );
    assert_eq!(
        app.pr_action,
        Some(crate::app::PrUiAction::Command(
            crate::pr_actions::PrCommand::NewComment
        ))
    );
    let github = app.github.as_ref().unwrap();
    let form = crate::forms::Form::pr(crate::forms::PrIntent {
        kind: crate::forms::PrFormKind::Comment,
        target: github.pr.as_ref().unwrap().details.summary.clone(),
        identity: github.identity.clone(),
    })
    .unwrap();
    assert_eq!(form.fields[0].key, "body");
    app.form = Some(form);
    apply_quote(&mut app);
    assert_eq!(
        app.form.as_ref().unwrap().fields[0].editor.text,
        "> @octocat wrote:\n> Line one\n> Line two\n\n"
    );
    assert!(app.quote_reply_v115.is_none());
    // A body that already holds text (a restored draft) is never overwritten.
    app.quote_reply_v115 = Some("> other\n\n".into());
    apply_quote(&mut app);
    assert_eq!(
        app.form.as_ref().unwrap().fields[0].editor.text,
        "> @octocat wrote:\n> Line one\n> Line two\n\n"
    );
    // Any other card or bar action drops a pending quote.
    app.quote_reply_v115 = Some("> stale".into());
    dispatch(&mut app, CardsActionV115::Bar(BarControlV115::Comment));
    assert!(app.quote_reply_v115.is_none());
}
