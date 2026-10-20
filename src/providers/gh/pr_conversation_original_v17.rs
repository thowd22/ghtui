//! Purpose-closed IssueComment observations on the ONE original measured Core.
//! No Deserialize, DTO constructor, provider/window escape or write permission.
use super::*;
use crate::pr_actions::PrOperation;
use crate::providers::runner::ScopedReadBudgetV1;
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Instant};
#[path = "pr_conversation_original_v17/publication.rs"]
mod publication;
#[path = "pr_conversation_original_v17/read.rs"]
mod read;
#[path = "pr_conversation_original_v17/recovery.rs"]
mod recovery;
pub(crate) use recovery::ConversationRecoveryCurrentV17;
#[path = "pr_conversation_original_v17/refresh_v19.rs"]
mod refresh_v19;
pub(crate) use refresh_v19::*;

pub(crate) const CONVERSATION_COMMENT_QUERY_V17: &str = "query($id:ID!){node(id:$id){__typename ... on IssueComment{id fullDatabaseId body createdAt updatedAt viewerCanUpdate viewerCanDelete viewerCanReact viewerDidAuthor repository{id nameWithOwner} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id nameWithOwner}}}}}";
pub(crate) const CONVERSATION_ABSENCE_QUERY_V17: &str =
    "query($id:ID!){node(id:$id){__typename ... on IssueComment{id}}}";
