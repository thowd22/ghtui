use super::*;
use crate::providers::gh::search::canonical_source_url_matches;

// ONE parser and bounded pagination implementation. The only post-close
// transport is a privately issued, already charged fixed-slot allowance.
enum ReviewReadTransportV2<'a> {
    Measured(&'a GithubProvider),
    Receipt {
        provider: &'a GithubProvider,
        captures: &'a mut PostWriteCaptureAllowanceV2,
    },
}
impl ReviewReadTransportV2<'_> {
    fn provider(&self) -> &GithubProvider {
        match self {
            Self::Measured(provider) | Self::Receipt { provider, .. } => provider,
        }
    }
    fn get(&mut self, endpoint: &str, cancel: &CancellationToken) -> GithubResult<Value> {
        match self {
            Self::Measured(provider) => provider.api(endpoint, "GET", None, None, false, cancel),
            Self::Receipt { provider, captures } => {
                captures.api(provider, endpoint, "GET", None, None, cancel)
            }
        }
    }
}

fn parent_url(provider: &GithubProvider, parent: &ReviewParentV2, raw: &Value) -> GithubResult<()> {
    let path = format!("repos/{}/pulls/{}", parent.repository(), parent.number());
    if raw["pull_request_url"]
        .as_str()
        .is_none_or(|url| !canonical_source_url_matches(&provider.host, url, &path, true, false))
    {
        return Err(refusal(
            GithubErrorKind::InvalidTarget,
            "Review belongs to another original pull request",
        ));
    }
    Ok(())
}
fn raw_review(
    provider: &GithubProvider,
    parent: Arc<ReviewParentV2>,
    raw: &Value,
) -> GithubResult<Arc<ReviewRecordV2>> {
    parent_url(provider, &parent, raw)?;
    let id = positive(&raw["id"])
        .ok_or_else(|| refusal(GithubErrorKind::Json, "Review has no positive actual ID"))?;
    let node = node(raw)?;
    let (author, author_id) = actor(&raw["user"])?;
    let body = observed_text(raw, "body", 65_536, true)?.unwrap();
    let state = ObservedReviewStateV2::parse(observed_text(raw, "state", 64, true)?.unwrap());
    let commit = observed_text(raw, "commit_id", 64, true)?.unwrap();
    validate_sha(&commit)?;
    let submitted_at = observed_text(raw, "submitted_at", 128, false)?;
    if state == ObservedReviewStateV2::Pending && submitted_at.is_some() {
        return Err(refusal(
            GithubErrorKind::Json,
            "Pending review unexpectedly claims a submission timestamp",
        ));
    }
    Ok(Arc::new(ReviewRecordV2 {
        parent,
        id,
        node,
        author,
        author_id,
        body,
        state,
        commit,
        submitted_at,
    }))
}
fn optional_line(raw: &Value, field: &str) -> GithubResult<Option<u64>> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => positive(value).map(Some).ok_or_else(|| {
            refusal(
                GithubErrorKind::Json,
                "Review comment has an invalid observed line or ID",
            )
        }),
    }
}
fn optional_side(raw: &Value, field: &str) -> GithubResult<Option<DiffSide>> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(side)) if side == "LEFT" => Ok(Some(DiffSide::Left)),
        Some(Value::String(side)) if side == "RIGHT" => Ok(Some(DiffSide::Right)),
        _ => Err(refusal(
            GithubErrorKind::Json,
            "Review comment has no recognized observed side",
        )),
    }
}
fn raw_comment(
    provider: &GithubProvider,
    review: &ReviewRecordV2,
    raw: &Value,
) -> GithubResult<ReviewCommentRecordV2> {
    parent_url(provider, &review.parent, raw)?;
    let id = positive(&raw["id"]).ok_or_else(|| {
        refusal(
            GithubErrorKind::Json,
            "Review comment has no positive actual ID",
        )
    })?;
    if positive(&raw["pull_request_review_id"]) != Some(review.id) {
        return Err(refusal(
            GithubErrorKind::InvalidTarget,
            "Comment belongs to another actual review",
        ));
    }
    let node = node(raw)?;
    let (author, author_id) = actor(&raw["user"])?;
    let body = observed_text(raw, "body", 65_536, true)?.unwrap();
    let path = observed_text(raw, "path", 4096, true)?.unwrap();
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\0')
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(refusal(
            GithubErrorKind::Json,
            "Review comment has no canonical relative API filename",
        ));
    }
    let commit = observed_text(raw, "commit_id", 64, true)?.unwrap();
    validate_sha(&commit)?;
    let side = optional_side(raw, "side")?;
    let line = optional_line(raw, "line")?;
    let start_line = optional_line(raw, "start_line")?;
    let start_side = optional_side(raw, "start_side")?;
    if start_line.is_some() != start_side.is_some()
        || start_line
            .zip(line)
            .is_some_and(|(first, last)| first > last)
        || start_side
            .zip(side)
            .is_some_and(|(first, last)| first != last)
    {
        return Err(refusal(
            GithubErrorKind::Json,
            "Review comment has contradictory observed range metadata",
        ));
    }
    Ok(ReviewCommentRecordV2 {
        id,
        node,
        review_id: review.id,
        author,
        author_id,
        body,
        path,
        commit,
        side,
        line,
        start_line,
        start_side,
        in_reply_to: optional_line(raw, "in_reply_to_id")?,
    })
}

