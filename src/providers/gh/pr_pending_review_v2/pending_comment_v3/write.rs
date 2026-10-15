//! One explicit pending-only mutation and bounded genuine receipt. Ordinary
//! write transport spends the original closed session's fixed prepaid cursor.
use super::*;

const RETURN_COMMENT_V3:&str="__typename id fullDatabaseId body state path line startLine commit{oid} updatedAt author{__typename login ... on User{databaseId} ... on Bot{databaseId}} pullRequestReview{id fullDatabaseId state} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}";
const ABSENCE_V3: &str =
    "query($id:ID!){node(id:$id){__typename ... on PullRequestReviewComment{id}}}";
const REVIEW_ABSENCE_V3: &str =
    "query($id:ID!){node(id:$id){__typename ... on PullRequestReview{id}}}";
impl<'a> PendingCommentMutationV3<'a> {
    fn review(self) -> &'a PendingReviewOriginalV3 {
        match self {
            Self::Add { review, .. } => review,
            Self::Edit { comment, .. } | Self::Delete { comment } => &comment.review,
        }
    }
    fn selected(self) -> Option<&'a PendingCommentOriginalV3> {
        match self {
            Self::Add { .. } => None,
            Self::Edit { comment, .. } | Self::Delete { comment } => Some(comment),
        }
    }
}
fn same_intent(
    expected: PendingCommentMutationV3<'_>,
    fresh: PendingCommentMutationV3<'_>,
) -> bool {
    if !expected
        .review()
        .snapshot
        .same_observation(&fresh.review().snapshot)
    {
        return false;
    }
    match (expected, fresh) {
        (
            PendingCommentMutationV3::Add { comment: a, .. },
            PendingCommentMutationV3::Add { comment: b, .. },
        ) => {
            a.path == b.path
                && a.side == b.side
                && a.line == b.line
                && a.start_line == b.start_line
                && a.start_side == b.start_side
                && a.body == b.body
        }
        (
            PendingCommentMutationV3::Edit {
                comment: a,
                body: x,
            },
            PendingCommentMutationV3::Edit {
                comment: b,
                body: y,
            },
        ) => a.same_observation(b) && x == y,
        (
            PendingCommentMutationV3::Delete { comment: a },
            PendingCommentMutationV3::Delete { comment: b },
        ) => a.same_observation(b),
        _ => false,
    }
}
fn payload(mutation: PendingCommentMutationV3<'_>) -> GithubResult<Value> {
    let review = mutation.review();
    let (input_type, operation, input, fields) = match mutation {
        PendingCommentMutationV3::Add { comment, .. } => {
            comment.validate()?;
            if !review.has_current_commit()
                || comment.line > i32::MAX as u64
                || comment
                    .start_line
                    .is_some_and(|line| line > i32::MAX as u64)
            {
                return Err(refusal(GithubErrorKind::InvalidTarget,
                    "Pending add requires current H and positive signed-32-bit canonical GraphQL coordinates"));
            }
            super::super::write::body_bound("", std::slice::from_ref(comment))?;
            let mut input = json!({"pullRequestReviewId":review.snapshot.review().node(),
                "path":comment.path,"line":comment.line,"side":comment.side,
                "subjectType":"LINE","body":comment.body});
            if let Some(first) = comment.start_line {
                input["startLine"] = json!(first);
                input["startSide"] = json!(comment.start_side);
            }
            ("AddPullRequestReviewThreadInput","addPullRequestReviewThread",input,
                format!("thread{{id comments(first:2){{nodes{{{RETURN_COMMENT_V3}}} totalCount pageInfo{{hasNextPage}}}}}}"))
        }
        PendingCommentMutationV3::Edit { comment, body } => {
            super::super::write::body_bound(body, &[])?;
            if body.trim().is_empty() || comment.can_update != Some(true) {
                return Err(refusal(GithubErrorKind::Permission,
                    "Pending edit needs nonempty bounded text and this actual comment's update capability"));
            }
            (
                "UpdatePullRequestReviewCommentInput",
                "updatePullRequestReviewComment",
                json!({"pullRequestReviewCommentId":comment.comment().node(),"body":body}),
                format!("pullRequestReviewComment{{{RETURN_COMMENT_V3}}}"),
            )
        }
        PendingCommentMutationV3::Delete { comment } => {
            if comment.can_delete != Some(true) {
                return Err(refusal(
                    GithubErrorKind::Permission,
                    "Only this actual pending comment's delete capability enables deletion",
                ));
            }
            ("DeletePullRequestReviewCommentInput","deletePullRequestReviewComment",
                json!({"id":comment.comment().node()}),
                "pullRequestReview{id fullDatabaseId state} pullRequestReviewComment{id fullDatabaseId}".into())
        }
    };
    let query = format!("mutation($input:{input_type}!){{{operation}(input:$input){{{fields}}}}}");
    let value = json!({"query":query,"variables":{"input":input}});
    // Actual bounded encoding, not raw body length; the query and opaque IDs
    // also consume this envelope. Checked before extra validation transport.
    if serde_json::to_vec(&value).map_or(true, |encoded| encoded.len() > 128 * 1024) {
        return Err(refusal(
            GithubErrorKind::InvalidTarget,
            "Actual encoded pending-comment mutation exceeds128KiB; nothing queued",
        ));
    }
    Ok(value)
}
fn returned<'a>(value: &'a Value, operation: &str) -> GithubResult<&'a Value> {
    if !error_free(value) {
        return Err(refusal(
            GithubErrorKind::AmbiguousWrite,
            "Pending mutation returned GraphQL errors; inspect without resending",
        ));
    }
    value
        .get("data")
        .and_then(|data| data.get(operation))
        .filter(|raw| raw.is_object())
        .ok_or_else(|| {
            refusal(
                GithubErrorKind::AmbiguousWrite,
                "Pending mutation has no genuine typed payload",
            )
        })
}
fn returned_comment(
    raw: &Value,
    review: &PendingReviewOriginalV3,
    body: &str,
) -> GithubResult<(u64, Arc<str>)> {
    let id = positive_bigint(&raw["fullDatabaseId"]).ok_or_else(|| {
        refusal(
            GithubErrorKind::AmbiguousWrite,
            "Pending mutation comment has no actual positive full-width ID",
        )
    })?;
    let node = raw["id"]
        .as_str()
        .filter(|node| {
            !node.is_empty() && node.len() <= 1024 && !node.chars().any(char::is_control)
        })
        .ok_or_else(|| {
            refusal(
                GithubErrorKind::AmbiguousWrite,
                "Pending mutation comment has no actual opaque node",
            )
        })?;
    if raw["__typename"].as_str() != Some("PullRequestReviewComment")
        || raw["state"].as_str() != Some("PENDING")
        || raw["body"].as_str() != Some(body)
        || raw["pullRequestReview"]["id"].as_str() != Some(review.snapshot.review().node())
        || positive_bigint(&raw["pullRequestReview"]["fullDatabaseId"])
            != Some(review.snapshot.review().id())
        || raw["pullRequestReview"]["state"].as_str() != Some("PENDING")
        || positive(&raw["author"]["databaseId"]) != Some(review.principal())
        || !matches!(raw["author"]["__typename"].as_str(), Some("User" | "Bot"))
        || !read::same_parent(raw, review.parent())
    {
        return Err(refusal(
            GithubErrorKind::AmbiguousWrite,
            "Actual pending response changed parent/review/principal/state/body; not confirmed",
        ));
    }
    Ok((id, Arc::from(node)))
}
fn same_except_body(a: &ReviewCommentRecordV2, b: &ReviewCommentRecordV2) -> bool {
    a.id() == b.id()
        && a.node() == b.node()
        && a.review_id() == b.review_id()
        && a.author_account_id() == b.author_account_id()
        && a.path() == b.path()
        && a.commit() == b.commit()
        && a.side() == b.side()
        && a.line() == b.line()
        && a.start_line() == b.start_line()
        && a.start_side() == b.start_side()
        && a.in_reply_to() == b.in_reply_to()
}
fn receipt_matches(
    before: &ReviewSnapshotV2,
    after: &ReviewSnapshotV2,
    mutation: PendingCommentMutationV3<'_>,
    id: u64,
    node: &str,
) -> bool {
    if !before.review().same_observation(after.review()) || !after.complete() {
        return false;
    }
    let untouched = before
        .comments()
        .iter()
        .filter(|old| {
            mutation
                .selected()
                .is_none_or(|changed| changed.comment().id() != old.id())
        })
        .all(|old| after.comments().iter().any(|now| old.same_observation(now)));
    if !untouched {
        return false;
    }
    match mutation {
        PendingCommentMutationV3::Add { comment, .. } => {
            after.comments().len() == before.comments().len() + 1
                && !before
                    .comments()
                    .iter()
                    .any(|old| old.id() == id || old.node() == node)
                && after.comments().iter().any(|actual| {
                    actual.id() == id
                        && actual.node() == node
                        && actual.body() == comment.body
                        && actual.path() == comment.path
                        && actual.commit() == before.review().parent().head()
                        && actual.author_account_id()
                            == Some(before.review().parent().observed_by())
                        && actual.side() == Some(comment.side)
                        && actual.line() == Some(comment.line)
                        && actual.start_line() == comment.start_line
                        && actual.start_side() == comment.start_side
                        && actual.in_reply_to().is_none()
                })
        }
        PendingCommentMutationV3::Edit { comment, body } => {
            after.comments().len() == before.comments().len()
                && id == comment.comment().id()
                && node == comment.comment().node()
                && after.comments().iter().any(|actual| {
                    same_except_body(actual, comment.comment()) && actual.body() == body
                })
        }
        PendingCommentMutationV3::Delete { comment } => {
            before.comments().len() == after.comments().len() + 1
                && id == comment.comment().id()
                && node == comment.comment().node()
                && !after
                    .comments()
                    .iter()
                    .any(|actual| actual.id() == id || actual.node() == node)
        }
    }
}

