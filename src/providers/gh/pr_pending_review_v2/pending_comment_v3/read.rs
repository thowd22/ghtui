//! Exact PENDING protocol observations, never a submitted-thread fallback.
use super::*;

pub const PENDING_REVIEW_QUERY_V3: &str = "query($id:ID!){node(id:$id){__typename ... on PullRequestReview{id fullDatabaseId body state commit{oid} author{__typename login ... on User{databaseId} ... on Bot{databaseId}} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}}}";
pub const PENDING_COMMENT_QUERY_V3: &str = "query($id:ID!){node(id:$id){__typename ... on PullRequestReviewComment{id fullDatabaseId body state path line startLine commit{oid} updatedAt author{__typename login ... on User{databaseId} ... on Bot{databaseId}} viewerCanUpdate viewerCanDelete pullRequestReview{id fullDatabaseId state} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}}}";

pub(super) fn same_parent(raw: &Value, parent: &ReviewParentV2) -> bool {
    let pr = &raw["pullRequest"];
    let repo = &raw["repository"];
    pr["id"].as_str() == Some(parent.pr_node())
        && positive_bigint(&pr["fullDatabaseId"]) == Some(parent.pr_id())
        && positive(&pr["number"]) == Some(parent.number())
        && pr["headRefOid"].as_str() == Some(parent.head())
        && pr["baseRefOid"].as_str() == Some(parent.base())
        && repo["id"].as_str() == Some(parent.repository_node())
        && positive(&repo["databaseId"]) == Some(parent.repository_id())
        && repo["nameWithOwner"]
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(parent.repository()))
        && pr["repository"]["id"] == repo["id"]
        && pr["repository"]["databaseId"] == repo["databaseId"]
        && pr["repository"]["nameWithOwner"] == repo["nameWithOwner"]
}
pub(super) fn own_pending(
    snapshot: &ReviewSnapshotV2,
    identity: &HostIdentity,
    parent: &PrResourceParentV1,
) -> GithubResult<()> {
    let uid = numeric_account(identity, &parent.summary().pr.host)?;
    let expected = ReviewParentV2::capture(identity, parent)?;
    if !snapshot.complete()
        || snapshot.review().parent().as_ref() != expected.as_ref()
        || !snapshot.review().is_observed_own_pending()
        || snapshot.review().author_account_id() != Some(uid)
        || snapshot.comments().iter().any(|comment| {
            comment.author_account_id() != Some(uid)
                || comment.review_id() != snapshot.review().id()
        })
    {
        return Err(refusal(GithubErrorKind::Permission,
            "Only a complete actual server-PENDING review owned by the original numeric account can supply pending-comment intent"));
    }
    Ok(())
}
pub(super) fn current_node<'a>(value: &'a Value, kind: &str) -> GithubResult<&'a Value> {
    if !error_free(value) {
        return Err(refusal(
            GithubErrorKind::Json,
            "Pending observation contains GraphQL errors; no REST/submitted fallback",
        ));
    }
    value
        .get("data")
        .and_then(|data| data.get("node"))
        .filter(|node| node["__typename"].as_str() == Some(kind))
        .ok_or_else(|| {
            refusal(
                GithubErrorKind::InvalidTarget,
                "Actual pending resource is missing or has changed type",
            )
        })
}
fn bounded_graphql(
    provider: &GithubProvider,
    query: &str,
    id: &str,
    cancel: &CancellationToken,
) -> GithubResult<Value> {
    if id.is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
        return Err(refusal(
            GithubErrorKind::InvalidTarget,
            "Pending resource requires a bounded actual opaque node",
        ));
    }
    provider.api(
        "graphql",
        "POST",
        Some(json!({"query":query,"variables":{"id":id}})),
        None,
        false,
        cancel,
    )
}

