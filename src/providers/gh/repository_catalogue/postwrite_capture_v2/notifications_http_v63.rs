//! Notifications-only admission over the ONE pinned gh header grammar.
//! HTTP observations are not source, membership, permission or a write receipt.
pub(in crate::providers::gh::repository_catalogue) use super::pinned_http_response_v5::NotificationPollHeadersV63;
use super::pinned_http_response_v5::{response, ProfileV5};
use super::*;

pub(in crate::providers::gh::repository_catalogue) enum NotificationHttpPageV63<'a> {
    Fresh {
        body: &'a str,
        poll: NotificationPollHeadersV63<'a>,
    },
    NotModified {
        poll: NotificationPollHeadersV63<'a>,
    },
}
impl std::fmt::Debug for NotificationHttpPageV63<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "NotificationHttpPageV63(<bounded protocol observation; private bytes redacted>)",
        )
    }
}

/// `exit` must come ONLY from the closed normally-reaped runner result, never
/// a synthesized status, failure diagnostic, cached record or caller string.
pub(in crate::providers::gh::repository_catalogue) fn page(
    bytes: &[u8],
    exit: i32,
) -> GithubResult<NotificationHttpPageV63<'_>> {
    let parsed = response(bytes, ProfileV5::NotificationPage200Or304)?;
    match (exit, parsed.status) {
        (0, 200) => Ok(NotificationHttpPageV63::Fresh {
            body: parsed.body,
            poll: parsed.notification_poll,
        }),
        (1, 304) => Ok(NotificationHttpPageV63::NotModified {
            poll: parsed.notification_poll,
        }),
        _ => Err(error(
            GithubErrorKind::Transport,
            "Notification conditional GET exit/status disagree; no cached/private rows admitted",
        )),
    }
}

pub(in crate::providers::gh::repository_catalogue) fn read_ack(bytes: &[u8]) -> GithubResult<()> {
    response(bytes, ProfileV5::NotificationReadEmpty205).map(|_| ())
}
pub(in crate::providers::gh::repository_catalogue) fn subscription(
    bytes: &[u8],
) -> GithubResult<Value> {
    let parsed = response(bytes, ProfileV5::NotificationSubscription200)?;
    let value: Value = serde_json::from_str(parsed.body).map_err(|_| {
        error(
            GithubErrorKind::AmbiguousWrite,
            "Notification subscription acknowledgement is not a bounded JSON object",
        )
    })?;
    if !value.is_object() {
        return Err(error(
            GithubErrorKind::AmbiguousWrite,
            "Notification subscription acknowledgement is not an object",
        ));
    }
    Ok(value)
}

/// Purpose-closed transport only: the opaque origin is produced by the real
/// notification factory. HTTP alone is never a publication receipt; the same
/// fixed cursor must still read the exact target and close numeric visibility.
impl PostWriteCaptureAllowanceV2 {
    pub(in crate::providers::gh::repository_catalogue) fn notification_write_ack_v63(
        &mut self,
        provider: &GithubProvider,
        origin: &notifications_v63::NotificationWriteOriginV63,
        requested: crate::storage::NotificationRequestedV63,
        cancel: &CancellationToken,
    ) -> GithubResult<Option<Value>> {
        if origin.scope().host != provider.host
            || !origin.matches_runner_v63(&provider.runner)
            || !origin.phase_matches(cancel, self.deadline())
            || !notifications_v63::parse::canonical_id(origin.thread())
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Genuine original notification host/target/token required",
            ));
        }
        let (endpoint, method, input, cap) = match requested {
            crate::storage::NotificationRequestedV63::MarkRead => (
                format!("notifications/threads/{}", origin.thread()),
                "PATCH",
                None,
                16 * 1024,
            ),
            crate::storage::NotificationRequestedV63::Ignore => (
                format!("notifications/threads/{}/subscription", origin.thread()),
                "PUT",
                Some(json!({"ignored":true})),
                128 * 1024,
            ),
            crate::storage::NotificationRequestedV63::Unignore => (
                format!("notifications/threads/{}/subscription", origin.thread()),
                "PUT",
                Some(json!({"ignored":false})),
                128 * 1024,
            ),
        };
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let (deadline, original_cap) = bounded.scoped_read_window.expect("fixed original slot");
        let bounded = bounded.with_scoped_read_window(deadline, original_cap.min(cap))?;
        let resource = origin.runner_resource_v63();
        let mut args = vec![
            "api".into(),
            endpoint,
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            method.into(),
            "--include".into(),
        ];
        if input.is_some() {
            args.extend(["--input".into(), "-".into()]);
        }
        let result = (|| {
            let raw = bounded.execute(args, input, Some(resource.clone()), &self.cancellation)?;
            self.check_issuer(provider, cancel)?;
            let result = if requested == crate::storage::NotificationRequestedV63::MarkRead {
                read_ack(&raw)?;
                None
            } else {
                Some(subscription(&raw)?)
            };
            self.check_issuer(provider, cancel)?;
            Ok(result)
        })();
        if result.is_err() {
            provider.runner.mark_ambiguous(&resource);
        }
        result
    }
}

#[cfg(test)]
#[path = "notifications_http_v63_tests.rs"]
mod tests;
