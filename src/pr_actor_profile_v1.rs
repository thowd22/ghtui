//! Read-only GitHub actor profiles on an open pull request and an open Actions run (TASK-34).
//!
//! The PR author, comment and review authors, assignees and requested reviewers open one
//! shared profile. The Commits tab lists every commit author and committer (an honest "No
//! GitHub account" profile with the Git name and email when the host linked none), and an
//! Actions run lists its triggering and run actors. It is built from the same
//! `ActorIdentity::github` the rows paint, so the username, initials and bot/deleted/unknown
//! state match every list and pane. Opening it reads nothing: no `gh`, Git or cache effect.

use crate::app::{Action, App};
use crate::git::GitActor;
use crate::github_workspace::{GithubWorkspace, PrTab};
use crate::identity::{ActorIdentity, ActorKind, AvatarState, GitRole};
use crate::providers::gh::{Actor, CommitSummary};

/// The first line of every profile notice; it also selects the overlay title.
pub const HEADER: &str = "Actor profile";
pub const TITLE: &str = " Actor profile • read-only ";
const NO_PR: &str = "Open a pull request first.";
const NO_TAB: &str = "Actor profiles open from Summary, Conversation, Commits or Metadata.";

/// The actor of the selected row and the role it plays in this pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subject {
    pub role: &'static str,
    pub actor: Actor,
}

fn role(tab: PrTab, label: &str, table_row: bool) -> &'static str {
    if label == "PR description" || label.starts_with("opened ") {
        "PR author"
    } else if label.starts_with("comment #") {
        "Comment author"
    } else if label.starts_with("review ") && tab == PrTab::Conversation {
        "Review author"
    } else if label.starts_with("committer ") {
        "Commit committer (GitHub account)"
    } else if label.starts_with("author ") || table_row && tab == PrTab::Commits {
        "Commit author (GitHub account)"
    } else if label == "assigned" {
        "Assignee"
    } else if label == "review requested" {
        "Requested reviewer"
    } else {
        "Actor"
    }
}

/// The tabs whose key bar offers `@ Actor profile`.
pub fn offered(tab: PrTab) -> bool {
    matches!(
        tab,
        PrTab::Summary | PrTab::Conversation | PrTab::Commits | PrTab::Metadata
    )
}

/// The actor of the selected row when that row is already a key and click target (a
/// comment); this never makes another row selectable. Any other selection, or none, names
/// the PR author, so the profile control always has an explicit subject.
pub fn subject(github: &GithubWorkspace, selected: usize) -> Option<Subject> {
    let page = github.pr.as_ref()?;
    let row = page
        .rows
        .get(selected)
        .filter(|_| github.pr_row_is_interactive(selected));
    if let Some(row) = row {
        if let Some(comment) = &row.comment {
            return Some(Subject {
                role: match comment.kind {
                    crate::providers::gh::CommentKind::Review => "Review comment author",
                    crate::providers::gh::CommentKind::Conversation => "Comment author",
                },
                actor: comment.observed.author.clone(),
            });
        }
        if let Some(actor) = &row.actor {
            return Some(Subject {
                role: role(page.tab, &row.label, row.cells.is_some()),
                actor: actor.clone(),
            });
        }
    }
    Some(Subject {
        role: "PR author",
        actor: page.details.summary.pr.author.clone(),
    })
}

fn kind_label(kind: ActorKind) -> &'static str {
    match kind {
        ActorKind::User => "User account",
        ActorKind::Organization => "Organization",
        ActorKind::Bot => "Bot (automation account)",
        ActorKind::Deleted => "Deleted account (GitHub shows it as ghost)",
        ActorKind::Unknown => "Unknown — GitHub returned no usable account",
        ActorKind::Local => "Local Git identity",
    }
}

fn avatar_label(identity: &ActorIdentity, state: Option<&AvatarState>) -> &'static str {
    match state {
        Some(AvatarState::Ready(_)) => "Avatar: public image loaded",
        Some(AvatarState::Loading | AvatarState::Busy) => {
            "Avatar: loading in the background • initials shown meanwhile"
        }
        Some(AvatarState::Failed(_)) => "Avatar: image unavailable • initials shown",
        _ if identity.avatar_url.is_some() => "Avatar: public image not loaded • initials shown",
        _ => "Avatar: none reported • initials shown",
    }
}

/// The profile notice text. `viewer` is the signed-in login, compared exactly.
pub fn text(
    host: &str,
    viewer: &str,
    pr: &str,
    subject: &Subject,
    avatar: Option<&AvatarState>,
) -> String {
    let mut lines = vec![format!("{HEADER} — {}", subject.role)];
    lines.extend(profile_lines(host, viewer, pr, &subject.actor, avatar));
    lines.push(READ_ONLY.into());
    lines.join("\n")
}

