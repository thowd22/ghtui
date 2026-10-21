//! TASK-34 actor profile contracts: the selected row's actor, its role, the shared identity
//! presentation, bot/deleted/unknown states, and key/click parity on the PR page.
use super::*;
use crate::app::{Overlay, View};
use crate::github_workspace::{GithubAction, GithubWorkspace, PrPage};
use crate::providers::gh::{
    CommitSummary, ConversationComment, Coverage, HostIdentity, PrDetails, PrSummary, PullRequest,
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

fn commit(sha: char, author: Actor, committer: Actor) -> CommitSummary {
    CommitSummary {
        sha: sha.to_string().repeat(40),
        message: format!("Commit {sha} subject\n\nbody"),
        author,
        committer,
        author_name: "Alice Author".into(),
        committer_name: "Local Committer".into(),
        author_email: "alice@example.invalid".into(),
        committer_email: "committer@example.invalid".into(),
        authored_at: "2026-10-12T12:00:00Z".into(),
        parents: vec![],
    }
}

fn commits_app() -> App {
    let mut app = app(PrTab::Commits);
    let page = app.github.as_mut().unwrap().pr.as_mut().unwrap();
    page.details.commits = vec![
        commit('c', actor("alice", "User"), actor("", "")),
        commit('d', actor("", ""), actor("web-flow", "User")),
    ];
    page.prepare_rows().unwrap();
    app
}

#[test]
fn commits_list_github_and_no_account_authors_and_committers() {
    let app = commits_app();
    let page = page(&app);
    let text = commits_text(
        "github.invalid",
        "octocat",
        "example/repo #7",
        &page.details.commits,
        |_| None,
    );
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines[0], "Actor profiles — commit authors and committers");
    assert_eq!(
        crate::scoped_pr_observed_tabs_v1::resources::native::notice_title(&text),
        TITLE
    );
    // The GitHub-linked author: the same chip and label the rows paint, verified, plus the
    // Git identity the commit recorded.
    let alice = ActorIdentity::github("github.invalid", &actor("alice", "User"));
    let first = lines
        .iter()
        .position(|line| *line == "Commit 1 of 2: ccccccc Commit c subject")
        .unwrap();
    assert_eq!(lines[first + 1], "Commit author (GitHub account)");
    assert_eq!(
        lines[first + 2],
        format!("[{}] {}", alice.initials(), alice.label())
    );
    assert!(text.contains("Verified: github.invalid reported this account on example/repo #7"));
    assert!(text.contains("Git records: Alice Author <alice@example.invalid>"));
    // The committer without an account is honest: no username, the Git name and email only.
    let committer = lines
        .iter()
        .position(|line| *line == "Commit committer — no GitHub account")
        .unwrap();
    assert!(committer > first);
    assert_eq!(
        lines[committer + 1],
        "[LC] committer Local Committer <committer@example.invalid> [local Git]"
    );
    assert_eq!(
        lines[committer + 2],
        "No GitHub account: the host linked none to this Git identity"
    );
    assert_eq!(lines[committer + 3], "Git name: Local Committer");
    assert_eq!(lines[committer + 4], "Git email: committer@example.invalid");
    assert!(lines[committer..committer + 6]
        .iter()
        .all(|line| !line.starts_with("Username")));
    // The second commit swaps them.
    assert!(text.contains("Commit 2 of 2: ddddddd Commit d subject"));
    assert!(text.contains("[AA] author Alice Author <alice@example.invalid> [local Git]"));
    assert!(text.contains("Username: @web-flow"));
    assert!(text.ends_with("Read-only • reads nothing from GitHub or Git"));
    // A missing email is reported as unavailable, never invented.
    let mut bare = page.details.commits[1].clone();
    bare.author_email.clear();
    let text = commits_text("github.invalid", "", "x", &[bare], |_| None);
    assert!(text.contains("Git email: unavailable"), "{text}");
}

#[test]
fn commits_listing_stays_under_the_notice_cut() {
    let commits: Vec<_> = (0..200)
        .map(|_| commit('e', actor("alice", "User"), actor("", "")))
        .collect();
    let text = commits_text("github.invalid", "", "example/repo #7", &commits, |_| None);
    assert!(text.chars().count() < 16_000, "{}", text.len());
    assert!(text.contains("more commits are not listed here"));
    assert!(text.ends_with("Read-only • reads nothing from GitHub or Git"));
}

