//! ONE pinned gh --include framing/header grammar, not an HTTP client.
//! Closed profiles preserve the already shipped Review/reaction policies;
//! parsed bytes alone are never source, write authority or a typed receipt.
use super::*;
#[derive(Clone, Copy)]
pub(super) enum ProfileV5 {
    ReviewEmpty204,
    ActionsEmpty202,
    ReactionAdd200Or201,
    ReactionRemove204,
    FileCommentCreated201,
    ActionsDispatchCreated200,
    NotificationPage200Or304,
    NotificationReadEmpty205,
    NotificationSubscription200,
    WorkflowBranch200Or404V9,
    WorkflowPrCreated201V9,
    WorkflowHistory200Or404V9,
}
impl ProfileV5 {
    fn strict_empty(self) -> bool {
        matches!(self, Self::ReviewEmpty204 | Self::ActionsEmpty202)
    }
    fn add(self) -> bool {
        matches!(
            self,
            Self::ReactionAdd200Or201
                | Self::ActionsDispatchCreated200
                | Self::WorkflowBranch200Or404V9
                | Self::WorkflowHistory200Or404V9
        )
    }
    fn file_created(self) -> bool {
        matches!(self, Self::FileCommentCreated201)
    }
    fn created_json(self) -> bool {
        matches!(
            self,
            Self::FileCommentCreated201
                | Self::ActionsDispatchCreated200
                | Self::WorkflowPrCreated201V9
        )
    }
    fn notifications(self) -> bool {
        matches!(
            self,
            Self::NotificationPage200Or304
                | Self::NotificationReadEmpty205
                | Self::NotificationSubscription200
        )
    }
    fn literal_zero(self) -> bool {
        self.strict_empty() || matches!(self, Self::NotificationReadEmpty205)
    }
    fn bad(self, review: &'static str) -> GithubError {
        error(
            if matches!(
                self,
                Self::NotificationPage200Or304
                    | Self::WorkflowBranch200Or404V9
                    | Self::WorkflowHistory200Or404V9
            ) {
                GithubErrorKind::Transport
            } else {
                GithubErrorKind::AmbiguousWrite
            },
            if matches!(
                self,
                Self::WorkflowBranch200Or404V9 | Self::WorkflowHistory200Or404V9
            ) {
                "Original Workflow exact branch HTTP response is unavailable or malformed; no absence inferred"
            } else if matches!(self, Self::WorkflowPrCreated201V9) {
                "Original Workflow PR creation acknowledgement unavailable or malformed; protection retained"
            } else if self.file_created() {
                "File comment HTTP201 acknowledgement is malformed, incomplete or outside its bounded documented shape"
            } else if matches!(self, Self::ActionsDispatchCreated200) {
                "Dispatch HTTP200 acknowledgement is malformed, incomplete or outside its bounded documented shape"
            } else if self.notifications() {
                "Notification HTTP response is malformed, incomplete or outside its exact status/header/body bound"
            } else if self.strict_empty() {
                review
            } else {
                "Reaction HTTP acknowledgement is malformed, incomplete or outside the exact documented status/body shape"
            },
        )
    }
}
pub(super) struct ResponseV5<'a> {
    pub(super) status: u16,
    pub(super) body: &'a str,
    pub(super) notification_poll: NotificationPollHeadersV63<'a>,
}
/// Borrowed observations only, never a validator/source grant or a scheduler.
/// Old profiles deliberately leave these fields empty and retain old policy.
#[derive(Default)]
pub(in crate::providers::gh::repository_catalogue) struct NotificationPollHeadersV63<'a> {
    pub(in crate::providers::gh::repository_catalogue) last_modified: Option<&'a str>,
    pub(in crate::providers::gh::repository_catalogue) etag: Option<&'a str>,
    pub(in crate::providers::gh::repository_catalogue) link: Option<&'a str>,
    pub(in crate::providers::gh::repository_catalogue) poll_interval: Option<&'a str>,
    pub(in crate::providers::gh::repository_catalogue) retry_after: Option<&'a str>,
}
pub(super) fn response(bytes: &[u8], profile: ProfileV5) -> GithubResult<ResponseV5<'_>> {
    let cap = match profile {
        ProfileV5::ReviewEmpty204
        | ProfileV5::ActionsEmpty202
        | ProfileV5::NotificationReadEmpty205
        | ProfileV5::WorkflowBranch200Or404V9 => 16 * 1024,
        ProfileV5::FileCommentCreated201
        | ProfileV5::NotificationPage200Or304
        | ProfileV5::WorkflowPrCreated201V9 => 256 * 1024,
        ProfileV5::ActionsDispatchCreated200 => 64 * 1024,
        _ => 128 * 1024,
    };
    if bytes.is_empty() || bytes.len() > cap {
        return Err(
            profile.bad("Review comment DELETE acknowledgement exceeds its closed header bound")
        );
    }
    let raw = std::str::from_utf8(bytes)
        .map_err(|_| profile.bad("Review comment DELETE returned non-UTF8 HTTP headers"))?;
    // Preserve cli/cli processResponse LF status + printHeaders CRLF exactly,
    // including current uniform fixtures. No control/newline normalization.
    let missing = || profile.bad("Review comment DELETE omitted the complete HTTP header boundary");
    let (status, remaining) = raw.split_once('\n').ok_or_else(missing)?;
    let status_crlf = status.ends_with('\r');
    let status = status.strip_suffix('\r').unwrap_or(status);
    // FILE JSON may itself contain literal CRLF. Its header delimiter is
    // determined from the first actual header line, never body bytes. The
    // original four profiles retain their existing exact delimiter policy.
    let crlf = if profile.created_json() {
        remaining
            .split_once('\n')
            .ok_or_else(missing)?
            .0
            .ends_with('\r')
    } else {
        remaining.contains("\r\n")
    };
    let separator = if crlf { "\r\n" } else { "\n" };
    if status_crlf && separator != "\r\n" {
        return Err(missing());
    }
    let (headers, body) = if let Some(body) = remaining.strip_prefix(separator) {
        ("", body)
    } else {
        remaining
            .split_once(&format!("{separator}{separator}"))
            .ok_or_else(missing)?
    };
    if profile.literal_zero() && !body.is_empty() {
        return Err(profile.bad("HTTP204 must have no content; trailing payload is not a receipt"));
    }
    if profile.created_json()
        && (raw.len().saturating_sub(body.len()) > 16 * 1024
            || body.is_empty()
            || body.len()
                > if matches!(profile, ProfileV5::ActionsDispatchCreated200) {
                    48 * 1024
                } else {
                    240 * 1024
                })
    {
        return Err(
            profile.bad("File comment HTTP201 exceeds its original bounded header/body capture")
        );
    }
    if !profile.strict_empty() && headers.len() > 16 * 1024 {
        return Err(profile.bad("Review-comment HTTP header count/line bound exceeded"));
    }
    if profile.notifications() && raw.len().saturating_sub(body.len()) > 16 * 1024 {
        return Err(
            profile.bad("Notification HTTP framing plus headers exceed their original bound")
        );
    }
    if status.len() > 1024 || status.bytes().any(|b| !matches!(b, 0x20..=0x7e)) {
        return Err(profile.bad("Malformed review-comment HTTP status line"));
    }
    let mut parts = status.splitn(3, ' ');
    if !matches!(
        parts.next(),
        Some("HTTP/1.0" | "HTTP/1.1" | "HTTP/2" | "HTTP/2.0" | "HTTP/3" | "HTTP/3.0")
    ) {
        return Err(profile.bad("Review comment DELETE requires exact documented HTTP204; 200/404/null is not an acknowledgement"));
    }
    let code=match(profile,parts.next()){
        (ProfileV5::ReviewEmpty204|ProfileV5::ReactionRemove204,Some("204"))=>204,
        (ProfileV5::ActionsEmpty202,Some("202"))=>202,
        (ProfileV5::ReactionAdd200Or201,Some("200"))=>200,
        (ProfileV5::ReactionAdd200Or201,Some("201"))=>201,
        (ProfileV5::FileCommentCreated201,Some("201"))=>201,
        (ProfileV5::ActionsDispatchCreated200,Some("200"))=>200,
        (ProfileV5::NotificationPage200Or304 | ProfileV5::NotificationSubscription200,Some("200"))=>200,
        (ProfileV5::NotificationPage200Or304,Some("304"))=>304,
        (ProfileV5::NotificationReadEmpty205,Some("205"))=>205,
        (ProfileV5::WorkflowBranch200Or404V9,Some("200"))=>200,
        (ProfileV5::WorkflowBranch200Or404V9,Some("404"))=>404,
        (ProfileV5::WorkflowHistory200Or404V9,Some("200"))=>200,
        (ProfileV5::WorkflowHistory200Or404V9,Some("404"))=>404,
        (ProfileV5::WorkflowPrCreated201V9,Some("201"))=>201,
        _=>return Err(profile.bad("Review comment DELETE requires exact documented HTTP204; 200/404/null is not an acknowledgement")),
    };
    let mut length = None;
    let mut notification_chunked = false;
    let mut notification_poll = NotificationPollHeadersV63::default();
    for (index, line) in headers
        .split(separator)
        .filter(|line| !line.is_empty())
        .enumerate()
    {
        if index >= 128 || line.len() > 4096 {
            return Err(profile.bad("Review-comment HTTP header count/line bound exceeded"));
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| profile.bad("Malformed review-comment HTTP header"))?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            || value.bytes().any(|b| !matches!(b, b'\t' | 0x20..=0x7e))
        {
            return Err(profile.bad("Invalid review-comment HTTP header bytes"));
        }
        if name.eq_ignore_ascii_case("content-length") {
            let value = value.trim();
            if length.is_some()
                || if profile.literal_zero() {
                    value != "0"
                } else {
                    value.is_empty()
                        || !value.bytes().all(|b| b.is_ascii_digit())
                        || (profile.created_json() || profile.notifications()) && value.len() > 20
                        || profile.notifications()
                            && code != 200
                            && value.len() > 1
                            && value.starts_with('0')
                }
            {
                return Err(profile.bad("HTTP204 content length is not uniquely zero"));
            }
            // FILE is an actual gh decoded/sanitized JSON capture. Its
            // syntactically bounded wire Content-Length is not a stdout-length
            // oracle. Retain only a presence marker to refuse duplicates.
            length = Some(if profile.created_json() {
                0
            } else {
                value
                    .parse::<usize>()
                    .map_err(|_| profile.bad("HTTP204 content length is not uniquely zero"))?
            });
        }
        if name.eq_ignore_ascii_case("transfer-encoding") && !profile.created_json() {
            if matches!(profile, ProfileV5::NotificationPage200Or304)
                && code == 200
                && !notification_chunked
                && value.trim().eq_ignore_ascii_case("chunked")
            {
                // gh returns the already-decoded body. Only this closed200
                // profile admits the exact observed encoding, not old4/304.
                notification_chunked = true;
            } else {
                return Err(profile.bad("HTTP204 must not advertise a transferred body"));
            }
        }
        if matches!(profile, ProfileV5::WorkflowHistory200Or404V9)
            && name.eq_ignore_ascii_case("link")
        {
            // W9 reads exactly page1, never follows arbitrary Link URLs. Even
            // an otherwise short list with ANY pagination metadata is outside
            // this closed complete-page profile; no absence/ACK is inferred.
            // Malformed/duplicate/unknown/next links are all refused here.
            return Err(profile.bad("Workflow historical page has unresolved pagination metadata"));
        }
        if profile.notifications() {
            let (slot, cap) = if name.eq_ignore_ascii_case("last-modified") {
                (&mut notification_poll.last_modified, 128)
            } else if name.eq_ignore_ascii_case("etag") {
                (&mut notification_poll.etag, 1024)
            } else if name.eq_ignore_ascii_case("link") {
                (&mut notification_poll.link, 4096)
            } else if name.eq_ignore_ascii_case("x-poll-interval") {
                (&mut notification_poll.poll_interval, 20)
            } else if name.eq_ignore_ascii_case("retry-after") {
                (&mut notification_poll.retry_after, 128)
            } else {
                continue;
            };
            let value = value.trim();
            if slot.is_some() || value.is_empty() || value.len() > cap {
                return Err(
                    profile.bad("Notification poll metadata is duplicated or outside its bound")
                );
            }
            *slot = Some(value);
        }
    }
    if profile.notifications() {
        if code == 304 {
            // 304 Content-Length describes the selected representation, NOT
            // bytes in this empty response. Never apply 204/equality policy.
            if !body.is_empty() || length.is_some_and(|n| n > 256 * 1024) {
                return Err(
                    profile.bad("Notification304 must be empty and representation length bounded")
                );
            }
        } else if matches!(profile, ProfileV5::NotificationPage200Or304) {
            if notification_chunked && length.is_some() {
                return Err(profile.bad("Notification200 has conflicting transfer/content lengths"));
            }
            // The real installed gh can sanitize/decode JSON before stdout;
            // wire Content-Length is NOT an output-byte equality oracle here.
        } else if length.is_some_and(|n| n != body.len()) {
            return Err(profile.bad("Notification body disagrees with its unique content length"));
        }
    } else if !profile.strict_empty()
        && !profile.created_json()
        && (length.is_some_and(|length| length != body.len()) || !profile.add() && !body.is_empty())
    {
        return Err(profile.bad("HTTP204 must have no content; trailing payload is not a receipt"));
    }
    Ok(ResponseV5 {
        status: code,
        body,
        notification_poll,
    })
}
#[cfg(test)]
#[path = "dispatch_http_profile_v7_tests.rs"]
mod dispatch_profile_v7_tests;
#[cfg(test)]
#[path = "file_comment_http_profile_v76_tests.rs"]
mod file_profile_v76_tests;
#[cfg(test)]
#[path = "pinned_http_response_v5_tests.rs"]
mod tests;
