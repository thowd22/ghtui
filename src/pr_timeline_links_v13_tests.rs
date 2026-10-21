//! Timeline items open their own comment, review or commit, and return (TASK-13).
use super::*;
use crate::github_workspace::{GithubWorkspace, PrPage};
use crate::providers::gh::{
    Actor, Check, CommitSummary, ConversationComment, Coverage, HostIdentity, PrDetails, PrSummary,
    PullRequest,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use serde_json::json;
use std::collections::BTreeSet;

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OLDER: &str = "cccccccccccccccccccccccccccccccccccccccc";

fn commit(sha: &str, message: &str) -> CommitSummary {
    CommitSummary {
        sha: sha.into(),
        message: message.into(),
        author: Actor {
            login: "alice".into(),
            ..Default::default()
        },
        committer: Actor::default(),
        author_name: "Alice".into(),
        committer_name: "Alice".into(),
        author_email: String::new(),
        committer_email: String::new(),
        authored_at: "2026-10-12T12:00:00Z".into(),
        parents: vec![],
    }
}

fn page() -> PrPage {
    let summary = PrSummary {
        pr: PullRequest {
            host: "github.invalid".into(),
            id: "PR_timeline_7".into(),
            repository: "org/alpha".into(),
            number: 7,
            title: "Improve auth".into(),
            body: "A useful change".into(),
            url: String::new(),
            author: Actor::default(),
            state: "open".into(),
            draft: false,
            created_at: String::new(),
            updated_at: String::new(),
            queues: BTreeSet::new(),
        },
        head_sha: HEAD.into(),
        base_sha: "b".repeat(40),
        head_ref: "feature".into(),
        base_ref: "main".into(),
        merged: false,
        auto_merge: false,
        merge_commit_sha: None,
        mergeable: None,
        mergeable_state: "clean".into(),
        additions: 1,
        deletions: 1,
        changed_files: 1,
    };
    let comment = ConversationComment {
        id: 10,
        node_id: "IC10".into(),
        body: "Please keep auth strict".into(),
        author: Actor {
            login: "reviewer".into(),
            ..Default::default()
        },
        created_at: String::new(),
        updated_at: "2026-10-12T12:00:00Z".into(),
        url: String::new(),
    };
    let timeline = vec![
        // First, so the initial cursor (row 0) is not on an item.
        json!({"event":"labeled","label":{"name":"security"},"actor":{"login":"alice"},"created_at":"2026-10-12T13:30:00Z"}),
        json!({"event":"commented","id":10,"user":{"login":"reviewer"},"created_at":"2026-10-12T12:00:00Z","body":"Please keep auth strict"}),
        // A comment this PR's details do not hold stays plain text.
        json!({"event":"commented","id":99,"user":{"login":"someone"},"created_at":"2026-10-12T12:30:00Z"}),
        json!({"event":"committed","sha":OLDER,"message":"Add parser guard","author":{"name":"Alice","date":"2026-10-11T00:00:00Z"}}),
        json!({"event":"committed","sha":HEAD,"message":"Fix auth boundary","author":{"name":"Alice","date":"2026-10-12T00:00:00Z"}}),
        // Not a full object ID: never a commit link.
        json!({"event":"committed","sha":"0123abcd","message":"Short","author":{"name":"Alice"}}),
        json!({"event":"reviewed","id":12,"state":"changes_requested","user":{"login":"reviewer"},"submitted_at":"2026-10-12T13:00:00Z","commit_id":HEAD}),
    ];
    let mut page = PrPage {
        details: PrDetails {
            summary,
            comments: vec![comment],
            commits: vec![
                commit(OLDER, "Add parser guard"),
                commit(HEAD, "Fix auth boundary"),
            ],
            files: vec![],
            reviews: vec![
                json!({"id":12,"state":"CHANGES_REQUESTED","body":"Needs a guard","user":{"login":"reviewer"}}),
            ],
            timeline,
            coverage: Coverage {
                complete: true,
                ..Default::default()
            },
        },
        tab: PrTab::Timeline,
        diff: None,
        required_checks: None,
        checks: Some(vec![Check {
            id: 50,
            name: "Rust tests".into(),
            status: "completed".into(),
            conclusion: "success".into(),
            url: String::new(),
            head_sha: HEAD.into(),
            kind: "check".into(),
            app_id: None,
        }]),
        rows: vec![],
        context: None,
        metadata_display_v51: None,
        review_progress: None,
    };
    page.prepare_rows().unwrap();
    page
}

fn app() -> App {
    let mut github = GithubWorkspace::empty(
        HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "2.101.0".into(),
        },
        vec![],
        BTreeSet::new(),
    );
    github.pr = Some(page());
    App {
        view: View::Inbox,
        github: Some(github),
        github_content_focused: true,
        ..Default::default()
    }
}

fn links(page: &PrPage) -> Vec<(usize, PrRowLinkV13)> {
    page.rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| row.link.clone().map(|link| (index, link)))
        .collect()
}