impl GithubProvider {
    #[allow(clippy::too_many_arguments)]
    pub fn mutate_pending_comment_guarded_v3(
        &self,
        measured: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        expected: PendingCommentMutationV3<'_>,
        fresh: PendingCommentMutationV3<'_>,
        limits: &ReviewReadLimitsV2,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: impl FnOnce() -> GithubResult<()>,
    ) -> Result<PendingCommentReceiptV3, ReviewPublicationFailureV2> {
        let mut invoked = false;
        let result = (|| {
            let deadline = fresh
                .review()
                .original_producer(measured, cancel, deadline)?
                .min(captures.deadline());
            phase(cancel, deadline)?;
            if self.scoped_read_budget.is_some()
                || self.host != measured.host
                || self.program != measured.program
                || self.env != measured.env
                || !self.runner.same_instance(&measured.runner)
                || !same_intent(expected, fresh)
            {
                return Err(refusal(GithubErrorKind::InvalidTarget,
                    "Pending mutation must retain exact original ordinary issuer/fresh source/Preview intent"));
            }
            limits.validate()?;
            captures.validate_prewrite(measured, cancel)?;
            let receipt_slots = limits
                .max_pages
                .checked_add(
                    if matches!(fresh, PendingCommentMutationV3::Delete { .. }) {
                        4
                    } else {
                        3
                    },
                )
                .ok_or_else(|| {
                    refusal(
                        GithubErrorKind::InvalidTarget,
                        "Pending receipt slot bound overflow",
                    )
                })?;
            if captures
                .reserved_requests()
                .saturating_sub(captures.consumed_requests())
                < receipt_slots
            {
                return Err(refusal(
                    GithubErrorKind::InvalidTarget,
                    "Original prepaid cursor cannot cover the complete pending receipt",
                ));
            }
            // Includes >i32MAX coordinate and actual encoded-body refusal
            // before fresh validation children, not a downcast or truncation.
            let payload = payload(fresh)?;
            let provider = bounded_provider(measured, deadline, limits.max_document_bytes)?;
            let writer = bounded_provider(self, deadline, limits.max_document_bytes)?;
            provider.review_guard_v2(identity, parent, cancel)?;
            let final_review = provider.pending_review_original_v3(
                identity,
                parent,
                &fresh.review().snapshot,
                limits,
                cancel,
                deadline,
            )?;
            if let Some(selected) = fresh.selected() {
                let final_comment = provider.pending_comment_original_v3(
                    final_review.clone(),
                    selected.index,
                    cancel,
                    deadline,
                )?;
                if !selected.same_observation(&final_comment) {
                    return Err(refusal(GithubErrorKind::StaleHead,
                        "Pending comment state/body/time/capability changed after Preview; no write sent"));
                }
            }
            if let PendingCommentMutationV3::Add { comment, .. } = fresh {
                provider.review_files_v2(
                    parent,
                    std::slice::from_ref(comment),
                    limits,
                    cancel,
                    deadline,
                )?;
            }
            provider.review_parent_summary_v2(parent, cancel)?;
            phase(cancel, deadline)?;
            before_write()?;
            phase(cancel, deadline)?;
            invoked = true;
            let value = captures.api(
                &writer,
                "graphql",
                "POST",
                Some(payload),
                Some(writer.resource(parent.repository(), parent.number())),
                cancel,
            )?;
            phase(cancel, deadline)?;
            let (id, node, deleted) = match fresh {
                PendingCommentMutationV3::Add { comment, .. } => {
                    let raw = returned(&value, "addPullRequestReviewThread")?;
                    let thread = &raw["thread"];
                    let nodes =
                        thread["comments"]["nodes"]
                            .as_array()
                            .filter(|nodes| nodes.len() == 1)
                            .ok_or_else(|| {
                                refusal(GithubErrorKind::AmbiguousWrite,
                            "New pending thread does not contain exactly one acknowledged root")
                            })?;
                    if thread["id"].as_str().is_none_or(|node| {
                        node.is_empty() || node.len() > 1024 || node.chars().any(char::is_control)
                    }) || thread["comments"]["totalCount"].as_u64() != Some(1)
                        || thread["comments"]["pageInfo"]["hasNextPage"].as_bool() != Some(false)
                    {
                        return Err(refusal(GithubErrorKind::AmbiguousWrite,
                            "Actual pending thread response is partial/ambiguous; no first-root guess"));
                    }
                    let (id, node) = returned_comment(&nodes[0], &final_review, &comment.body)?;
                    (id, node, false)
                }
                PendingCommentMutationV3::Edit { body, .. } => {
                    let raw = returned(&value, "updatePullRequestReviewComment")?;
                    let (id, node) =
                        returned_comment(&raw["pullRequestReviewComment"], &final_review, body)?;
                    (id, node, false)
                }
                PendingCommentMutationV3::Delete { comment } => {
                    let raw = returned(&value, "deletePullRequestReviewComment")?;
                    let review = &raw["pullRequestReview"];
                    let deleted = &raw["pullRequestReviewComment"];
                    if deleted["id"].as_str() != Some(comment.comment().node())
                        || positive_bigint(&deleted["fullDatabaseId"])
                            != Some(comment.comment().id())
                    {
                        return Err(refusal(GithubErrorKind::AmbiguousWrite,
                            "Actual deleted-comment acknowledgement is null/changed; no last-comment absence inferred"));
                    }
                    if raw.get("pullRequestReview") == Some(&Value::Null) {
                        // No GET404 fallback: this closed branch exists ONLY
                        // for explicit nullable parent plus actual comment ack.
                        let catalogue = writer.read_postwrite_catalogue_v2(
                            captures, identity, parent, limits, cancel, deadline,
                        )?;
                        if !catalogue.complete()
                            || catalogue.rows().iter().any(|review| {
                                review.id() == final_review.snapshot.review().id()
                                    || review.node() == final_review.snapshot.review().node()
                            })
                        {
                            return Err(refusal(GithubErrorKind::AmbiguousWrite,
                                "Nullable parent needs complete original catalogue and exact review numeric/opaque absence"));
                        }
                        let original_review = final_review.snapshot.review();
                        for (query, node) in [
                            (REVIEW_ABSENCE_V3, original_review.node()),
                            (ABSENCE_V3, comment.comment().node()),
                        ] {
                            phase(cancel, deadline)?;
                            let absent = captures.api(
                                &writer,
                                "graphql",
                                "POST",
                                Some(json!({"query":query,"variables":{"id":node}})),
                                None,
                                cancel,
                            )?;
                            if !error_free(&absent)
                                || absent.get("data").and_then(|data| data.get("node"))
                                    != Some(&Value::Null)
                            {
                                return Err(refusal(GithubErrorKind::AmbiguousWrite,
                                    "Both actual original review and pending comment need separate error-free exact-node null observations"));
                            }
                        }
                        phase(cancel, deadline)?;
                        let receipt = PendingCommentReceiptV3 {
                            snapshot: None,
                            original_review_id: original_review.id(),
                            review_and_comment_absent: true,
                            comment_id: comment.comment().id(),
                            comment_node: Arc::from(comment.comment().node()),
                            deleted: true,
                        };
                        phase(cancel, deadline)?;
                        return Ok(receipt);
                    }
                    if review["id"].as_str() != Some(final_review.snapshot.review().node())
                        || positive_bigint(&review["fullDatabaseId"])
                            != Some(final_review.snapshot.review().id())
                        || review["state"].as_str() != Some("PENDING")
                    {
                        return Err(refusal(GithubErrorKind::AmbiguousWrite,
                            "Actual deletion parent is missing/changed; no pending/discard inference"));
                    }
                    (
                        comment.comment().id(),
                        Arc::from(comment.comment().node()),
                        true,
                    )
                }
            };
            let after = writer.receipt_snapshot_v3(
                captures,
                identity,
                parent,
                final_review.snapshot.review(),
                limits,
                cancel,
                deadline,
            )?;
            read::own_pending(&after, identity, parent)?;
            if !receipt_matches(&final_review.snapshot, &after, fresh, id, &node) {
                return Err(refusal(GithubErrorKind::AmbiguousWrite,
                    "Pending receipt changed review summary/state or other actual comments; inspect without resend"));
            }
            if deleted {
                let absent = captures.api(
                    &writer,
                    "graphql",
                    "POST",
                    Some(json!({"query":ABSENCE_V3,"variables":{"id":node.as_ref()}})),
                    None,
                    cancel,
                )?;
                if !error_free(&absent)
                    || absent.get("data").and_then(|data| data.get("node")) != Some(&Value::Null)
                {
                    return Err(refusal(GithubErrorKind::AmbiguousWrite,
                        "Pending delete requires acknowledgement PLUS error-free exact-node null readback"));
                }
            }
            phase(cancel, deadline)?;
            let receipt = PendingCommentReceiptV3 {
                snapshot: Some(after),
                original_review_id: final_review.snapshot.review().id(),
                review_and_comment_absent: false,
                comment_id: id,
                comment_node: node,
                deleted,
            };
            phase(cancel, deadline)?;
            Ok(receipt)
        })();
        result.map_err(|error| {
            if invoked {
                self.runner
                    .mark_ambiguous(&self.resource(parent.repository(), parent.number()));
                ReviewPublicationFailureV2::WriteMayHaveStarted(error)
            } else {
                ReviewPublicationFailureV2::NotStarted(error)
            }
        })
    }
}