impl GithubProvider {
    /// Intermediate observations only; caller owns the SAME original-source
    /// session/local/final-principal closing proof before any private display.
    pub fn review_catalogue_v2(
        &self,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        limits: &ReviewReadLimitsV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<ReviewCatalogueV2> {
        if self.scoped_read_budget.is_none() {
            return Err(refusal(
                GithubErrorKind::InvalidTarget,
                "Native review reads require their original measured source producer",
            ));
        }
        let provider = bounded_provider(self, deadline, limits.max_document_bytes)?;
        catalogue(
            &mut ReviewReadTransportV2::Measured(&provider),
            identity,
            parent,
            limits,
            cancel,
            deadline,
        )
    }

    pub fn review_snapshot_v2(
        &self,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        selected: &Arc<ReviewRecordV2>,
        limits: &ReviewReadLimitsV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<ReviewSnapshotV2> {
        if self.scoped_read_budget.is_none() {
            return Err(refusal(
                GithubErrorKind::InvalidTarget,
                "Native review reads require their original measured source producer",
            ));
        }
        let provider = bounded_provider(self, deadline, limits.max_document_bytes)?;
        snapshot(
            &mut ReviewReadTransportV2::Measured(&provider),
            identity,
            parent,
            selected,
            limits,
            cancel,
            deadline,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Keep the original receipt transport, identity, target, limits, and phase explicit"
    )]
    pub(super) fn read_created_review_v2(
        &self,
        captures: &mut PostWriteCaptureAllowanceV2,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        raw: &Value,
        limits: &ReviewReadLimitsV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<ReviewSnapshotV2> {
        let context = ReviewParentV2::capture(identity, parent)?;
        let selected = raw_review(self, context, raw)?;
        let observed = snapshot(
            &mut ReviewReadTransportV2::Receipt {
                provider: self,
                captures,
            },
            identity,
            parent,
            &selected,
            limits,
            cancel,
            deadline,
        )?;
        if !selected.same_observation(&observed.review) {
            return Err(self.ambiguous_pr(
                parent.summary(),
                "Review mutation response and exact readback changed original review metadata",
            ));
        }
        Ok(observed)
    }
    /// SAME existing snapshot parser and SAME prepaid immutable slot cursor.
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep the original receipt cursor, identity, parent, selected member, limits, cancellation and deadline explicit"
    )]
    pub(super) fn receipt_snapshot_v3(
        &self,
        captures: &mut PostWriteCaptureAllowanceV2,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        selected: &Arc<ReviewRecordV2>,
        limits: &ReviewReadLimitsV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<ReviewSnapshotV2> {
        snapshot(
            &mut ReviewReadTransportV2::Receipt {
                provider: self,
                captures,
            },
            identity,
            parent,
            selected,
            limits,
            cancel,
            deadline,
        )
    }
    pub(super) fn read_postwrite_catalogue_v2(
        &self,
        captures: &mut PostWriteCaptureAllowanceV2,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        limits: &ReviewReadLimitsV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<ReviewCatalogueV2> {
        catalogue(
            &mut ReviewReadTransportV2::Receipt {
                provider: self,
                captures,
            },
            identity,
            parent,
            limits,
            cancel,
            deadline,
        )
    }
    pub(super) fn review_mutation_response_record_v2(
        &self,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        raw: &Value,
    ) -> GithubResult<Arc<ReviewRecordV2>> {
        raw_review(self, ReviewParentV2::capture(identity, parent)?, raw)
    }
}

fn catalogue(
    transport: &mut ReviewReadTransportV2<'_>,
    identity: &HostIdentity,
    parent: &PrResourceParentV1,
    limits: &ReviewReadLimitsV2,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<ReviewCatalogueV2> {
    limits.validate()?;
    phase(cancel, deadline)?;
    let parent = ReviewParentV2::capture(identity, parent)?;
    let mut rows = Vec::new();
    let mut ids = BTreeSet::new();
    let mut nodes = BTreeSet::new();
    let mut complete = false;
    let mut pages = 0;
    for page in 1..=limits.max_pages {
        phase(cancel, deadline)?;
        let raw = transport.get(
            &format!(
                "repos/{}/pulls/{}/reviews?per_page=100&page={page}",
                parent.repository(),
                parent.number()
            ),
            cancel,
        )?;
        let values = raw
            .as_array()
            .filter(|values| values.len() <= 100)
            .ok_or_else(|| {
                refusal(
                    GithubErrorKind::Json,
                    "Review page is not a bounded actual array",
                )
            })?;
        pages += 1;
        let before = rows.len();
        for value in values {
            phase(cancel, deadline)?;
            if rows.len() >= limits.max_reviews {
                break;
            }
            let row = raw_review(transport.provider(), parent.clone(), value)?;
            if !ids.insert(row.id) || !nodes.insert(row.node.clone()) {
                return Err(refusal(
                    GithubErrorKind::Json,
                    "Review pages repeat or conflict on actual identities",
                ));
            }
            rows.push(row);
        }
        if values.len() < 100 {
            // A capped page cannot establish absence of an unseen member.
            complete = rows.len() - before == values.len();
            break;
        }
        if rows.len() >= limits.max_reviews {
            break;
        }
    }
    let result = ReviewCatalogueV2 {
        parent,
        rows: rows.into(),
        complete,
        pages,
    };
    phase(cancel, deadline)?;
    Ok(result)
}

fn snapshot(
    transport: &mut ReviewReadTransportV2<'_>,
    identity: &HostIdentity,
    parent: &PrResourceParentV1,
    selected: &Arc<ReviewRecordV2>,
    limits: &ReviewReadLimitsV2,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<ReviewSnapshotV2> {
    limits.validate()?;
    phase(cancel, deadline)?;
    let current_parent = ReviewParentV2::capture(identity, parent)?;
    if current_parent != selected.parent {
        return Err(refusal(
            GithubErrorKind::InvalidTarget,
            "Review selection belongs to another original source or numeric account",
        ));
    }
    let endpoint = format!(
        "repos/{}/pulls/{}/reviews/{}",
        parent.repository(),
        parent.number(),
        selected.id
    );
    let first_raw = transport.get(&endpoint, cancel)?;
    let first = raw_review(transport.provider(), current_parent.clone(), &first_raw)?;
    if first.id != selected.id || first.node != selected.node {
        return Err(refusal(
            GithubErrorKind::InvalidTarget,
            "Selected original review identity changed",
        ));
    }
    let mut comments = Vec::new();
    let mut ids = BTreeSet::new();
    let mut nodes = BTreeSet::new();
    let mut complete = false;
    for page in 1..=limits.max_pages {
        phase(cancel, deadline)?;
        let raw = transport.get(
            &format!("{endpoint}/comments?per_page=100&page={page}"),
            cancel,
        )?;
        let values = raw
            .as_array()
            .filter(|values| values.len() <= 100)
            .ok_or_else(|| {
                refusal(
                    GithubErrorKind::Json,
                    "Review comment page is not a bounded actual array",
                )
            })?;
        let before = comments.len();
        for value in values {
            phase(cancel, deadline)?;
            if comments.len() >= limits.max_comments {
                break;
            }
            let row = raw_comment(transport.provider(), &first, value)?;
            if !ids.insert(row.id) || !nodes.insert(row.node.clone()) {
                return Err(refusal(
                    GithubErrorKind::Json,
                    "Review comments repeat or conflict on actual identities",
                ));
            }
            comments.push(row);
        }
        if values.len() < 100 {
            complete = comments.len() - before == values.len();
            break;
        }
        if comments.len() >= limits.max_comments {
            break;
        }
    }
    let last_raw = transport.get(&endpoint, cancel)?;
    let last = raw_review(transport.provider(), current_parent, &last_raw)?;
    if !first.same_observation(&last) {
        return Err(refusal(
            GithubErrorKind::StaleHead,
            "Review summary/state changed during bounded comment observation",
        ));
    }
    let result = ReviewSnapshotV2 {
        review: last,
        comments: comments.into(),
        complete,
    };
    phase(cancel, deadline)?;
    Ok(result)
}