const READ_ONLY: &str = "Read-only • reads nothing from GitHub or Git";
/// Commits listed in one Commits-tab profile notice; a longer page says how many it left out.
pub const COMMIT_LIMIT: usize = 100;
/// Listing stops before this many bytes, under the 16000-character notice cut.
const TEXT_BUDGET: usize = 13_000;

/// One GitHub actor's profile lines (chip and label, username, kind, verification, signed-in
/// match and avatar state), shared by every profile notice. `context` names where the host
/// reported the actor, such as `example/repo #7` or `example/repo run 123`.
fn profile_lines(
    host: &str,
    viewer: &str,
    context: &str,
    actor: &Actor,
    avatar: Option<&AvatarState>,
) -> Vec<String> {
    let identity = ActorIdentity::github(host, actor);
    let pr = context;
    let mut lines = vec![
        format!("[{}] {}", identity.initials(), identity.label()),
        format!(
            "Username: {}",
            identity
                .login
                .as_deref()
                .map_or_else(|| "unavailable".to_owned(), |login| format!("@{login}"))
        ),
    ];
    if !identity.name.is_empty() {
        lines.push(format!("Name: {}", identity.name));
    }
    lines.push(format!("Kind: {}", kind_label(identity.kind)));
    let verified = identity.host_associated
        && identity.login.is_some()
        && !matches!(identity.kind, ActorKind::Deleted | ActorKind::Unknown);
    lines.push(match (&identity.host, verified) {
        (Some(host), true) => format!("Verified: {host} reported this account on {pr}"),
        (Some(host), false) => format!("Not verified: {host} reported no live account here"),
        (None, _) => "Not verified: the reported actor exceeded display limits".into(),
    });
    if verified && identity.login.as_deref() == Some(viewer) && !viewer.is_empty() {
        lines.push(format!("Signed-in account: @{viewer} (same login)"));
    }
    if verified {
        lines.push("Identity comes from GitHub, never from a name or email match".into());
    }
    lines.push(avatar_label(&identity, avatar).into());
    lines
}

/// A commit role the host associated with no GitHub account: the Git name and email the
/// commit records, never an account guessed from them.
fn no_account_lines(role: GitRole, name: &str, email: &str) -> Vec<String> {
    let identity = ActorIdentity::local(
        &GitActor {
            name: name.to_owned(),
            email: email.to_owned(),
            timestamp: 0,
        },
        role,
    );
    let field = |value: &str| {
        if crate::git_actor_labels_v1::display_limited(value, None) {
            "unavailable [display limit]".to_owned()
        } else if value.trim().is_empty() {
            "unavailable".to_owned()
        } else {
            value.to_owned()
        }
    };
    vec![
        format!("[{}] {}", identity.initials(), identity.label()),
        "No GitHub account: the host linked none to this Git identity".into(),
        format!("Git name: {}", field(name)),
        format!("Git email: {}", field(email)),
        "Never guessed from a Git name or email".into(),
    ]
}

fn short_subject(message: &str) -> String {
    let subject = message.lines().next().unwrap_or_default();
    let mut short: String = subject.chars().take(40).collect();
    if subject.chars().count() > 40 {
        short.push('…');
    }
    short
}

/// Every commit author and committer on a pull request's Commits tab, in commit order. An
/// account the host linked opens the shared GitHub profile (with the Git name and email the
/// commit recorded); a role without one shows an honest "No GitHub account" profile.
pub fn commits_text<'a>(
    host: &str,
    viewer: &str,
    pr: &str,
    commits: &[CommitSummary],
    avatar: impl Fn(&ActorIdentity) -> Option<&'a AvatarState>,
) -> String {
    let mut lines = vec![
        format!("{HEADER}s — commit authors and committers"),
        format!("{} commits on {pr} • PgDn for more", commits.len()),
    ];
    let mut listed = 0;
    for (index, commit) in commits.iter().take(COMMIT_LIMIT).enumerate() {
        // Notices keep 16000 characters; stop listing whole commits before that cut.
        if lines.iter().map(|line| line.len() + 1).sum::<usize>() > TEXT_BUDGET {
            break;
        }
        listed += 1;
        lines.push(String::new());
        lines.push(format!(
            "Commit {} of {}: {} {}",
            index + 1,
            commits.len(),
            crate::ui_table::short_sha(&commit.sha),
            short_subject(&commit.message)
        ));
        for (role, title, actor, name, email) in [
            (
                GitRole::Author,
                "Commit author",
                &commit.author,
                &commit.author_name,
                &commit.author_email,
            ),
            (
                GitRole::Committer,
                "Commit committer",
                &commit.committer,
                &commit.committer_name,
                &commit.committer_email,
            ),
        ] {
            if actor.login.is_empty() {
                lines.push(format!("{title} — no GitHub account"));
                lines.extend(no_account_lines(role, name, email));
            } else {
                lines.push(format!("{title} (GitHub account)"));
                let identity = ActorIdentity::github(host, actor);
                lines.extend(profile_lines(host, viewer, pr, actor, avatar(&identity)));
                lines.push(format!(
                    "Git records: {}",
                    crate::git_actor_labels_v1::actor_text(&GitActor {
                        name: name.clone(),
                        email: email.clone(),
                        timestamp: 0,
                    })
                ));
            }
        }
    }
    if commits.len() > listed {
        lines.push(String::new());
        lines.push(format!(
            "{} more commits are not listed here",
            commits.len() - listed
        ));
    }
    lines.push(String::new());
    lines.push(READ_ONLY.into());
    lines.join("\n")
}

