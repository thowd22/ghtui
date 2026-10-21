//! Read-only GitHub actor profiles on an open pull request (TASK-34).
//!
//! The PR author, comment and review authors, GitHub-linked commit authors and committers,
//! assignees and requested reviewers open one shared profile. It is built from the same
//! `ActorIdentity::github` the rows paint, so the username, initials and bot/deleted/unknown
//! state match every list and pane. Opening it reads nothing: no `gh`, Git or cache effect.

use crate::app::{Action, App};
use crate::github_workspace::{GithubWorkspace, PrTab};
use crate::identity::{ActorIdentity, ActorKind, AvatarState};
use crate::providers::gh::Actor;

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
    let identity = ActorIdentity::github(host, &subject.actor);
    let mut lines = vec![
        format!("{HEADER} — {}", subject.role),
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
    lines.push("Read-only • reads nothing from GitHub or Git".into());
    lines.join("\n")
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
