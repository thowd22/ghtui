//! Genuine selected submitted review comment. No legacy login/push fallback,
//! DTO receipt constructor or authority from protected historical metadata.
use super::*;
use std::sync::Arc;

pub const THREAD_SELECTED_COMMENT_QUERY_V2: &str = "query($id:ID!){node(id:$id){__typename ... on PullRequestReviewComment{id fullDatabaseId body createdAt updatedAt state path line startLine originalLine originalStartLine outdated commit{oid} originalCommit{oid} replyTo{id fullDatabaseId} viewerCanUpdate viewerCanDelete viewerCanReact viewerDidAuthor author{__typename login avatarUrl ... on User{databaseId name} ... on Bot{databaseId}} pullRequestReview{id fullDatabaseId state} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}}}";
pub const THREAD_COMMENT_ABSENCE_QUERY_V2: &str =
    "query($id:ID!){node(id:$id){__typename ... on PullRequestReviewComment{id}}}";
/// Exact response + exact readback + opening UID/repo/raw-PR/final UID.
/// The entire allowance is prepaid before ONE measured prewrite allocation.
pub const THREAD_COMMENT_POSTWRITE_CAPS_V2: [usize; 6] = [
    256 * 1024,
    256 * 1024,
    64 * 1024,
    128 * 1024,
    128 * 1024,
    64 * 1024,
];

pub struct OriginalThreadCommentV2 {
    original: Arc<OriginalThreadPublicationV1>,
    index: usize,
    principal: u64,
    author: Option<u64>,
    reply_parent_id: Option<u64>,
    review_id: u64,
    review_node: String,
    can_update: Option<bool>,
    can_delete: Option<bool>,
    can_react: Option<bool>,
    did_author: Option<bool>,
    outdated: Option<bool>,
}
impl std::fmt::Debug for OriginalThreadCommentV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalThreadCommentV2(<actual current submitted member; no write permit>)")
    }
}
impl OriginalThreadCommentV2 {
    pub fn original(&self) -> &Arc<OriginalThreadPublicationV1> {
        &self.original
    }
    pub fn comment(&self) -> &MetricsThreadComment {
        &self.original.thread.comments[self.index]
    }
    pub fn principal(&self) -> u64 {
        self.principal
    }
    pub fn author_account_id(&self) -> Option<u64> {
        self.author
    }
    pub fn review_id(&self) -> u64 {
        self.review_id
    }
    pub fn review_node(&self) -> &str {
        &self.review_node
    }
    pub fn viewer_can_update(&self) -> Option<bool> {
        self.can_update
    }
    pub fn viewer_can_delete(&self) -> Option<bool> {
        self.can_delete
    }
    pub fn viewer_can_react(&self) -> Option<bool> {
        self.can_react
    }
    pub fn viewer_did_author(&self) -> Option<bool> {
        self.did_author
    }
    pub fn outdated(&self) -> Option<bool> {
        self.outdated
    }
    /// Fresh Confirm compares this independently observed snapshot to Preview;
    /// no saved/history record can construct either side of the comparison.
    pub(crate) fn same_observed_comment_v2(&self, other: &Self) -> bool {
        let a = self.comment();
        let b = other.comment();
        self.principal == other.principal
            && self.author == other.author
            && self.review_id == other.review_id
            && self.review_node == other.review_node
            && self.reply_parent_id == other.reply_parent_id
            && self.original.parent.summary().pr.host == other.original.parent.summary().pr.host
            && self.original.parent.repository() == other.original.parent.repository()
            && self.original.parent.repository_id() == other.original.parent.repository_id()
            && self.original.parent.repository_node() == other.original.parent.repository_node()
            && self.original.parent.node_id() == other.original.parent.node_id()
            && self.original.parent.database_id() == other.original.parent.database_id()
            && self.original.parent.head_sha() == other.original.parent.head_sha()
            && self.original.parent.base_tip_sha() == other.original.parent.base_tip_sha()
            && self.original.thread.node_id == other.original.thread.node_id
            && a.node_id == b.node_id
            && a.database_id == b.database_id
            && a.body == b.body
            && a.updated_at == b.updated_at
            && a.created_at == b.created_at
            && a.commit_sha == b.commit_sha
            && a.original_commit_sha == b.original_commit_sha
            && a.observed_path == b.observed_path
            && a.reply_to == b.reply_to
            && a.line == b.line
            && a.start_line == b.start_line
            && a.original_line == b.original_line
            && a.original_start_line == b.original_start_line
            && self.outdated == other.outdated
    }
}

