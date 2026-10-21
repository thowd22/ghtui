//! Native PR collaboration operations. Requests are typed and bodies use JSON stdin.
use super::*;
mod inspect;
pub use inspect::*;
mod projects;
pub use projects::*;
#[path = "pr_metadata_capability_v25.rs"]
mod metadata_capability_v25;
#[cfg(test)]
pub(in crate::providers::gh) use metadata_capability_v25::METADATA_TEXT_CAPABILITY_QUERY_V25;
#[path = "pr_conversation_original_v17.rs"]
mod conversation_original_v17;
pub(crate) use conversation_original_v17::*;
#[path = "pr_inline_v1.rs"]
mod inline_v1;
pub(crate) use inline_v1::FreshInlineCommentObservationV14;
pub use inline_v1::{InlineCommentReceiptV1, InlinePublicationFailureV1};
#[path = "pr_thread_publication_v1.rs"]
mod thread_publication_v1;
pub use thread_publication_v1::*;
#[path = "pr_pending_review_v2.rs"]
mod pending_review_v2;
pub use pending_review_v2::*;

const STATE_CAPABILITY_QUERY_V68: &str = "query($id:ID!){node(id:$id){__typename ... on PullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged viewerCanClose viewerCanReopen repository{id nameWithOwner}}}}";