/// What the background tab worker does for a tab that needs no read: install the page
/// on the new tab, then the runtime's after-install step.
fn install(app: &mut App) {
    let Some(GithubAction::PrTab(tab)) = app.github_action.take() else {
        panic!("expected a queued tab switch, got {:?}", app.github_action);
    };
    let page = app.github.as_mut().unwrap().pr.as_mut().unwrap();
    page.tab = tab;
    page.prepare_rows().unwrap();
    after_install(app);
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

#[test]
fn timeline_links_v13_only_full_records_in_these_details_become_links() {
    let page = page();
    let found = links(&page);
    let labels: Vec<_> = found
        .iter()
        .map(|(row, link)| (page.rows[*row].label.clone(), link.clone()))
        .collect();
    assert_eq!(found.len(), 5, "{labels:#?}");
    assert!(page.rows[found[0].0]
        .label
        .starts_with("comment author • commented"));
    assert_eq!(found[0].1, PrRowLinkV13::Comment(10));
    assert_eq!(found[1].1, PrRowLinkV13::Commit(OLDER.into()));
    assert_eq!(found[2].1, PrRowLinkV13::Commit(HEAD.into()));
    assert_eq!(found[3].1, PrRowLinkV13::Review(12));
    // The review's own commit row links to the head too.
    assert_eq!(page.rows[found[4].0].label, format!("Commit: {HEAD}"));
    assert_eq!(found[4].1, PrRowLinkV13::Commit(HEAD.into()));
    // Comment 99 and the short SHA are not in the details: no link, no target.
    assert!(page
        .rows
        .iter()
        .filter(|row| row.label.contains("2026-10-12T12:30:00Z") || row.label.contains("0123abcd"))
        .all(|row| row.link.is_none()));
    assert_eq!(page.timeline_target_v13(&found[0].1), PrTab::Conversation);
    assert_eq!(page.timeline_target_v13(&found[1].1), PrTab::Commits);
    assert_eq!(page.timeline_target_v13(&found[2].1), PrTab::Checks);
    assert_eq!(page.timeline_target_v13(&found[3].1), PrTab::Conversation);
    // Links mean nothing on another tab.
    let mut other = page.clone();
    other.tab = PrTab::Conversation;
    other.prepare_rows().unwrap();
    assert!((0..other.rows.len()).all(|row| other.timeline_link_at(row).is_none()));
}

#[test]
fn timeline_links_v13_each_target_tab_finds_the_exact_record_row() {
    let mut page = page();
    page.tab = PrTab::Conversation;
    page.prepare_rows().unwrap();
    let comment = page.link_row_v13(&PrRowLinkV13::Comment(10)).unwrap();
    assert!(page.rows[comment].label.starts_with("comment #10"));
    assert_eq!(page.rows[comment].comment.as_ref().unwrap().observed.id, 10);
    let review = page.link_row_v13(&PrRowLinkV13::Review(12)).unwrap();
    assert!(page.rows[review]
        .label
        .starts_with("review 12 • CHANGES_REQUESTED"));
    assert_eq!(page.link_row_v13(&PrRowLinkV13::Commit(OLDER.into())), None);
    page.tab = PrTab::Commits;
    page.prepare_rows().unwrap();
    let older = page
        .link_row_v13(&PrRowLinkV13::Commit(OLDER.into()))
        .unwrap();
    assert!(page.rows[older].label.starts_with(OLDER));
    assert!(page.rows[older].label.ends_with("Add parser guard"));
    page.tab = PrTab::Checks;
    page.prepare_rows().unwrap();
    let check = page
        .link_row_v13(&PrRowLinkV13::Commit(HEAD.into()))
        .unwrap();
    assert_eq!(page.check_at(check).unwrap().name, "Rust tests");
}

#[test]
fn timeline_links_v13_keyboard_and_click_open_item_then_t_returns_to_it() {
    for mouse in [false, true] {
        let mut app = app();
        let first = links(app.github.as_ref().unwrap().pr.as_ref().unwrap())[0].0;
        // Nothing selected yet: the footer control is disabled and says why.
        let control = crate::github_ui::footer_keys(&app)
            .into_iter()
            .find(|control| control.label == "Go to item")
            .expect("Timeline offers Go to item");
        assert!(matches!(control.action, Action::Explain(_)));
        if mouse {
            // A click on a linked row is the same GithubRow a row hit carries.
            app.dispatch(Action::GithubRow(first));
        } else {
            key(&mut app, KeyCode::Down);
        }
        assert_eq!(app.inbox_list.selected, first);
        assert!(
            app.status
                .contains("Selected timeline comment #10 • Enter shows it in Conversation"),
            "{}",
            app.status
        );
        let control = crate::github_ui::footer_keys(&app)
            .into_iter()
            .find(|control| control.label == "Go to item")
            .unwrap();
        assert_eq!(control.action, Action::PrTimelineGoV13);
        assert!(control.disabled.is_none());
        if mouse {
            app.dispatch(control.action.clone());
        } else {
            key(&mut app, KeyCode::Enter);
        }
        assert_eq!(
            app.github_action,
            Some(GithubAction::PrTab(PrTab::Conversation))
        );
        let saved = app.pr_timeline_return_v13.clone().unwrap();
        assert_eq!(
            (saved.row, saved.link.clone()),
            (first, PrRowLinkV13::Comment(10))
        );
        install(&mut app);
        let page = app.github.as_ref().unwrap().pr.as_ref().unwrap();
        let selected = &page.rows[app.inbox_list.selected];
        assert!(
            selected.label.starts_with("comment #10"),
            "{}",
            selected.label
        );
        assert_eq!(app.inbox_list.top, app.inbox_list.selected);
        assert!(app
            .status
            .contains("Timeline comment #10 → Conversation • t returns to the Timeline item"));
        // `t` (or its tab) goes back to the same Timeline item.
        if mouse {
            app.dispatch(Action::Github(GithubAction::PrTab(PrTab::Timeline)));
        } else {
            key(&mut app, KeyCode::Char('t'));
        }
        install(&mut app);
        assert_eq!(app.inbox_list.selected, first);
        assert!(app.status.contains("Back at the Timeline comment #10"));
        assert!(app.pr_timeline_return_v13.is_none());
        // Commits: the older commit opens in Commits; the head commit opens its Checks.
        for (sha, tab, text) in [
            (OLDER, PrTab::Commits, "Add parser guard"),
            (HEAD, PrTab::Checks, "Rust tests"),
        ] {
            let row = links(app.github.as_ref().unwrap().pr.as_ref().unwrap())
                .into_iter()
                .find(|(_, link)| *link == PrRowLinkV13::Commit(sha.into()))
                .unwrap()
                .0;
            app.dispatch(Action::GithubRow(row));
            app.dispatch(Action::PrTimelineGoV13);
            assert_eq!(app.github_action, Some(GithubAction::PrTab(tab)));
            install(&mut app);
            let page = app.github.as_ref().unwrap().pr.as_ref().unwrap();
            assert!(page.rows[app.inbox_list.top].label.contains(text));
            app.dispatch(Action::Github(GithubAction::PrTab(PrTab::Timeline)));
            install(&mut app);
            assert_eq!(app.inbox_list.selected, row);
        }
        // Back to the Inbox ends any return.
        app.dispatch(Action::GithubRow(first));
        app.dispatch(Action::PrTimelineGoV13);
        app.dispatch(Action::Github(GithubAction::Back));
        assert!(app.pr_timeline_return_v13.is_none());
    }
}

#[test]
fn timeline_links_v13_refuses_unlinked_scoped_and_loading_without_a_tab_switch() {
    let mut app = app();
    // A label row has no item.
    let label = app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|row| row.label == "Label: security")
        .unwrap();
    app.inbox_list.selected = label;
    assert!(
        matches!(action(&app), Action::Explain(reason) if reason.contains("Select a Timeline"))
    );
    go(&mut app);
    assert!(app.github_action.is_none() && app.pr_timeline_return_v13.is_none());
    let first = links(app.github.as_ref().unwrap().pr.as_ref().unwrap())[0].0;
    app.inbox_list.selected = first;
    app.loading[View::Inbox.index()] = true;
    assert!(matches!(action(&app), Action::Explain(reason) if reason.contains("Wait")));
    app.loading[View::Inbox.index()] = false;
    // Another PR installed while the item was opening ends the return.
    app.dispatch(Action::PrTimelineGoV13);
    app.github_action = None;
    app.github
        .as_mut()
        .unwrap()
        .pr
        .as_mut()
        .unwrap()
        .details
        .summary
        .pr
        .id = "PR_other".into();
    after_install(&mut app);
    assert!(app.pr_timeline_return_v13.is_none());
}