pub enum ThreadCommentMutationV2<'a> {
    Edit(&'a str),
    Delete,
}
pub struct ThreadCommentEditReceiptV2 {
    comment: ConversationComment,
}
impl ThreadCommentEditReceiptV2 {
    /// Actual observed comment/author, NOT inferred editing actor or causality.
    pub fn comment(&self) -> &ConversationComment {
        &self.comment
    }
}
pub struct ThreadCommentDeleteReceiptV2 {
    id: u64,
    node: String,
}
impl ThreadCommentDeleteReceiptV2 {
    pub fn comment_id(&self) -> u64 {
        self.id
    }
    pub fn comment_node(&self) -> &str {
        &self.node
    }
}
pub enum ThreadCommentReceiptV2 {
    Edited(ThreadCommentEditReceiptV2),
    Deleted(ThreadCommentDeleteReceiptV2),
}

fn bounded_edit_body(body: &str) -> GithubResult<()> {
    if body.len() > 65_536 || body.trim().is_empty() {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Edited comment requires nonempty text within64KiB, without truncation",
        ));
    }
    // Exact {"body":"..."} scalar preflight BEFORE Value/string materialization.
    let mut encoded = 11usize;
    for ch in body.chars() {
        encoded += match ch {
            '"' | '\\' | '\u{8}' | '\u{c}' | '\n' | '\r' | '\t' => 2,
            '\u{0}'..='\u{1f}' => 6,
            _ => ch.len_utf8(),
        };
        if encoded > 128 * 1024 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Encoded edited comment exceeds128KiB; no body allocated or truncated",
            ));
        }
    }
    Ok(())
}

impl GithubProvider {
    fn comment_rest_observation_v2(
        &self,
        selected: &OriginalThreadCommentV2,
        raw: &Value,
        requested_body: Option<&str>,
    ) -> bool {
        let comment = selected.comment();
        let unchanged = requested_body.is_none();
        self.comment_belongs(
            selected.original.parent.summary(),
            raw,
            comment.database_id.unwrap_or(0),
            CommentKind::Review,
        ) && raw["node_id"].as_str() == Some(comment.node_id.as_str())
            && raw["pull_request_review_id"].as_u64() == Some(selected.review_id)
            && raw["body"].as_str() == requested_body.or(comment.body.as_deref())
            && raw["created_at"].as_str() == comment.created_at.as_ref().map(MetricsTimestamp::raw)
            && raw["updated_at"]
                .as_str()
                .and_then(crate::github_workspace::timestamp_seconds)
                .is_some()
            && (!unchanged
                || raw["updated_at"].as_str()
                    == comment.updated_at.as_ref().map(MetricsTimestamp::raw))
            && raw["path"].as_str() == comment.observed_path.as_deref()
            && raw["commit_id"].as_str() == comment.commit_sha.as_deref()
            && raw["original_commit_id"].as_str() == comment.original_commit_sha.as_deref()
            && raw["user"].is_null() == selected.author.is_none()
            && selected
                .author
                .is_none_or(|id| raw["user"]["id"].as_u64() == Some(id))
            && raw["in_reply_to_id"].as_u64() == selected.reply_parent_id
            && (selected.reply_parent_id.is_some()
                || raw.get("in_reply_to_id").is_none_or(Value::is_null))
            && ["login", "name", "type", "avatar_url"].iter().all(|field| {
                raw["user"][*field]
                    .as_str()
                    .is_none_or(|value| value.len() <= 4096)
            })
            && ["html_url", "url", "pull_request_url"]
                .iter()
                .all(|field| raw[*field].as_str().is_none_or(|value| value.len() <= 4096))
    }

