use super::model::*;
use super::*;
use crate::providers::gh::search::canonical_source_url_matches;

pub(in crate::providers::gh::repository_catalogue) fn timestamp(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return false;
    }
    if ![0..4, 5..7, 8..10, 11..13, 14..16, 17..19]
        .into_iter()
        .all(|r| b[r].iter().all(u8::is_ascii_digit))
    {
        return false;
    }
    let n = |r: std::ops::Range<usize>| value[r].parse::<u32>().unwrap_or(u32::MAX);
    let (year, month, day, hour, min, sec) =
        (n(0..4), n(5..7), n(8..10), n(11..13), n(14..16), n(17..19));
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    year > 0 && day > 0 && day <= days && hour < 24 && min < 60 && sec < 60
}
fn text<'a>(v: &'a Value, key: &str, cap: usize) -> GithubResult<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= cap)
        .ok_or_else(|| bad_v63(GithubErrorKind::Json))
}
fn identity_text<'a>(v: &'a Value, key: &str, cap: usize) -> GithubResult<&'a str> {
    text(v, key, cap).and_then(|s| {
        if s.chars().any(char::is_control) {
            Err(bad_v63(GithubErrorKind::Json))
        } else {
            Ok(s)
        }
    })
}
fn positive(v: &Value, key: &str) -> GithubResult<u64> {
    v.get(key)
        .and_then(Value::as_u64)
        .filter(|n| *n > 0 && *n <= i64::MAX as u64)
        .ok_or_else(|| bad_v63(GithubErrorKind::Json))
}
pub(in crate::providers::gh::repository_catalogue) fn canonical_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('0')
        && value.bytes().all(|b| b.is_ascii_digit())
}
fn node(v: &Value, key: &str) -> GithubResult<String> {
    let s = identity_text(v, key, 512)?;
    if !s
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"_=-".contains(&b))
    {
        return Err(bad_v63(GithubErrorKind::Json));
    }
    Ok(s.to_owned())
}
fn exact_url(host: &str, url: &str, path: &str) -> GithubResult<()> {
    if canonical_source_url_matches(host, url, path, true, false) {
        Ok(())
    } else {
        Err(bad_v63(GithubErrorKind::InvalidTarget))
    }
}
fn repo_name(value: &str) -> bool {
    let Some((owner, name)) = value.split_once('/') else {
        return false;
    };
    !owner.is_empty()
        && !name.is_empty()
        && owner.len() <= 100
        && name.len() <= 100
        && owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        && name != "."
        && name != ".."
}
pub(in crate::providers::gh::repository_catalogue) fn repository(
    host: &str,
    v: &Value,
) -> GithubResult<NotificationRepositoryV63> {
    let id = positive(v, "id")?;
    let repo_node = node(v, "node_id")?;
    let name = identity_text(v, "full_name", 201)?;
    if !repo_name(name) {
        return Err(bad_v63(GithubErrorKind::Json));
    }
    exact_url(host, text(v, "url", 4096)?, &format!("repos/{name}"))?;
    let private = v
        .get("private")
        .and_then(Value::as_bool)
        .ok_or_else(|| bad_v63(GithubErrorKind::Json))?;
    let owner = match v.get("owner") {
        None | Some(Value::Null) => None,
        Some(raw) => {
            let login = identity_text(raw, "login", 100)?;
            if !name
                .split('/')
                .next()
                .is_some_and(|s| s.eq_ignore_ascii_case(login))
            {
                return Err(bad_v63(GithubErrorKind::IdentityChanged));
            }
            let kind = match text(raw, "type", 32)? {
                "User" => RepositoryCatalogueOwnerKind::User,
                "Organization" => RepositoryCatalogueOwnerKind::Organization,
                _ => return Err(bad_v63(GithubErrorKind::Json)),
            };
            Some(RepositoryCatalogueOwner {
                id: positive(raw, "id")?,
                node_id: Some(node(raw, "node_id")?),
                login: login.to_owned(),
                kind,
            })
        }
    };
    Ok(NotificationRepositoryV63 {
        id,
        node: repo_node,
        full_name: name.to_owned(),
        private,
        owner,
    })
}
fn subject(
    host: &str,
    repo: &NotificationRepositoryV63,
    v: &Value,
) -> GithubResult<(
    NotificationSubjectV63,
    String,
    Option<String>,
    Option<String>,
)> {
    let kind = identity_text(v, "type", 128)?;
    let title = text(v, "title", 4096)?.to_owned();
    let nullable = |key: &str| -> GithubResult<Option<&str>> {
        match v.get(key) {
            Some(Value::Null) => Ok(None),
            Some(Value::String(value))
                if value.len() <= 4096 && !value.chars().any(char::is_control) =>
            {
                Ok(Some(value))
            }
            _ => Err(bad_v63(GithubErrorKind::Json)),
        }
    };
    let url = nullable("url")?;
    let latest = nullable("latest_comment_url")?;
    let Some(url) = url else {
        return Ok((
            NotificationSubjectV63::Unresolved {
                observed_kind: kind.into(),
            },
            title,
            None,
            latest.map(str::to_owned),
        ));
    };
    let path_base = format!("repos/{}/", repo.full_name);
    let parse_tail = |segment: &str| -> GithubResult<u64> {
        let marker = format!("/{path_base}{segment}/");
        let tail = url
            .rsplit_once(&marker)
            .map(|(_, s)| s)
            .filter(|s| canonical_id(s))
            .ok_or_else(|| bad_v63(GithubErrorKind::InvalidTarget))?;
        let n = tail
            .parse::<u64>()
            .ok()
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(|| bad_v63(GithubErrorKind::Json))?;
        exact_url(host, url, &format!("{path_base}{segment}/{n}"))?;
        Ok(n)
    };
    let subject = match kind {
        "PullRequest" => NotificationSubjectV63::PullRequest {
            number: parse_tail("pulls")?,
        },
        "Issue" => NotificationSubjectV63::Issue {
            number: parse_tail("issues")?,
        },
        "CheckSuite" => NotificationSubjectV63::CheckSuite {
            id: parse_tail("check-suites")?,
        },
        "Commit" => {
            let sha = url
                .rsplit('/')
                .next()
                .filter(|s| {
                    s.len() == 40
                        && s.bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
                .ok_or_else(|| bad_v63(GithubErrorKind::InvalidTarget))?;
            exact_url(host, url, &format!("{path_base}commits/{sha}"))?;
            NotificationSubjectV63::Commit {
                sha: sha.to_owned(),
            }
        }
        _ => {
            // Unknown kinds remain visible as bounded observations, but their
            // URL is NEVER followed or promoted into a PR/run producer.
            if url.chars().any(char::is_control) {
                return Err(bad_v63(GithubErrorKind::InvalidTarget));
            }
            NotificationSubjectV63::Other {
                observed_kind: kind.to_owned(),
            }
        }
    };
    Ok((
        subject,
        title,
        Some(url.to_owned()),
        latest.map(str::to_owned),
    ))
}
/// The latest-activity read for a row, as a closed same-repository endpoint.
/// Only a comment (issue, review or commit) or the subject itself qualifies;
/// any other or foreign URL is never followed.
pub(in crate::providers::gh::repository_catalogue) fn latest_comment_endpoint(
    host: &str,
    row: &NotificationRowV63,
) -> Option<(String, bool)> {
    let url = row.latest_comment_url.as_deref()?;
    let base = format!("repos/{}/", row.repository.full_name);
    let marker = format!("/{base}");
    let (_, tail) = url.rsplit_once(&marker)?;
    let parts = tail.split('/').collect::<Vec<_>>();
    let valid = match parts.as_slice() {
        ["issues", "comments", id] | ["pulls", "comments", id] | ["comments", id] => {
            canonical_id(id)
        }
        ["pulls", n] | ["issues", n] => canonical_id(n),
        _ => false,
    };
    let endpoint = format!("{base}{tail}");
    if !valid || exact_url(host, url, &endpoint).is_err() {
        return None;
    }
    let is_subject = row.subject_url.as_deref() == Some(url);
    Some((endpoint, is_subject))
}
/// The `user` of a latest-comment (or subject) response. Bounded and strict:
/// a missing, empty or control-bearing login is no actor at all.
pub(in crate::providers::gh::repository_catalogue) fn latest_actor(
    v: &Value,
) -> GithubResult<crate::providers::gh::Actor> {
    let user = v
        .get("user")
        .filter(|u| u.is_object())
        .ok_or_else(|| bad_v63(GithubErrorKind::Json))?;
    let login = identity_text(user, "login", 100)?;
    if !login
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-[]_".contains(&b))
    {
        return Err(bad_v63(GithubErrorKind::Json));
    }
    let kind = user
        .get("type")
        .and_then(Value::as_str)
        .filter(|kind| matches!(*kind, "User" | "Bot" | "Organization"))
        .unwrap_or("Unknown");
    Ok(crate::providers::gh::Actor {
        login: login.to_owned(),
        display_name: None,
        avatar_url: None,
        actor_type: kind.into(),
    })
}
pub(in crate::providers::gh::repository_catalogue) fn row(
    host: &str,
    v: &Value,
) -> GithubResult<NotificationRowV63> {
    let id = text(v, "id", 64)?;
    if !canonical_id(id) {
        return Err(bad_v63(GithubErrorKind::Json));
    }
    exact_url(
        host,
        text(v, "url", 4096)?,
        &format!("notifications/threads/{id}"),
    )?;
    exact_url(
        host,
        text(v, "subscription_url", 4096)?,
        &format!("notifications/threads/{id}/subscription"),
    )?;
    let repository = repository(host, &v["repository"])?;
    let (subject, title, subject_url, latest_comment_url) =
        subject(host, &repository, &v["subject"])?;
    let updated = text(v, "updated_at", 20)?;
    if !timestamp(updated) {
        return Err(bad_v63(GithubErrorKind::Json));
    }
    let last_read_at = match v.get("last_read_at") {
        Some(Value::Null) => None,
        Some(Value::String(s)) if timestamp(s) => Some(s.clone()),
        _ => return Err(bad_v63(GithubErrorKind::Json)),
    };
    let reason = identity_text(v, "reason", 128)?.to_owned();
    let unread = v
        .get("unread")
        .and_then(Value::as_bool)
        .ok_or_else(|| bad_v63(GithubErrorKind::Json))?;
    Ok(NotificationRowV63 {
        id: id.to_owned(),
        repository,
        title,
        subject,
        subject_url,
        latest_comment_url,
        reason,
        unread,
        updated_at: updated.to_owned(),
        last_read_at,
    })
}
pub(in crate::providers::gh::repository_catalogue) fn page(
    host: &str,
    raw: &[u8],
    remaining_metadata: usize,
) -> GithubResult<NotificationPageV63> {
    if raw.len() > PAGE_CAP_V63 {
        return Err(bad_v63(GithubErrorKind::Transport));
    }
    let value: Value = serde_json::from_slice(raw).map_err(|_| bad_v63(GithubErrorKind::Json))?;
    let values = value
        .as_array()
        .filter(|r| r.len() <= 50)
        .ok_or_else(|| bad_v63(GithubErrorKind::Json))?;
    let mut rows = Vec::with_capacity(values.len());
    let mut retained = 0usize;
    for raw in values {
        let row = row(host, raw)?;
        retained = retained
            .checked_add(row.retained_bytes())
            .ok_or_else(|| bad_v63(GithubErrorKind::Transport))?;
        if retained > remaining_metadata.min(METADATA_CAP_V63) {
            return Err(bad_v63(GithubErrorKind::Transport));
        }
        rows.push(row);
    }
    retained = retained
        .checked_add(rows.capacity() * std::mem::size_of::<NotificationRowV63>())
        .ok_or_else(|| bad_v63(GithubErrorKind::Transport))?;
    if retained > remaining_metadata.min(METADATA_CAP_V63) {
        return Err(bad_v63(GithubErrorKind::Transport));
    }
    Ok(NotificationPageV63 {
        raw_rows: values.len(),
        exhausted: values.len() < 50,
        rows,
        retained_bytes: retained,
    })
}