#[test]
fn v13_summary_shows_full_base_and_head_and_closing_keyword_issues() {
    let mut page = page();
    page.details.summary.pr.body = "Fixes #12 and closes org/beta#3\nSecond paragraph".into();
    page.tab = PrTab::Summary;
    page.prepare_rows().unwrap();
    let labels: Vec<&str> = page.rows.iter().map(|row| row.label.as_str()).collect();
    assert!(
        labels.contains(&format!("Head {HEAD}").as_str()),
        "{labels:?}"
    );
    assert!(labels.contains(&format!("Base {}", "b".repeat(40)).as_str()));
    let linked = labels
        .iter()
        .position(|label| label.starts_with("Linked issues: #12, org/beta#3 • "))
        .expect("linked issues row");
    let source = labels
        .iter()
        .position(|label| label.starts_with("Source: "))
        .unwrap();
    assert!(linked < source, "{labels:?}");
    // Without closing keywords the Summary adds no linked-issues row.
    page.details.summary.pr.body = "Mentions #12 only".into();
    page.prepare_rows().unwrap();
    assert!(!page
        .rows
        .iter()
        .any(|row| row.label.starts_with("Linked issues")));
    // Many references are capped here; Metadata lists them all.
    page.details.summary.pr.body = (1..=8).map(|n| format!("fixes #{n} ")).collect();
    page.prepare_rows().unwrap();
    let row = page
        .rows
        .iter()
        .find(|row| row.label.starts_with("Linked issues"))
        .unwrap();
    assert!(
        row.label
            .starts_with("Linked issues: #1, #2, #3, #4, #5, +3 more in u Metadata • "),
        "{}",
        row.label
    );
}