    /// The actual fresh Confirm producer is distinct from Preview; all write
    /// capability comes from fresh, never expected/history/restored approval.
    /// Caller supplies its SAME original SQL/journal/ACK close at before_write.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mutate_thread_comment_guarded_v2(
        &self,
        identity: &HostIdentity,
        expected: &OriginalThreadCommentV2,
        fresh: &OriginalThreadCommentV2,
        measured: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        mutation: ThreadCommentMutationV2<'_>,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: impl FnOnce() -> GithubResult<()>,
    ) -> Result<ThreadCommentReceiptV2, ThreadPublicationFailureV1> {
        let mut invoked = false;
        let result = (|| {
            let deadline = self.thread_original_producer_v2(
                &fresh.original,
                measured,
                captures,
                cancel,
                deadline,
            )?;
            if !fresh.same_observed_comment_v2(expected)
                || identity.account_id != Some(fresh.principal)
            {
                return Err(error(GithubErrorKind::StaleHead, "Exact selected comment changed after Preview; historical/restored intent is not approval"));
            }
            let body = match mutation {
                ThreadCommentMutationV2::Edit(body) if fresh.can_update == Some(true) => {
                    bounded_edit_body(body)?; Some(body)
                }
                ThreadCommentMutationV2::Delete if fresh.can_delete == Some(true) => None,
                _ => return Err(error(GithubErrorKind::Permission, "Actual fresh selected-comment capability is false/unknown; no push/login fallback")),
            };
            measured.thread_write_principal_v1(identity, &fresh.original, cancel, deadline)?;
            let id = fresh
                .comment()
                .database_id
                .expect("genuine actual RESTID checked");
            let endpoint =
                self.comment_endpoint(fresh.original.parent.summary(), id, CommentKind::Review)?;
            let latest = measured.api(&endpoint, "GET", None, None, false, cancel)?;
            if !measured.comment_rest_observation_v2(fresh, &latest, None) {
                return Err(error(
                    GithubErrorKind::StaleHead,
                    "Actual submitted comment changed before final original SQL/principal close",
                ));
            }
            before_write()?;
            phase(cancel, deadline)?;
            let provider = self.clone().with_catalogue_read_deadline(deadline);
            invoked = true;
            let receipt = if let Some(body) = body {
                let created = captures.api(
                    &provider,
                    &endpoint,
                    "PATCH",
                    Some(json!({"body":body})),
                    Some(provider.resource(
                        fresh.original.parent.repository(),
                        fresh.original.parent.number(),
                    )),
                    cancel,
                )?;
                if !provider.comment_rest_observation_v2(fresh, &created, Some(body)) {
                    return Err(error(GithubErrorKind::AmbiguousWrite, "Exact edit response does not observe requested body/original comment identity"));
                }
                let readback = captures.api(&provider, &endpoint, "GET", None, None, cancel)?;
                if !provider.comment_rest_observation_v2(fresh, &readback, Some(body))
                    || readback["updated_at"] != created["updated_at"]
                {
                    return Err(error(
                        GithubErrorKind::AmbiguousWrite,
                        "Exact edit readback differs; observe/inspect without resend",
                    ));
                }
                ThreadCommentReceiptV2::Edited(ThreadCommentEditReceiptV2 {
                    comment: comment_from_json(&readback),
                })
            } else {
                let ack = captures.review_comment_delete_ack_v2(
                    &provider,
                    &fresh.original.parent,
                    id,
                    cancel,
                )?;
                if ack.comment_id() != id {
                    return Err(error(
                        GithubErrorKind::AmbiguousWrite,
                        "Closed delete acknowledgement targets another actual comment",
                    ));
                }
                let value = captures.api(&provider, "graphql", "POST", Some(json!({"query":THREAD_COMMENT_ABSENCE_QUERY_V2,"variables":{"id":fresh.comment().node_id}})), None, cancel)?;
                if graph_errors(&value)
                    || value.get("data").and_then(|value| value.get("node")) != Some(&Value::Null)
                {
                    return Err(error(GithubErrorKind::AmbiguousWrite, "HTTP204 plus error-free exact-node explicit null are BOTH required; absence alone is not causality"));
                }
                ThreadCommentReceiptV2::Deleted(ThreadCommentDeleteReceiptV2 {
                    id,
                    node: fresh.comment().node_id.clone(),
                })
            };
            phase(cancel, deadline)?;
            Ok(receipt)
        })();
        result.map_err(|failure| {
            if invoked {
                self.runner.mark_ambiguous(&self.resource(
                    fresh.original.parent.repository(),
                    fresh.original.parent.number(),
                ));
                ThreadPublicationFailureV1::WriteMayHaveStarted(failure)
            } else {
                ThreadPublicationFailureV1::NotStarted(failure)
            }
        })
    }
}

