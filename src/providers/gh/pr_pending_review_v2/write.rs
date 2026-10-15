use super::*;

/// A typed requested effect, NOT a permit. Only the original source planner's
/// explicit Confirm callback may close its current SQL/source/principal proof.
#[derive(Clone, Copy, Debug)]
pub enum ReviewMutationV2<'a> {
    CreatePending {
        body: &'a str,
        comments: &'a [InlineComment],
    },
    UpdatePendingSummary {
        observed: &'a ReviewSnapshotV2,
        body: &'a str,
    },
    SubmitPending {
        observed: &'a ReviewSnapshotV2,
        event: ReviewEvent,
        body: &'a str,
    },
    DiscardPending {
        observed: &'a ReviewSnapshotV2,
    },
    /// Eligibility is explicitly UNKNOWN until the server accepts and an exact
    /// readback confirms it. viewerCanDelete/Update is never used as dismissal.
    DismissSubmitted {
        observed: &'a ReviewSnapshotV2,
        reason: &'a str,
    },
}

fn quoted_bytes(text: &str) -> Option<usize> {
    text.bytes().try_fold(2usize, |size, byte| {
        size.checked_add(match byte {
            b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 8 | 12 => 2,
            0..=31 => 6,
            _ => 1,
        })
    })
}
pub(super) fn body_bound(body: &str, comments: &[InlineComment]) -> GithubResult<()> {
    if body.len() > 65_536 || comments.len() > 32 {
        return Err(refusal(
            GithubErrorKind::InvalidTarget,
            "Review text or comment count exceeds the bounded draft contract",
        ));
    }
    let mut encoded = quoted_bytes(body)
        .and_then(|bytes| bytes.checked_add(512))
        .ok_or_else(|| {
            refusal(
                GithubErrorKind::InvalidTarget,
                "Review encoded length overflow",
            )
        })?;
    let mut files = BTreeSet::new();
    for comment in comments {
        comment.validate()?;
        if comment.body.len() > 65_536 || comment.path.len() > 4096 {
            return Err(refusal(
                GithubErrorKind::InvalidTarget,
                "Review comment exceeds bounded body/path metadata",
            ));
        }
        files.insert(comment.path.as_str());
        encoded = quoted_bytes(&comment.body)
            .zip(quoted_bytes(&comment.path))
            .and_then(|(body, path)| body.checked_add(path))
            .and_then(|bytes| bytes.checked_add(300))
            .and_then(|bytes| encoded.checked_add(bytes))
            .ok_or_else(|| {
                refusal(
                    GithubErrorKind::InvalidTarget,
                    "Review encoded length overflow",
                )
            })?;
        if files.len() > 8 || encoded > 128 * 1024 {
            return Err(refusal(GithubErrorKind::InvalidTarget,"Review requires at most8files and128KiB actual encoded draft bytes; nothing was truncated"));
        }
    }
    if encoded > 128 * 1024 {
        return Err(refusal(
            GithubErrorKind::InvalidTarget,
            "Review encoded body exceeds128KiB; no payload or publication was prepared",
        ));
    }
    Ok(())
}
fn decision(
    identity: &HostIdentity,
    parent: &PrResourceParentV1,
    event: ReviewEvent,
    body: &str,
) -> GithubResult<()> {
    body_bound(body, &[])?;
    if !matches!(event, ReviewEvent::Approve) && body.trim().is_empty() {
        return Err(refusal(
            GithubErrorKind::InvalidTarget,
            "Comment and change-request reviews need a summary",
        ));
    }
    let uid = numeric_account(identity, &parent.summary().pr.host)?;
    if !matches!(event, ReviewEvent::Comment) {
        let author =
            parent.author_account_id().ok_or_else(|| {
                refusal(GithubErrorKind::Permission,
            "Actual numeric PR author is unavailable; approval/change request remains disabled")
            })?;
        if author == uid {
            return Err(refusal(
                GithubErrorKind::Permission,
                "The actual numeric PR author cannot approve or request changes on their own PR",
            ));
        }
    }
    Ok(())
}
fn own_pending(observed: &ReviewSnapshotV2, head: &str, require_current: bool) -> GithubResult<()> {
    if !observed.complete
        || !observed.review.is_observed_own_pending()
        || observed
            .comments
            .iter()
            .any(|comment| comment.author_id != Some(observed.review.parent.observed_by()))
    {
        return Err(refusal(
            GithubErrorKind::Permission,
            "Select a complete actual pending review owned by this numeric account",
        ));
    }
    if require_current
        && (observed.review.commit.as_ref() != head
            || observed
                .comments
                .iter()
                .any(|comment| comment.commit.as_ref() != head))
    {
        return Err(refusal(GithubErrorKind::StaleHead,"Pending review refers to an older head; preserve drafts and explicitly re-anchor before submission"));
    }
    Ok(())
}
fn expected_state(event: ReviewEvent) -> ObservedReviewStateV2 {
    match event {
        ReviewEvent::Approve => ObservedReviewStateV2::Approved,
        ReviewEvent::Comment => ObservedReviewStateV2::Commented,
        ReviewEvent::RequestChanges => ObservedReviewStateV2::ChangesRequested,
    }
}
fn same_comments(a: &ReviewSnapshotV2, b: &ReviewSnapshotV2) -> bool {
    a.complete
        && b.complete
        && a.comments.len() == b.comments.len()
        && a.comments
            .iter()
            .zip(b.comments.iter())
            .all(|(a, b)| a.same_observation(b))
}
fn created_comments(
    snapshot: &ReviewSnapshotV2,
    comments: &[InlineComment],
    uid: u64,
    head: &str,
) -> bool {
    if !snapshot.complete || snapshot.comments.len() != comments.len() {
        return false;
    }
    let mut used = BTreeSet::new();
    snapshot.comments.iter().all(|actual| {
        comments
            .iter()
            .enumerate()
            .find(|(index, expected)| {
                !used.contains(index)
                    && actual.author_id == Some(uid)
                    && actual.commit.as_ref() == head
                    && actual.in_reply_to.is_none()
                    && actual.path.as_ref() == expected.path
                    && actual.body.as_ref() == expected.body
                    && actual.side == Some(expected.side)
                    && actual.line == Some(expected.line)
                    && actual.start_line == expected.start_line
                    && actual.start_side == expected.start_side
            })
            .is_some_and(|(index, _)| used.insert(index))
    })
}

