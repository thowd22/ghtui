//! Bounded local Git display only. Original GitActor fields/selected object IDs
//! remain canonical observations; no username, URL, signature or auth inference.
use crate::{
    git::{Commit, GitActor},
    identity::{ActorIdentity, ActorKind, GitRole},
};

pub const FIELD_BYTES: usize = 4096;
const PAIR_BYTES: usize = 8192;
fn safe_field(value: &str) -> bool {
    value.len() <= FIELD_BYTES
        && !value.chars().any(|ch| {
            ch.is_control() || matches!(ch,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
        })
}
pub fn display_limited(name: &str, email: Option<&str>) -> bool {
    !safe_field(name)
        || email.is_some_and(|email| !safe_field(email))
        || name.len().saturating_add(email.map_or(0, str::len)) > PAIR_BYTES
}
fn role_label(role: Option<GitRole>) -> &'static str {
    match role {
        Some(GitRole::Author) => "author ",
        Some(GitRole::Committer) => "committer ",
        None => "local Git actor ",
    }
}
pub fn actor_text(actor: &GitActor) -> String {
    parts(&actor.name, Some(&actor.email))
}
fn parts(name: &str, email: Option<&str>) -> String {
    if display_limited(name, email) {
        return "metadata unavailable [display limit]".into();
    }
    let name = if name.trim().is_empty() {
        "unknown name"
    } else {
        name
    };
    match email.filter(|email| !email.trim().is_empty()) {
        Some(email) => format!("{name} <{email}>"),
        None => format!("{name} [email unavailable]"),
    }
}
pub fn local_label(name: &str, email: Option<&str>, role: Option<GitRole>) -> String {
    format!("{}{} [local Git]", role_label(role), parts(name, email))
}
pub fn commit_summary(commit: &Commit) -> String {
    format!(
        "author {} • committer {}",
        actor_text(&commit.author),
        actor_text(&commit.committer)
    )
}
/// Called BEFORE central ActorIdentity::local copies name/email. The source
/// GitActor is never cropped/rewritten and this explicit display placeholder
/// cannot be associated to a remote account or request an image.
pub fn local_fallback(actor: &GitActor, role: GitRole) -> Option<ActorIdentity> {
    display_limited(&actor.name, Some(&actor.email)).then(|| ActorIdentity {
        host: None,
        login: None,
        name: "metadata unavailable [display limit]".into(),
        email: None,
        avatar_url: None,
        kind: ActorKind::Unknown,
        git_role: Some(role),
        host_associated: false,
    })
}
pub fn display_identity(actor: &GitActor, role: GitRole) -> ActorIdentity {
    ActorIdentity::local(actor, role)
}
/// At most two terminal cells inside the shared four-cell `[initials]` box;
/// whitespace/missing/unsafe names remain `?`, not a fabricated username.
pub fn local_initials(name: &str) -> String {
    if !safe_field(name) || name.trim().is_empty() {
        return "?".into();
    }
    let mut words = name
        .split(|ch: char| ch.is_whitespace() || ch == '-' || ch == '_')
        .filter(|part| !part.is_empty());
    let Some(first) = words.next() else {
        return "?".into();
    };
    let letters = if let Some(last) = words.next_back() {
        first
            .chars()
            .take(1)
            .chain(last.chars().take(1))
            .collect::<String>()
    } else {
        first.chars().take(2).collect()
    };
    let mut text = String::new();
    let mut cells = 0usize;
    for ch in letters.chars().flat_map(char::to_uppercase) {
        // Use the same cell-width implementation as the actual actor widget;
        // no extra dependency or byte-length approximation for wide initials.
        let width = ratatui::text::Span::raw(ch.to_string()).width();
        if width == 0 {
            continue;
        }
        if cells + width > 2 {
            break;
        }
        text.push(ch);
        cells += width;
    }
    if text.is_empty() {
        "?".into()
    } else {
        text
    }
}

#[cfg(test)]
#[path = "git_actor_labels_v1_tests.rs"]
mod tests;