fn selected_member(
    original: &OriginalThreadPublicationV1,
    node: &str,
    id: u64,
) -> GithubResult<usize> {
    if !opaque_node(node)
        || id == 0
        || id > i64::MAX as u64
        || !original.thread.comment_coverage.enumeration_complete
        || original.thread.comment_coverage.excluded_rows != 0
    {
        return Err(error(GithubErrorKind::Unsupported, "Exact selected comment requires complete bounded member identity; partial is not absence/permission"));
    }
    let mut selected = None;
    for (index, comment) in original.thread.comments.iter().enumerate() {
        if comment.node_id == node || comment.database_id == Some(id) {
            if selected.is_some() || comment.node_id != node || comment.database_id != Some(id) {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Contradictory or duplicate selected comment numeric/node membership",
                ));
            }
            selected = Some(index);
        }
    }
    selected.ok_or_else(|| {
        error(
            GithubErrorKind::InvalidTarget,
            "Selected actual comment was not observed in this exact original thread",
        )
    })
}
fn actual_author(value: &Value) -> GithubResult<Option<u64>> {
    if value.is_null() {
        return Ok(None);
    }
    if !value.is_object()
        || ["login", "name", "avatarUrl", "__typename"]
            .iter()
            .any(|field| {
                value[*field]
                    .as_str()
                    .is_some_and(|value| value.len() > 4096)
            })
    {
        return Err(error(
            GithubErrorKind::Json,
            "Selected comment actor metadata exceeds its bounded original observation",
        ));
    }
    if !matches!(value["__typename"].as_str(), Some("User" | "Bot"))
        || value["login"].as_str().is_none_or(|login| login.is_empty())
    {
        return Err(error(
            GithubErrorKind::Unsupported,
            "Selected comment author identity is unavailable; no login-only ownership inferred",
        ));
    }
    canonical_bigint(&value["databaseId"])
        .filter(|id| *id <= i64::MAX as u64)
        .map(Some)
        .ok_or_else(|| {
            error(
                GithubErrorKind::Unsupported,
                "Actual selected comment numeric author unavailable",
            )
        })
}
fn reply_rest_id(comment: &MetricsThreadComment, raw: &Value) -> GithubResult<Option<u64>> {
    match &comment.reply_to {
        MetricsThreadReplyTo::Original if raw.get("replyTo") == Some(&Value::Null) => Ok(None),
        MetricsThreadReplyTo::Reply(node)
            if raw["replyTo"]["id"].as_str() == Some(node.as_str()) =>
        {
            canonical_bigint(&raw["replyTo"]["fullDatabaseId"])
                .filter(|id| *id <= i64::MAX as u64)
                .map(Some)
                .ok_or_else(|| {
                    error(
                        GithubErrorKind::Unsupported,
                        "Reply parent actual REST ID unavailable; no opaque decoding",
                    )
                })
        }
        _ => Err(error(
            GithubErrorKind::IdentityChanged,
            "Selected comment original/reply membership changed or is unknown",
        )),
    }
}
fn exact_coordinate(raw: &Value, field: &str) -> GithubResult<Option<u64>> {
    match raw.get(field) {
        Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .filter(|line| (1..=i32::MAX as u64).contains(line))
            .map(Some)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Selected comment coordinate is not an explicit nullable positive GraphQL line",
                )
            }),
        None => Err(error(
            GithubErrorKind::Json,
            "Selected comment coordinate omitted; unknown is not explicit null",
        )),
    }
}
fn comment_graph_parent(original: &OriginalThreadPublicationV1, raw: &Value) -> bool {
    let parent = &original.parent;
    let pr = &raw["pullRequest"];
    let repo = &raw["repository"];
    pr["id"].as_str() == Some(parent.node_id())
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

impl GithubProvider {
    /// Worker-only genuine factory. The caller still must close its exact
    /// catalogue/current-numeric-principal/original SQL source bracket.
    pub(crate) fn original_thread_comment_v2(
        &self,
        identity: &HostIdentity,
        original: Arc<OriginalThreadPublicationV1>,
        node: &str,
        id: u64,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<OriginalThreadCommentV2> {
        let deadline = deadline.min(original.deadline);
        phase(cancel, deadline)?;
        if !original.cancellation.same_instance(cancel)
            || self
                .scoped_read_budget
                .as_ref()
                .is_none_or(|actual| !Arc::ptr_eq(actual, &original.producer))
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Selected comment cannot adopt another original measured producer/token",
            ));
        }
        original
            .producer
            .check_clock(cancel)
            .map_err(GithubError::from)?;
        let index = selected_member(&original, node, id)?;
        let selected = &original.thread.comments[index];
        if selected
            .body
            .as_deref()
            .is_none_or(|body| body.len() > 65_536)
            || selected.updated_at.is_none()
            || selected.created_at.is_none()
            || selected
                .observed_path
                .as_deref()
                .is_none_or(|path| path.is_empty() || path.len() > 4096)
            || selected
                .commit_sha
                .as_deref()
                .is_none_or(|sha| !crate::search::model::valid_sha(sha))
        {
            return Err(error(GithubErrorKind::Unsupported, "Exact bounded comment body/time/path/independent commit unavailable; no H substitution"));
        }
        let (deadline, cap) = self
            .scoped_read_window
            .map_or((deadline, 128 * 1024), |(clock, cap)| {
                (deadline.min(clock), cap.min(128 * 1024))
            });
        let bounded = self.clone().with_scoped_read_window(deadline, cap)?;
        bounded.thread_write_principal_v1(identity, &original, cancel, deadline)?;
        let value = bounded.api(
            "graphql",
            "POST",
            Some(json!({"query":THREAD_SELECTED_COMMENT_QUERY_V2,"variables":{"id":node}})),
            None,
            false,
            cancel,
        )?;
        let raw = &value["data"]["node"];
        if graph_errors(&value)
            || raw["__typename"] != "PullRequestReviewComment"
            || raw["id"].as_str() != Some(node)
            || canonical_bigint(&raw["fullDatabaseId"]) != Some(id)
            || !comment_graph_parent(&original, raw)
        {
            return Err(error(GithubErrorKind::IdentityChanged, "Exact selected comment node/RESTID/original repo/PR/head/base differs or has GraphQL errors"));
        }
        if raw["state"].as_str() != Some("SUBMITTED")
            || !matches!(
                raw["pullRequestReview"]["state"].as_str(),
                Some("APPROVED" | "CHANGES_REQUESTED" | "COMMENTED" | "DISMISSED")
            )
        {
            return Err(error(GithubErrorKind::Unsupported, "Only genuinely submitted comments are managed here; pending/unknown review belongs to explicit server-review workflow"));
        }
        let review_id = canonical_bigint(&raw["pullRequestReview"]["fullDatabaseId"])
            .filter(|id| *id <= i64::MAX as u64)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Unsupported,
                    "Actual submitted parent review ID unavailable",
                )
            })?;
        let review_node = raw["pullRequestReview"]["id"]
            .as_str()
            .filter(|node| opaque_node(node))
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Unsupported,
                    "Actual submitted review opaque node unavailable",
                )
            })?;
        let author = actual_author(&raw["author"])?;
        if selected
            .author
            .as_ref()
            .and_then(|a| a.account_id)
            .is_some_and(|id| Some(id) != author)
            || selected.author.is_none() != raw["author"].is_null()
            || raw["viewerDidAuthor"]
                .as_bool()
                .is_some_and(|did| did != (author == identity.account_id))
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Selected numeric author/current viewer observation is contradictory",
            ));
        }
        let reply = reply_rest_id(selected, raw)?;
        let line = exact_coordinate(raw, "line")?;
        let start = exact_coordinate(raw, "startLine")?;
        let original_line = exact_coordinate(raw, "originalLine")?;
        let original_start = exact_coordinate(raw, "originalStartLine")?;
        if (line, start, original_line, original_start)
            != (
                selected.line,
                selected.start_line,
                selected.original_line,
                selected.original_start_line,
            )
            || start.is_some_and(|start| line.is_none_or(|end| start > end))
            || original_start.is_some_and(|start| original_line.is_none_or(|end| start > end))
        {
            return Err(error(GithubErrorKind::IdentityChanged,"Dedicated comment coordinates contradict actual selected observation; no current/original substitution"));
        }
        if raw["body"].as_str() != selected.body.as_deref()
            || raw["updatedAt"].as_str() != selected.updated_at.as_ref().map(MetricsTimestamp::raw)
            || raw["createdAt"].as_str() != selected.created_at.as_ref().map(MetricsTimestamp::raw)
            || raw["path"].as_str() != selected.observed_path.as_deref()
            || raw["commit"]["oid"].as_str() != selected.commit_sha.as_deref()
            || raw["originalCommit"]["oid"].as_str() != selected.original_commit_sha.as_deref()
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Selected submitted body/time/independent commit/path changed during observation",
            ));
        }
        let rest = bounded.api(
            &bounded.comment_endpoint(original.parent.summary(), id, CommentKind::Review)?,
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        if !bounded.comment_belongs(original.parent.summary(), &rest, id, CommentKind::Review)
            || rest["node_id"].as_str() != Some(node)
            || rest["pull_request_review_id"].as_u64() != Some(review_id)
            || rest["body"].as_str() != selected.body.as_deref()
            || rest["updated_at"].as_str()
                != selected.updated_at.as_ref().map(MetricsTimestamp::raw)
            || rest["created_at"].as_str()
                != selected.created_at.as_ref().map(MetricsTimestamp::raw)
            || rest["path"].as_str() != selected.observed_path.as_deref()
            || rest["commit_id"].as_str() != selected.commit_sha.as_deref()
            || rest["original_commit_id"].as_str() != selected.original_commit_sha.as_deref()
            || rest["in_reply_to_id"].as_u64() != reply
            || (reply.is_none() && !rest.get("in_reply_to_id").is_none_or(Value::is_null))
            || rest["user"].is_null() != author.is_none()
            || author.is_some_and(|id| rest["user"]["id"].as_u64() != Some(id))
        {
            return Err(error(GithubErrorKind::IdentityChanged, "Exact REST comment bridge differs in numeric/node/parent/author/body/time/provenance"));
        }
        phase(cancel, deadline)?;
        original
            .producer
            .check_clock(cancel)
            .map_err(GithubError::from)?;
        let result = OriginalThreadCommentV2 {
            original,
            index,
            principal: identity
                .account_id
                .expect("positive UID checked by original principal guard"),
            author,
            reply_parent_id: reply,
            review_id,
            review_node: review_node.into(),
            can_update: raw["viewerCanUpdate"].as_bool(),
            can_delete: raw["viewerCanDelete"].as_bool(),
            can_react: raw["viewerCanReact"].as_bool(),
            did_author: raw["viewerDidAuthor"].as_bool(),
            outdated: raw["outdated"].as_bool(),
        };
        phase(cancel, deadline)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn review_threads_v1_comment_edit_body_is_bounded_before_json_materialization() {
        for body in ["normal text 界\nline", &"x".repeat(65_536)] {
            assert!(bounded_edit_body(body).is_ok());
            assert!(serde_json::to_vec(&json!({"body":body})).unwrap().len() <= 128 * 1024);
        }
        assert!(bounded_edit_body("").is_err());
        assert!(bounded_edit_body(" \n\t").is_err());
        assert!(bounded_edit_body(&"x".repeat(65_537)).is_err());
        assert!(bounded_edit_body(&"\u{1}".repeat(65_536)).is_err());
        let exact = format!("{}x", "\u{1}".repeat((128 * 1024 - 12) / 6));
        assert!(bounded_edit_body(&exact).is_ok());
        assert_eq!(
            serde_json::to_vec(&json!({"body":exact})).unwrap().len(),
            128 * 1024 - 2
        );
    }
}
