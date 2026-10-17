//! Pure bounded polling state. No transport, implicit mutation or source grant.
use super::*;
use current_access_v1::notifications_http_v63::NotificationPollHeadersV63;

#[derive(Clone, Default, PartialEq, Eq)]
pub(in crate::providers::gh::repository_catalogue) struct NotificationValidatorsV63 {
    pub(in crate::providers::gh::repository_catalogue) last_modified: Option<String>,
    pub(in crate::providers::gh::repository_catalogue) etag: Option<String>,
}
impl std::fmt::Debug for NotificationValidatorsV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationValidatorsV63(<private conditional validators redacted>)")
    }
}
pub(in crate::providers::gh::repository_catalogue) struct NotificationPollMetadataV63 {
    pub(in crate::providers::gh::repository_catalogue) validators: NotificationValidatorsV63,
    pub(in crate::providers::gh::repository_catalogue) interval_seconds: Option<u64>,
    pub(in crate::providers::gh::repository_catalogue) retry_seconds: u64,
}
fn decimal(value: &str) -> Option<u64> {
    if value.is_empty()
        || value.len() > 20
        || value.len() > 1 && value.starts_with('0')
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        None
    } else {
        value.parse().ok()
    }
}

/// Exact IMF-fixdate, as emitted by documented Last-Modified/Retry-After.
/// This is header validation, not a replacement JSON/provider date parser.
fn http_date(value: &str) -> Option<u64> {
    let b = value.as_bytes();
    if !value.is_ascii()
        || b.len() != 29
        || &b[3..5] != b", "
        || b[7] != b' '
        || b[11] != b' '
        || b[16] != b' '
        || b[19] != b':'
        || b[22] != b':'
        || &b[25..] != b" GMT"
    {
        return None;
    }
    let week = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
        .iter()
        .position(|w| *w == &value[..3])?;
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|m| *m == &value[8..11])? as i64
        + 1;
    for range in [5..7, 12..16, 17..19, 20..22, 23..25] {
        if !b[range].iter().all(u8::is_ascii_digit) {
            return None;
        }
    }
    let year = value[12..16].parse::<i64>().ok()?;
    let day = value[5..7].parse::<i64>().ok()?;
    let iso = format!("{year:04}-{month:02}-{day:02}T{}Z", &value[17..25]);
    if year < 1970 || !parse::timestamp(&iso) {
        return None;
    }
    // Gregorian civil date -> days since Unix epoch; no platform libc/timezone.
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = y / 400;
    let yoe = y - era * 400;
    let m = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    if days < 0 || ((days + 4) % 7) as usize != week {
        return None;
    }
    let h = value[17..19].parse::<u64>().ok()?;
    let min = value[20..22].parse::<u64>().ok()?;
    let sec = value[23..25].parse::<u64>().ok()?;
    Some(days as u64 * 86400 + h * 3600 + min * 60 + sec)
}
fn etag(value: &str) -> bool {
    let s = value.strip_prefix("W/").unwrap_or(value);
    s.len() >= 2
        && s.len() <= 1024
        && s.starts_with('"')
        && s.ends_with('"')
        && s[1..s.len() - 1]
            .bytes()
            .all(|b| matches!(b, 0x21 | 0x23..=0x7e))
}
pub(in crate::providers::gh::repository_catalogue) fn metadata(
    headers: &NotificationPollHeadersV63<'_>,
    now_unix: u64,
) -> GithubResult<NotificationPollMetadataV63> {
    let last_modified = match headers.last_modified {
        Some(v) if http_date(v).is_some() => Some(v.to_owned()),
        None => None,
        _ => return Err(bad_v63(GithubErrorKind::Json)),
    };
    let tag = match headers.etag {
        Some(v) if etag(v) => Some(v.to_owned()),
        None => None,
        _ => return Err(bad_v63(GithubErrorKind::Json)),
    };
    let interval_seconds = match headers.poll_interval {
        Some(v) => Some(decimal(v).ok_or_else(|| bad_v63(GithubErrorKind::Json))?),
        None => None,
    };
    let retry_seconds = match headers.retry_after {
        None => 0,
        Some(v) => match decimal(v) {
            Some(s) => s,
            None => http_date(v)
                .ok_or_else(|| bad_v63(GithubErrorKind::Json))?
                .saturating_sub(now_unix),
        },
    };
    Ok(NotificationPollMetadataV63 {
        validators: NotificationValidatorsV63 {
            last_modified,
            etag: tag,
        },
        interval_seconds,
        retry_seconds,
    })
}