#[test]
fn commits_tab_key_and_click_list_commit_actors_without_new_targets() {
    for click in [false, true] {
        let mut app = commits_app();
        let github = app.github.as_ref().unwrap();
        // Commit rows stay read-only: the profile never makes them targets.
        assert!((0..page(&app).rows.len()).all(|index| !github.pr_row_is_interactive(index)));
        if click {
            let keys = crate::github_ui::footer_keys(&app);
            let control = keys.iter().find(|control| control.key == "@").unwrap();
            assert!(control.disabled.is_none());
            app.dispatch(control.action.clone());
        } else {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('@'),
                KeyModifiers::NONE,
            )));
        }
        assert_eq!(app.overlay, Some(Overlay::Notice), "click={click}");
        assert!(app
            .notice
            .starts_with("Actor profiles — commit authors and committers\n"));
        assert!(app.notice.contains("Commit committer — no GitHub account"));
        assert!(app.github_action.is_none());
        assert!(page(&app).tab == PrTab::Commits);
    }
}

fn run_app() -> App {
    use crate::actions_workspace::{ActionsView, ActionsWorkspace, RunPage};
    use crate::providers::gh::{RunAttempt, WorkflowRun};
    let mut actions = ActionsWorkspace::new(
        HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "2.99.0".into(),
        },
        vec!["example/repo".into()],
    );
    actions.run = Some(RunPage {
        repository: "example/repo".into(),
        attempt: RunAttempt {
            run: WorkflowRun {
                id: 123,
                workflow_id: 7,
                name: "CI".into(),
                title: "Original run".into(),
                head_sha: "a".repeat(40),
                head_branch: "feature".into(),
                event: "pull_request".into(),
                status: "completed".into(),
                conclusion: "failure".into(),
                attempt: 2,
                actor: actor("alice", "User"),
                triggering_actor: actor("renovate[bot]", "Bot"),
                created_at: String::new(),
                updated_at: String::new(),
                url: String::new(),
            },
            jobs: vec![],
            definition_note: "exact original attempt".into(),
        },
        selected_job: None,
        logs: None,
        annotations: vec![],
    });
    actions.view = ActionsView::Run;
    actions.prepare_rows();
    App {
        view: View::Actions,
        actions: Some(actions),
        actions_content_focused: true,
        ..Default::default()
    }
}

#[test]
fn run_profile_names_the_triggering_and_run_actors() {
    let app = run_app();
    let run = &app
        .actions
        .as_ref()
        .unwrap()
        .run
        .as_ref()
        .unwrap()
        .attempt
        .run;
    let text = run_text("github.invalid", "octocat", "example/repo", run, |_| None);
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines[0], "Actor profile — Actions run actors");
    assert_eq!(lines[1], "Run 123 attempt 2 on example/repo");
    let trigger = lines
        .iter()
        .position(|line| *line == "Triggering actor (started attempt 2)")
        .unwrap();
    let bot = ActorIdentity::github("github.invalid", &run.triggering_actor);
    assert_eq!(
        lines[trigger + 1],
        format!("[{}] {}", bot.initials(), bot.label())
    );
    assert!(lines[trigger + 1].ends_with("] @renovate[bot] [bot]"));
    assert!(text.contains("Kind: Bot (automation account)"));
    let actor = lines
        .iter()
        .position(|line| *line == "Run actor (started the first attempt)")
        .unwrap();
    assert!(actor > trigger);
    assert_eq!(lines[actor + 2], "Username: @alice");
    assert!(text.contains("Verified: github.invalid reported this account on example/repo run 123"));
    assert!(!text.contains("Signed-in account"));
}

#[test]
fn run_detail_control_key_and_click_open_the_run_actors() {
    use crate::actions_workspace::ActionsAction;
    for click in [false, true] {
        let mut app = run_app();
        let rows_before: Vec<_> = app
            .actions
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .map(|row| row.action.clone())
            .collect();
        let keys = crate::actions_ui::footer_keys(app.actions.as_ref().unwrap());
        let control = keys.iter().find(|control| control.key == "@").unwrap();
        assert_eq!(control.label, "Actor profile");
        assert_eq!(control.action, Action::Actions(ActionsAction::ActorProfile));
        assert!(control.disabled.is_none());
        if click {
            app.dispatch(control.action.clone());
        } else {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('@'),
                KeyModifiers::NONE,
            )));
        }
        assert_eq!(app.overlay, Some(Overlay::Notice), "click={click}");
        assert!(app
            .notice
            .starts_with("Actor profile — Actions run actors\n"));
        assert!(app.actions_action.is_none(), "no worker action is queued");
        let actions = app.actions.as_ref().unwrap();
        // The actor rows stay read-only.
        assert_eq!(
            actions
                .rows
                .iter()
                .map(|row| row.action.clone())
                .collect::<Vec<_>>(),
            rows_before
        );
    }
    // Without an open run the Runs list offers no profile control.
    let mut app = run_app();
    let actions = app.actions.as_mut().unwrap();
    actions.run = None;
    actions.view = crate::actions_workspace::ActionsView::Runs;
    actions.prepare_rows();
    assert!(crate::actions_ui::footer_keys(actions)
        .iter()
        .all(|control| control.key != "@"));
}
