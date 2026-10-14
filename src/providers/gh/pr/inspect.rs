//! Fresh typed comment choices and explicit, generation-stamped PR reconciliation.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommentReaction {
    pub id: u64,
    pub node_id: String,
    pub content: String,
    pub actor: Actor,
    /// Stable account ID, if the host supplies it. A username alone can be reassigned.
    #[serde(default)]
    pub account_id: Option<u64>,
    pub created_at: String,
}

/// A nullable server observation is not a subscribed default. Permission and
/// state belong to the freshly verified viewer, not the PR author.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CurrentPrSubscription {
    pub state: Option<Subscription>,
    pub viewer_can_subscribe: bool,
}
impl CurrentPrSubscription {
    pub fn state_label(&self) -> &'static str {
        match self.state {
            Some(Subscription::Subscribed) => "subscribed",
            Some(Subscription::Unsubscribed) => "unsubscribed",
            Some(Subscription::Ignored) => "ignored",
            None => "unavailable (null)",
        }
    }
    pub fn same_observation(&self, other: &Self) -> bool {
        self.state_label() == other.state_label()
            && self.viewer_can_subscribe == other.viewer_can_subscribe
    }
}
pub const PR_CURRENT_SUBSCRIPTION_QUERY: &str = "query($id:ID!){node(id:$id){__typename ... on PullRequest{id number headRefOid baseRefOid repository{nameWithOwner} viewerSubscription viewerCanSubscribe}}}";
/// Deliberately not deserializable: acknowledgement needs a fresh provider-produced read.
#[derive(Clone, Debug)]
pub struct PrObservation {
    host: String,
    producer: CommandRunner,
    identity: HostIdentity,
    target: PrSummary,
    details: PrDetails,
    resource: String,
    generation: u64,
}
impl PrObservation {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn target(&self) -> &PrSummary {
        &self.target
    }
    pub fn details(&self) -> &PrDetails {
        &self.details
    }
}
impl GithubProvider {
    /// Read-only despite GraphQL's POST transport. No write resource is claimed
    /// and an unresolved publication is neither released nor acknowledged.
    pub fn current_subscription(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> GithubResult<CurrentPrSubscription> {
        if identity.account_id.is_none_or(|id| id == 0) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Verify a positive numeric account before reading private subscription state",
            ));
        }
        self.guard_target(identity, target, cancel)?;
        let result = self.graphql(
            PR_CURRENT_SUBSCRIPTION_QUERY,
            json!({"id":target.pr.id}),
            None,
            cancel,
        )?;
        let node = &result["data"]["node"];
        if text(node, "__typename") != "PullRequest"
            || text(node, "id") != target.pr.id
            || number(node, "number") != target.pr.number
            || !text(&node["repository"], "nameWithOwner")
                .eq_ignore_ascii_case(&target.pr.repository)
            || text(node, "headRefOid") != target.head_sha
            || text(node, "baseRefOid") != target.base_sha
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Subscription snapshot does not match the exact selected PR node/repository/number/base/head",
            ));
        }
        let viewer_can_subscribe = node["viewerCanSubscribe"].as_bool().ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Subscription snapshot has no verified viewerCanSubscribe permission",
            )
        })?;
        let state = match node.get("viewerSubscription") {
            Some(Value::Null) => None,
            Some(Value::String(state)) => Some(match state.as_str() {
                "SUBSCRIBED" => Subscription::Subscribed,
                "UNSUBSCRIBED" => Subscription::Unsubscribed,
                "IGNORED" => Subscription::Ignored,
                _ => return Err(error(
                    GithubErrorKind::Unsupported,
                    "Host returned an unsupported subscription state; no default is substituted",
                )),
            }),
            _ => return Err(error(
                GithubErrorKind::Json,
                "Subscription snapshot omits its nullable current state; no default is substituted",
            )),
        };
        let fresh = self.pr_summary(&target.pr.repository, target.pr.number, cancel)?;
        if fresh.pr.id != target.pr.id
            || fresh.head_sha != target.head_sha
            || fresh.base_sha != target.base_sha
            || !fresh
                .pr
                .repository
                .eq_ignore_ascii_case(&target.pr.repository)
        {
            return Err(error(GithubErrorKind::StaleHead,"PR identity/base/head changed while reading subscription state; refresh before changing it"));
        }
        if !self.identity(cancel)?.same_account(identity) {
            return Err(error(GithubErrorKind::IdentityChanged,"Account changed while reading private subscription state; old viewer state is hidden"));
        }
        Ok(CurrentPrSubscription {
            state,
            viewer_can_subscribe,
        })
    }

    pub fn inspect_comment(
        &self,
        target: &PrSummary,
        observed: &ConversationComment,
        kind: CommentKind,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationComment> {
        validate_pr(&target.pr.repository, target.pr.number)?;
        if target.pr.host != self.host || target.pr.id.is_empty() || observed.node_id.is_empty() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Select a canonical PR on the active host before inspecting a comment",
            ));
        }
        let value = self.api(
            &self.comment_endpoint(target, observed.id, kind)?,
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        if !self.comment_belongs(target, &value, observed.id, kind)
            || text(&value, "node_id") != observed.node_id
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "The selected comment ID/node does not belong to this exact repository and PR",
            ));
        }
        // Return the fresh body/timestamp. The controller must present any changed
        // observation rather than silently replacing a user's pending edit intent.
        Ok(comment_from_json(&value))
    }
    pub fn comment_reactions(
        &self,
        target: &PrSummary,
        observed: &ConversationComment,
        kind: CommentKind,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<CommentReaction>> {
        self.inspect_comment(target, observed, kind, cancel)?;
        let values = self.api_list(
            &format!(
                "{}/reactions?per_page=100",
                self.comment_endpoint(target, observed.id, kind)?
            ),
            None,
            cancel,
        )?;
        let mut ids = std::collections::BTreeSet::new();
        values
            .into_iter()
            .map(|value| {
                let id = number(&value, "id");
                let content = text(&value, "content");
                if id == 0 || !ids.insert(id) || content.is_empty() {
                    return Err(error(GithubErrorKind::Json,"Comment reaction lacks a unique ID/content; no label-derived target is substituted"));
                }
                Ok(CommentReaction {
                    id,
                    node_id: text(&value, "node_id"),
                    content,
                    actor: Actor::from_json(&value["user"]),
                    account_id: value["user"]["id"].as_u64().filter(|id| *id > 0),
                    created_at: text(&value, "created_at"),
                })
            })
            .collect()
    }
    pub fn inspect_pr_target(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> GithubResult<PrObservation> {
        validate_pr(&target.pr.repository, target.pr.number)?;
        if identity.account_id.is_none_or(|id| id == 0) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Verify a positive numeric account ID before inspecting private PR effects",
            ));
        }
        if target.pr.host != self.host || target.pr.id.is_empty() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Select an exact PR host/repository/node/number to reconcile",
            ));
        }
        let resource = self.resource(&target.pr.repository, target.pr.number);
        let generation = self.runner.resource_generation(&resource);
        let inspected_identity = self.identity(cancel)?;
        if !inspected_identity.same_account(identity) {
            return Err(error(GithubErrorKind::IdentityChanged,"Account changed before PR effect inspection; old private context is not acknowledged"));
        }
        let details = self.pr_details(&target.pr.repository, target.pr.number, cancel)?;
        if !self.identity(cancel)?.same_account(&inspected_identity) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Account changed during PR inspection; private effects are not acknowledged",
            ));
        }
        let fresh = &details.summary.pr;
        if fresh.host != self.host
            || fresh.id != target.pr.id
            || fresh.number != target.pr.number
            || !fresh.repository.eq_ignore_ascii_case(&target.pr.repository)
        {
            return Err(error(GithubErrorKind::StaleHead,"PR identity changed during effect inspection; no different resource is acknowledged"));
        }
        if self.runner.resource_generation(&resource) != generation {
            return Err(error(
                GithubErrorKind::StaleHead,
                "A newer write began during PR inspection; inspect again before acknowledging",
            ));
        }
        // An uncertain merge/update may intentionally change state or head SHA.
        // Preserve those fresh effects for display instead of demanding the old head.
        Ok(PrObservation {
            host: self.host.clone(),
            producer: self.runner.clone(),
            identity: inspected_identity,
            target: target.clone(),
            details,
            resource,
            generation,
        })
    }
    pub fn acknowledge_pr_observation(&self, observation: PrObservation) -> GithubResult<()> {
        if observation.host != self.host || !self.runner.same_instance(&observation.producer) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "The inspected PR belongs to another host or provider execution state",
            ));
        }
        if !self
            .runner
            .mark_reconciled_generation(&observation.resource, observation.generation)
        {
            return Err(error(GithubErrorKind::StaleHead,"This PR observation predates a newer write; inspect its actual effects before acknowledging"));
        }
        Ok(())
    }
}
