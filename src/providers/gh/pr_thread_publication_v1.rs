//! Exact original-thread publication. No legacy login-only write fallback,
//! opaque-ID decoding, implicit root selection or repeat-write capability.
use super::*;
use std::time::Instant;
#[path = "pr_thread_comment_v2.rs"]
mod comment_v2;
pub use comment_v2::*;

#[derive(Debug)]
pub enum ThreadPublicationFailureV1 {
    NotStarted(GithubError),
    WriteMayHaveStarted(GithubError),
}
/// Constructed only after this provider actually reads the selected opaque
/// node under a genuine raw original parent. Private fields, no Deserialize.
pub struct OriginalThreadPublicationV1 {
    parent: PrResourceParentV1,
    thread: MetricsReviewThreadObservation,
    producer: std::sync::Arc<crate::providers::runner::ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
}
impl OriginalThreadPublicationV1 {
    pub fn parent(&self) -> &PrResourceParentV1 {
        &self.parent
    }
    pub fn thread(&self) -> &MetricsReviewThreadObservation {
        &self.thread
    }
    pub fn original_reply_root(&self) -> GithubResult<&MetricsThreadComment> {
        if !self.thread.comment_coverage.enumeration_complete
            || self.thread.comment_coverage.excluded_rows != 0
        {
            return Err(error(GithubErrorKind::Unsupported, "Bounded comment enumeration is incomplete; original-root uniqueness remains unknown, not permission to reply"));
        }
        let mut roots = self
            .thread
            .comments
            .iter()
            .filter(|comment| matches!(comment.reply_to, MetricsThreadReplyTo::Original));
        let root = roots.next().filter(|_| roots.next().is_none()).ok_or_else(|| error(GithubErrorKind::Unsupported, "No unique explicit replyTo:null original comment was actually observed within the bounded pages"))?;
        if root
            .database_id
            .is_none_or(|id| id == 0 || id > i64::MAX as u64)
            || root
                .commit_sha
                .as_deref()
                .is_none_or(|sha| !crate::search::model::valid_sha(sha))
            || root
                .observed_path
                .as_deref()
                .is_none_or(|path| path.is_empty() || path.len() > 4096)
        {
            return Err(error(GithubErrorKind::Unsupported, "Actual original REST comment ID/commit/path unavailable; no opaque-ID decoding or H substitution"));
        }
        Ok(root)
    }
}
impl std::fmt::Debug for OriginalThreadPublicationV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalThreadPublicationV1(<actual bounded observation; no write permit>)")
    }
}
pub struct ThreadResolutionReceiptV1 {
    thread_node: String,
    resolved: bool,
    resolver: Option<MetricsActor>,
}
impl ThreadResolutionReceiptV1 {
    pub fn thread_node(&self) -> &str {
        &self.thread_node
    }
    pub fn resolved(&self) -> bool {
        self.resolved
    }
    /// Observed resolver only; independent readback cannot establish causality.
    pub fn resolver(&self) -> Option<&MetricsActor> {
        self.resolver.as_ref()
    }
}
pub struct ThreadReplyReceiptV1 {
    comment: ConversationComment,
    account_id: u64,
    original_comment_id: u64,
    observed_parent_commit: String,
}
impl ThreadReplyReceiptV1 {
    pub fn comment(&self) -> &ConversationComment {
        &self.comment
    }
    pub fn account_id(&self) -> u64 {
        self.account_id
    }
    pub fn original_comment_id(&self) -> u64 {
        self.original_comment_id
    }
    pub fn observed_parent_commit(&self) -> &str {
        &self.observed_parent_commit
    }
}
fn phase(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(error(
            GithubErrorKind::Transport,
            "Original thread publication phase cancelled or expired",
        ))
    } else {
        Ok(())
    }
}
fn opaque_node(node: &str) -> bool {
    !node.is_empty() && node.len() <= 512 && !node.chars().any(char::is_control)
}
pub(super) fn canonical_bigint(value: &Value) -> Option<u64> {
    if let Some(id) = value.as_u64() {
        return (id > 0).then_some(id);
    }
    value
        .as_str()
        .filter(|raw| {
            !raw.is_empty()
                && raw.len() <= 20
                && !raw.starts_with('0')
                && raw.bytes().all(|byte| byte.is_ascii_digit())
        })
        .and_then(|raw| raw.parse().ok())
        .filter(|id| *id > 0)
}
pub const THREAD_RESOLUTION_STATE_QUERY_V2: &str = "query($id:ID!){node(id:$id){__typename ... on PullRequestReviewThread{id isResolved resolvedBy{__typename databaseId login name avatarUrl} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}}}";
pub const THREAD_RESOLUTION_MUTATION_FIELDS_V2: &str = "thread{id isResolved resolvedBy{__typename databaseId login name avatarUrl} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}";
pub const THREAD_POSTWRITE_CAPS_V2: [usize; 6] = [
    256 * 1024,
    256 * 1024,
    64 * 1024,
    128 * 1024,
    128 * 1024,
    64 * 1024,
];
fn graph_errors(value: &Value) -> bool {
    value.get("errors").is_some_and(|errors| {
        !errors.is_null() && errors.as_array().is_none_or(|errors| !errors.is_empty())
    })
}