/// Follow only a verified same-query next page, generated locally; never pass
/// the server Link URL to gh. Parameter order may differ, identity may not.
pub(in crate::providers::gh::repository_catalogue) fn next_page(
    host: &str,
    query: &NotificationQueryV63,
    current: usize,
    link: Option<&str>,
) -> GithubResult<Option<usize>> {
    let Some(link) = link else {
        return Ok(None);
    };
    if link.len() > 4096 || current >= 4 {
        return Err(bad_v63(GithubErrorKind::InvalidTarget));
    }
    let mut next = None;
    for (index, entry) in link.split(',').enumerate() {
        if index >= 4 {
            return Err(bad_v63(GithubErrorKind::InvalidTarget));
        }
        let (url, rel) = entry
            .trim()
            .split_once(';')
            .ok_or_else(|| bad_v63(GithubErrorKind::Json))?;
        let url = url
            .trim()
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'))
            .ok_or_else(|| bad_v63(GithubErrorKind::Json))?;
        let rel = rel
            .trim()
            .strip_prefix("rel=\"")
            .and_then(|s| s.strip_suffix('"'))
            .ok_or_else(|| bad_v63(GithubErrorKind::Json))?;
        if !matches!(rel, "next" | "prev" | "first" | "last") {
            return Err(bad_v63(GithubErrorKind::Json));
        }
        if rel != "next" {
            continue;
        }
        if next.is_some() {
            return Err(bad_v63(GithubErrorKind::Json));
        }
        let (base, params) = url
            .split_once('?')
            .ok_or_else(|| bad_v63(GithubErrorKind::InvalidTarget))?;
        if !crate::providers::gh::search::canonical_source_url_matches(
            host,
            base,
            "notifications",
            true,
            false,
        ) {
            return Err(bad_v63(GithubErrorKind::InvalidTarget));
        }
        let normalize = |s: &str| -> GithubResult<BTreeMap<String, String>> {
            let mut map = BTreeMap::new();
            for (i, item) in s.split('&').enumerate() {
                if i >= 6 {
                    return Err(bad_v63(GithubErrorKind::InvalidTarget));
                }
                let (k, v) = item
                    .split_once('=')
                    .ok_or_else(|| bad_v63(GithubErrorKind::Json))?;
                let v = v.replace("%3A", ":").replace("%3a", ":");
                if !matches!(
                    k,
                    "all" | "participating" | "per_page" | "page" | "since" | "before"
                ) || v.contains('%')
                    || map.insert(k.to_owned(), v).is_some()
                {
                    return Err(bad_v63(GithubErrorKind::InvalidTarget));
                }
            }
            Ok(map)
        };
        let expected = query.endpoint(current + 1)?;
        let expected = expected.split_once('?').expect("closed generated query").1;
        if normalize(params)? != normalize(expected)? {
            return Err(bad_v63(GithubErrorKind::InvalidTarget));
        }
        next = Some(current + 1);
    }
    Ok(next)
}

pub(crate) struct NotificationWatchV63 {
    configured: Duration,
    not_before: Option<Instant>,
    remaining: u8,
    in_flight: bool,
    stopped: bool,
}
impl NotificationWatchV63 {
    pub(crate) fn new(configured_seconds: u64) -> GithubResult<Self> {
        if !(15..=86400).contains(&configured_seconds) {
            return Err(bad_v63(GithubErrorKind::InvalidTarget));
        }
        Ok(Self {
            configured: Duration::from_secs(configured_seconds),
            not_before: None,
            remaining: 30,
            in_flight: false,
            stopped: false,
        })
    }
    pub(crate) fn claim_due(&mut self, now: Instant) -> bool {
        if self.stopped
            || self.in_flight
            || self.remaining == 0
            || self.not_before.is_some_and(|t| now < t)
        {
            return false;
        }
        self.remaining -= 1;
        self.in_flight = true;
        true
    }
    pub(in crate::providers::gh::repository_catalogue) fn complete(
        &mut self,
        now: Instant,
        poll: &NotificationPollMetadataV63,
        jitter_seed: u64,
    ) -> GithubResult<()> {
        if !self.in_flight {
            return Err(bad_v63(GithubErrorKind::InvalidTarget));
        }
        self.in_flight = false;
        let base = self
            .configured
            .as_secs()
            .max(poll.interval_seconds.unwrap_or(0))
            .max(poll.retry_seconds);
        let jitter = jitter_seed % (self.configured.as_secs() / 10).clamp(1, 5);
        let delay = base
            .checked_add(jitter)
            .and_then(|s| now.checked_add(Duration::from_secs(s)));
        if delay.is_none() {
            self.stopped = true;
            return Err(bad_v63(GithubErrorKind::Transport));
        }
        self.not_before = delay;
        Ok(())
    }
    pub(crate) fn stop(&mut self) {
        self.stopped = true;
    }
    pub(crate) fn remaining(&self) -> u8 {
        self.remaining
    }
    pub(crate) fn in_flight_v63(&self) -> bool {
        self.in_flight
    }
}
