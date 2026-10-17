//! Fresh observed outcome, never receipt/permission/attribution of an unknown
//! create. SAME measured Core and bounded existing runner resource state.
use super::*;
use crate::pr_write_journal::{
    conversation_history_v17::{ConversationActionV17, ConversationOriginSeedV17},
    PrWriteTarget,
};
pub(crate) struct ConversationRecoveryCurrentV17 {
    issuer: crate::providers::runner::CommandRunner,
    host: String,
    program: std::ffi::OsString,
    environment: std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>,
    parent: Arc<PrResourceParentV1>,
    identity: HostIdentity,
    resource: String,
    generation: u64,
    cancel: CancellationToken,
    deadline: Instant,
    lines: Arc<[String]>,
}
impl ConversationRecoveryCurrentV17 {
    pub(crate) fn lines(&self) -> &[String] {
        &self.lines
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn matches_target(&self, target: &PrWriteTarget) -> bool {
        crate::storage::Scope::github(&self.identity.host, self.identity.account_id)
            .ok()
            .as_ref()
            == Some(target.scope())
            && target.repository() == self.parent.repository()
            && target.number() == self.parent.number()
            && target.node_id() == self.parent.node_id()
    }
    fn validate(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        let deadline = deadline.min(self.deadline);
        live(cancel, deadline)?;
        live(&self.cancel, deadline)?;
        if !cancel.same_instance(&self.cancel)
            || !provider.runner.same_instance(&self.issuer)
            || provider.host != self.host
            || provider.program != self.program
            || provider.env != self.environment
            || provider
                .runner
                .review_resource_generation_v11(&self.resource, cancel, deadline)
                .map_err(GithubError::from)?
                != self.generation
        {
            return Err(bad(GithubErrorKind::StaleHead));
        }
        Ok(())
    }
}
impl GithubProvider {
    pub(crate) fn inspect_conversation_outcome_v17(
        &self,
        parent: Arc<PrResourceParentV1>,
        identity: &HostIdentity,
        seed: &ConversationOriginSeedV17,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationRecoveryCurrentV17> {
        let (_, deadline) = measured(self, cancel)?;
        seed.validate()
            .map_err(|_| bad(GithubErrorKind::InvalidTarget))?;
        if seed.scope
            != crate::storage::Scope::github(&identity.host, identity.account_id)
                .map_err(|_| bad(GithubErrorKind::IdentityChanged))?
            || seed.repository_id != parent.repository_id()
            || seed.repository_node != parent.repository_node()
            || seed.pr_id != parent.database_id()
            || seed.pr_node != parent.node_id()
            || seed.number != parent.number()
            || seed.repository != parent.repository()
        {
            return Err(bad(GithubErrorKind::IdentityChanged));
        }
        let resource = self.resource(parent.repository(), parent.number());
        let generation = self
            .runner
            .review_resource_generation_v11(&resource, cancel, deadline)
            .map_err(GithubError::from)?;
        let mut lines = vec![
            "FRESH CURRENT OUTCOME • point-in-time observation; causality remains UNKNOWN".into(),
            format!(
                "Current H {} • historical requested H {}",
                parent.head_sha(),
                seed.head
            ),
        ];
        let selected = if let Some(selected) = &seed.selected {
            let gql = self.graphql(
                CONVERSATION_COMMENT_QUERY_V17,
                json!({"id":selected.node}),
                None,
                cancel,
            )?;
            if !publication::error_free(&gql) {
                return Err(bad(GithubErrorKind::Json));
            }
            match gql.get("data").and_then(|v| v.get("node")) {
                Some(Value::Null) => {
                    lines.push(format!("Exact original IssueComment #{} / {} is currently node:null; observed absence, NOT attribution or retry permission",selected.id,selected.node));
                    None
                }
                Some(node) if node.is_object() => {
                    if node["id"].as_str() != Some(selected.node.as_str())
                        || thread_publication_v1::canonical_bigint(&node["fullDatabaseId"])
                            != Some(selected.id)
                    {
                        return Err(bad(GithubErrorKind::IdentityChanged));
                    }
                    let endpoint = self.comment_endpoint(
                        parent.summary(),
                        selected.id,
                        CommentKind::Conversation,
                    )?;
                    let raw = self.api(&endpoint, "GET", None, None, false, cancel)?;
                    if !bounded_comment(&raw) {
                        return Err(bad(GithubErrorKind::Unsupported));
                    }
                    let actual = crate::github_workspace::PrCommentSelection {
                        kind: CommentKind::Conversation,
                        observed: comment_from_json(&raw),
                    };
                    let comment = self.conversation_comment_pair_v17(
                        parent.clone(),
                        identity,
                        &actual.observed,
                        raw,
                        gql,
                        cancel,
                    )?;
                    if comment.observed.id != selected.id
                        || comment.observed.node_id != selected.node
                    {
                        return Err(bad(GithubErrorKind::IdentityChanged));
                    }
                    let actor = crate::identity::ActorIdentity::github(
                        &self.host,
                        &comment.observed.author,
                    );
                    lines.push(format!(
                        "Current comment #{} • [{}] {} • actual author UID {:?}",
                        selected.id,
                        actor.initials(),
                        actor.label(),
                        comment.author_account
                    ));
                    append_body(&mut lines, &comment.observed.body)?;
                    Some(comment)
                }
                _ => return Err(bad(GithubErrorKind::Json)),
            }
        } else {
            None
        };
        match seed.action {
            ConversationActionV17::Create => {
                lines.push("Original created comment identity was never observed; UNKNOWN attribution. No first/newest comment is chosen.".into());
                let mut complete = false;
                let mut seen = std::collections::BTreeSet::new();
                for page in 1..=4 {
                    live(cancel, deadline)?;
                    let value = self.api(
                        &format!(
                            "repos/{}/issues/{}/comments?per_page=100&page={page}",
                            parent.repository(),
                            parent.number()
                        ),
                        "GET",
                        None,
                        None,
                        false,
                        cancel,
                    )?;
                    let rows = value
                        .as_array()
                        .filter(|r| r.len() <= 100)
                        .ok_or_else(|| bad(GithubErrorKind::Json))?;
                    for raw in rows {
                        let id = positive(&raw["id"])
                            .filter(|id| seen.insert(*id))
                            .ok_or_else(|| bad(GithubErrorKind::Json))?;
                        if !bounded_comment(raw)
                            || !self.comment_belongs(
                                parent.summary(),
                                raw,
                                id,
                                CommentKind::Conversation,
                            )
                        {
                            return Err(bad(GithubErrorKind::IdentityChanged));
                        }
                        let author = Actor::from_json(&raw["user"]);
                        let actor = crate::identity::ActorIdentity::github(&self.host, &author);
                        lines.push(format!("Current catalogue comment #{id} • [{}] {} • NOT attributed to original attempt",actor.initials(),actor.label()));
                        append_body(
                            &mut lines,
                            raw["body"]
                                .as_str()
                                .ok_or_else(|| bad(GithubErrorKind::Json))?,
                        )?;
                    }
                    if rows.len() < 100 {
                        complete = true;
                        break;
                    }
                }
                if !complete {
                    return Err(bad(GithubErrorKind::Unsupported));
                }
            }
            ConversationActionV17::AddReaction | ConversationActionV17::RemoveReaction => {
                if let Some(comment) = selected {
                    let reactions = self.observe_conversation_reactions_v17(comment, cancel)?;
                    if !reactions.complete() {
                        return Err(bad(GithubErrorKind::Unsupported));
                    }
                    lines.push(format!(
                        "Complete current reaction catalogue • {} entries",
                        reactions.reactions().len()
                    ));
                    for reaction in reactions.reactions() {
                        let actor =
                            crate::identity::ActorIdentity::github(&self.host, &reaction.actor);
                        lines.push(format!(
                            "Current reaction #{} / {} • {} • [{}] {} • actual UID {:?}",
                            reaction.id,
                            reaction.node_id,
                            crate::git::display_bytes(reaction.content.as_bytes()),
                            actor.initials(),
                            actor.label(),
                            reaction.account_id
                        ));
                    }
                } else {
                    lines.push("Original comment is currently absent; individual reaction outcome/causality remains UNKNOWN".into());
                }
            }
            ConversationActionV17::Subscription => {
                let current =
                    self.observe_conversation_subscription_v17(parent.clone(), identity, cancel)?;
                lines.push(format!("Currently observed subscription {} • actual viewerCanSubscribe={}; not notification delivery",current.current.state_label(),current.current.viewer_can_subscribe));
            }
            ConversationActionV17::Edit | ConversationActionV17::Delete => {}
        }
        if lines.len() > 50_000
            || lines
                .iter()
                .try_fold(0usize, |n, s| n.checked_add(s.len()))
                .is_none_or(|n| n > 8 * 1024 * 1024)
        {
            return Err(bad(GithubErrorKind::Unsupported));
        }
        let observed = ConversationRecoveryCurrentV17 {
            issuer: self.runner.clone(),
            host: self.host.clone(),
            program: self.program.clone(),
            environment: self.env.clone(),
            parent,
            identity: identity.clone(),
            resource,
            generation,
            cancel: cancel.clone(),
            deadline,
            lines: lines.into(),
        };
        observed.validate(self, cancel, deadline)?;
        Ok(observed)
    }
    pub(crate) fn validate_conversation_inspection_v17(
        &self,
        current: &ConversationRecoveryCurrentV17,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        current.validate(self, cancel, deadline)?;
        if !GithubProvider::same_thread_history_parent_v1(&current.parent, parent)
            || current.identity.host != identity.host
            || current.identity.account_id != identity.account_id
        {
            return Err(bad(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
    pub(crate) fn reconcile_conversation_inspection_v17(
        &self,
        current: ConversationRecoveryCurrentV17,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        current.validate(self, cancel, deadline)?;
        self.runner
            .reconcile_review_generation_v11(
                &current.resource,
                current.generation,
                cancel,
                deadline.min(current.deadline),
            )
            .map_err(GithubError::from)
    }
}
fn append_body(lines: &mut Vec<String>, body: &str) -> GithubResult<()> {
    // Exact escaped display-size/row preflight BEFORE cloning each body.
    let existing = lines
        .iter()
        .try_fold(0usize, |n, s| n.checked_add(s.len()))
        .ok_or_else(|| bad(GithubErrorKind::Unsupported))?;
    let mut bytes = existing;
    let mut rows = lines.len();
    for line in body.lines() {
        rows = rows
            .checked_add(1)
            .ok_or_else(|| bad(GithubErrorKind::Unsupported))?;
        let escaped = crate::git::display_bytes(line.as_bytes());
        bytes = bytes
            .checked_add(escaped.len())
            .filter(|n| *n <= 8 * 1024 * 1024)
            .ok_or_else(|| bad(GithubErrorKind::Unsupported))?;
        if rows > 50_000 {
            return Err(bad(GithubErrorKind::Unsupported));
        }
    }
    lines.extend(
        body.lines()
            .map(|s| crate::git::display_bytes(s.as_bytes())),
    );
    Ok(())
}