pub(crate) const CONVERSATION_BODY_CAP_V17: usize = 65_536;
pub(crate) const CONVERSATION_DRAFT_CAP_V17: usize = 128 * 1024;
pub(crate) const CONVERSATION_COMMENT_POST_CAPS_V17: [usize; 6] = [
    1024 * 1024,
    1024 * 1024,
    64 * 1024,
    128 * 1024,
    128 * 1024,
    64 * 1024,
];
pub(crate) const CONVERSATION_DELETE_POST_CAPS_V17: [usize; 6] = [
    64 * 1024,
    128 * 1024,
    64 * 1024,
    128 * 1024,
    128 * 1024,
    64 * 1024,
];
pub(crate) const CONVERSATION_REACTION_POST_CAPS_V17: [usize; 9] = [256 * 1024; 9];
pub(crate) const CONVERSATION_SUBSCRIPTION_POST_CAPS_V17: [usize; 6] = [
    128 * 1024,
    128 * 1024,
    64 * 1024,
    128 * 1024,
    128 * 1024,
    64 * 1024,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConversationCapabilitiesV17 {
    pub(crate) can_update: Option<bool>,
    pub(crate) can_delete: Option<bool>,
    pub(crate) can_react: Option<bool>,
    pub(crate) did_author: Option<bool>,
}
pub(crate) struct OriginalConversationCommentV17 {
    parent: Arc<PrResourceParentV1>,
    identity: HostIdentity,
    observed: ConversationComment,
    author_account: Option<u64>,
    capabilities: ConversationCapabilitiesV17,
    measured: Arc<ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
}
impl std::fmt::Debug for OriginalConversationCommentV17 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "OriginalConversationCommentV17(<actual IssueComment/REST parent; no permission>)",
        )
    }
}
impl OriginalConversationCommentV17 {
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn observed(&self) -> &ConversationComment {
        &self.observed
    }
    pub(crate) fn author_account(&self) -> Option<u64> {
        self.author_account
    }
    pub(crate) fn capabilities(&self) -> ConversationCapabilitiesV17 {
        self.capabilities
    }
    pub(crate) fn selected(&self) -> crate::pr_actions::SelectedComment {
        crate::pr_actions::SelectedComment {
            kind: CommentKind::Conversation,
            observed: self.observed.clone(),
        }
    }
    pub(crate) fn require(&self, command: crate::pr_actions::PrCommand) -> GithubResult<()> {
        use crate::pr_actions::PrCommand::*;
        let allowed = match command {
            EditComment => self.capabilities.can_update,
            DeleteComment => self.capabilities.can_delete,
            AddReaction | RemoveReaction => self.capabilities.can_react,
            _ => return Err(bad(GithubErrorKind::InvalidTarget)),
        };
        if allowed != Some(true) {
            return Err(error(GithubErrorKind::Permission,
            "Actual selected IssueComment capability is false or unknown; no login/repository-role substitution"));
        }
        Ok(())
    }
}
pub(crate) struct OriginalConversationReactionsV17 {
    original: Arc<OriginalConversationCommentV17>,
    reactions: Vec<CommentReaction>,
    complete: bool,
    pages: usize,
}
impl OriginalConversationReactionsV17 {
    pub(crate) fn original(&self) -> &Arc<OriginalConversationCommentV17> {
        &self.original
    }
    pub(crate) fn reactions(&self) -> &[CommentReaction] {
        &self.reactions
    }
    pub(crate) fn complete(&self) -> bool {
        self.complete
    }
    pub(crate) fn pages(&self) -> usize {
        self.pages
    }
}
pub(crate) struct OriginalConversationSubscriptionV17 {
    parent: Arc<PrResourceParentV1>,
    identity: HostIdentity,
    current: CurrentPrSubscription,
    measured: Arc<ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
}
pub(crate) struct FreshConversationPublicationV17 {
    parent: Arc<PrResourceParentV1>,
    identity: HostIdentity,
    measured: Arc<ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
    digest: [u8; 32],
    comment: Option<Arc<OriginalConversationCommentV17>>,
    reactions: Option<OriginalConversationReactionsV17>,
    subscription: Option<OriginalConversationSubscriptionV17>,
}
impl std::fmt::Debug for FreshConversationPublicationV17 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "FreshConversationPublicationV17(<same original measured request; no approval>)",
        )
    }
}
impl FreshConversationPublicationV17 {
    /// Restriction only: the SAME genuine original pool/observation cutoff.
    /// No new window, token, source or current access is issued.
    pub(crate) fn publication_cutoff_v51(&self) -> Instant {
        self.deadline.min(self.measured.deadline())
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn comment(&self) -> Option<&Arc<OriginalConversationCommentV17>> {
        self.comment.as_ref()
    }
}
pub(crate) enum ConversationPublicationFailureV17 {
    NotStarted(GithubError),
    WriteMayHaveStarted(GithubError),
}
pub(crate) enum ConversationReceiptV17 {
    Comment {
        comment: ConversationComment,
        author_account: Option<u64>,
    },
    Deleted {
        id: u64,
        node: String,
    },
    Reaction {
        reaction: CommentReaction,
        status: u16,
    },
    RemovedReaction {
        id: u64,
        node: String,
    },
    Subscription {
        state: Subscription,
    },
}
impl ConversationReceiptV17 {
    pub(crate) fn observed_label_v17(&self) -> &'static str {
        match self {
            Self::Comment {
                author_account: Some(_),
                ..
            } => "Actual comment ID/body and numeric author observed",
            Self::Comment {
                author_account: None,
                ..
            } => "Actual comment ID/body observed; author UID unknown",
            Self::Deleted { .. } => "Actual204 and exact original comment-node absence observed",
            Self::Reaction { status: 200, .. } => {
                "Existing reaction observed (HTTP200); no new-creation claim"
            }
            Self::Reaction { status: 201, .. } => "Created reaction observed (HTTP201)",
            Self::Reaction { .. } => "Reaction protocol unavailable; no creation claim",
            Self::RemovedReaction { .. } => {
                "Actual204 and complete current reaction absence observed"
            }
            Self::Subscription { .. } => {
                "Requested subscription state observed; notification delivery unknown"
            }
        }
    }
}
pub(crate) enum ConversationHttpRequestV17<'a> {
    DeleteComment { comment_id: u64 },
    AddReaction { comment_id: u64, content: &'a str },
    RemoveReaction { comment_id: u64, reaction_id: u64 },
}
pub(crate) enum ConversationHttpAckV17 {
    DeletedComment { comment_id: u64 },
    AddedReaction { status: u16, value: Value },
    RemovedReaction { reaction_id: u64 },
}
pub(crate) fn conversation_post_caps_v17(
    operation: &PrOperation,
) -> GithubResult<&'static [usize]> {
    Ok(match operation {
        PrOperation::DeleteComment(_) => &CONVERSATION_DELETE_POST_CAPS_V17,
        PrOperation::AddReaction(_, _) | PrOperation::RemoveReaction(_, _) => {
            &CONVERSATION_REACTION_POST_CAPS_V17
        }
        PrOperation::Subscription(_) => &CONVERSATION_SUBSCRIPTION_POST_CAPS_V17,
        PrOperation::NewComment(_) | PrOperation::EditComment(_, _) => {
            &CONVERSATION_COMMENT_POST_CAPS_V17
        }
        PrOperation::ViewedFile(_, _)
        | PrOperation::Merge(_)
        | PrOperation::State(_)
        | PrOperation::Governed(_) => return Err(bad(GithubErrorKind::InvalidTarget)),
    })
}
fn operation_digest(
    parent: &PrResourceParentV1,
    operation: &PrOperation,
) -> GithubResult<[u8; 32]> {
    let selected = operation.selected_v17();
    if selected.is_some_and(|s| s.kind != CommentKind::Conversation) {
        return Err(bad(GithubErrorKind::InvalidTarget));
    }
    let values = match operation {
        PrOperation::NewComment(body) | PrOperation::EditComment(_, body) => {
            if body.trim().is_empty() || body.len() > CONVERSATION_BODY_CAP_V17 {
                return Err(bad(GithubErrorKind::InvalidTarget));
            }
            json!({"body":body})
        }
        PrOperation::DeleteComment(_) => json!({}),
        PrOperation::AddReaction(_, reaction) => json!({"content":reaction.value()}),
        PrOperation::RemoveReaction(_, reaction) => {
            json!({"reaction_id":reaction.id,"node":reaction.node_id,
            "content":reaction.content,"account_id":reaction.account_id})
        }
        PrOperation::Subscription(state) => json!({"state":state}),
        PrOperation::ViewedFile(_, _)
        | PrOperation::Merge(_)
        | PrOperation::State(_)
        | PrOperation::Governed(_) => return Err(bad(GithubErrorKind::InvalidTarget)),
    };
    let raw = serde_json::to_vec(&json!({"command":operation.command_v17().key(),
        "pr":parent.node_id(),"repository":parent.repository_id(),"number":parent.number(),
        "H":parent.head_sha(),"base":parent.base_tip_sha(),
        "selected":selected.map(|s| json!({"id":s.observed.id,"node":s.observed.node_id,
            "updated":s.observed.updated_at,"body_sha":format!("{:x}",Sha256::digest(s.observed.body.as_bytes()))})),
        "values":values})).map_err(|_| bad(GithubErrorKind::Json))?;
    if raw.len() > CONVERSATION_DRAFT_CAP_V17 {
        return Err(bad(GithubErrorKind::InvalidTarget));
    }
    Ok(Sha256::digest(raw).into())
}
impl OriginalConversationSubscriptionV17 {
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn current(&self) -> &CurrentPrSubscription {
        &self.current
    }
}
fn bad(kind: GithubErrorKind) -> GithubError {
    error(
        kind,
        "Exact original Conversation parent/comment/principal unavailable; no substitute or retry",
    )
}
fn live(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(bad(GithubErrorKind::Transport))
    } else {
        Ok(())
    }
}
fn measured(
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> GithubResult<(Arc<ScopedReadBudgetV1>, Instant)> {
    let actual = provider
        .scoped_read_budget
        .as_ref()
        .ok_or_else(|| bad(GithubErrorKind::InvalidTarget))?;
    let deadline = provider
        .scoped_read_window
        .ok_or_else(|| bad(GithubErrorKind::InvalidTarget))?
        .0
        .min(actual.deadline());
    live(cancel, deadline)?;
    actual.check_clock(cancel).map_err(GithubError::from)?;
    Ok((actual.clone(), deadline))
}
fn positive(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .filter(|id| (1..=i64::MAX as u64).contains(id))
}
fn opaque(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn bounded_comment(raw: &Value) -> bool {
    raw["body"].as_str().is_some_and(|s| s.len() <= 1024 * 1024)
        && raw["node_id"].as_str().is_some_and(opaque)
        && ["url", "html_url", "issue_url"]
            .iter()
            .all(|k| raw[*k].as_str().is_none_or(|s| s.len() <= 4096))
        && ["created_at", "updated_at"].iter().all(|k| {
            raw[*k]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 128)
        })
        && raw["user"].as_object().is_none_or(|u| {
            ["login", "node_id", "type"].iter().all(|k| {
                u.get(*k)
                    .and_then(Value::as_str)
                    .is_none_or(|s| s.len() <= 1024)
            }) && u
                .get("avatar_url")
                .and_then(Value::as_str)
                .is_none_or(|s| s.len() <= 4096)
        })
}