/// Pure data parser shared with the closed State response validators. This
/// delegates the existing canonical integer rules without exposing transport.
pub(in crate::providers::gh) fn canonical_database_id_v87(value: &Value) -> Option<u64> {
    thread_publication_v1::canonical_bigint(value)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestedTeam {
    pub id: u64,
    pub slug: String,
    pub name: String,
}
/// Context is loaded separately so the inbox never needs expensive metadata hydration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrContext {
    pub repository: Repository,
    pub issue_metadata: Value,
    pub requested_reviewers: Vec<Actor>,
    pub requested_teams: Vec<RequestedTeam>,
    pub head_repository: Option<Repository>,
    pub base_repository: Repository,
    pub is_cross_repository: bool,
    pub maintainer_can_modify: bool,
    pub head_sha: String,
    pub base_sha: String,
    pub source_urls: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MergeQueueEntry {
    pub id: String,
    pub state: String,
    pub position: u64,
    pub head_sha: Option<String>,
    pub base_sha: Option<String>,
    pub enqueuer: Actor,
    pub enqueued_at: String,
    pub jump: bool,
}
/// Repository merge settings plus the caller's permission. `None` means the
/// host did not report the setting; it is never guessed.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoMergeSettings {
    pub allow_merge_commit: Option<bool>,
    pub allow_squash_merge: Option<bool>,
    pub allow_rebase_merge: Option<bool>,
    pub allow_auto_merge: Option<bool>,
    pub default_branch: String,
    pub permissions: Permissions,
}
impl RepoMergeSettings {
    pub fn can_write(&self) -> bool {
        self.permissions.push || self.permissions.maintain || self.permissions.admin
    }
    pub fn allows(&self, method: MergeMethod) -> bool {
        let setting = match method {
            MergeMethod::Merge => self.allow_merge_commit,
            MergeMethod::Squash => self.allow_squash_merge,
            MergeMethod::Rebase => self.allow_rebase_merge,
        };
        setting != Some(false)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MergeContext {
    pub merge_state_status: String,
    pub review_decision: String,
    pub viewer_can_merge_as_admin: bool,
    pub viewer_can_enable_auto_merge: bool,
    pub viewer_can_disable_auto_merge: bool,
    pub auto_merge_request: Option<Value>,
    pub queue: Option<MergeQueueEntry>,
}
/// The pending auto-merge request as GitHub reported it: who enabled it,
/// with which method and when. A missing enabler keeps the honest
/// `unknown actor` fallback; nothing is guessed from the current account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoMergeRequestInfo {
    pub enabled_by: Actor,
    pub method: String,
    pub enabled_at: String,
}
impl MergeContext {
    pub fn auto_merge_info(&self) -> Option<AutoMergeRequestInfo> {
        let request = self.auto_merge_request.as_ref()?;
        Some(AutoMergeRequestInfo {
            enabled_by: Actor::from_json(&request["enabledBy"]),
            method: text(request, "mergeMethod"),
            enabled_at: text(request, "enabledAt"),
        })
    }
}
pub const PR_MERGE_CONTEXT_QUERY: &str = "query($id:ID!){node(id:$id){... on PullRequest{id headRefOid baseRefOid mergeStateStatus reviewDecision viewerCanMergeAsAdmin viewerCanEnableAutoMerge viewerCanDisableAutoMerge autoMergeRequest{mergeMethod enabledAt enabledBy{__typename login avatarUrl}} mergeQueueEntry{id state position headCommit{oid} baseCommit{oid} enqueuer{__typename login avatarUrl} enqueuedAt jump}}}}";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PrTextEdit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IssueMetadataEdit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignees: Option<Vec<String>>,
    /// None keeps the existing milestone; Some(None) explicitly removes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone: Option<Option<u64>>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReviewerSelection {
    pub reviewers: Vec<String>,
    pub team_reviewers: Vec<String>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum DiffSide {
    Left,
    Right,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InlineComment {
    pub path: String,
    pub side: DiffSide,
    pub line: u64,
    pub start_line: Option<u64>,
    pub start_side: Option<DiffSide>,
    pub body: String,
}
impl InlineComment {
    pub fn validate(&self) -> GithubResult<()> {
        if self.path.is_empty()
            || self.path.starts_with('/')
            || self.path.split('/').any(|p| p == "..")
            || self.path.contains('\0')
            || self.line == 0
            || self.body.trim().is_empty()
        {
            return Err(error(GithubErrorKind::InvalidTarget,"Inline comment needs a canonical relative path, positive diff line and nonempty body"));
        }
        match (self.start_line,self.start_side) {
            (None,None)=>Ok(()),
            (Some(start),Some(side)) if start>0&&side==self.side&&start<self.line=>Ok(()),
            _=>Err(error(GithubErrorKind::InvalidTarget,"Multiline ranges must use a consistent side and start before their end; mixed-side selections require separate comments")),
        }
    }
    fn payload(&self) -> Value {
        let mut value =
            json!({"path":self.path,"side":self.side,"line":self.line,"body":self.body});
        if let Some(line) = self.start_line {
            value["start_line"] = json!(line);
            value["start_side"] = json!(self.start_side);
        }
        value
    }
    pub fn validate_patch(&self, patch: &str) -> GithubResult<()> {
        self.validate()?;
        inline_v1::validate_inline_patch(self, patch)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrReview {
    pub id: u64,
    pub node_id: String,
    pub author: Actor,
    pub body: String,
    pub state: String,
    pub commit_id: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentKind {
    Conversation,
    Review,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Reaction {
    ThumbsUp,
    ThumbsDown,
    Laugh,
    Confused,
    Heart,
    Hooray,
    Rocket,
    Eyes,
}
impl Reaction {
    fn value(self) -> &'static str {
        match self {
            Self::ThumbsUp => "+1",
            Self::ThumbsDown => "-1",
            Self::Laugh => "laugh",
            Self::Confused => "confused",
            Self::Heart => "heart",
            Self::Hooray => "hooray",
            Self::Rocket => "rocket",
            Self::Eyes => "eyes",
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Subscription {
    Subscribed,
    Unsubscribed,
    Ignored,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewThread {
    pub id: String,
    pub path: String,
    pub side: String,
    pub line: Option<u64>,
    pub start_line: Option<u64>,
    pub resolved: bool,
    pub outdated: bool,
    pub viewer_can_resolve: bool,
    pub viewer_can_unresolve: bool,
    pub comments: Vec<Value>,
    pub comments_complete: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ViewedFile {
    pub path: String,
    pub state: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatePr {
    pub repository: String,
    pub head_repository: String,
    pub head: String,
    pub base: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
    pub expected_head_sha: String,
    pub expected_base_sha: String,
}

// SAME legacy validation/payload grammar. W9's closed provider only adds its
// own borrowed resource bounds/provenance before calling these pure helpers.
pub(in crate::providers::gh) fn validate_create_pr_request_v9(
    request: &CreatePr,
) -> GithubResult<()> {
    validate_repo(&request.repository)?;
    validate_repo(&request.head_repository)?;
    validate_sha(&request.expected_head_sha)?;
    validate_sha(&request.expected_base_sha)?;
    if request.title.trim().is_empty()
        || request.head.is_empty()
        || request.base.is_empty()
        || request.head.contains('\0')
        || request.base.contains('\0')
    {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "PR publication needs an explicit remote head/base and nonempty title",
        ));
    }
    Ok(())
}
pub(in crate::providers::gh) fn create_pr_payload_v9(request: &CreatePr, head_name: &str) -> Value {
    json!({"head":head_name,"head_repo":request.head_repository.split('/').nth(1),"base":request.base,"title":request.title,"body":request.body,"draft":request.draft})
}

impl GithubProvider {
    pub fn merge_context(
        &self,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> GithubResult<MergeContext> {
        let value = self.graphql(
            PR_MERGE_CONTEXT_QUERY,
            json!({"id":target.pr.id}),
            None,
            cancel,
        )?;
        let node = &value["data"]["node"];
        if text(node, "id") != target.pr.id
            || text(node, "headRefOid") != target.head_sha
            || text(node, "baseRefOid") != target.base_sha
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Merge context belongs to a different PR comparison; refresh before confirming",
            ));
        }
        for field in [
            "viewerCanMergeAsAdmin",
            "viewerCanEnableAutoMerge",
            "viewerCanDisableAutoMerge",
        ] {
            if !node[field].is_boolean() {
                return Err(error(
                    GithubErrorKind::Unsupported,
                    format!("The host did not provide {field}; no permission is inferred"),
                ));
            }
        }
        let queue = if node["mergeQueueEntry"].is_null() {
            None
        } else {
            let q = &node["mergeQueueEntry"];
            if text(q, "id").is_empty() {
                return Err(error(
                    GithubErrorKind::Json,
                    "Merge queue entry has no canonical ID",
                ));
            }
            Some(MergeQueueEntry {
                id: text(q, "id"),
                state: text(q, "state"),
                position: number(q, "position"),
                head_sha: q["headCommit"]["oid"].as_str().map(str::to_owned),
                base_sha: q["baseCommit"]["oid"].as_str().map(str::to_owned),
                enqueuer: Actor::from_json(&q["enqueuer"]),
                enqueued_at: text(q, "enqueuedAt"),
                jump: boolean(q, "jump"),
            })
        };
        Ok(MergeContext {
            merge_state_status: text(node, "mergeStateStatus"),
            review_decision: text(node, "reviewDecision"),
            viewer_can_merge_as_admin: boolean(node, "viewerCanMergeAsAdmin"),
            viewer_can_enable_auto_merge: boolean(node, "viewerCanEnableAutoMerge"),
            viewer_can_disable_auto_merge: boolean(node, "viewerCanDisableAutoMerge"),
            auto_merge_request: if node["autoMergeRequest"].is_null() {
                None
            } else {
                Some(node["autoMergeRequest"].clone())
            },
            queue,
        })
    }
    pub fn enqueue_pr(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        jump: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<MergeContext> {
        if target.pr.draft || target.pr.state != "open" {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Only an open ready PR can enter a merge queue",
            ));
        }
        if self.merge_context(target, cancel)?.queue.is_some() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "The PR is already in a merge queue; inspect the existing entry",
            ));
        }
        let result = self.gql_pr_write(
            identity,
            target,
            (
                "enqueuePullRequest",
                "EnqueuePullRequestInput",
                "mergeQueueEntry{id}",
            ),
            json!({"pullRequestId":target.pr.id,"expectedHeadOid":target.head_sha,"jump":jump}),
            cancel,
        )?;
        let created_id = text(
            &result["data"]["enqueuePullRequest"]["mergeQueueEntry"],
            "id",
        );
        if created_id.is_empty() {
            return Err(self.ambiguous_pr(
                target,
                "Enqueue response did not identify its queue entry; reconcile",
            ));
        }
        let fresh = self.after_write(
            &self.resource(&target.pr.repository, target.pr.number),
            self.merge_context(target, cancel),
        )?;
        if fresh
            .queue
            .as_ref()
            .is_none_or(|queue| queue.id != created_id || queue.jump != jump)
        {
            return Err(self.ambiguous_pr(target,"Merge queue entry/commit/priority was not confirmed; reconcile without enqueuing again"));
        }
        Ok(fresh)
    }
    pub fn dequeue_pr(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        observed: &MergeQueueEntry,
        cancel: &CancellationToken,
    ) -> GithubResult<MergeContext> {
        let context = self.merge_context(target, cancel)?;
        if context.queue.as_ref().map(|queue| queue.id.as_str()) != Some(observed.id.as_str()) {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Merge queue entry changed or disappeared; refresh before removing it",
            ));
        }
        self.gql_pr_write(
            identity,
            target,
            (
                "dequeuePullRequest",
                "DequeuePullRequestInput",
                "mergeQueueEntry{id}",
            ),
            json!({"id":target.pr.id}),
            cancel,
        )?;
        let fresh = self.after_write(
            &self.resource(&target.pr.repository, target.pr.number),
            self.merge_context(target, cancel),
        )?;
        if fresh.queue.is_some() {
            return Err(self.ambiguous_pr(
                target,
                "A merge queue entry remains visible after removal; reconcile",
            ));
        }
        Ok(fresh)
    }
    pub fn disable_auto_merge(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> GithubResult<MergeContext> {
        let context = self.merge_context(target, cancel)?;
        if context.auto_merge_request.is_none() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Auto-merge is not enabled",
            ));
        }
        if !context.viewer_can_disable_auto_merge {
            return Err(error(
                GithubErrorKind::Permission,
                "GitHub reports that the current actor cannot disable this auto-merge request",
            ));
        }
        self.gql_pr_write(
            identity,
            target,
            (
                "disablePullRequestAutoMerge",
                "DisablePullRequestAutoMergeInput",
                "pullRequest{id}",
            ),
            json!({"pullRequestId":target.pr.id}),
            cancel,
        )?;
        let fresh = self.after_write(
            &self.resource(&target.pr.repository, target.pr.number),
            self.merge_context(target, cancel),
        )?;
        if fresh.auto_merge_request.is_some() {
            return Err(self.ambiguous_pr(
                target,
                "Auto-merge is still enabled after the request; reconcile",
            ));
        }
        Ok(fresh)
    }
    pub fn pr_context(
        &self,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> GithubResult<PrContext> {
        validate_pr(&target.pr.repository, target.pr.number)?;
        let raw = self.api(&self.pr_endpoint(target), "GET", None, None, false, cancel)?;
        if text(&raw["head"], "sha") != target.head_sha
            || text(&raw["base"], "sha") != target.base_sha
            || text(&raw, "node_id") != target.pr.id
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "PR identity or comparison changed while opening metadata; refresh before editing",
            ));
        }
        let issue_metadata = self.issue_metadata(target, cancel)?;
        let repository = self.repository(&target.pr.repository, cancel)?;
        let head_repository = if raw["head"]["repo"].is_null() {
            None
        } else {
            Some(self.repo_from_json(&raw["head"]["repo"])?)
        };
        let base_repository = if raw["base"]["repo"].is_null() {
            repository.clone()
        } else {
            self.repo_from_json(&raw["base"]["repo"])?
        };
        let fresh = self.pr_summary(&target.pr.repository, target.pr.number, cancel)?;
        if fresh.head_sha != target.head_sha
            || fresh.base_sha != target.base_sha
            || fresh.pr.id != target.pr.id
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "PR comparison changed during metadata loading; refresh",
            ));
        }
        Ok(PrContext {
            is_cross_repository: head_repository
                .as_ref()
                .is_some_and(|head| head.full_name != base_repository.full_name),
            repository,
            issue_metadata,
            requested_reviewers: raw["requested_reviewers"]
                .as_array()
                .into_iter()
                .flatten()
                .map(Actor::from_json)
                .collect(),
            requested_teams: raw["requested_teams"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| RequestedTeam {
                    id: number(v, "id"),
                    slug: text(v, "slug"),
                    name: text(v, "name"),
                })
                .collect(),
            head_repository,
            base_repository,
            maintainer_can_modify: boolean(&raw, "maintainer_can_modify"),
            head_sha: target.head_sha.clone(),
            base_sha: target.base_sha.clone(),
            source_urls: vec![
                target.pr.url.clone(),
                format!("https://{}/{}", self.host, target.pr.repository),
            ],
        })
    }
    fn ambiguous_pr(&self, target: &PrSummary, message: &str) -> GithubError {
        self.runner
            .mark_ambiguous(&self.resource(&target.pr.repository, target.pr.number));
        error(GithubErrorKind::AmbiguousWrite, message)
    }
    pub(super) fn guard_target(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> GithubResult<PrSummary> {
        if target.pr.host != self.host || target.pr.id.is_empty() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "The selected PR lacks its canonical node or belongs to another host",
            ));
        }
        let fresh = self.guard(
            identity,
            &target.pr.repository,
            target.pr.number,
            &target.head_sha,
            cancel,
        )?;
        if fresh.pr.id != target.pr.id {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "The PR node identity changed; reload the selected resource",
            ));
        }
        if fresh.base_sha != target.base_sha {
            return Err(error(
                GithubErrorKind::StaleHead,
                "PR base changed since preview; refresh before applying this action",
            ));
        }
        Ok(fresh)
    }
    fn pr_endpoint(&self, target: &PrSummary) -> String {
        format!("repos/{}/pulls/{}", target.pr.repository, target.pr.number)
    }
    fn target_write(
        &self,
        target: &PrSummary,
        endpoint: &str,
        method: &str,
        input: Option<Value>,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        self.api(
            endpoint,
            method,
            input,
            Some(self.resource(&target.pr.repository, target.pr.number)),
            false,
            cancel,
        )
    }
    fn after_pr_write<T>(&self, target: &PrSummary, observed: GithubResult<T>) -> GithubResult<T> {
        self.after_write(
            &self.resource(&target.pr.repository, target.pr.number),
            observed,
        )
    }
    pub fn edit_pr_text(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        edit: &PrTextEdit,
        cancel: &CancellationToken,
    ) -> GithubResult<PrSummary> {
        if edit.title.as_ref().is_some_and(|t| t.trim().is_empty())
            || edit
                .base
                .as_ref()
                .is_some_and(|b| b.is_empty() || b.contains('\0'))
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "PR title and base cannot be empty",
            ));
        }
        if edit.title.is_none() && edit.body.is_none() && edit.base.is_none() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "No PR fields selected",
            ));
        }
        let fresh = self.guard_target(identity, target, cancel)?;
        if edit.title.is_some() && fresh.pr.title != target.pr.title
            || edit.body.is_some() && fresh.pr.body != target.pr.body
            || edit.base.is_some() && fresh.base_ref != target.base_ref
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "PR metadata changed since preview; refresh before replacing it",
            ));
        }
        self.target_write(
            target,
            &self.pr_endpoint(target),
            "PATCH",
            Some(
                serde_json::to_value(edit)
                    .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?,
            ),
            cancel,
        )?;
        let result = self.after_pr_write(
            target,
            self.pr_summary(&target.pr.repository, target.pr.number, cancel),
        )?;
        if edit.title.as_ref().is_some_and(|v| v != &result.pr.title)
            || edit.body.as_ref().is_some_and(|v| v != &result.pr.body)
            || edit.base.as_ref().is_some_and(|v| v != &result.base_ref)
        {
            self.runner
                .mark_ambiguous(&self.resource(&target.pr.repository, target.pr.number));
            return Err(error(
                GithubErrorKind::AmbiguousWrite,
                "PR metadata read-back did not match the selected values; reconcile before retry",
            ));
        }
        Ok(result)
    }
    pub fn issue_metadata(
        &self,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        self.api(
            &format!("repos/{}/issues/{}", target.pr.repository, target.pr.number),
            "GET",
            None,
            None,
            false,
            cancel,
        )
    }
    pub fn edit_issue_metadata(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        edit: &IssueMetadataEdit,
        observed: &Value,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        for login in edit.assignees.iter().flatten() {
            validate_name(login)?;
        }
        if edit.labels.is_none() && edit.assignees.is_none() && edit.milestone.is_none() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "No issue metadata fields selected",
            ));
        }
        self.guard_target(identity, target, cancel)?;
        let fresh = self.issue_metadata(target, cancel)?;
        for (changed, key) in [
            (edit.labels.is_some(), "labels"),
            (edit.assignees.is_some(), "assignees"),
            (edit.milestone.is_some(), "milestone"),
        ] {
            if changed && fresh.get(key) != observed.get(key) {
                return Err(error(
                    GithubErrorKind::StaleHead,
                    format!("{key} changed since preview; refresh before replacing them"),
                ));
            }
        }
        let endpoint = format!("repos/{}/issues/{}", target.pr.repository, target.pr.number);
        self.target_write(
            target,
            &endpoint,
            "PATCH",
            Some(
                serde_json::to_value(edit)
                    .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?,
            ),
            cancel,
        )?;
        let result = self.after_pr_write(target, self.issue_metadata(target, cancel))?;
        let names = |v: &Value, key: &str, field: &str| {
            let mut names = v[key]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.get(field).and_then(Value::as_str).or_else(|| v.as_str()))
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            names.sort();
            names.dedup();
            names
        };
        let desired = |values: &Vec<String>| {
            let mut v = values.clone();
            v.sort();
            v.dedup();
            v
        };
        if edit
            .labels
            .as_ref()
            .is_some_and(|v| desired(v) != names(&result, "labels", "name"))
            || edit
                .assignees
                .as_ref()
                .is_some_and(|v| desired(v) != names(&result, "assignees", "login"))
            || edit.milestone.as_ref().is_some_and(|m| match m {
                Some(n) => result["milestone"]["number"].as_u64() != Some(*n),
                None => !result["milestone"].is_null(),
            })
        {
            return Err(self.ambiguous_pr(target,"Issue metadata read-back did not match the selected values; reconcile partial fields before retry"));
        }
        Ok(result)
    }
    pub fn metadata_choices(
        &self,
        repo: &str,
        kind: MetadataChoices,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Value>> {
        validate_repo(repo)?;
        let endpoint = match kind {
            MetadataChoices::Labels => "labels?per_page=100",
            MetadataChoices::Assignees => "assignees?per_page=100",
            MetadataChoices::Milestones => "milestones?state=open&per_page=100",
            MetadataChoices::Branches => "branches?per_page=100",
            MetadataChoices::Collaborators => "collaborators?per_page=100",
            MetadataChoices::Teams => "teams?per_page=100",
        };
        self.api_list(&format!("repos/{repo}/{endpoint}"), None, cancel)
    }
    pub fn request_reviewers(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        selection: &ReviewerSelection,
        remove: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        for login in selection.reviewers.iter().chain(&selection.team_reviewers) {
            validate_name(login)?;
        }
        if selection.reviewers.is_empty() && selection.team_reviewers.is_empty() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Select at least one reviewer or team",
            ));
        }
        self.guard_target(identity, target, cancel)?;
        let endpoint = format!("{}/requested_reviewers", self.pr_endpoint(target));
        self.target_write(
            target,
            &endpoint,
            if remove { "DELETE" } else { "POST" },
            Some(
                serde_json::to_value(selection)
                    .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?,
            ),
            cancel,
        )?;
        let result = self.after_pr_write(
            target,
            self.api(&endpoint, "GET", None, None, false, cancel),
        )?;
        let users = result["users"]
            .as_array()
            .map(|a| a.iter().map(|v| text(v, "login")).collect::<BTreeSet<_>>())
            .unwrap_or_default();
        let teams = result["teams"]
            .as_array()
            .map(|a| a.iter().map(|v| text(v, "slug")).collect::<BTreeSet<_>>())
            .unwrap_or_default();
        if selection
            .reviewers
            .iter()
            .any(|u| users.contains(u) == remove)
            || selection
                .team_reviewers
                .iter()
                .any(|t| teams.contains(t) == remove)
        {
            return Err(self.ambiguous_pr(
                target,
                "Reviewer requests did not match server read-back; inspect partial changes",
            ));
        }
        Ok(result)
    }
    fn comment_endpoint(
        &self,
        target: &PrSummary,
        id: u64,
        kind: CommentKind,
    ) -> GithubResult<String> {
        if id == 0 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Comment ID must be positive",
            ));
        }
        Ok(format!(
            "repos/{}/{}/comments/{id}",
            target.pr.repository,
            if kind == CommentKind::Conversation {
                "issues"
            } else {
                "pulls"
            }
        ))
    }
    fn comment_belongs(
        &self,
        target: &PrSummary,
        value: &Value,
        id: u64,
        kind: CommentKind,
    ) -> bool {
        let parent = text(
            value,
            if kind == CommentKind::Conversation {
                "issue_url"
            } else {
                "pull_request_url"
            },
        );
        let Some((authority, path)) = parent
            .strip_prefix("https://")
            .and_then(|url| url.split_once('/'))
        else {
            return false;
        };
        let expected = format!(
            "repos/{}/{}/{}",
            target.pr.repository,
            if kind == CommentKind::Conversation {
                "issues"
            } else {
                "pulls"
            },
            target.pr.number
        );
        let path = path.strip_prefix("api/v3/").unwrap_or(path);
        number(value, "id") == id
            && (authority.eq_ignore_ascii_case(&self.host)
                || authority.eq_ignore_ascii_case(&format!("api.{}", self.host)))
            && path.eq_ignore_ascii_case(&expected)
    }
    fn rest_actor_matches(identity: &HostIdentity, actor: &Value) -> bool {
        text(actor, "login").eq_ignore_ascii_case(&identity.login)
            && identity
                .account_id
                .is_none_or(|id| id > 0 && number(actor, "id") == id)
    }
    fn owned_comment(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        observed: &ConversationComment,
        kind: CommentKind,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        self.guard_target(identity, target, cancel)?;
        let value = self.api(
            &self.comment_endpoint(target, observed.id, kind)?,
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        if !self.comment_belongs(target, &value, observed.id, kind)
            || !observed.node_id.is_empty() && text(&value, "node_id") != observed.node_id
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Comment does not belong to the selected PR",
            ));
        }
        if text(&value, "body") != observed.body
            || text(&value, "updated_at") != observed.updated_at
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Comment changed since preview; refresh before editing or deleting it",
            ));
        }
        if !Self::rest_actor_matches(identity, &value["user"]) {
            let permissions = self.repository(&target.pr.repository, cancel)?.permissions;
            if !permissions.push && !permissions.maintain && !permissions.admin {
                return Err(error(
                    GithubErrorKind::Permission,
                    "Only the comment author or an eligible repository collaborator may change it",
                ));
            }
        }
        Ok(value)
    }
    pub fn create_comment(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        body: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationComment> {
        if body.trim().is_empty() || body.len() > 65_536 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Comment needs text and must fit within 65536 UTF-8 bytes",
            ));
        }
        self.guard_target(identity, target, cancel)?;
        let created = self.target_write(
            target,
            &format!(
                "repos/{}/issues/{}/comments",
                target.pr.repository, target.pr.number
            ),
            "POST",
            Some(json!({"body":body})),
            cancel,
        )?;
        let id = number(&created, "id");
        let node = text(&created, "node_id");
        let valid = |value: &Value| {
            self.comment_belongs(target, value, id, CommentKind::Conversation)
                && text(value, "node_id") == node
                && text(value, "body") == body
                && Self::rest_actor_matches(identity, &value["user"])
        };
        if id == 0 || node.is_empty() || !valid(&created) {
            return Err(self.ambiguous_pr(target,"Comment response does not confirm the exact author/body/comment/node/PR identity; inspect before retrying"));
        }
        let fresh = self.after_pr_write(
            target,
            self.api(
                &self.comment_endpoint(target, id, CommentKind::Conversation)?,
                "GET",
                None,
                None,
                false,
                cancel,
            ),
        )?;
        if !valid(&fresh) {
            return Err(self.ambiguous_pr(target,"Created comment read-back changed author/body/comment/node/PR identity; inspect before retrying"));
        }
        Ok(comment_from_json(&fresh))
    }
    pub fn create_review(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        event: ReviewEvent,
        body: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        if identity.account_id.is_none_or(|id| id == 0) || body.len() > 65_536 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Review needs a verified numeric account and a summary within 65536 UTF-8 bytes",
            ));
        }
        let created = self.review(identity, target, event, body, cancel)?;
        let id = number(&created, "id");
        let node = text(&created, "node_id");
        let state = match event {
            ReviewEvent::Approve => "APPROVED",
            ReviewEvent::Comment => "COMMENTED",
            ReviewEvent::RequestChanges => "CHANGES_REQUESTED",
        };
        let valid = |value: &Value| {
            self.comment_belongs(target, value, id, CommentKind::Review)
                && text(value, "node_id") == node
                && text(value, "body") == body
                && text(value, "state") == state
                && text(value, "commit_id") == target.head_sha
                && Self::rest_actor_matches(identity, &value["user"])
        };
        if id == 0 || node.is_empty() || !valid(&created) {
            return Err(self.ambiguous_pr(target,"Review response does not confirm the exact author/body/decision/head/node/PR identity; inspect before retrying"));
        }
        let fresh = self.after_pr_write(
            target,
            self.api(
                &format!("{}/reviews/{id}", self.pr_endpoint(target)),
                "GET",
                None,
                None,
                false,
                cancel,
            ),
        )?;
        if !valid(&fresh) {
            return Err(self.ambiguous_pr(target,"Review read-back changed author/body/decision/head/node/PR identity; inspect before retrying"));
        }
        Ok(fresh)
    }
    pub fn edit_comment(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        observed: &ConversationComment,
        kind: CommentKind,
        body: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationComment> {
        if body.trim().is_empty() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Comment cannot be empty",
            ));
        }
        self.owned_comment(identity, target, observed, kind, cancel)?;
        let endpoint = self.comment_endpoint(target, observed.id, kind)?;
        self.target_write(
            target,
            &endpoint,
            "PATCH",
            Some(json!({"body":body})),
            cancel,
        )?;
        let result = self.after_pr_write(
            target,
            self.api(&endpoint, "GET", None, None, false, cancel),
        )?;
        if text(&result, "body") != body
            || !self.comment_belongs(target, &result, observed.id, kind)
            || !observed.node_id.is_empty() && text(&result, "node_id") != observed.node_id
        {
            self.runner
                .mark_ambiguous(&self.resource(&target.pr.repository, target.pr.number));
            return Err(error(
                GithubErrorKind::AmbiguousWrite,
                "Comment read-back differs from the submitted text or exact comment identity",
            ));
        }
        Ok(comment_from_json(&result))
    }
    pub fn delete_comment(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        observed: &ConversationComment,
        kind: CommentKind,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.owned_comment(identity, target, observed, kind, cancel)?;
        self.target_write(
            target,
            &self.comment_endpoint(target, observed.id, kind)?,
            "DELETE",
            None,
            cancel,
        )?;
        let endpoint = format!(
            "{}/comments?per_page=100",
            if kind == CommentKind::Conversation {
                format!("repos/{}/issues/{}", target.pr.repository, target.pr.number)
            } else {
                self.pr_endpoint(target)
            }
        );
        if self
            .after_pr_write(target, self.api_list(&endpoint, None, cancel))?
            .iter()
            .any(|v| number(v, "id") == observed.id)
        {
            self.runner
                .mark_ambiguous(&self.resource(&target.pr.repository, target.pr.number));
            return Err(error(
                GithubErrorKind::AmbiguousWrite,
                "Deleted comment is still visible; reconcile before retry",
            ));
        }
        Ok(())
    }
    pub fn react_comment(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        id: u64,
        kind: CommentKind,
        reaction: Reaction,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Value>> {
        self.guard_target(identity, target, cancel)?;
        let comment = self.comment_endpoint(target, id, kind)?;
        let parent = self.api(&comment, "GET", None, None, false, cancel)?;
        if !self.comment_belongs(target, &parent, id, kind) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Reaction target belongs to another PR",
            ));
        }
        let endpoint = format!("{comment}/reactions");
        let created = self.target_write(
            target,
            &endpoint,
            "POST",
            Some(json!({"content":reaction.value()})),
            cancel,
        )?;
        let result = self.after_pr_write(
            target,
            self.api_list(&format!("{endpoint}?per_page=100"), None, cancel),
        )?;
        let created_id = number(&created, "id");
        let matches = |value: &Value| {
            number(value, "id") == created_id
                && text(value, "content") == reaction.value()
                && Self::rest_actor_matches(identity, &value["user"])
        };
        if created_id == 0 || !matches(&created) || !result.iter().any(matches) {
            return Err(self.ambiguous_pr(
                target,
                "Reaction response/read-back does not confirm the exact requested reaction and current account; inspect before retrying",
            ));
        }
        Ok(result)
    }
    pub fn delete_reaction(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        comment_id: u64,
        kind: CommentKind,
        reaction_id: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Value>> {
        if reaction_id == 0 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Select a reaction identity",
            ));
        }
        self.guard_target(identity, target, cancel)?;
        let comment = self.api(
            &self.comment_endpoint(target, comment_id, kind)?,
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        if !self.comment_belongs(target, &comment, comment_id, kind) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Reaction target belongs to another PR",
            ));
        }
        let endpoint = format!(
            "{}/reactions",
            self.comment_endpoint(target, comment_id, kind)?
        );
        let reactions = self.api_list(&format!("{endpoint}?per_page=100"), None, cancel)?;
        let selected = reactions
            .iter()
            .find(|r| number(r, "id") == reaction_id)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Reaction is no longer on the selected comment",
                )
            })?;
        if !Self::rest_actor_matches(identity, &selected["user"]) {
            return Err(error(
                GithubErrorKind::Permission,
                "Only your own reaction may be removed",
            ));
        }
        self.target_write(
            target,
            &format!("{endpoint}/{reaction_id}"),
            "DELETE",
            None,
            cancel,
        )?;
        let result = self.after_pr_write(
            target,
            self.api_list(&format!("{endpoint}?per_page=100"), None, cancel),
        )?;
        if result.iter().any(|r| number(r, "id") == reaction_id) {
            return Err(self.ambiguous_pr(target, "Reaction is still present after removal"));
        }
        Ok(result)
    }
    pub fn inline_comment(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        comment: &InlineComment,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        self.inline_comment_response_v1(identity, target, comment, cancel)
    }
    pub fn reply_inline(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        comment_id: u64,
        body: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        if body.trim().is_empty() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Reply cannot be empty",
            ));
        }
        self.guard_target(identity, target, cancel)?;
        let comment = self.api(
            &self.comment_endpoint(target, comment_id, CommentKind::Review)?,
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        if !text(&comment, "pull_request_url").ends_with(&format!(
            "/repos/{}/pulls/{}",
            target.pr.repository, target.pr.number
        )) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Reply target is in another PR",
            ));
        }
        self.target_write(
            target,
            &format!("{}/comments/{comment_id}/replies", self.pr_endpoint(target)),
            "POST",
            Some(json!({"body":body})),
            cancel,
        )
    }
    pub fn reviews(
        &self,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<PrReview>> {
        Ok(self
            .api_list(
                &format!("{}/reviews?per_page=100", self.pr_endpoint(target)),
                None,
                cancel,
            )?
            .iter()
            .map(review_from_json)
            .collect())
    }
    pub fn create_pending_review(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        body: &str,
        comments: &[InlineComment],
        cancel: &CancellationToken,
    ) -> GithubResult<PrReview> {
        for comment in comments {
            comment.validate()?;
        }
        self.guard_target(identity, target, cancel)?;
        if self.reviews(target, cancel)?.iter().any(|review| {
            review.author.login.eq_ignore_ascii_case(&identity.login) && review.state == "PENDING"
        }) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "A pending review already exists. Resume it or discard it before creating another.",
            ));
        }
        if !comments.is_empty() {
            let files = self.api_list(
                &format!("{}/files?per_page=100", self.pr_endpoint(target)),
                None,
                cancel,
            )?;
            for comment in comments {
                let file = files
                    .iter()
                    .find(|v| text(v, "filename") == comment.path)
                    .ok_or_else(|| {
                        error(
                            GithubErrorKind::InvalidTarget,
                            "Pending comment file is absent from the PR",
                        )
                    })?;
                let patch = file.get("patch").and_then(Value::as_str).ok_or_else(|| {
                    error(
                        GithubErrorKind::Unsupported,
                        "Pending comment file has no canonical text patch",
                    )
                })?;
                comment.validate_patch(patch)?;
            }
            let fresh = self.pr_summary(&target.pr.repository, target.pr.number, cancel)?;
            if fresh.head_sha != target.head_sha || fresh.base_sha != target.base_sha {
                return Err(error(
                    GithubErrorKind::StaleHead,
                    "PR changed while validating pending comment ranges",
                ));
            }
        }
        let value=self.target_write(target,&format!("{}/reviews",self.pr_endpoint(target)),"POST",Some(json!({"body":body,"commit_id":target.head_sha,"comments":comments.iter().map(InlineComment::payload).collect::<Vec<_>>()})),cancel)?;
        let review = review_from_json(&value);
        if review.id == 0 || review.state != "PENDING" {
            self.runner
                .mark_ambiguous(&self.resource(&target.pr.repository, target.pr.number));
            return Err(error(
                GithubErrorKind::AmbiguousWrite,
                "Expected a server pending review; reconcile before retry",
            ));
        }
        Ok(review)
    }
    pub fn add_pending_comment(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        review_id: u64,
        comment: &InlineComment,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        comment.validate()?;
        let review = self.pending_review(identity, target, review_id, true, cancel)?;
        let files = self.api_list(
            &format!("{}/files?per_page=100", self.pr_endpoint(target)),
            None,
            cancel,
        )?;
        let file = files
            .iter()
            .find(|v| text(v, "filename") == comment.path)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Pending comment file is absent from the PR",
                )
            })?;
        comment.validate_patch(file.get("patch").and_then(Value::as_str).ok_or_else(|| {
            error(
                GithubErrorKind::Unsupported,
                "Pending comment file has no text patch",
            )
        })?)?;
        let mut input = json!({"pullRequestReviewId":review.node_id,"path":comment.path,"side":comment.side,"line":comment.line,"body":comment.body});
        if let Some(start) = comment.start_line {
            input["startLine"] = json!(start);
            input["startSide"] = json!(comment.start_side);
        }
        self.gql_pr_write(
            identity,
            target,
            (
                "addPullRequestReviewThread",
                "AddPullRequestReviewThreadInput",
                "thread{id isResolved comments(first:1){nodes{id body}}}",
            ),
            input,
            cancel,
        )
    }
    pub fn reply_pending_thread(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        review_id: u64,
        thread_id: &str,
        body: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        if body.trim().is_empty() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Pending reply cannot be empty",
            ));
        }
        let review = self.pending_review(identity, target, review_id, true, cancel)?;
        if !self
            .review_threads(target, cancel)?
            .iter()
            .any(|thread| thread.id == thread_id)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Pending reply thread is in another PR",
            ));
        }
        self.gql_pr_write(identity,target,("addPullRequestReviewThreadReply","AddPullRequestReviewThreadReplyInput","comment{id body}"),json!({"pullRequestReviewId":review.node_id,"pullRequestReviewThreadId":thread_id,"body":body}),cancel)
    }
    fn pending_review(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        id: u64,
        require_current_commit: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<PrReview> {
        self.guard_target(identity, target, cancel)?;
        let review = review_from_json(&self.api(
            &format!("{}/reviews/{id}", self.pr_endpoint(target)),
            "GET",
            None,
            None,
            false,
            cancel,
        )?);
        if review.id != id
            || !review.author.login.eq_ignore_ascii_case(&identity.login)
            || review.state != "PENDING"
        {
            return Err(error(
                GithubErrorKind::Permission,
                "Review is not a pending review owned by the selected account",
            ));
        }
        if require_current_commit && review.commit_id != target.head_sha {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Pending review refers to an older head; re-anchor comments before submitting",
            ));
        }
        Ok(review)
    }
    pub fn update_pending_review(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        id: u64,
        body: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<PrReview> {
        self.pending_review(identity, target, id, false, cancel)?;
        let endpoint = format!("{}/reviews/{id}", self.pr_endpoint(target));
        self.target_write(target, &endpoint, "PUT", Some(json!({"body":body})), cancel)?;
        let review = review_from_json(&self.after_pr_write(
            target,
            self.api(&endpoint, "GET", None, None, false, cancel),
        )?);
        if review.id != id || review.body != body || review.state != "PENDING" {
            return Err(self.ambiguous_pr(
                target,
                "Pending review text was not confirmed; reconcile before retrying",
            ));
        }
        Ok(review)
    }
    pub fn submit_pending_review(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        id: u64,
        event: ReviewEvent,
        body: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<PrReview> {
        if !matches!(event, ReviewEvent::Approve) && body.trim().is_empty() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Comment and change-request reviews need a summary",
            ));
        }
        self.pending_review(identity, target, id, true, cancel)?;
        self.target_write(
            target,
            &format!("{}/reviews/{id}/events", self.pr_endpoint(target)),
            "POST",
            Some(json!({"event":event.api_value(),"body":body})),
            cancel,
        )?;
        let review = review_from_json(&self.after_pr_write(
            target,
            self.api(
                &format!("{}/reviews/{id}", self.pr_endpoint(target)),
                "GET",
                None,
                None,
                false,
                cancel,
            ),
        )?);
        let expected_state = match event {
            ReviewEvent::Approve => "APPROVED",
            ReviewEvent::RequestChanges => "CHANGES_REQUESTED",
            ReviewEvent::Comment => "COMMENTED",
        };
        if review.id != id || review.state != expected_state || review.commit_id != target.head_sha
        {
            return Err(self.ambiguous_pr(
                target,
                "Submitted review state was not confirmed; reconcile without resubmitting",
            ));
        }
        Ok(review)
    }
    pub fn discard_pending_review(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        id: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.pending_review(identity, target, id, false, cancel)?;
        self.target_write(
            target,
            &format!("{}/reviews/{id}", self.pr_endpoint(target)),
            "DELETE",
            None,
            cancel,
        )?;
        if self
            .after_pr_write(target, self.reviews(target, cancel))?
            .iter()
            .any(|r| r.id == id)
        {
            return Err(self.ambiguous_pr(target, "Pending review is still visible after deletion"));
        }
        Ok(())
    }
    pub fn dismiss_review(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        id: u64,
        message: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<PrReview> {
        if id == 0 || message.trim().is_empty() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Dismissal needs a review and reason",
            ));
        }
        self.guard_target(identity, target, cancel)?;
        let endpoint = format!("{}/reviews/{id}", self.pr_endpoint(target));
        let review = self.api(&endpoint, "GET", None, None, false, cancel)?;
        if !matches!(
            text(&review, "state").as_str(),
            "APPROVED" | "CHANGES_REQUESTED"
        ) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Only an approval or change request can be dismissed",
            ));
        }
        self.target_write(
            target,
            &format!("{endpoint}/dismissals"),
            "PUT",
            Some(json!({"message":message,"event":"DISMISS"})),
            cancel,
        )?;
        let review = review_from_json(&self.after_pr_write(
            target,
            self.api(&endpoint, "GET", None, None, false, cancel),
        )?);
        if review.id != id || review.state != "DISMISSED" {
            return Err(self.ambiguous_pr(
                target,
                "Review dismissal was not confirmed; reconcile before retrying",
            ));
        }
        Ok(review)
    }
    /// One fixed read role from the actual original State producer. Neither a
    /// caller-selected query nor an unvalidated raw response leaves this seam.
    pub(in crate::providers::gh) fn original_state_capability_v87(
        &self,
        observed: &PrStateReadV68,
        cancel: &CancellationToken,
    ) -> GithubResult<StateCapabilityV68> {
        observed.validate_capability_source_v87(self, cancel)?;
        let raw = self.graphql(
            STATE_CAPABILITY_QUERY_V68,
            json!({"id":observed.parent().node_id()}),
            None,
            cancel,
        )?;
        observed.validate_capability_raw_v87(&raw)
    }
    fn graphql(
        &self,
        query: &str,
        variables: Value,
        write: Option<String>,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        self.api(
            "graphql",
            "POST",
            Some(json!({"query":query,"variables":variables})),
            write,
            false,
            cancel,
        )
    }
    fn gql_pr_write(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        operation: (&str, &str, &str),
        input: Value,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        self.guard_target(identity, target, cancel)?;
        let (name, input_type, fields) = operation;
        let query = format!("mutation($input:{input_type}!){{{name}(input:$input){{{fields}}}}}");
        self.graphql(
            &query,
            json!({"input":input}),
            Some(self.resource(&target.pr.repository, target.pr.number)),
            cancel,
        )
    }
    pub fn subscription(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        state: Subscription,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        let result = self.gql_pr_write(
            identity,
            target,
            (
                "updateSubscription",
                "UpdateSubscriptionInput",
                "subscribable{id viewerSubscription}",
            ),
            json!({"subscribableId":target.pr.id,"state":state}),
            cancel,
        )?;
        if result["data"]["updateSubscription"]["subscribable"]["id"] != target.pr.id
            || result["data"]["updateSubscription"]["subscribable"]["viewerSubscription"]
                != serde_json::to_value(state)
                    .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?
        {
            return Err(self.ambiguous_pr(
                target,
                "Subscription result did not match the requested account/PR state",
            ));
        }
        Ok(result)
    }
    pub fn mark_viewed(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        path: &str,
        viewed: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<ViewedFile>> {
        if path.is_empty() || path.contains('\0') {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Select a canonical PR file path",
            ));
        }
        let (name, input) = if viewed {
            ("markFileAsViewed", "MarkFileAsViewedInput")
        } else {
            ("unmarkFileAsViewed", "UnmarkFileAsViewedInput")
        };
        self.gql_pr_write(
            identity,
            target,
            (name, input, "pullRequest{id}"),
            json!({"pullRequestId":target.pr.id,"path":path}),
            cancel,
        )?;
        let files = self.after_pr_write(target, self.viewed_files(target, cancel))?;
        let expected = if viewed { "VIEWED" } else { "UNVIEWED" };
        if !files.iter().any(|f| f.path == path && f.state == expected) {
            return Err(self.ambiguous_pr(
                target,
                "Viewed-file state did not match read-back; inspect changed files before retry",
            ));
        }
        Ok(files)
    }
    pub fn viewed_files(
        &self,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<ViewedFile>> {
        let query="query($id:ID!,$cursor:String){node(id:$id){... on PullRequest{files(first:100,after:$cursor){nodes{path viewerViewedState} pageInfo{hasNextPage endCursor}}}}}";
        self.gql_pages(query, target, "files", cancel)
            .map(|values| {
                values
                    .iter()
                    .map(|v| ViewedFile {
                        path: text(v, "path"),
                        state: text(v, "viewerViewedState"),
                    })
                    .collect()
            })
    }
    fn gql_pages(
        &self,
        query: &str,
        target: &PrSummary,
        field: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Value>> {
        let mut cursor = Value::Null;
        let mut all = Vec::new();
        let mut bytes = 0usize;
        for _ in 0..100 {
            let value = self.graphql(
                query,
                json!({"id":target.pr.id,"cursor":cursor}),
                None,
                cancel,
            )?;
            let collection = &value["data"]["node"][field];
            let nodes = collection
                .get("nodes")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    error(
                        GithubErrorKind::Unsupported,
                        format!("Host does not expose PR {field}"),
                    )
                })?;
            bytes = bytes.saturating_add(nodes.iter().map(|v| v.to_string().len()).sum::<usize>());
            if bytes > 32 * 1024 * 1024 {
                return Err(error(
                    GithubErrorKind::Transport,
                    "GraphQL collection exceeded its 32 MiB budget; select a narrower discussion",
                ));
            }
            all.extend(nodes.iter().cloned());
            let has_next = collection["pageInfo"]["hasNextPage"]
                .as_bool()
                .ok_or_else(|| {
                    error(
                        GithubErrorKind::Unsupported,
                        "Host omitted required GraphQL pagination coverage",
                    )
                })?;
            if !has_next {
                return Ok(all);
            }
            let next = collection["pageInfo"]["endCursor"].clone();
            if next.is_null() || next == cursor {
                return Err(error(
                    GithubErrorKind::Json,
                    "GraphQL pagination cursor did not advance",
                ));
            }
            cursor = next;
        }
        Err(error(
            GithubErrorKind::Transport,
            "GraphQL page budget exceeded; narrow or refresh this PR",
        ))
    }
    pub fn review_threads(
        &self,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<ReviewThread>> {
        let query="query($id:ID!,$cursor:String){node(id:$id){... on PullRequest{reviewThreads(first:100,after:$cursor){nodes{id path diffSide line startLine isResolved isOutdated viewerCanResolve viewerCanUnresolve comments(first:100){nodes{id databaseId body author{login avatarUrl} commit{oid}} pageInfo{hasNextPage}}} pageInfo{hasNextPage endCursor}}}}}";
        self.gql_pages(query, target, "reviewThreads", cancel)
            .map(|values| {
                values
                    .iter()
                    .map(|v| ReviewThread {
                        id: text(v, "id"),
                        path: text(v, "path"),
                        side: text(v, "diffSide"),
                        line: v.get("line").and_then(Value::as_u64),
                        start_line: v.get("startLine").and_then(Value::as_u64),
                        resolved: boolean(v, "isResolved"),
                        outdated: boolean(v, "isOutdated"),
                        viewer_can_resolve: boolean(v, "viewerCanResolve"),
                        viewer_can_unresolve: boolean(v, "viewerCanUnresolve"),
                        comments: v["comments"]["nodes"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default(),
                        comments_complete: v["comments"]["pageInfo"]["hasNextPage"].as_bool()
                            == Some(false),
                    })
                    .collect()
            })
    }
    pub fn thread_comments(
        &self,
        target: &PrSummary,
        thread_id: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Value>> {
        if !self
            .review_threads(target, cancel)?
            .iter()
            .any(|thread| thread.id == thread_id)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Thread comments belong to another PR",
            ));
        }
        let query="query($id:ID!,$cursor:String){node(id:$id){... on PullRequestReviewThread{comments(first:100,after:$cursor){nodes{id databaseId body author{login avatarUrl} commit{oid}} pageInfo{hasNextPage endCursor}}}}}";
        let mut thread_target = target.clone();
        thread_target.pr.id = thread_id.into();
        self.gql_pages(query, &thread_target, "comments", cancel)
    }
    pub fn resolve_thread(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        thread_id: &str,
        resolve: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<ReviewThread> {
        let threads = self.review_threads(target, cancel)?;
        let thread = threads.iter().find(|t| t.id == thread_id).ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Thread is not in the selected PR",
            )
        })?;
        if resolve && !thread.viewer_can_resolve || !resolve && !thread.viewer_can_unresolve {
            return Err(error(
                GithubErrorKind::Permission,
                "The selected account cannot change this thread resolution state",
            ));
        }
        let (name, input) = if resolve {
            ("resolveReviewThread", "ResolveReviewThreadInput")
        } else {
            ("unresolveReviewThread", "UnresolveReviewThreadInput")
        };
        self.gql_pr_write(
            identity,
            target,
            (name, input, "thread{id isResolved}"),
            json!({"threadId":thread_id}),
            cancel,
        )?;
        self.after_pr_write(target, self.review_threads(target, cancel))?
            .into_iter()
            .find(|t| t.id == thread_id && t.resolved == resolve)
            .ok_or_else(|| {
                self.ambiguous_pr(
                    target,
                    "Thread resolution read-back did not match the selected state",
                )
            })
    }
    pub fn lock_pr(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        reason: Option<LockReason>,
        lock: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        self.guard_target(identity, target, cancel)?;
        let endpoint = format!(
            "repos/{}/issues/{}/lock",
            target.pr.repository, target.pr.number
        );
        self.target_write(
            target,
            &endpoint,
            if lock { "PUT" } else { "DELETE" },
            if lock {
                reason.map(|r| json!({"lock_reason":r.value()}))
            } else {
                None
            },
            cancel,
        )?;
        let result = self.after_pr_write(target, self.issue_metadata(target, cancel))?;
        if result["locked"].as_bool() != Some(lock) {
            return Err(self.ambiguous_pr(target, "PR lock state did not match server read-back"));
        }
        Ok(result)
    }
    pub fn update_pr_branch(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> GithubResult<PrSummary> {
        self.guard_target(identity, target, cancel)?;
        self.target_write(
            target,
            &format!("{}/update-branch", self.pr_endpoint(target)),
            "PUT",
            Some(json!({"expected_head_sha":target.head_sha})),
            cancel,
        )?;
        self.after_pr_write(
            target,
            self.pr_summary(&target.pr.repository, target.pr.number, cancel),
        )
    }
    pub fn revert_pr(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        title: &str,
        body: &str,
        draft: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<PrSummary> {
        if !target.merged || title.trim().is_empty() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Revert requires a merged PR and a title",
            ));
        }
        let value = self.gql_pr_write(
            identity,
            target,
            (
                "revertPullRequest",
                "RevertPullRequestInput",
                "revertPullRequest{id number url}",
            ),
            json!({"pullRequestId":target.pr.id,"title":title,"body":body,"draft":draft}),
            cancel,
        )?;
        let n = number(
            &value["data"]["revertPullRequest"]["revertPullRequest"],
            "number",
        );
        if n == 0 {
            self.runner
                .mark_ambiguous(&self.resource(&target.pr.repository, target.pr.number));
            return Err(error(
                GithubErrorKind::AmbiguousWrite,
                "Revert did not return a new PR identity",
            ));
        }
        let url = text(
            &value["data"]["revertPullRequest"]["revertPullRequest"],
            "url",
        );
        let created =
            self.after_pr_write(target, self.pr_summary(&target.pr.repository, n, cancel))?;
        // Provenance: the read-back must be the PR the mutation named, aimed
        // at the original base branch.
        if n == target.pr.number || created.pr.url != url || created.base_ref != target.base_ref {
            return Err(self.ambiguous_pr(
                target,
                "The reverting PR read back does not match the mutation result; reconcile",
            ));
        }
        Ok(created)
    }
    /// Repository merge-method settings and the caller's permission. A setting
    /// the host does not report stays unknown; GitHub still decides.
    pub fn repository_merge_settings(
        &self,
        repository: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<RepoMergeSettings> {
        validate_repo(repository)?;
        let raw = self.api(
            &format!("repos/{repository}"),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        if !text(&raw, "full_name").eq_ignore_ascii_case(repository) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Repository settings belong to another or renamed repository; refresh",
            ));
        }
        let flag = |key: &str| raw.get(key).and_then(Value::as_bool);
        let permissions = &raw["permissions"];
        Ok(RepoMergeSettings {
            allow_merge_commit: flag("allow_merge_commit"),
            allow_squash_merge: flag("allow_squash_merge"),
            allow_rebase_merge: flag("allow_rebase_merge"),
            allow_auto_merge: flag("allow_auto_merge"),
            default_branch: text(&raw, "default_branch"),
            permissions: Permissions {
                pull: boolean(permissions, "pull"),
                push: boolean(permissions, "push"),
                maintain: boolean(permissions, "maintain"),
                admin: boolean(permissions, "admin"),
            },
        })
    }
    /// Delete a merged PR's same-repository head branch on GitHub. GitHub's
    /// ref deletion has no expected-SHA guard, so the tip is re-read right
    /// before deleting and a moved branch is refused. Local branches and the
    /// working tree are never touched.
    pub fn delete_head_branch(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        default_branch: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<String> {
        let fresh = self.guard_target(identity, target, cancel)?;
        if !fresh.merged {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Only a merged PR's head branch can be deleted here",
            ));
        }
        let raw = self.api(&self.pr_endpoint(target), "GET", None, None, false, cancel)?;
        let head_repo = raw["head"]["repo"]["full_name"].as_str().unwrap_or("");
        if !head_repo.eq_ignore_ascii_case(&target.pr.repository) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "The head branch lives in another repository (a fork); delete it there",
            ));
        }
        let branch = text(&raw["head"], "ref");
        if branch.is_empty() || branch == default_branch || branch == fresh.base_ref {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Refusing to delete the default or base branch",
            ));
        }
        let read = format!(
            "repos/{}/git/ref/heads/{}",
            target.pr.repository,
            url_component(&branch)
        );
        let tip = self.api(&read, "GET", None, None, false, cancel)?;
        if text(&tip["object"], "sha") != target.head_sha {
            return Err(error(
                GithubErrorKind::StaleHead,
                "The branch moved after the PR merged; it was not deleted",
            ));
        }
        self.target_write(
            target,
            &format!(
                "repos/{}/git/refs/heads/{}",
                target.pr.repository,
                url_component(&branch)
            ),
            "DELETE",
            None,
            cancel,
        )?;
        match self.api(&read, "GET", None, None, false, cancel) {
            Err(failure) if matches!(failure.kind, GithubErrorKind::NotFound) => Ok(branch),
            Ok(_) => Err(self.ambiguous_pr(
                target,
                "The branch is still visible after deletion; reconcile before retrying",
            )),
            Err(failure) => self.after_pr_write(target, Err(failure)),
        }
    }
    /// Publication uses an already-pushed remote branch; this never changes a local checkout.
    pub fn create_pr(
        &self,
        identity: &HostIdentity,
        request: &CreatePr,
        cancel: &CancellationToken,
    ) -> GithubResult<PrSummary> {
        validate_create_pr_request_v9(request)?;
        let current = self.identity(cancel)?;
        if !current.same_account(identity) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Account changed before PR publication",
            ));
        }
        let head = self.api(
            &format!(
                "repos/{}/git/ref/heads/{}",
                request.head_repository,
                url_component(&request.head)
            ),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let base = self.api(
            &format!(
                "repos/{}/git/ref/heads/{}",
                request.repository,
                url_component(&request.base)
            ),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        if text(&head["object"], "sha") != request.expected_head_sha
            || text(&base["object"], "sha") != request.expected_base_sha
        {
            return Err(error(GithubErrorKind::StaleHead,"Remote base or head changed since the proposed-diff preview; refresh before publishing"));
        }
        let owner = request
            .head_repository
            .split('/')
            .next()
            .expect("validated owner/repo");
        let head_name = format!("{}:{}", owner.to_ascii_lowercase(), request.head);
        let existing = self.api_list(
            &format!(
                "repos/{}/pulls?state=open&head={}&base={}&per_page=100",
                request.repository,
                url_component(&head_name),
                url_component(&request.base)
            ),
            None,
            cancel,
        )?;
        if let Some(pr) = existing.first() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                format!(
                    "An open PR already exists for this head/base: {}",
                    text(pr, "html_url")
                ),
            ));
        }
        let resource = self.canonical_resource(&format!(
            "{}/{}/create/{head_name}/{}",
            self.host, request.repository, request.base
        ));
        let value = self.api(
            &format!("repos/{}/pulls", request.repository),
            "POST",
            Some(create_pr_payload_v9(request, &head_name)),
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let n = number(&value, "number");
        if n == 0 {
            self.runner.mark_ambiguous(&resource);
            return Err(error(GithubErrorKind::AmbiguousWrite,"PR publication did not return its canonical identity; check existing PRs before retry"));
        }
        let result =
            self.after_write(&resource, self.pr_summary(&request.repository, n, cancel))?;
        if result.pr.title != request.title
            || result.pr.body != request.body
            || result.pr.draft != request.draft
            || result.head_sha != request.expected_head_sha
            || result.base_ref != request.base
        {
            self.runner.mark_ambiguous(&resource);
            return Err(error(GithubErrorKind::AmbiguousWrite,format!("PR #{} was created but its read-back differs from the publication preview; inspect it before retry",n)));
        }
        Ok(result)
    }
    pub fn reconcile_create_pr(
        &self,
        request: &CreatePr,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<PrSummary>> {
        validate_repo(&request.repository)?;
        validate_repo(&request.head_repository)?;
        let owner = request
            .head_repository
            .split('/')
            .next()
            .expect("validated owner/repo");
        let head = format!("{}:{}", owner.to_ascii_lowercase(), request.head);
        let values = self.api_list(
            &format!(
                "repos/{}/pulls?state=all&head={}&base={}&per_page=100",
                request.repository,
                url_component(&head),
                url_component(&request.base)
            ),
            None,
            cancel,
        )?;
        let mut records = Vec::new();
        for value in values {
            records.push(self.pr_summary(&request.repository, number(&value, "number"), cancel)?);
        }
        self.runner
            .mark_reconciled(&self.canonical_resource(&format!(
                "{}/{}/create/{head}/{}",
                self.host, request.repository, request.base
            )));
        Ok(records)
    }
    pub fn fork_choices(&self, repo: &str, cancel: &CancellationToken) -> GithubResult<Vec<Value>> {
        validate_repo(repo)?;
        self.api_list(
            &format!("repos/{repo}/forks?per_page=100&sort=newest"),
            None,
            cancel,
        )
    }
}
#[derive(Clone, Copy, Debug)]
pub enum MetadataChoices {
    Labels,
    Assignees,
    Milestones,
    Branches,
    Collaborators,
    Teams,
}
#[derive(Clone, Copy, Debug)]
pub enum LockReason {
    OffTopic,
    TooHeated,
    Resolved,
    Spam,
}
impl LockReason {
    fn value(self) -> &'static str {
        match self {
            Self::OffTopic => "off-topic",
            Self::TooHeated => "too heated",
            Self::Resolved => "resolved",
            Self::Spam => "spam",
        }
    }
}
fn review_from_json(v: &Value) -> PrReview {
    PrReview {
        id: number(v, "id"),
        node_id: text(v, "node_id"),
        author: Actor::from_json(&v["user"]),
        body: text(v, "body"),
        state: text(v, "state"),
        commit_id: text(v, "commit_id"),
    }
}
pub(super) fn url_component(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(*b, b'-' | b'_' | b'.' | b'~') {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
