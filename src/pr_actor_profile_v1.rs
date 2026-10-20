//! Read-only GitHub actor profiles on an open pull request (TASK-34).
//!
//! The PR author, comment and review authors, GitHub-linked commit authors and committers,
//! assignees and requested reviewers open one shared profile. It is built from the same
//! `ActorIdentity::github` the rows paint, so the username, initials and bot/deleted/unknown
//! state match every list and pane. Opening it reads nothing: no `gh`, Git or cache effect.

use crate::app::{Action, App};
use crate::github_workspace::{PrPage, PrTab};
use crate::identity::{ActorIdentity, ActorKind, AvatarState};
use crate::providers::gh::Actor;

/// The first line of every profile notice; it also selects the overlay title.
pub const HEADER: &str = "Actor profile";
pub const TITLE: &str = " Actor profile • read-only ";
const NO_ACTOR: &str = "Select a row with a GitHub actor (PR author, comment, review or commit author) to open its profile.";

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

/// The selected row's actor. A comment's body lines belong to their comment author; on
/// Summary and Conversation a prose line belongs to the nearest actor heading above it.
/// Other tabs need the actor row itself, so a commit's SHA line never names its committer.
pub fn subject(page: &PrPage, selected: usize) -> Option<Subject> {
    let row = page.rows.get(selected)?;
    if let Some(actor) = &row.actor {
        return Some(Subject {
            role: role(page.tab, &row.label, row.cells.is_some()),
            actor: actor.clone(),
        });
    }
    if let Some(comment) = &row.comment {
        return Some(Subject {
            role: "Comment author",
            actor: comment.observed.author.clone(),
        });
    }
    if !matches!(page.tab, PrTab::Summary | PrTab::Conversation) {
        return None;
    }
    page.rows[..selected]
        .iter()
        .rev()
        .find_map(|row| row.actor.as_ref().map(|actor| (row, actor)))
        .map(|(row, actor)| Subject {
            role: role(page.tab, &row.label, row.cells.is_some()),
            actor: actor.clone(),
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

/// The footer control's action: the profile of the selected row, or why there is none.
pub fn action(app: &App) -> Result<Action, String> {
    let page = app
        .github
        .as_ref()
        .and_then(|github| github.pr.as_ref())
        .ok_or("Open a pull request first.")?;
    subject(page, app.inbox_list.selected).ok_or_else(|| NO_ACTOR.to_owned())?;
    Ok(Action::Github(
        crate::github_workspace::GithubAction::ActorProfile,
    ))
}

/// Open the selected row's profile as a read-only notice, or explain why there is none.
pub fn open(app: &mut App) {
    let message = match app.github.as_ref().and_then(|github| {
        let page = github.pr.as_ref()?;
        Some((github, page))
    }) {
        None => "Open a pull request first.".to_owned(),
        Some((github, page)) => match subject(page, app.inbox_list.selected) {
            None => NO_ACTOR.to_owned(),
            Some(subject) => {
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
        },
    };
    app.dispatch(Action::Explain(message));
}

#[cfg(test)]
#[path = "pr_actor_profile_v1_tests.rs"]
mod tests;
