//! TASK-34 actor profile contracts: the selected row's actor, its role, the shared identity
//! presentation, bot/deleted/unknown states, and key/click parity on the PR page.
use super::*;
use crate::app::{Overlay, View};
use crate::github_workspace::{GithubAction, GithubWorkspace, PrPage};
use crate::providers::gh::{
    ConversationComment, Coverage, HostIdentity, PrDetails, PrSummary, PullRequest,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeSet;

fn actor(login: &str, kind: &str) -> Actor {
    Actor {
        login: login.into(),
        display_name: None,
        avatar_url: None,
        actor_type: kind.into(),
    }
}
fn comment(id: u64, author: Actor, body: &str) -> ConversationComment {
    ConversationComment {
        id,
        node_id: format!("IC_{id}"),
        body: body.into(),
        author,
        created_at: "2026-10-13T00:00:00Z".into(),
        updated_at: "2026-10-13T00:00:00Z".into(),
        url: String::new(),
    }
}
fn app(tab: PrTab) -> App {
    let mut github = GithubWorkspace::empty(
        HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "2.99.0".into(),
        },
        vec![],
        BTreeSet::new(),
    );
    let pr = PullRequest {
        host: "github.invalid".into(),
        id: "PR_profile_7".into(),
        repository: "example/repo".into(),
        number: 7,
        title: "Profiles".into(),
        body: "Description line".into(),
        url: String::new(),
        author: actor("octocat", "User"),
        state: "open".into(),
        draft: false,
        created_at: "2026-10-01T00:00:00Z".into(),
        updated_at: String::new(),
        queues: BTreeSet::new(),
    };
    let summary = PrSummary {
        pr,
        head_sha: "a".repeat(40),
        base_sha: "b".repeat(40),
        head_ref: "feature".into(),
        base_ref: "main".into(),
        merged: false,
        auto_merge: false,
        merge_commit_sha: None,
        mergeable: None,
        mergeable_state: "unknown".into(),
        additions: 0,
        deletions: 0,
        changed_files: 0,
    };
    let mut page = PrPage {
        details: PrDetails {
            summary,
            comments: vec![
                comment(9, actor("dependabot[bot]", "Bot"), "Bot body"),
                comment(10, actor("ghost", "User"), "Deleted body"),
                comment(11, actor("", "User"), "Unknown body"),
            ],
            commits: vec![],
            files: vec![],
            reviews: vec![],
            timeline: vec![],
            coverage: Coverage {
                complete: true,
                ..Default::default()
            },
        },
        tab,
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
fn row(app: &App, label: &str) -> usize {
    let page = app.github.as_ref().unwrap().pr.as_ref().unwrap();
    page.rows
        .iter()
        .position(|row| row.label == label)
        .unwrap_or_else(|| panic!("no row {label}"))
}
fn page(app: &App) -> &PrPage {
    app.github.as_ref().unwrap().pr.as_ref().unwrap()
}

#[test]
fn conversation_comments_name_their_author_and_other_rows_the_pr_author() {
    let app = app(PrTab::Conversation);
    let github = app.github.as_ref().unwrap();
    // Profiles never add targets: the PR description heading and its prose stay read-only.
    assert!(!github.pr_row_is_interactive(row(&app, "PR description")));
    assert!(!github.pr_row_is_interactive(row(&app, "Description line")));
    for (index, _) in page(&app).rows.iter().enumerate() {
        let interactive = github.pr_row_is_interactive(index);
        let comment = page(&app).rows[index].comment.is_some();
        assert_eq!(
            interactive, comment,
            "row {index} interactivity is unchanged"
        );
    }
    // A non-target selection names the PR author explicitly.
    let author = subject(github, row(&app, "PR description")).unwrap();
    assert_eq!(
        (author.role, author.actor.login.as_str()),
        ("PR author", "octocat")
    );
    assert_eq!(
        subject(github, row(&app, "Description line")).unwrap(),
        author
    );
    assert_eq!(subject(github, usize::MAX).unwrap(), author);
    // A selected comment (an existing target) names that comment's author.
    let bot_body = subject(github, row(&app, "Bot body")).unwrap();
    assert_eq!(
        (bot_body.role, bot_body.actor.login.as_str()),
        ("Comment author", "dependabot[bot]")
    );
    let heading = page(&app)
        .rows
        .iter()
        .position(|row| row.label.starts_with("comment #10 "))
        .unwrap();
    assert_eq!(subject(github, heading).unwrap().actor.login, "ghost");
}

#[test]
fn summary_and_commits_offer_the_pr_author_and_other_tabs_explain() {
    // Metadata offers it too, but its rows need loaded metadata this fixture lacks.
    for tab in [PrTab::Summary, PrTab::Commits] {
        let app = app(tab);
        let github = app.github.as_ref().unwrap();
        assert!(
            (0..page(&app).rows.len())
                .all(|index| subject(github, index).unwrap().actor.login == "octocat")
        );
        assert_eq!(
            action(&app).unwrap(),
            Action::Github(GithubAction::ActorProfile)
        );
        let keys = crate::github_ui::footer_keys(&app);
        let control = keys.iter().find(|control| control.key == "@").unwrap();
        assert!(control.disabled.is_none(), "{tab:?}");
        // The lowest-priority view key: it leaves a narrow bar before any other control.
        assert!(keys
            .iter()
            .filter(|other| other.group != crate::footer::ControlGroup::Global)
            .all(|other| other.priority <= control.priority));
    }
    let app = app(PrTab::Timeline);
    assert_eq!(action(&app).unwrap_err(), NO_TAB);
    assert!(crate::github_ui::footer_keys(&app)
        .iter()
        .all(|control| control.key != "@"));
}

#[test]
fn profile_uses_the_shared_identity_label_and_initials() {
    let host = "github.invalid";
    for (login, kind, state) in [
        ("octocat", "User", "Kind: User account"),
        ("dependabot[bot]", "Bot", "Kind: Bot (automation account)"),
        ("ghost", "User", "Kind: Deleted account"),
        ("", "User", "Kind: Unknown"),
    ] {
        let subject = Subject {
            role: "Comment author",
            actor: actor(login, kind),
        };
        let identity = ActorIdentity::github(host, &subject.actor);
        let text = text(host, "octocat", "example/repo #7", &subject, None);
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines[0], "Actor profile — Comment author");
        // The exact chip and label every row paints for this actor.
        assert_eq!(
            lines[1],
            format!("[{}] {}", identity.initials(), identity.label())
        );
        assert!(text.contains(state), "{text}");
        let verified = matches!(kind, "User" | "Bot") && !login.is_empty() && login != "ghost";
        assert_eq!(
            text.contains("Verified: github.invalid reported this account on example/repo #7"),
            verified,
            "{text}"
        );
        assert_eq!(text.contains("Not verified"), !verified, "{text}");
        assert_eq!(
            text.contains("Signed-in account: @octocat"),
            login == "octocat"
        );
        assert!(text.contains("Avatar: none reported • initials shown"));
    }
    let bot = text(
        host,
        "octocat",
        "example/repo #7",
        &Subject {
            role: "Comment author",
            actor: actor("dependabot[bot]", "Bot"),
        },
        None,
    );
    assert!(bot.contains("@dependabot[bot] [bot]"), "{bot}");
    let deleted = text(
        host,
        "",
        "x",
        &Subject {
            role: "PR author",
            actor: actor("ghost", "User"),
        },
        None,
    );
    assert!(deleted.contains("[×] @ghost [deleted]"), "{deleted}");
}

#[test]
fn avatar_state_is_reported_without_a_url_or_credentials() {
    let host = "github.invalid";
    let mut subject = Subject {
        role: "PR author",
        actor: actor("octocat", "User"),
    };
    subject.actor.avatar_url = Some("https://avatars.githubusercontent.com/u/1?v=4&secret".into());
    let pending = text(host, "", "x", &subject, Some(&AvatarState::Loading));
    assert!(pending.contains("Avatar: loading in the background"));
    let failed = text(
        host,
        "",
        "x",
        &subject,
        Some(&AvatarState::Failed("boom".into())),
    );
    assert!(failed.contains("Avatar: image unavailable • initials shown"));
    let absent = text(host, "", "x", &subject, None);
    assert!(absent.contains("Avatar: public image not loaded • initials shown"));
    for text in [pending, failed, absent] {
        assert!(!text.contains("avatars.githubusercontent.com") && !text.contains("secret"));
    }
}

#[test]
fn key_and_footer_click_open_the_same_read_only_profile() {
    for click in [false, true] {
        let mut app = app(PrTab::Conversation);
        app.inbox_list.selected = row(&app, "Bot body");
        if click {
            let keys = crate::github_ui::footer_keys(&app);
            let control = keys
                .iter()
                .find(|control| control.key == "@")
                .expect("the PR footer offers @ Actor profile");
            assert_eq!(control.label, "Actor profile");
            assert!(control.disabled.is_none());
            app.dispatch(control.action.clone());
        } else {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('@'),
                KeyModifiers::NONE,
            )));
        }
        assert_eq!(app.overlay, Some(Overlay::Notice), "click={click}");
        assert!(app.notice.starts_with("Actor profile — Comment author\n"));
        assert!(app.notice.contains("@dependabot[bot] [bot]"));
        assert_eq!(
            crate::scoped_pr_observed_tabs_v1::resources::native::notice_title(&app.notice),
            TITLE
        );
        // Read-only: no GitHub action was queued and the page stays open.
        assert!(app.github_action.is_none());
        assert!(page(&app).tab == PrTab::Conversation);
        assert_eq!(
            Action::Github(GithubAction::ActorProfile),
            action(&app).unwrap()
        );
    }
}

#[test]
fn key_and_click_on_a_non_comment_selection_open_the_pr_author() {
    for click in [false, true] {
        let mut app = app(PrTab::Summary);
        app.inbox_list.selected = 0;
        if click {
            let keys = crate::github_ui::footer_keys(&app);
            let control = keys.iter().find(|control| control.key == "@").unwrap();
            app.dispatch(control.action.clone());
        } else {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('@'),
                KeyModifiers::NONE,
            )));
        }
        assert_eq!(app.overlay, Some(Overlay::Notice), "click={click}");
        assert!(app.notice.starts_with("Actor profile — PR author\n"));
        assert!(app
            .notice
            .contains("Signed-in account: @octocat (same login)"));
        assert!(app.github_action.is_none());
        assert_eq!(
            app.inbox_list.selected, 0,
            "opening a profile moves no selection"
        );
    }
}