impl GithubProvider {
    pub(crate) fn original_thread_publication_v1(
        &self,
        parent: PrResourceParentV1,
        node: &str,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<OriginalThreadPublicationV1> {
        phase(cancel, deadline)?;
        let producer = self
            .scoped_read_budget
            .as_ref()
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Genuine original measured thread producer required",
                )
            })?
            .clone();
        producer.check_clock(cancel).map_err(GithubError::from)?;
        if !opaque_node(node) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Selected actual thread node unavailable",
            ));
        }
        let thread = self.pr_selected_thread_observation_v1(&parent, node, cancel, deadline)?;
        phase(cancel, deadline)?;
        producer.check_clock(cancel).map_err(GithubError::from)?;
        Ok(OriginalThreadPublicationV1 {
            parent,
            thread,
            producer,
            cancellation: cancel.clone(),
            deadline,
        })
    }
    fn thread_original_producer_v2(
        &self,
        original: &OriginalThreadPublicationV1,
        measured: &GithubProvider,
        capture: &PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Instant> {
        let effective = deadline.min(original.deadline).min(capture.deadline());
        phase(cancel, effective)?;
        if self.scoped_read_budget.is_some()
            || !self.runner.same_instance(&measured.runner)
            || self.host != measured.host
            || self.program != measured.program
            || self.env != measured.env
            || !original.cancellation.same_instance(cancel)
            || measured
                .scoped_read_budget
                .as_ref()
                .is_none_or(|actual| !std::sync::Arc::ptr_eq(actual, &original.producer))
        {
            return Err(error(GithubErrorKind::InvalidTarget, "Thread publication cannot adopt a foreign original token/producing allocation or measured WRITE transport"));
        }
        capture.validate_prewrite(measured, cancel)?;
        Ok(effective)
    }
    /// Four prepaid children after exact creation/readback: current UID,
    /// actual original repo+raw PR, caller's original SQL/draft cleanup, final
    /// UID. No second pool, version/auth fallback or payload read after fence.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn close_thread_postwrite_visibility_v2(
        &self,
        identity: &HostIdentity,
        original: &OriginalThreadPublicationV1,
        current: &CurrentCatalogueRepository,
        capture: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
        local: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<()> {
        let deadline = deadline.min(original.deadline).min(capture.deadline());
        phase(cancel, deadline)?;
        if !original.cancellation.same_instance(cancel)
            || identity.host != self.host
            || identity.account_id.is_none_or(|id| id == 0)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original postwrite thread principal/token unavailable",
            ));
        }
        let opening = capture.api(self, "user", "GET", None, None, cancel)?;
        if opening["id"].as_u64() != identity.account_id {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Postwrite opening numeric account changed",
            ));
        }
        self.pr_thread_postwrite_parent_v2(current, &original.parent, capture, cancel)?;
        phase(cancel, deadline)?;
        local()?;
        phase(cancel, deadline)?;
        let closing = capture.api(self, "user", "GET", None, None, cancel)?;
        if closing["id"].as_u64() != identity.account_id {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Postwrite final numeric account changed; original private content unavailable",
            ));
        }
        phase(cancel, deadline)
    }
    fn thread_write_principal_v1(
        &self,
        identity: &HostIdentity,
        original: &OriginalThreadPublicationV1,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        phase(cancel, deadline)?;
        if identity.host != self.host
            || identity
                .account_id
                .is_none_or(|id| id == 0 || id > i64::MAX as u64)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Thread write needs the original positive numeric account",
            ));
        }
        let current = self.api("user", "GET", None, None, false, cancel)?;
        if current["id"].as_u64() != identity.account_id {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Thread write numeric account changed",
            ));
        }
        // Exact raw repo/PR IDs, full head and base; login is display only.
        self.pr_thread_resource_parent_v1(&original.parent, cancel)?;
        phase(cancel, deadline)
    }
    fn thread_state_response_v1(
        &self,
        original: &OriginalThreadPublicationV1,
        value: &Value,
        desired: bool,
    ) -> bool {
        let parent = &original.parent;
        let pr = &value["pullRequest"];
        let repo = &value["repository"];
        value["id"].as_str() == Some(original.thread.node_id.as_str())
            && value["isResolved"].as_bool() == Some(desired)
            && pr["id"].as_str() == Some(parent.node_id())
            && canonical_bigint(&pr["fullDatabaseId"]) == Some(parent.database_id())
            && pr["number"].as_u64() == Some(parent.number())
            && pr["headRefOid"].as_str() == Some(parent.head_sha())
            && pr["baseRefOid"].as_str() == Some(parent.base_tip_sha())
            && repo["id"].as_str() == Some(parent.repository_node())
            && repo["databaseId"].as_u64() == Some(parent.repository_id())
            && repo["nameWithOwner"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(parent.repository()))
            && pr["repository"] == *repo
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_thread_guarded_v1(
        &self,
        identity: &HostIdentity,
        original: &OriginalThreadPublicationV1,
        measured: &GithubProvider,
        capture: &mut PostWriteCaptureAllowanceV2,
        desired: bool,
        cancel: &CancellationToken,
        deadline: Instant,
        before_post: impl FnOnce() -> GithubResult<()>,
    ) -> Result<ThreadResolutionReceiptV1, ThreadPublicationFailureV1> {
        let mut invoked = false;
        let result = (|| {
            let deadline =
                self.thread_original_producer_v2(original, measured, capture, cancel, deadline)?;
            phase(cancel, deadline)?;
            if self.scoped_read_budget.is_some()
                || original.thread.resolved.is_none()
                || original.thread.resolved == Some(desired)
            {
                return Err(error(GithubErrorKind::InvalidTarget, "Fresh different known thread state required; an observation is not a new write"));
            }
            if (desired && original.thread.viewer_can_resolve != Some(true))
                || (!desired && original.thread.viewer_can_unresolve != Some(true))
            {
                return Err(error(
                    GithubErrorKind::Permission,
                    "Actual current thread capability does not allow this state transition",
                ));
            }
            measured.thread_write_principal_v1(identity, original, cancel, deadline)?;
            let provider = self.clone().with_catalogue_read_deadline(deadline);
            let (name, input) = if desired {
                ("resolveReviewThread", "ResolveReviewThreadInput")
            } else {
                ("unresolveReviewThread", "UnresolveReviewThreadInput")
            };
            let fields = THREAD_RESOLUTION_MUTATION_FIELDS_V2;
            let mutation = format!("mutation($input:{input}!){{{name}(input:$input){{{fields}}}}}");
            before_post()?;
            phase(cancel, deadline)?;
            invoked = true;
            let created = capture.api(&provider, "graphql", "POST", Some(json!({"query":mutation,"variables":{"input":{"threadId":original.thread.node_id}}})), Some(provider.resource(original.parent.repository(), original.parent.number())), cancel)?;
            if graph_errors(&created) {
                return Err(provider.ambiguous_pr(
                    original.parent.summary(),
                    "Thread mutation GraphQL errors forbid a receipt even beside data",
                ));
            }
            if !provider.thread_state_response_v1(
                original,
                &created["data"][name]["thread"],
                desired,
            ) {
                return Err(provider.ambiguous_pr(original.parent.summary(), "Thread mutation response did not confirm exact node/parent/observed state; inspect without retry"));
            }
            phase(cancel, deadline)?;
            let value = capture.api(&provider, "graphql", "POST", Some(json!({"query":THREAD_RESOLUTION_STATE_QUERY_V2,"variables":{"id":original.thread.node_id}})), None, cancel)?;
            let observed = &value["data"]["node"];
            if graph_errors(&value)
                || observed["__typename"] != "PullRequestReviewThread"
                || !provider.thread_state_response_v1(original, observed, desired)
            {
                return Err(provider.ambiguous_pr(original.parent.summary(), "Thread exact-node readback does not observe requested state; no causality or retry inferred"));
            }
            phase(cancel, deadline)?;
            let receipt = ThreadResolutionReceiptV1 {
                thread_node: text(observed, "id"),
                resolved: desired,
                resolver: observed
                    .get("resolvedBy")
                    .filter(|value| value.is_object())
                    .filter(|value| {
                        ["login", "name", "avatarUrl", "__typename"]
                            .iter()
                            .all(|field| {
                                value[*field]
                                    .as_str()
                                    .is_none_or(|value| value.len() <= 4096)
                            })
                    })
                    .map(|value| MetricsActor {
                        account_id: if value["__typename"] == "User" {
                            value["databaseId"].as_u64().filter(|id| *id > 0)
                        } else {
                            None
                        },
                        actor: Actor::from_json(value),
                    }),
            };
            phase(cancel, deadline)?;
            Ok(receipt)
        })();
        result.map_err(|error| {
            if invoked {
                self.runner.mark_ambiguous(
                    &self.resource(original.parent.repository(), original.parent.number()),
                );
                ThreadPublicationFailureV1::WriteMayHaveStarted(error)
            } else {
                ThreadPublicationFailureV1::NotStarted(error)
            }
        })
    }
    #[allow(clippy::too_many_arguments)]
    fn thread_reply_matches_v1(
        &self,
        identity: &HostIdentity,
        original: &OriginalThreadPublicationV1,
        root: &MetricsThreadComment,
        body: &str,
        value: &Value,
        id: u64,
        node: &str,
    ) -> bool {
        id > 0
            && id <= i64::MAX as u64
            && opaque_node(node)
            && self.comment_belongs(original.parent.summary(), value, id, CommentKind::Review)
            && value["node_id"].as_str() == Some(node)
            && value["user"]["id"].as_u64() == identity.account_id
            && value["user"]["login"]
                .as_str()
                .is_some_and(|login| !login.is_empty() && login.len() <= 4096)
            && ["login", "name", "type", "avatar_url"].iter().all(|field| {
                value["user"][*field]
                    .as_str()
                    .is_none_or(|text| text.len() <= 4096)
            })
            && value["body"].as_str() == Some(body)
            && value["in_reply_to_id"].as_u64() == root.database_id
            && value["commit_id"].as_str() == root.commit_sha.as_deref()
            && value["path"].as_str() == root.observed_path.as_deref()
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn reply_thread_guarded_v1(
        &self,
        identity: &HostIdentity,
        original: &OriginalThreadPublicationV1,
        measured: &GithubProvider,
        capture: &mut PostWriteCaptureAllowanceV2,
        body: &str,
        cancel: &CancellationToken,
        deadline: Instant,
        before_post: impl FnOnce() -> GithubResult<()>,
    ) -> Result<ThreadReplyReceiptV1, ThreadPublicationFailureV1> {
        let mut invoked = false;
        let result = (|| {
            let deadline =
                self.thread_original_producer_v2(original, measured, capture, cancel, deadline)?;
            phase(cancel, deadline)?;
            if self.scoped_read_budget.is_some()
                || original.thread.viewer_can_reply != Some(true)
                || body.len() > 65_536
                || body.trim().is_empty()
            {
                return Err(error(
                    GithubErrorKind::Permission,
                    "Fresh viewerCanReply and nonempty bounded body required",
                ));
            }
            let root = original.original_reply_root()?;
            let payload = json!({"body":body});
            if serde_json::to_vec(&payload)
                .map_err(|_| error(GithubErrorKind::InvalidTarget, "Reply body cannot encode"))?
                .len()
                > 128 * 1024
            {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Encoded reply exceeds128KiB; no text truncated",
                ));
            }
            measured.thread_write_principal_v1(identity, original, cancel, deadline)?;
            let provider = self.clone().with_catalogue_read_deadline(deadline);
            let root_id = root.database_id.expect("actual root ID checked");
            let endpoint = format!(
                "{}/comments/{root_id}/replies",
                provider.pr_endpoint(original.parent.summary())
            );
            before_post()?;
            phase(cancel, deadline)?;
            invoked = true;
            let created = capture.api(
                &provider,
                &endpoint,
                "POST",
                Some(payload),
                Some(provider.resource(original.parent.repository(), original.parent.number())),
                cancel,
            )?;
            let id = number(&created, "id");
            let node = text(&created, "node_id");
            if !provider
                .thread_reply_matches_v1(identity, original, root, body, &created, id, &node)
            {
                return Err(provider.ambiguous_pr(original.parent.summary(), "Thread reply create response differs from actual parent root/commit/path/body/current numeric actor"));
            }
            phase(cancel, deadline)?;
            let readback = capture.api(
                &provider,
                &provider.comment_endpoint(original.parent.summary(), id, CommentKind::Review)?,
                "GET",
                None,
                None,
                cancel,
            )?;
            if !provider
                .thread_reply_matches_v1(identity, original, root, body, &readback, id, &node)
            {
                return Err(provider.ambiguous_pr(
                    original.parent.summary(),
                    "Thread reply exact-ID readback differs; inspect without resend",
                ));
            }
            phase(cancel, deadline)?;
            let receipt = ThreadReplyReceiptV1 {
                comment: comment_from_json(&readback),
                account_id: identity
                    .account_id
                    .expect("positive original principal checked"),
                original_comment_id: root_id,
                observed_parent_commit: root
                    .commit_sha
                    .as_ref()
                    .expect("actual observed root commit checked")
                    .clone(),
            };
            phase(cancel, deadline)?;
            Ok(receipt)
        })();
        result.map_err(|error| {
            if invoked {
                self.runner.mark_ambiguous(
                    &self.resource(original.parent.repository(), original.parent.number()),
                );
                ThreadPublicationFailureV1::WriteMayHaveStarted(error)
            } else {
                ThreadPublicationFailureV1::NotStarted(error)
            }
        })
    }
}
