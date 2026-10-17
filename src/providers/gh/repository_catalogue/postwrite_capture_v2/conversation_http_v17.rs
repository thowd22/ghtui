//! Closed IssueComment transport, sharing the ONE pinned HTTP grammar.
//! This consumes the existing activated prepaid cursor; no raw getter/client,
//! arbitrary endpoint, retry or source/publication authority is exposed.
use super::*;
use crate::providers::gh::{ConversationHttpAckV17, ConversationHttpRequestV17};
impl PostWriteCaptureAllowanceV2 {
    pub(crate) fn conversation_http_ack_v17(
        &mut self,
        provider: &GithubProvider,
        original: &PrResourceParentV1,
        request: ConversationHttpRequestV17<'_>,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationHttpAckV17> {
        use pinned_http_response_v5::ProfileV5;
        if original.summary().pr.host != provider.host {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual original IssueComment host required",
            ));
        }
        let (endpoint, method, input, profile, comment_id, reaction_id) = match request {
            ConversationHttpRequestV17::DeleteComment { comment_id } => (
                format!(
                    "repos/{}/issues/comments/{comment_id}",
                    original.repository()
                ),
                "DELETE",
                None,
                ProfileV5::ReviewEmpty204,
                comment_id,
                0,
            ),
            ConversationHttpRequestV17::AddReaction {
                comment_id,
                content,
            } if crate::providers::gh::ThreadReactionContentV3::parse(content).is_some() => (
                format!(
                    "repos/{}/issues/comments/{comment_id}/reactions",
                    original.repository()
                ),
                "POST",
                Some(json!({"content":content})),
                ProfileV5::ReactionAdd200Or201,
                comment_id,
                0,
            ),
            ConversationHttpRequestV17::RemoveReaction {
                comment_id,
                reaction_id,
            } if (1..=i64::MAX as u64).contains(&reaction_id) => (
                format!(
                    "repos/{}/issues/comments/{comment_id}/reactions/{reaction_id}",
                    original.repository()
                ),
                "DELETE",
                None,
                ProfileV5::ReactionRemove204,
                comment_id,
                reaction_id,
            ),
            _ => {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Exact known issue-comment reaction required",
                ))
            }
        };
        if !(1..=i64::MAX as u64).contains(&comment_id) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual positive IssueComment REST ID required",
            ));
        }
        if matches!(
            profile,
            ProfileV5::NotificationPage200Or304
                | ProfileV5::NotificationReadEmpty205
                | ProfileV5::NotificationSubscription200
        ) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Notification HTTP profile cannot issue a Conversation publication",
            ));
        }
        if matches!(
            profile,
            ProfileV5::WorkflowBranch200Or404V9
                | ProfileV5::WorkflowPrCreated201V9
                | ProfileV5::WorkflowHistory200Or404V9
        ) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Workflow HTTP profile cannot issue a Conversation publication",
            ));
        }
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let (deadline, cap) = bounded.scoped_read_window.expect("original prepaid slot");
        let bounded = bounded.with_scoped_read_window(deadline, cap.min(128 * 1024))?;
        let resource = provider.resource(original.repository(), original.number());
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
            let bytes = bounded.execute(args, input, Some(resource.clone()), &self.cancellation)?;
            self.check_issuer(provider, cancel)?;
            let response = pinned_http_response_v5::response(&bytes, profile)?;
            let ack = match profile {
                ProfileV5::ReviewEmpty204 => ConversationHttpAckV17::DeletedComment { comment_id },
                ProfileV5::ReactionRemove204 => {
                    ConversationHttpAckV17::RemovedReaction { reaction_id }
                }
                ProfileV5::ReactionAdd200Or201 => {
                    let value: Value = serde_json::from_str(response.body).map_err(|_| {
                        error(
                            GithubErrorKind::AmbiguousWrite,
                            "Reaction acknowledgement JSON unavailable",
                        )
                    })?;
                    if !value.is_object() {
                        return Err(error(
                            GithubErrorKind::AmbiguousWrite,
                            "Reaction acknowledgement must be one JSON object",
                        ));
                    }
                    ConversationHttpAckV17::AddedReaction {
                        status: response.status,
                        value,
                    }
                }
                ProfileV5::ActionsEmpty202
                | ProfileV5::FileCommentCreated201
                | ProfileV5::ActionsDispatchCreated200 => {
                    unreachable!("closed IssueComment request profiles")
                }
                ProfileV5::NotificationPage200Or304
                | ProfileV5::NotificationReadEmpty205
                | ProfileV5::NotificationSubscription200 => {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "Notification HTTP profile cannot admit a Conversation receipt",
                    ));
                }
                ProfileV5::WorkflowBranch200Or404V9
                | ProfileV5::WorkflowPrCreated201V9
                | ProfileV5::WorkflowHistory200Or404V9 => {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "Workflow HTTP profile cannot admit a Conversation receipt",
                    ));
                }
            };
            self.check_issuer(provider, cancel)?;
            Ok(ack)
        })();
        if result.is_err() {
            provider.runner.mark_ambiguous(&resource);
        }
        result
    }
}