impl GithubProvider {
    pub(super) fn review_guard_v2(
        &self,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let uid = numeric_account(identity, &self.host)?;
        let current = self.identity(cancel)?;
        if numeric_account(&current, &self.host)? != uid {
            return Err(refusal(
                GithubErrorKind::IdentityChanged,
                "Numeric review account changed before publication",
            ));
        }
        self.review_parent_summary_v2(parent, cancel)
    }
    pub(super) fn review_parent_summary_v2(
        &self,
        parent: &PrResourceParentV1,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let fresh = self.pr_summary(parent.repository(), parent.number(), cancel)?;
        let original = parent.summary();
        if fresh.pr.host != self.host
            || fresh.pr.id != parent.node_id()
            || fresh.head_sha != parent.head_sha()
            || fresh.base_sha != parent.base_tip_sha()
            || fresh.head_ref != original.head_ref
            || fresh.base_ref != original.base_ref
            || fresh.pr.state != original.pr.state
            || fresh.pr.draft != original.pr.draft
            || fresh.merged != original.merged
        {
            return Err(refusal(
                GithubErrorKind::StaleHead,
                "Original review PR identity/head/base/lifecycle changed; no write was sent",
            ));
        }
        Ok(())
    }
    pub(super) fn review_files_v2(
        &self,
        parent: &PrResourceParentV1,
        comments: &[InlineComment],
        limits: &ReviewReadLimitsV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        let mut found = BTreeSet::new();
        let mut names = BTreeSet::new();
        let mut complete = false;
        for page in 1..=limits.max_pages {
            phase(cancel, deadline)?;
            let raw = self.api(
                &format!(
                    "repos/{}/pulls/{}/files?per_page=100&page={page}",
                    parent.repository(),
                    parent.number()
                ),
                "GET",
                None,
                None,
                false,
                cancel,
            )?;
            let files = raw
                .as_array()
                .filter(|files| files.len() <= 100)
                .ok_or_else(|| {
                    refusal(
                        GithubErrorKind::Json,
                        "Review Files page is not a bounded actual array",
                    )
                })?;
            for file in files {
                phase(cancel, deadline)?;
                let path = file["filename"]
                    .as_str()
                    .filter(|path| path.len() <= 4096)
                    .ok_or_else(|| {
                        refusal(
                            GithubErrorKind::Json,
                            "Review file has no bounded canonical filename",
                        )
                    })?;
                if !names.insert(path.to_owned()) {
                    return Err(refusal(
                        GithubErrorKind::Json,
                        "Review Files repeats a canonical filename",
                    ));
                }
                for (index, comment) in comments
                    .iter()
                    .enumerate()
                    .filter(|(_, comment)| comment.path == path)
                {
                    let patch = file["patch"].as_str().ok_or_else(|| refusal(GithubErrorKind::Unsupported,
                        "Selected review file has no canonical text patch; preserve draft and re-anchor"))?;
                    comment.validate_patch(patch)?;
                    found.insert(index);
                }
            }
            if files.len() < 100 {
                complete = true;
                break;
            }
        }
        if !complete || found.len() != comments.len() {
            return Err(refusal(
                GithubErrorKind::InvalidTarget,
                "Review file coverage is incomplete or an original anchor is unavailable",
            ));
        }
        Ok(())
    }

