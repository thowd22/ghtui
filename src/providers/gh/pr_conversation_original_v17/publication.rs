use super::*;
impl GithubProvider {
    /// All operation-specific current reads use the same measured allocation
    /// as both actual raw parents. No UID/helper session is hidden here.
    pub(crate) fn observe_conversation_publication_v17(
        &self,
        parent: Arc<PrResourceParentV1>,
        identity: &HostIdentity,
        operation: &PrOperation,
        cancel: &CancellationToken,
    ) -> GithubResult<FreshConversationPublicationV17> {
        let (measured, deadline) = measured(self, cancel)?;
        let digest = operation_digest(&parent, operation)?;
        if identity.host != self.host
            || parent.summary().pr.host != self.host
            || identity
                .account_id
                .is_none_or(|id| !(1..=i64::MAX as u64).contains(&id))
        {
            return Err(bad(GithubErrorKind::IdentityChanged));
        }
        let comment = operation
            .selected_v17()
            .map(|selected| {
                let actual = self.observe_conversation_operation_comment_v17(
                    parent.clone(),
                    identity,
                    selected,
                    cancel,
                )?;
                actual.require(operation.command_v17())?;
                Ok::<_, GithubError>(actual)
            })
            .transpose()?;
        let reactions = if let PrOperation::RemoveReaction(_, selected) = operation {
            let rows =
                self.observe_conversation_reactions_v17(comment.as_ref().unwrap().clone(), cancel)?;
            if !rows.complete
                || !rows.reactions.iter().any(|r| {
                    r.id == selected.id
                        && r.node_id == selected.node_id
                        && r.content == selected.content
                        && r.account_id == identity.account_id
                })
            {
                return Err(error(GithubErrorKind::Permission,
                    "Exact current-owned reaction absent or catalogue incomplete; no guessed removal"));
            }
            Some(rows)
        } else {
            None
        };
        let subscription = if let PrOperation::Subscription(_) = operation {
            let current =
                self.observe_conversation_subscription_v17(parent.clone(), identity, cancel)?;
            if !current.current.viewer_can_subscribe {
                return Err(bad(GithubErrorKind::Permission));
            }
            Some(current)
        } else {
            None
        };
        live(cancel, deadline)?;
        Ok(FreshConversationPublicationV17 {
            parent,
            identity: identity.clone(),
            measured,
            cancellation: cancel.clone(),
            deadline,
            digest,
            comment,
            reactions,
            subscription,
        })
    }
    /// One possible write; actual authority closes on the ORIGINAL Core and
    /// returns its actual closing identity. No read window is reused for POST.
    pub(crate) fn publish_conversation_v17(
        &self,
        actual: &GithubProvider,
        captures: &mut repository_catalogue::PostWriteCaptureAllowanceV2,
        observed: &FreshConversationPublicationV17,
        operation: &PrOperation,
        cancel: &CancellationToken,
        before_write: impl FnOnce() -> GithubResult<HostIdentity>,
    ) -> Result<ConversationReceiptV17, ConversationPublicationFailureV17> {
        let mut invoked = false;
        let result = (|| {
            observed.validate_prewrite(self, actual, captures, operation, cancel)?;
            let identity = before_write()?;
            if identity.host != observed.identity.host
                || identity.account_id != observed.identity.account_id
                || identity
                    .account_id
                    .is_none_or(|id| !(1..=i64::MAX as u64).contains(&id))
            {
                return Err(bad(GithubErrorKind::IdentityChanged));
            }
            live(cancel, observed.deadline.min(captures.deadline()))?;
            let parent = &observed.parent;
            let resource = self.resource(parent.repository(), parent.number());
            let receipt = match operation {
                PrOperation::NewComment(body) | PrOperation::EditComment(_, body) => {
                    let editing = observed.comment.as_ref();
                    let endpoint = match editing {
                        Some(comment) => self.comment_endpoint(
                            parent.summary(),
                            comment.observed.id,
                            CommentKind::Conversation,
                        )?,
                        None => format!(
                            "repos/{}/issues/{}/comments",
                            parent.repository(),
                            parent.number()
                        ),
                    };
                    invoked = true;
                    let response = captures.api(
                        self,
                        &endpoint,
                        if editing.is_some() { "PATCH" } else { "POST" },
                        Some(json!({"body":body})),
                        Some(resource.clone()),
                        cancel,
                    )?;
                    let id = positive(&response["id"])
                        .ok_or_else(|| bad(GithubErrorKind::AmbiguousWrite))?;
                    let node = response["node_id"]
                        .as_str()
                        .filter(|s| opaque(s))
                        .ok_or_else(|| bad(GithubErrorKind::AmbiguousWrite))?;
                    let valid = |raw: &Value| {
                        bounded_comment(raw)
                            && self.comment_belongs(
                                parent.summary(),
                                raw,
                                id,
                                CommentKind::Conversation,
                            )
                            && raw["node_id"].as_str() == Some(node)
                            && raw["body"].as_str() == Some(body.as_str())
                            && match editing {
                                Some(original) => {
                                    id == original.observed.id
                                        && node == original.observed.node_id
                                        && positive(&raw["user"]["id"]) == original.author_account
                                }
                                None => Self::rest_actor_matches(&identity, &raw["user"]),
                            }
                    };
                    if !valid(&response) {
                        return Err(bad(GithubErrorKind::AmbiguousWrite));
                    }
                    let fresh = captures.api(
                        self,
                        &self.comment_endpoint(parent.summary(), id, CommentKind::Conversation)?,
                        "GET",
                        None,
                        None,
                        cancel,
                    )?;
                    if !valid(&fresh) {
                        return Err(bad(GithubErrorKind::AmbiguousWrite));
                    }
                    ConversationReceiptV17::Comment {
                        comment: comment_from_json(&fresh),
                        author_account: positive(&fresh["user"]["id"]),
                    }
                }
                PrOperation::DeleteComment(selected) => {
                    let original = observed
                        .comment
                        .as_ref()
                        .ok_or_else(|| bad(GithubErrorKind::InvalidTarget))?;
                    if original.observed.id != selected.observed.id {
                        return Err(bad(GithubErrorKind::InvalidTarget));
                    }
                    invoked = true;
                    match captures.conversation_http_ack_v17(
                        self,
                        parent,
                        ConversationHttpRequestV17::DeleteComment {
                            comment_id: original.observed.id,
                        },
                        cancel,
                    )? {
                        ConversationHttpAckV17::DeletedComment { comment_id }
                            if comment_id == original.observed.id => {}
                        _ => return Err(bad(GithubErrorKind::AmbiguousWrite)),
                    }
                    let raw = captures.api(self,"graphql","POST",Some(json!({
                        "query":CONVERSATION_ABSENCE_QUERY_V17,"variables":{"id":original.observed.node_id}
                    })),None,cancel)?;
                    if !error_free(&raw)
                        || raw.get("data").and_then(|v| v.get("node")) != Some(&Value::Null)
                    {
                        return Err(bad(GithubErrorKind::AmbiguousWrite));
                    }
                    ConversationReceiptV17::Deleted {
                        id: original.observed.id,
                        node: original.observed.node_id.clone(),
                    }
                }
                PrOperation::AddReaction(_, content) => {
                    let original = observed
                        .comment
                        .as_ref()
                        .ok_or_else(|| bad(GithubErrorKind::InvalidTarget))?;
                    invoked = true;
                    let (status, value) = match captures.conversation_http_ack_v17(
                        self,
                        parent,
                        ConversationHttpRequestV17::AddReaction {
                            comment_id: original.observed.id,
                            content: content.value(),
                        },
                        cancel,
                    )? {
                        ConversationHttpAckV17::AddedReaction { status, value } => (status, value),
                        _ => return Err(bad(GithubErrorKind::AmbiguousWrite)),
                    };
                    let created = reaction(&value)?;
                    if created.content != content.value()
                        || created.account_id != identity.account_id
                    {
                        return Err(bad(GithubErrorKind::AmbiguousWrite));
                    }
                    let rows = self.conversation_reaction_receipt_v17(
                        parent,
                        original.observed.id,
                        captures,
                        cancel,
                    )?;
                    if !rows.iter().any(|r| {
                        r.id == created.id
                            && r.node_id == created.node_id
                            && r.content == created.content
                            && r.account_id == identity.account_id
                    }) {
                        return Err(bad(GithubErrorKind::AmbiguousWrite));
                    }
                    ConversationReceiptV17::Reaction {
                        reaction: created,
                        status,
                    }
                }
                PrOperation::RemoveReaction(_, selected) => {
                    let current = observed
                        .reactions
                        .as_ref()
                        .ok_or_else(|| bad(GithubErrorKind::InvalidTarget))?;
                    if !current.complete
                        || !current.reactions.iter().any(|r| {
                            r.id == selected.id
                                && r.node_id == selected.node_id
                                && r.content == selected.content
                                && r.account_id == identity.account_id
                        })
                    {
                        return Err(bad(GithubErrorKind::InvalidTarget));
                    }
                    invoked = true;
                    match captures.conversation_http_ack_v17(
                        self,
                        parent,
                        ConversationHttpRequestV17::RemoveReaction {
                            comment_id: current.original.observed.id,
                            reaction_id: selected.id,
                        },
                        cancel,
                    )? {
                        ConversationHttpAckV17::RemovedReaction { reaction_id }
                            if reaction_id == selected.id => {}
                        _ => return Err(bad(GithubErrorKind::AmbiguousWrite)),
                    }
                    let rows = self.conversation_reaction_receipt_v17(
                        parent,
                        current.original.observed.id,
                        captures,
                        cancel,
                    )?;
                    if rows
                        .iter()
                        .any(|r| r.id == selected.id || r.node_id == selected.node_id)
                    {
                        return Err(bad(GithubErrorKind::AmbiguousWrite));
                    }
                    ConversationReceiptV17::RemovedReaction {
                        id: selected.id,
                        node: selected.node_id.clone(),
                    }
                }
                PrOperation::Subscription(state) => {
                    let original = observed
                        .subscription
                        .as_ref()
                        .ok_or_else(|| bad(GithubErrorKind::InvalidTarget))?;
                    if !original.current.viewer_can_subscribe {
                        return Err(bad(GithubErrorKind::Permission));
                    }
                    let desired =
                        serde_json::to_value(state).map_err(|_| bad(GithubErrorKind::Json))?;
                    let query = "mutation($input:UpdateSubscriptionInput!){updateSubscription(input:$input){subscribable{id viewerSubscription}}}";
                    invoked = true;
                    let raw = captures.api(self,"graphql","POST",Some(json!({"query":query,
                        "variables":{"input":{"subscribableId":parent.node_id(),"state":desired}}})),Some(resource.clone()),cancel)?;
                    let result = &raw["data"]["updateSubscription"]["subscribable"];
                    if !error_free(&raw)
                        || result["id"].as_str() != Some(parent.node_id())
                        || result["viewerSubscription"] != desired
                    {
                        return Err(bad(GithubErrorKind::AmbiguousWrite));
                    }
                    let raw = captures.api(self,"graphql","POST",Some(json!({
                        "query":PR_CURRENT_SUBSCRIPTION_QUERY,"variables":{"id":parent.node_id()}
                    })),None,cancel)?;
                    let pr = &raw["data"]["node"];
                    if !error_free(&raw)
                        || pr["__typename"] != "PullRequest"
                        || pr["id"].as_str() != Some(parent.node_id())
                        || pr["number"].as_u64() != Some(parent.number())
                        || pr["repository"]["nameWithOwner"]
                            .as_str()
                            .is_none_or(|n| !n.eq_ignore_ascii_case(parent.repository()))
                        || pr["headRefOid"].as_str() != Some(parent.head_sha())
                        || pr["baseRefOid"].as_str() != Some(parent.base_tip_sha())
                        || pr["viewerSubscription"] != desired
                        || pr["viewerCanSubscribe"].as_bool().is_none()
                    {
                        return Err(bad(GithubErrorKind::AmbiguousWrite));
                    }
                    ConversationReceiptV17::Subscription { state: *state }
                }
                PrOperation::ViewedFile(_, _) => return Err(bad(GithubErrorKind::InvalidTarget)),
            };
            live(cancel, observed.deadline.min(captures.deadline()))?;
            Ok(receipt)
        })();
        result.map_err(|failure| {
            if invoked {
                self.runner.mark_ambiguous(
                    &self.resource(observed.parent.repository(), observed.parent.number()),
                );
                ConversationPublicationFailureV17::WriteMayHaveStarted(failure)
            } else {
                ConversationPublicationFailureV17::NotStarted(failure)
            }
        })
    }
    fn conversation_reaction_receipt_v17(
        &self,
        parent: &PrResourceParentV1,
        comment: u64,
        captures: &mut repository_catalogue::PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<CommentReaction>> {
        let mut rows = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for page in 1..=4 {
            let raw = captures.api(
                self,
                &format!(
                    "repos/{}/issues/comments/{comment}/reactions?per_page=100&page={page}",
                    parent.repository()
                ),
                "GET",
                None,
                None,
                cancel,
            )?;
            let values = raw
                .as_array()
                .filter(|r| r.len() <= 100)
                .ok_or_else(|| bad(GithubErrorKind::AmbiguousWrite))?;
            for value in values {
                let value = reaction(value)?;
                if !seen.insert(value.id) {
                    return Err(bad(GithubErrorKind::AmbiguousWrite));
                }
                rows.push(value);
            }
            if values.len() < 100 {
                return Ok(rows);
            }
        }
        Err(error(GithubErrorKind::AmbiguousWrite,
            "Reaction receipt catalogue capped; absence/membership remains unknown and no retry is permitted"))
    }
}
pub(super) fn error_free(raw: &Value) -> bool {
    raw.get("errors")
        .is_none_or(|v| v.is_null() || v.as_array().is_some_and(Vec::is_empty))
        && raw.get("data").is_some_and(Value::is_object)
}
fn reaction(raw: &Value) -> GithubResult<CommentReaction> {
    let id = positive(&raw["id"]).ok_or_else(|| bad(GithubErrorKind::AmbiguousWrite))?;
    let node = raw["node_id"]
        .as_str()
        .filter(|s| opaque(s))
        .ok_or_else(|| bad(GithubErrorKind::AmbiguousWrite))?;
    let content = raw["content"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 128)
        .ok_or_else(|| bad(GithubErrorKind::AmbiguousWrite))?;
    if raw["user"]["login"]
        .as_str()
        .is_some_and(|s| s.len() > 1024)
        || raw["user"]["avatar_url"]
            .as_str()
            .is_some_and(|s| s.len() > 4096)
        || raw["created_at"].as_str().is_none_or(|s| s.len() > 128)
    {
        return Err(bad(GithubErrorKind::AmbiguousWrite));
    }
    Ok(CommentReaction {
        id,
        node_id: node.into(),
        content: content.into(),
        actor: Actor::from_json(&raw["user"]),
        account_id: positive(&raw["user"]["id"]),
        created_at: text(raw, "created_at"),
    })
}
impl FreshConversationPublicationV17 {
    pub(crate) fn subscription(&self) -> Option<&OriginalConversationSubscriptionV17> {
        self.subscription.as_ref()
    }
    fn validate_prewrite(
        &self,
        ordinary: &GithubProvider,
        actual: &GithubProvider,
        captures: &repository_catalogue::PostWriteCaptureAllowanceV2,
        operation: &PrOperation,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        live(cancel, self.deadline.min(captures.deadline()))?;
        if ordinary.scoped_read_budget.is_some()
            || !cancel.same_instance(&self.cancellation)
            || actual
                .scoped_read_budget
                .as_ref()
                .is_none_or(|b| !Arc::ptr_eq(b, &self.measured))
            || operation_digest(&self.parent, operation)? != self.digest
        {
            return Err(bad(GithubErrorKind::InvalidTarget));
        }
        if self.comment.as_ref().is_some_and(|comment| {
            comment.identity.host != self.identity.host
                || comment.identity.account_id != self.identity.account_id
                || !cancel.same_instance(&comment.cancellation)
                || !Arc::ptr_eq(&comment.measured, &self.measured)
        }) || self.subscription.as_ref().is_some_and(|subscription| {
            subscription.identity.host != self.identity.host
                || subscription.identity.account_id != self.identity.account_id
                || !cancel.same_instance(&subscription.cancellation)
                || !Arc::ptr_eq(&subscription.measured, &self.measured)
                || Instant::now() >= subscription.deadline
        }) {
            return Err(bad(GithubErrorKind::IdentityChanged));
        }
        captures.validate_prewrite(actual, cancel)
    }
}