/// The triggering actor (who started this attempt) and the run actor (who started the run)
/// of an open Actions run, each with the shared GitHub profile.
pub fn run_text<'a>(
    host: &str,
    viewer: &str,
    repository: &str,
    run: &crate::providers::gh::WorkflowRun,
    avatar: impl Fn(&ActorIdentity) -> Option<&'a AvatarState>,
) -> String {
    let context = format!("{repository} run {}", run.id);
    let mut lines = vec![
        format!("{HEADER} — Actions run actors"),
        format!("Run {} attempt {} on {repository}", run.id, run.attempt),
    ];
    for (title, actor) in [
        (
            format!("Triggering actor (started attempt {})", run.attempt),
            &run.triggering_actor,
        ),
        (
            "Run actor (started the first attempt)".to_owned(),
            &run.actor,
        ),
    ] {
        lines.push(String::new());
        lines.push(title);
        let identity = ActorIdentity::github(host, actor);
        lines.extend(profile_lines(
            host,
            viewer,
            &context,
            actor,
            avatar(&identity),
        ));
    }
    lines.push(String::new());
    lines.push(READ_ONLY.into());
    lines.join("\n")
}

const NO_RUN: &str = "Open an Actions run first.";

/// Open the run's triggering and run actor profiles as one read-only notice.
pub fn open_run(app: &mut App) {
    let message = match app
        .actions
        .as_ref()
        .and_then(|actions| Some((actions, actions.run.as_ref()?)))
    {
        Some((actions, page)) => run_text(
            &actions.identity.host,
            &actions.identity.login,
            &page.repository,
            &page.attempt.run,
            |identity| app.avatar_states.get(identity),
        ),
        None => NO_RUN.to_owned(),
    };
    app.dispatch(Action::Explain(message));
}

/// The footer control's action: the profile of the selected comment's author or the PR
/// author, or why there is none.
pub fn action(app: &App) -> Result<Action, String> {
    let github = app.github.as_ref().ok_or(NO_PR)?;
    let page = github.pr.as_ref().ok_or(NO_PR)?;
    if !offered(page.tab) {
        return Err(NO_TAB.to_owned());
    }
    subject(github, app.inbox_list.selected).ok_or_else(|| NO_PR.to_owned())?;
    Ok(Action::Github(
        crate::github_workspace::GithubAction::ActorProfile,
    ))
}

/// Open the selected comment author's (or the PR author's) profile as a read-only notice.
pub fn open(app: &mut App) {
    let message = match app.github.as_ref() {
        None => NO_PR.to_owned(),
        Some(github) => match (github.pr.as_ref(), subject(github, app.inbox_list.selected)) {
            // Commit rows are read-only (never selectable), so the Commits tab's profile lists
            // every commit's author and committer instead of naming one selected row.
            (Some(page), Some(_))
                if page.tab == PrTab::Commits && !page.details.commits.is_empty() =>
            {
                let summary = &page.details.summary.pr;
                commits_text(
                    &github.identity.host,
                    &github.identity.login,
                    &format!("{} #{}", summary.repository, summary.number),
                    &page.details.commits,
                    |identity| app.avatar_states.get(identity),
                )
            }
            (Some(page), Some(subject)) => {
                let host = &github.identity.host;
                let identity = ActorIdentity::github(host, &subject.actor);
                let summary = &page.details.summary.pr;
                text(
                    host,
                    &github.identity.login,
                    &format!("{} #{}", summary.repository, summary.number),
                    &subject,
                    app.avatar_states.get(&identity),
                )
            }
            _ => NO_PR.to_owned(),
        },
    };
    app.dispatch(Action::Explain(message));
}

#[cfg(test)]
#[path = "pr_actor_profile_v1_tests.rs"]
mod tests;
