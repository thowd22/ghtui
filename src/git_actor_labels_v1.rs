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
/// Compact daily-use byline: one name when author and committer are the same
/// local actor, both explicit roles otherwise, plus the authored UTC date.
pub fn commit_summary(commit: &Commit) -> String {
    let date = utc_date(commit.author.timestamp);
    if commit.author.name == commit.committer.name && commit.author.email == commit.committer.email
    {
        format!("{} • {date}", short_actor(&commit.author))
    } else {
        format!(
            "author {} • committer {} • {date}",
            short_actor(&commit.author),
            short_actor(&commit.committer)
        )
    }
}
/// A readable local name; the full name/email remains in Details and the
/// author/committer header. Missing or display-limited names keep the
/// complete honest fallback text.
fn short_actor(actor: &GitActor) -> String {
    if display_limited(&actor.name, Some(&actor.email)) || actor.name.trim().is_empty() {
        actor_text(actor)
    } else {
        actor.name.clone()
    }
}
/// Calendar date (UTC) for a Git Unix timestamp, e.g. `2023-11-14`.
pub fn utc_date(timestamp: i64) -> String {
    let (year, month, day) = civil_from_days(timestamp.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}
/// Date and minute (UTC) for a Git Unix timestamp, e.g. `2023-11-14 22:13 UTC`.
pub fn utc_datetime(timestamp: i64) -> String {
    let seconds = timestamp.rem_euclid(86_400);
    format!(
        "{} {:02}:{:02} UTC",
        utc_date(timestamp),
        seconds / 3600,
        seconds % 3600 / 60
    )
}
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Howard Hinnant's days-to-civil algorithm (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
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