impl GithubProvider {
    /// The existing bounded REST snapshot parser remains the ONE source of
    /// canonical review/comment membership. GraphQL only adds exact state and
    /// capabilities. The caller still owes original SQL/repo/final UID closure.
    #[allow(clippy::too_many_arguments)]
    pub fn pending_review_original_v3(
        &self,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        selected: &Arc<ReviewSnapshotV2>,
        limits: &ReviewReadLimitsV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<PendingReviewOriginalV3>> {
        let producer = self.scoped_read_budget.clone().ok_or_else(|| {
            refusal(
                GithubErrorKind::InvalidTarget,
                "Native pending comments require their original measured source producer",
            )
        })?;
        let deadline = deadline.min(producer.deadline()).min(
            self.scoped_read_window
                .map_or(deadline, |(original, _)| original),
        );
        phase(cancel, deadline)?;
        limits.validate()?;
        own_pending(selected, identity, parent)?;
        let provider = bounded_provider(self, deadline, limits.max_document_bytes)?;
        let fresh = provider.review_snapshot_v2(
            identity,
            parent,
            selected.review(),
            limits,
            cancel,
            deadline,
        )?;
        own_pending(&fresh, identity, parent)?;
        if !fresh.same_observation(selected) {
            return Err(refusal(
                GithubErrorKind::StaleHead,
                "Original pending review/comments changed; refresh before a new explicit intent",
            ));
        }
        let value = bounded_graphql(
            &provider,
            PENDING_REVIEW_QUERY_V3,
            fresh.review().node(),
            cancel,
        )?;
        let raw = current_node(&value, "PullRequestReview")?;
        if raw["id"].as_str() != Some(fresh.review().node())
            || positive_bigint(&raw["fullDatabaseId"]) != Some(fresh.review().id())
            || raw["state"].as_str() != Some("PENDING")
            || raw["body"].as_str() != Some(fresh.review().body())
            || raw["commit"]["oid"].as_str() != Some(fresh.review().commit())
            || positive(&raw["author"]["databaseId"]) != identity.account_id
            || !matches!(raw["author"]["__typename"].as_str(), Some("User" | "Bot"))
            || !same_parent(raw, fresh.review().parent())
        {
            return Err(refusal(
                GithubErrorKind::InvalidTarget,
                "GraphQL pending review differs from its exact original numeric REST observation",
            ));
        }
        phase(cancel, deadline)?;
        Ok(Arc::new(PendingReviewOriginalV3 {
            snapshot: Arc::new(fresh),
            producer,
            cancellation: cancel.clone(),
            deadline,
        }))
    }

    /// Index is a member of the genuine snapshot, never a guessed comment ID.
    pub fn pending_comment_original_v3(
        &self,
        review: Arc<PendingReviewOriginalV3>,
        index: usize,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<PendingCommentOriginalV3>> {
        let deadline = review.original_producer(self, cancel, deadline)?;
        let selected = review.snapshot.comments().get(index).ok_or_else(|| {
            refusal(
                GithubErrorKind::InvalidTarget,
                "Select an actual current pending comment",
            )
        })?;
        let provider = bounded_provider(self, deadline, 1024 * 1024)?;
        let value = bounded_graphql(&provider, PENDING_COMMENT_QUERY_V3, selected.node(), cancel)?;
        let raw = current_node(&value, "PullRequestReviewComment")?;
        let observed_line = |field: &str| -> GithubResult<Option<u64>> {
            match raw.get(field) {
                None | Some(Value::Null) => Ok(None),
                Some(value) => positive(value)
                    .filter(|line| *line <= i32::MAX as u64)
                    .map(Some)
                    .ok_or_else(|| {
                        refusal(
                            GithubErrorKind::Json,
                            "Pending comment coordinate has an invalid protocol type",
                        )
                    }),
            }
        };
        if raw["id"].as_str() != Some(selected.node())
            || positive_bigint(&raw["fullDatabaseId"]) != Some(selected.id())
            || raw["state"].as_str() != Some("PENDING")
            || raw["body"].as_str() != Some(selected.body())
            || raw["path"].as_str() != Some(selected.path())
            || raw["commit"]["oid"].as_str() != Some(selected.commit())
            || observed_line("line")? != selected.line()
            || observed_line("startLine")? != selected.start_line()
            || raw["pullRequestReview"]["id"].as_str() != Some(review.snapshot.review().node())
            || positive_bigint(&raw["pullRequestReview"]["fullDatabaseId"])
                != Some(review.snapshot.review().id())
            || raw["pullRequestReview"]["state"].as_str() != Some("PENDING")
            || positive(&raw["author"]["databaseId"]) != Some(review.principal())
            || !matches!(raw["author"]["__typename"].as_str(), Some("User" | "Bot"))
            || !same_parent(raw, review.parent())
        {
            return Err(refusal(GithubErrorKind::InvalidTarget,
                "Actual selected comment is not the same owned PENDING member; no submitted-comment fallback"));
        }
        let updated = raw["updatedAt"]
            .as_str()
            .filter(|text| {
                text.len() <= 128 && crate::github_workspace::timestamp_seconds(text).is_some()
            })
            .ok_or_else(|| {
                refusal(
                    GithubErrorKind::Json,
                    "Pending comment needs an actual valid bounded update timestamp",
                )
            })?;
        let can_update = optional_capability(&raw["viewerCanUpdate"])?;
        let can_delete = optional_capability(&raw["viewerCanDelete"])?;
        phase(cancel, deadline)?;
        Ok(Arc::new(PendingCommentOriginalV3 {
            review,
            index,
            can_update,
            can_delete,
            updated_at: Arc::from(updated),
        }))
    }
}
