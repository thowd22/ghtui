use super::*;
impl GithubProvider {
    pub(crate) fn observe_conversation_comment_v17(
        &self,
        parent: Arc<PrResourceParentV1>,
        identity: &HostIdentity,
        selected: &crate::github_workspace::PrCommentSelection,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<OriginalConversationCommentV17>> {
        if selected.kind != CommentKind::Conversation {
            return Err(bad(GithubErrorKind::InvalidTarget));
        }
        self.observe_conversation_selected_v17(parent, identity, &selected.observed, cancel)
    }
    pub(super) fn observe_conversation_operation_comment_v17(
        &self,
        parent: Arc<PrResourceParentV1>,
        identity: &HostIdentity,
        selected: &crate::pr_actions::SelectedComment,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<OriginalConversationCommentV17>> {
        if selected.kind != CommentKind::Conversation {
            return Err(bad(GithubErrorKind::InvalidTarget));
        }
        self.observe_conversation_selected_v17(parent, identity, &selected.observed, cancel)
    }
    fn observe_conversation_selected_v17(
        &self,
        parent: Arc<PrResourceParentV1>,
        identity: &HostIdentity,
        selected: &ConversationComment,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<OriginalConversationCommentV17>> {
        measured(self, cancel)?;
        if positive(&json!(selected.id)).is_none()
            || !opaque(&selected.node_id)
            || parent.summary().pr.host != self.host
            || identity.host != self.host
            || identity
                .account_id
                .is_none_or(|id| !(1..=i64::MAX as u64).contains(&id))
            || identity.login.len() > 100
            || identity.gh_version.len() > 256
        {
            return Err(bad(GithubErrorKind::InvalidTarget));
        }
        let endpoint =
            self.comment_endpoint(parent.summary(), selected.id, CommentKind::Conversation)?;
        let raw = self.api(&endpoint, "GET", None, None, false, cancel)?;
        let result = self.graphql(
            CONVERSATION_COMMENT_QUERY_V17,
            json!({"id":selected.node_id}),
            None,
            cancel,
        )?;
        self.conversation_comment_pair_v17(parent, identity, selected, raw, result, cancel)
    }
    // ONE canonical REST/GQL pair validator. Recovery supplies only an actual
    // REST-produced current selection; it never becomes a publication intent.
    pub(super) fn conversation_comment_pair_v17(
        &self,
        parent: Arc<PrResourceParentV1>,
        identity: &HostIdentity,
        selected: &ConversationComment,
        raw: Value,
        result: Value,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<OriginalConversationCommentV17>> {
        let (measured, deadline) = measured(self, cancel)?;
        if !publication::error_free(&result) {
            return Err(bad(GithubErrorKind::Json));
        }
        if !bounded_comment(&raw)
            || !self.comment_belongs(
                parent.summary(),
                &raw,
                selected.id,
                CommentKind::Conversation,
            )
            || raw["node_id"].as_str() != Some(selected.node_id.as_str())
            || raw["body"].as_str() != Some(selected.body.as_str())
            || raw["updated_at"].as_str() != Some(selected.updated_at.as_str())
        {
            return Err(bad(GithubErrorKind::StaleHead));
        }
        let node = &result["data"]["node"];
        if node["__typename"] != "IssueComment"
            || node["id"] != raw["node_id"]
            || thread_publication_v1::canonical_bigint(&node["fullDatabaseId"]) != Some(selected.id)
            || node["repository"]["id"].as_str() != Some(parent.repository_node())
            || node["repository"]["nameWithOwner"]
                .as_str()
                .is_none_or(|s| !s.eq_ignore_ascii_case(parent.repository()))
            || node["body"] != raw["body"]
            || node["updatedAt"] != raw["updated_at"]
            || node["createdAt"] != raw["created_at"]
        {
            return Err(bad(GithubErrorKind::IdentityChanged));
        }
        // IssueComment.issue.id is NOT the PullRequest node. The exact REST
        // issue_url plus actual raw PR are mandatory parent proof. The real
        // nullable pullRequest, if supplied, is additional corroboration only.
        if let Some(pr) = node.get("pullRequest").filter(|v| !v.is_null()) {
            if !pr.is_object()
                || pr["id"].as_str() != Some(parent.node_id())
                || thread_publication_v1::canonical_bigint(&pr["fullDatabaseId"])
                    != Some(parent.database_id())
                || pr["number"].as_u64() != Some(parent.number())
                || pr["headRefOid"].as_str() != Some(parent.head_sha())
                || pr["baseRefOid"].as_str() != Some(parent.base_tip_sha())
                || pr["repository"]["id"].as_str() != Some(parent.repository_node())
                || pr["repository"]["nameWithOwner"]
                    .as_str()
                    .is_none_or(|s| !s.eq_ignore_ascii_case(parent.repository()))
            {
                return Err(bad(GithubErrorKind::IdentityChanged));
            }
        }
        let observed = comment_from_json(&raw);
        let capabilities = ConversationCapabilitiesV17 {
            can_update: node["viewerCanUpdate"].as_bool(),
            can_delete: node["viewerCanDelete"].as_bool(),
            can_react: node["viewerCanReact"].as_bool(),
            did_author: node["viewerDidAuthor"].as_bool(),
        };
        let author_account = positive(&raw["user"]["id"]);
        live(cancel, deadline)?;
        measured.check_clock(cancel).map_err(GithubError::from)?;
        Ok(Arc::new(OriginalConversationCommentV17 {
            parent,
            identity: identity.clone(),
            observed,
            author_account,
            capabilities,
            measured,
            cancellation: cancel.clone(),
            deadline,
        }))
    }
    pub(crate) fn observe_conversation_reactions_v17(
        &self,
        original: Arc<OriginalConversationCommentV17>,
        cancel: &CancellationToken,
    ) -> GithubResult<OriginalConversationReactionsV17> {
        let (actual, deadline) = measured(self, cancel)?;
        if !Arc::ptr_eq(&actual, &original.measured)
            || !cancel.same_instance(&original.cancellation)
        {
            return Err(bad(GithubErrorKind::InvalidTarget));
        }
        let mut reactions = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        let mut complete = false;
        let mut pages = 0;
        for page in 1..=4 {
            live(cancel, deadline.min(original.deadline))?;
            let rows = self.api(
                &format!(
                    "repos/{}/issues/comments/{}/reactions?per_page=100&page={page}",
                    original.parent.repository(),
                    original.observed.id
                ),
                "GET",
                None,
                None,
                false,
                cancel,
            )?;
            let rows = rows
                .as_array()
                .filter(|r| r.len() <= 100)
                .ok_or_else(|| bad(GithubErrorKind::Json))?;
            pages += 1;
            for raw in rows {
                let id = positive(&raw["id"])
                    .filter(|id| seen.insert(*id))
                    .ok_or_else(|| bad(GithubErrorKind::Json))?;
                let content = raw["content"]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 128)
                    .ok_or_else(|| bad(GithubErrorKind::Json))?;
                if raw["node_id"].as_str().is_none_or(|s| !opaque(s))
                    || raw["created_at"].as_str().is_none_or(|s| s.len() > 128)
                    || raw["user"]["login"]
                        .as_str()
                        .is_some_and(|s| s.len() > 1024)
                    || raw["user"]["avatar_url"]
                        .as_str()
                        .is_some_and(|s| s.len() > 4096)
                {
                    return Err(bad(GithubErrorKind::Json));
                }
                reactions.push(CommentReaction {
                    id,
                    node_id: text(raw, "node_id"),
                    content: content.into(),
                    actor: Actor::from_json(&raw["user"]),
                    account_id: positive(&raw["user"]["id"]),
                    created_at: text(raw, "created_at"),
                });
            }
            if rows.len() < 100 {
                complete = true;
                break;
            }
        }
        live(cancel, deadline.min(original.deadline))?;
        Ok(OriginalConversationReactionsV17 {
            original,
            reactions,
            complete,
            pages,
        })
    }
    pub(crate) fn observe_conversation_subscription_v17(
        &self,
        parent: Arc<PrResourceParentV1>,
        identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> GithubResult<OriginalConversationSubscriptionV17> {
        let (measured, deadline) = measured(self, cancel)?;
        if identity.host != self.host
            || parent.summary().pr.host != self.host
            || identity
                .account_id
                .is_none_or(|id| !(1..=i64::MAX as u64).contains(&id))
            || identity.login.len() > 100
            || identity.gh_version.len() > 256
        {
            return Err(bad(GithubErrorKind::IdentityChanged));
        }
        let raw = self.graphql(
            PR_CURRENT_SUBSCRIPTION_QUERY,
            json!({"id":parent.node_id()}),
            None,
            cancel,
        )?;
        if !publication::error_free(&raw) {
            return Err(bad(GithubErrorKind::Json));
        }
        let node = &raw["data"]["node"];
        if node["__typename"] != "PullRequest"
            || node["id"].as_str() != Some(parent.node_id())
            || node["number"].as_u64() != Some(parent.number())
            || node["headRefOid"].as_str() != Some(parent.head_sha())
            || node["baseRefOid"].as_str() != Some(parent.base_tip_sha())
            || node["repository"]["nameWithOwner"]
                .as_str()
                .is_none_or(|s| !s.eq_ignore_ascii_case(parent.repository()))
        {
            return Err(bad(GithubErrorKind::IdentityChanged));
        }
        let viewer_can_subscribe = node["viewerCanSubscribe"]
            .as_bool()
            .ok_or_else(|| bad(GithubErrorKind::Json))?;
        let state = match node.get("viewerSubscription") {
            Some(Value::Null) => None,
            Some(Value::String(s)) => Some(match s.as_str() {
                "SUBSCRIBED" => Subscription::Subscribed,
                "UNSUBSCRIBED" => Subscription::Unsubscribed,
                "IGNORED" => Subscription::Ignored,
                _ => return Err(bad(GithubErrorKind::Unsupported)),
            }),
            _ => return Err(bad(GithubErrorKind::Json)),
        };
        live(cancel, deadline)?;
        Ok(OriginalConversationSubscriptionV17 {
            parent,
            identity: identity.clone(),
            current: CurrentPrSubscription {
                state,
                viewer_can_subscribe,
            },
            measured,
            cancellation: cancel.clone(),
            deadline,
        })
    }
}