    /// ONE measured prewrite producer and ONE prepaid ordinary mutation/receipt
    /// allowance. `before_write` closes/releases the SAME original read session
    /// after fresh validation; its successful closure activates the allowance.
    #[allow(clippy::too_many_arguments)]
    pub fn mutate_review_guarded_v2(
        &self,
        measured: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        mutation: ReviewMutationV2<'_>,
        limits: &ReviewReadLimitsV2,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: impl FnOnce() -> GithubResult<()>,
    ) -> Result<ReviewReceiptV2, ReviewPublicationFailureV2> {
        if self.scoped_read_budget.is_some()
            || !self.runner.same_instance(&measured.runner)
            || self.host != measured.host
            || self.program != measured.program
            || self.env != measured.env
        {
            return Err(ReviewPublicationFailureV2::NotStarted(refusal(GithubErrorKind::InvalidTarget,
                "Review write transport must retain the exact original issuer, separate from its measured READ producer")));
        }
        let deadline = deadline.min(captures.deadline());
        let mut invoked = false;
        let result = (|| {
            phase(cancel, deadline)?;
            captures.validate_prewrite(measured, cancel)?;
            limits.validate()?;
            let receipt_slots = if matches!(mutation, ReviewMutationV2::DiscardPending { .. }) {
                limits.max_pages.saturating_add(1)
            } else {
                limits.max_pages.saturating_add(3)
            };
            if captures
                .reserved_requests()
                .saturating_sub(captures.consumed_requests())
                < receipt_slots
            {
                return Err(refusal(GithubErrorKind::InvalidTarget,"Original prepaid slots cannot cover the worst bounded mutation and complete receipt"));
            }
            let uid = numeric_account(identity, &self.host)?;
            match mutation {
                ReviewMutationV2::CreatePending { body, comments } => body_bound(body, comments)?,
                ReviewMutationV2::UpdatePendingSummary { body, .. } => body_bound(body, &[])?,
                ReviewMutationV2::SubmitPending { event, body, .. } => {
                    decision(identity, parent, event, body)?
                }
                ReviewMutationV2::DiscardPending { .. } => {}
                ReviewMutationV2::DismissSubmitted { reason, .. } => {
                    body_bound(reason, &[])?;
                    if reason.trim().is_empty() {
                        return Err(refusal(
                            GithubErrorKind::InvalidTarget,
                            "Dismissal needs an explicit reason",
                        ));
                    }
                }
            }
            let provider = bounded_provider(measured, deadline, limits.max_document_bytes)?;
            let writer = bounded_provider(self, deadline, limits.max_document_bytes)?;
            provider.review_guard_v2(identity, parent, cancel)?;
            let endpoint = format!(
                "repos/{}/pulls/{}/reviews",
                parent.repository(),
                parent.number()
            );
            let selected = match mutation {
                ReviewMutationV2::CreatePending { .. } => None,
                ReviewMutationV2::UpdatePendingSummary { observed, .. }
                | ReviewMutationV2::SubmitPending { observed, .. }
                | ReviewMutationV2::DiscardPending { observed }
                | ReviewMutationV2::DismissSubmitted { observed, .. } => {
                    let fresh = provider.review_snapshot_v2(
                        identity,
                        parent,
                        &observed.review,
                        limits,
                        cancel,
                        deadline,
                    )?;
                    if !fresh.same_observation(observed) {
                        return Err(refusal(
                            GithubErrorKind::StaleHead,
                            "Selected review/comments changed since Preview; no write was sent",
                        ));
                    }
                    Some(fresh)
                }
            };
            let (method, url, payload) = match mutation {
                ReviewMutationV2::CreatePending { body, comments } => {
                    let catalogue =
                        provider.review_catalogue_v2(identity, parent, limits, cancel, deadline)?;
                    if !catalogue.complete
                        || catalogue.rows.iter().any(|review| {
                            review.state == ObservedReviewStateV2::Pending
                                && (review.author_id.is_none() || review.author_id == Some(uid))
                        })
                    {
                        return Err(refusal(GithubErrorKind::InvalidTarget,"Pending review absence is not proven; Resume or inspect before creating another"));
                    }
                    if !comments.is_empty() {
                        provider.review_files_v2(parent, comments, limits, cancel, deadline)?;
                    }
                    (
                        "POST",
                        endpoint.clone(),
                        Some(json!({"body":body,"commit_id":parent.head_sha(),
                        "comments":comments.iter().map(InlineComment::payload).collect::<Vec<_>>()})),
                    )
                }
                ReviewMutationV2::UpdatePendingSummary { body, .. } => {
                    let observed = selected.as_ref().unwrap();
                    own_pending(observed, parent.head_sha(), false)?;
                    (
                        "PUT",
                        format!("{endpoint}/{}", observed.review.id),
                        Some(json!({"body":body})),
                    )
                }
                ReviewMutationV2::SubmitPending { event, body, .. } => {
                    let observed = selected.as_ref().unwrap();
                    own_pending(observed, parent.head_sha(), true)?;
                    (
                        "POST",
                        format!("{endpoint}/{}/events", observed.review.id),
                        Some(json!({"event":event.api_value(),"body":body})),
                    )
                }
                ReviewMutationV2::DiscardPending { .. } => {
                    let observed = selected.as_ref().unwrap();
                    own_pending(observed, parent.head_sha(), false)?;
                    ("DELETE", format!("{endpoint}/{}", observed.review.id), None)
                }
                ReviewMutationV2::DismissSubmitted { reason, .. } => {
                    let observed = selected.as_ref().unwrap();
                    if !matches!(
                        observed.review.state,
                        ObservedReviewStateV2::Approved | ObservedReviewStateV2::ChangesRequested
                    ) || !observed.review.has_observed_submission_v2()
                    {
                        return Err(refusal(GithubErrorKind::InvalidTarget,"Only an actual approval or change request can be requested for dismissal"));
                    }
                    (
                        "PUT",
                        format!("{endpoint}/{}/dismissals", observed.review.id),
                        Some(json!({"message":reason,"event":"DISMISS"})),
                    )
                }
            };
            if payload.as_ref().is_some_and(|payload| {
                serde_json::to_vec(payload).map_or(true, |bytes| bytes.len() > 128 * 1024)
            }) {
                return Err(refusal(
                    GithubErrorKind::InvalidTarget,
                    "Actual encoded review payload exceeds128KiB",
                ));
            }
            provider.review_parent_summary_v2(parent, cancel)?;
            phase(cancel, deadline)?;
            before_write()?;
            phase(cancel, deadline)?;
            invoked = true;
            let raw = captures.api(
                &writer,
                &url,
                method,
                payload,
                Some(writer.resource(parent.repository(), parent.number())),
                cancel,
            )?;
            phase(cancel, deadline)?;
            if matches!(mutation, ReviewMutationV2::DiscardPending { .. }) {
                let observed = selected.as_ref().unwrap();
                // DELETE's returned object is not deletion proof. Require actual
                // complete absence, not a404 or a capped/forbidden list.
                let returned = writer.review_mutation_response_record_v2(identity, parent, &raw)?;
                if !returned.same_observation(&observed.review) {
                    return Err(writer.ambiguous_pr(
                        parent.summary(),
                        "Discard response differs from the exact pending review",
                    ));
                }
                let remaining = writer.read_postwrite_catalogue_v2(
                    captures, identity, parent, limits, cancel, deadline,
                )?;
                if !remaining.complete
                    || remaining.rows.iter().any(|review| {
                        review.id == observed.review.id || review.node == observed.review.node
                    })
                {
                    return Err(writer.ambiguous_pr(
                        parent.summary(),
                        "Complete exact pending-review absence was not confirmed after discard",
                    ));
                }
                phase(cancel, deadline)?;
                return Ok(ReviewReceiptV2 {
                    snapshot: None,
                    review_id: observed.review.id,
                    review_node: observed.review.node.clone(),
                    discarded: true,
                });
            }
            let returned = writer.review_mutation_response_record_v2(identity, parent, &raw)?;
            let post_valid = match mutation {
                ReviewMutationV2::CreatePending { body, .. } => {
                    returned.state == ObservedReviewStateV2::Pending
                        && returned.author_id == Some(uid)
                        && returned.commit.as_ref() == parent.head_sha()
                        && returned.body.as_ref() == body
                }
                ReviewMutationV2::UpdatePendingSummary { body, .. } => {
                    let before = selected.as_ref().unwrap();
                    returned.id == before.review.id
                        && returned.node == before.review.node
                        && returned.author_id == Some(uid)
                        && returned.state == ObservedReviewStateV2::Pending
                        && returned.body.as_ref() == body
                        && returned.commit == before.review.commit
                }
                ReviewMutationV2::SubmitPending { event, body, .. } => {
                    let before = selected.as_ref().unwrap();
                    returned.id == before.review.id
                        && returned.node == before.review.node
                        && returned.author_id == Some(uid)
                        && returned.state == expected_state(event)
                        && returned.body.as_ref() == body
                        && returned.commit.as_ref() == parent.head_sha()
                        && returned.submitted_at.is_some()
                }
                ReviewMutationV2::DismissSubmitted { .. } => {
                    let before = selected.as_ref().unwrap();
                    returned.id == before.review.id
                        && returned.node == before.review.node
                        && returned.author_id == before.review.author_id
                        && returned.state == ObservedReviewStateV2::Dismissed
                        && returned.body == before.review.body
                        && returned.commit == before.review.commit
                        && returned.submitted_at == before.review.submitted_at
                }
                ReviewMutationV2::DiscardPending { .. } => unreachable!(),
            };
            if !post_valid {
                return Err(writer.ambiguous_pr(parent.summary(),"Review mutation response does not confirm the exact selected effect; no readback or retry was authorized"));
            }
            let snapshot = writer.read_created_review_v2(
                captures, identity, parent, &raw, limits, cancel, deadline,
            )?;
            let review = &snapshot.review;
            let valid = match mutation {
                ReviewMutationV2::CreatePending { body, comments } => {
                    review.state == ObservedReviewStateV2::Pending
                        && review.author_id == Some(uid)
                        && review.commit.as_ref() == parent.head_sha()
                        && review.body.as_ref() == body
                        && created_comments(&snapshot, comments, uid, parent.head_sha())
                }
                ReviewMutationV2::UpdatePendingSummary { body, .. } => {
                    let before = selected.as_ref().unwrap();
                    review.id == before.review.id
                        && review.node == before.review.node
                        && review.author_id == Some(uid)
                        && review.state == ObservedReviewStateV2::Pending
                        && review.body.as_ref() == body
                        && review.commit == before.review.commit
                        && same_comments(before, &snapshot)
                }
                ReviewMutationV2::SubmitPending { event, body, .. } => {
                    let before = selected.as_ref().unwrap();
                    review.id == before.review.id
                        && review.node == before.review.node
                        && review.author_id == Some(uid)
                        && review.state == expected_state(event)
                        && review.body.as_ref() == body
                        && review.commit.as_ref() == parent.head_sha()
                        && same_comments(before, &snapshot)
                }
                ReviewMutationV2::DismissSubmitted { .. } => {
                    let before = selected.as_ref().unwrap();
                    review.id == before.review.id
                        && review.node == before.review.node
                        && review.author_id == before.review.author_id
                        && review.state == ObservedReviewStateV2::Dismissed
                        && review.body == before.review.body
                        && review.commit == before.review.commit
                        && same_comments(before, &snapshot)
                }
                ReviewMutationV2::DiscardPending { .. } => unreachable!(),
            };
            if !valid {
                return Err(writer.ambiguous_pr(parent.summary(),"Review receipt differs from exact numeric author/node/parent/head/body/decision/comment membership"));
            }
            let receipt = ReviewReceiptV2 {
                review_id: review.id,
                review_node: review.node.clone(),
                snapshot: Some(snapshot),
                discarded: false,
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
