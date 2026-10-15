//! Fresh worker-only review Preview. Saved anchor metadata is rehydrated through
//! the SAME genuine image producer; neither this result nor a draft is a lease.
use super::*;
use crate::{
    pr_write_journal::PrWriteIntent,
    providers::gh::{
        InlineComment, ObservedReviewStateV2, PrImageSideV1, PrImmutableFileLimitsV1,
        PrImmutableFileOriginalV1, ReviewEvent, ReviewMutationV2,
    },
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::AtomicBool,
};
#[path = "scoped_pr_review_execute_v2.rs"]
pub(super) mod execute;
pub use execute::{ScopedReviewPublicationStateV2, ScopedReviewPublicationV2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewRequestedActionV2 {
    CreatePending,
    UpdateSummary,
    SubmitComment,
    Approve,
    RequestChanges,
    Discard,
    Dismiss,
}
impl ReviewRequestedActionV2 {
    pub fn label(self) -> &'static str {
        match self {
            Self::CreatePending => "Create server PENDING review — not submitted",
            Self::UpdateSummary => "Update pending summary",
            Self::SubmitComment => "Submit comment review",
            Self::Approve => "Submit approval",
            Self::RequestChanges => "Submit change request",
            Self::Discard => "Discard exact server pending review",
            Self::Dismiss => "Request dismissal — server eligibility UNKNOWN",
        }
    }
    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::CreatePending => "review-create-pending-v2",
            Self::UpdateSummary => "review-update-pending-v2",
            Self::SubmitComment | Self::Approve | Self::RequestChanges => "review-submit-v2",
            Self::Discard => "review-discard-v2",
            Self::Dismiss => "review-dismiss-v2",
        }
    }
    pub(crate) fn draft_tag(self) -> &'static str {
        match self {
            Self::CreatePending => "create",
            Self::UpdateSummary => "summary",
            Self::SubmitComment => "comment",
            Self::Approve => "approve",
            Self::RequestChanges => "request-changes",
            Self::Discard => "discard",
            Self::Dismiss => "dismiss",
        }
    }
    fn event(self) -> Option<ReviewEvent> {
        match self {
            Self::SubmitComment => Some(ReviewEvent::Comment),
            Self::Approve => Some(ReviewEvent::Approve),
            Self::RequestChanges => Some(ReviewEvent::RequestChanges),
            _ => None,
        }
    }
}
pub struct ScopedReviewRequestV2 {
    pub(super) intent: Arc<ScopedReviewIntentV2>,
    pub(super) action: ReviewRequestedActionV2,
    pub(super) values: Arc<ReviewDraftValuesV2>,
    pub(super) selection: Option<Arc<ScopedReviewSelectionV2>>,
    pub(super) key: Arc<str>,
    pub(super) lineage_v12: Option<
        Arc<crate::pr_write_journal::review_action_lineage_v12::ReviewAcknowledgedRevisionV12>,
    >,
    blocked_new_v12: bool,
}
impl std::fmt::Debug for ScopedReviewRequestV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedReviewRequestV2(<closed source and requested effect; no approval>)")
    }
}
impl ScopedReviewRequestV2 {
    pub fn create(draft: &Arc<ScopedReviewDraftV2>, values: Arc<ReviewDraftValuesV2>) -> Arc<Self> {
        Arc::new(Self {
            intent: draft.intent.clone(),
            action: ReviewRequestedActionV2::CreatePending,
            values,
            selection: None,
            key: Arc::from(draft.key()),
            lineage_v12: draft.lineage_v12.clone(),
            blocked_new_v12: draft.needs_new_v12,
        })
    }
    pub fn selected(
        selection: Arc<ScopedReviewSelectionV2>,
        action: ReviewRequestedActionV2,
        summary: Arc<str>,
    ) -> Result<Arc<Self>, String> {
        if action == ReviewRequestedActionV2::CreatePending {
            return Err("Use actual draft batch for creating a pending review".into());
        }
        let values = Arc::new(ReviewDraftValuesV2::replace(summary, Arc::from([]))?);
        let key = selection.action_key(action)?;
        Ok(Arc::new(Self {
            intent: selection.intent.clone(),
            action,
            values,
            selection: Some(selection),
            key: Arc::from(key),
            lineage_v12: None,
            blocked_new_v12: false,
        }))
    }
    pub fn intent(&self) -> &Arc<ScopedReviewIntentV2> {
        &self.intent
    }
    pub(crate) fn selected_draft_v12(
        draft: &Arc<ScopedReviewDraftV2>,
        selection: Arc<ScopedReviewSelectionV2>,
        action: ReviewRequestedActionV2,
        summary: Arc<str>,
    ) -> Result<Arc<Self>, String> {
        if draft.needs_new_v12 || !Arc::ptr_eq(draft.intent(), selection.intent()) {
            return Err("Explicit new-action draft/selection mismatch".into());
        }
        let mut result = Self::selected(selection, action, summary)?;
        if let Some(revision) = &draft.lineage_v12 {
            let request =
                Arc::get_mut(&mut result).ok_or("New action request unexpectedly shared")?;
            request.key = draft.key.clone();
            request.lineage_v12 = Some(revision.clone());
        }
        Ok(result)
    }
    pub fn action(&self) -> ReviewRequestedActionV2 {
        self.action
    }
    pub fn values(&self) -> &Arc<ReviewDraftValuesV2> {
        &self.values
    }
    pub(super) fn validate_lineage_v12(&self, database: &Path) -> Result<(), GithubError> {
        let base = crate::pr_write_journal::review_action_lineage_v12::ReviewActionBaseV12::review(
            &self.intent,
            self.selection.as_ref(),
            self.action,
        )
        .map_err(|_| safe_local())?;
        if let Some(revision) = &self.lineage_v12 {
            if revision.key() != self.key.as_ref() {
                return Err(safe_local());
            }
            revision
                .validate(database, &base)
                .map_err(|_| safe_local())?;
        } else if PrWriteJournal::open(database)
            .map_err(|_| safe_local())?
            .action_revision_v12(&base)
            .map_err(|_| safe_local())?
            .is_some()
        {
            return Err(safe_local());
        }
        Ok(())
    }
    fn preflight(&self) -> Result<(), ScopedReviewFailureV2> {
        if self.blocked_new_v12 {
            return Err(refused(
                "Explicit New action required; protected history cannot prepare a request",
            ));
        }
        self.values
            .validate()
            .map_err(|_| refused("Review body/anchors exceed bounded encoded draft contract"))?;
        let body = self.values.summary();
        if matches!(
            self.action,
            ReviewRequestedActionV2::SubmitComment
                | ReviewRequestedActionV2::RequestChanges
                | ReviewRequestedActionV2::Dismiss
        ) && body.trim().is_empty()
        {
            return Err(refused(
                "This review action needs an explicit nonempty summary/reason",
            ));
        }
        if self.action == ReviewRequestedActionV2::Discard && !body.is_empty() {
            return Err(refused(
                "Discard does not publish an invented review summary",
            ));
        }
        if self.action == ReviewRequestedActionV2::CreatePending {
            for comment in self.values.comments() {
                if comment.anchor().head() != self.intent.parent().head_sha() {
                    return Err(refused("Saved anchors refer to an older H; retain text and explicitly re-anchor every changed selection"));
                }
            }
        } else if self
            .selection
            .as_ref()
            .is_none_or(|selected| !Arc::ptr_eq(&selected.intent, &self.intent))
        {
            return Err(refused("Select an actual current-source review; no raw/restored ID can create this request"));
        }
        Ok(())
    }
}

pub struct ScopedReviewPreviewV2 {
    pub(super) request: Arc<ScopedReviewRequestV2>,
    pub(super) observed: Option<Arc<ReviewSnapshotV2>>,
    pub(super) comments: Arc<[InlineComment]>,
    pub(super) identity: HostIdentity,
    pub(super) local: ResourceLocalGuard,
    pub(super) write: PrWriteIntent,
    pub(super) lines: Arc<[String]>,
    pub(super) display_v1: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    pub(super) actor_rows_v7: Arc<[(usize, super::super::avatars_v7::PreparedActorV7)]>,
    pub(super) requester_v7: super::super::avatars_v7::PreparedActorV7,
    pub(super) draft_key: Arc<str>,
    pub(super) started: AtomicBool,
}
impl std::fmt::Debug for ScopedReviewPreviewV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedReviewPreviewV2(<actual fresh observed effect; no publication permit>)")
    }
}
impl ScopedReviewPreviewV2 {
    pub fn request(&self) -> &Arc<ScopedReviewRequestV2> {
        &self.request
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn lines(&self) -> &Arc<[String]> {
        &self.lines
    }
    pub(crate) fn display_v1(&self) -> &Arc<crate::prepared_form_text_v1::PreparedFormTextV1> {
        &self.display_v1
    }
    pub fn draft_key(&self) -> &str {
        &self.draft_key
    }
    pub(crate) fn actor_rows_v7(&self) -> &[(usize, super::super::avatars_v7::PreparedActorV7)] {
        &self.actor_rows_v7
    }
    pub(crate) fn requester_v7(&self) -> &super::super::avatars_v7::PreparedActorV7 {
        &self.requester_v7
    }
    pub(super) fn mutation_with<'a>(
        &'a self,
        observed: Option<&'a ReviewSnapshotV2>,
        comments: &'a [InlineComment],
    ) -> ReviewMutationV2<'a> {
        let body = self.request.values.summary();
        match self.request.action {
            ReviewRequestedActionV2::CreatePending => {
                ReviewMutationV2::CreatePending { body, comments }
            }
            ReviewRequestedActionV2::UpdateSummary => ReviewMutationV2::UpdatePendingSummary {
                observed: observed.expect("closed selected preview"),
                body,
            },
            ReviewRequestedActionV2::SubmitComment
            | ReviewRequestedActionV2::Approve
            | ReviewRequestedActionV2::RequestChanges => ReviewMutationV2::SubmitPending {
                observed: observed.expect("closed selected preview"),
                event: self.request.action.event().expect("submit action"),
                body,
            },
            ReviewRequestedActionV2::Discard => ReviewMutationV2::DiscardPending {
                observed: observed.expect("closed selected preview"),
            },
            ReviewRequestedActionV2::Dismiss => ReviewMutationV2::DismissSubmitted {
                observed: observed.expect("closed selected preview"),
                reason: body,
            },
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        request: Arc<ScopedReviewRequestV2>,
        provider: &GithubProvider,
        database: &Path,
        images: &PrImmutableFileLimitsV1,
        reviews: &ReviewReadLimitsV2,
        resources: &PrResourceReadLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, ScopedReviewFailureV2> {
        clock(cancel, deadline)?;
        request.preflight()?;
        images.validate().map_err(source_failure)?;
        reviews.validate().map_err(source_failure)?;
        resources.validate().map_err(source_failure)?;
        let intent = &request.intent;
        let originals = original_files(&request, images, cancel, deadline)?;
        let request_count = originals
            .iter()
            .try_fold(reviews.max_pages + 4, |count, original| {
                count.checked_add(original.max_external_requests())
            })
            .ok_or_else(|| refused("Review read count exceeds original bounded pool"))?;
        let provider = provider.clone().with_catalogue_read_deadline(deadline);
        let mut session = provider
            .begin_repository_catalogue_access(intent.source.source(), access, cancel)
            .map_err(source_failure)?;
        let identity = session.identity().clone();
        original_scope(intent, &identity, cancel, deadline)?;
        let window = session
            .reserve_measured_external_reads(
                request_count,
                1024 * 1024,
                images.max_pool_bytes,
                2,
                cancel,
            )
            .map_err(source_failure)?;
        let measured = provider
            .clone()
            .with_measured_read_window(window)
            .map_err(source_failure)?;
        session
            .verify(intent.navigation.original(), cancel)
            .map_err(source_failure)?;
        let current = session
            .observation_v1(intent.parent().repository_id())
            .ok_or_else(|| source_failure(safe_local()))?;
        let parent = measured
            .pr_inline_original_parent_v1(current, &intent.files.observed, resources, cancel)
            .map_err(source_failure)?;
        let local =
            ResourceLocalGuard::capture(database, &intent.source, &identity, parent.summary())
                .map_err(source_failure)?;
        let comments = rehydrate(
            &request, &originals, &measured, current, &identity, &local, database, images, cancel,
            deadline,
        )?;
        let observed = if request.action == ReviewRequestedActionV2::CreatePending {
            let catalogue = measured
                .review_catalogue_v2(&identity, &parent, reviews, cancel, deadline)
                .map_err(source_failure)?;
            if !catalogue.complete()
                || catalogue.rows().iter().any(|review| {
                    review.state() == &ObservedReviewStateV2::Pending
                        && (review.author_account_id().is_none()
                            || review.author_account_id() == identity.account_id)
                })
            {
                return Err(refused("Complete own-pending absence not proven; Resume/inspect before creating another"));
            }
            None
        } else {
            let selected = request
                .selection
                .as_ref()
                .ok_or_else(|| refused("Original review selection unavailable"))?;
            let fresh = measured
                .review_snapshot_v2(
                    &identity,
                    &parent,
                    selected.snapshot.review(),
                    reviews,
                    cancel,
                    deadline,
                )
                .map_err(source_failure)?;
            validate_selected(&request, &fresh, &identity)?;
            Some(Arc::new(fresh))
        };
        let key = request.key.clone();
        let write = write_for_request(
            &request,
            observed.as_deref(),
            parent.head_sha(),
            cancel,
            deadline,
        )?;
        let actor_refs = observed
            .as_ref()
            .map(|snapshot| {
                std::iter::once((
                    snapshot.review().author(),
                    snapshot.review().author_account_id(),
                ))
                .chain(
                    snapshot
                        .comments()
                        .iter()
                        .map(|comment| (comment.author(), comment.author_account_id())),
                )
                .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let prepared_actors =
            super::super::avatars_v7::batch(&identity.host, &actor_refs, cancel, deadline)
                .map_err(|_| refused("Original review actor display exceeds bounded metadata"))?;
        let mut actor_rows = Vec::new();
        let mut lines=vec![request.action.label().into(),format!("{}#{} • actual repo ID {} • actual PR ID {}",parent.repository(),parent.number(),parent.repository_id(),parent.database_id()),
            format!("Current H {} • base {}",parent.head_sha(),parent.base_tip_sha()),
            "Not an expected-head CAS: GitHub may change after checks; no auto retry. Explicit Confirm rechecks original account/source/SQL/head.".into()];
        if let Some(observed) = &observed {
            let actor = &prepared_actors[0];
            actor_rows.push((lines.len(), actor.clone()));
            lines.push(format!(
                "Exact review {} • {} • author {} • actual UID {:?} • observed commit {}",
                observed.review().id(),
                observed.review().state().label(),
                actor.label(),
                actor.uid(),
                observed.review().commit()
            ));
        }
        lines.push(format!(
            "Exact summary/reason: {}",
            crate::git::display_bytes(request.values.summary().as_bytes())
        ));
        for (index, comment) in request.values.comments().iter().enumerate() {
            clock(cancel, deadline)?;
            lines.push(format!(
                "Comment {}: {} {:?} {}–{} • M {}",
                index + 1,
                crate::git::display_bytes(comment.anchor.server_path.as_bytes()),
                comment.anchor.side,
                comment.anchor.first,
                comment.anchor.last,
                comment.anchor.merge_base
            ));
            lines.push(crate::git::display_bytes(comment.body().as_bytes()));
        }
        if let Some(snapshot) = &observed {
            for (comment, actor) in snapshot
                .comments()
                .iter()
                .zip(prepared_actors.iter().skip(1))
            {
                clock(cancel, deadline)?;
                actor_rows.push((lines.len(), actor.clone()));
                lines.push(format!(
                    "Observed comment #{} • author {} • actual UID {:?}",
                    comment.id(),
                    actor.label(),
                    actor.uid()
                ));
            }
        }
        if lines
            .iter()
            .try_fold(0usize, |bytes, line| bytes.checked_add(line.len()))
            .is_none_or(|bytes| bytes > 8 * 1024 * 1024)
        {
            return Err(refused(
                "Review preview exceeds bounded display; no partial approval",
            ));
        }
        let display_v1 = crate::prepared_form_text_v1::PreparedFormTextV1::prepare(
            &lines, cancel, deadline,
        )
        .map_err(|_| {
            refused("Review Preview display exceeds bounded native presentation; draft retained")
        })?;
        measured
            .pr_inline_original_parent_v1(
                session
                    .observation_v1(intent.parent().repository_id())
                    .ok_or_else(|| source_failure(safe_local()))?,
                &intent.files.observed,
                resources,
                cancel,
            )
            .map_err(source_failure)?;
        session
            .verify(intent.navigation.original(), cancel)
            .map_err(source_failure)?;
        let closed = session
            .close_with_guard(cancel, || {
                private_clock(cancel, deadline)?;
                local.validate(database, &intent.source)?;
                request.validate_lineage_v12(database)?;
                private_clock(cancel, deadline)
            })
            .map_err(source_failure)?;
        clock(cancel, deadline)?;
        let requester_v7 = super::super::avatars_v7::PreparedActorV7::requester(
            closed.identity(),
            cancel,
            deadline,
        )
        .map_err(|_| refused("Original closing requester display unavailable"))?;
        let result = Arc::new(Self {
            request,
            observed,
            comments: comments.into(),
            identity: closed.identity().clone(),
            local,
            write,
            lines: lines.into(),
            display_v1,
            actor_rows_v7: actor_rows.into(),
            requester_v7,
            draft_key: key,
            started: AtomicBool::new(false),
        });
        clock(cancel, deadline)?;
        Ok(result)
    }
}

fn write_for_request(
    request: &ScopedReviewRequestV2,
    observed: Option<&ReviewSnapshotV2>,
    head: &str,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<PrWriteIntent, ScopedReviewFailureV2> {
    clock(cancel, deadline)?;
    let selection = observed
        .map_or_else(
            || request.intent.draft_key(),
            |observed| Ok(format!("review/{}", observed.review().id())),
        )
        .map_err(|_| refused("Original review draft target unavailable"))?;
    let mut values = BTreeMap::from([("summary".into(), request.values.summary().into())]);
    if request.action == ReviewRequestedActionV2::CreatePending {
        values.insert(
            "batch".into(),
            request
                .values
                .encoded_values()
                .map_err(|_| refused("Actual encoded review batch exceeds128KiB"))?,
        );
    }
    if let Some(observed) = observed {
        values.insert(
            "selected_snapshot_sha256".into(),
            snapshot_digest(observed, cancel, deadline)?,
        );
    }
    let mut write = PrWriteIntent {
        command: request.action.command().into(),
        selected_resource: selection,
        observed_head: head.into(),
        values,
    };
    if let Some(revision) = &request.lineage_v12 {
        revision
            .bind_write(&mut write)
            .map_err(|_| refused("Actual explicit new-action fingerprint changed"))?;
    }
    write
        .digest()
        .map_err(|_| refused("Prepared review fingerprint exceeds bounded payload contract"))?;
    clock(cancel, deadline)?;
    Ok(write)
}

fn original_files(
    request: &ScopedReviewRequestV2,
    limits: &PrImmutableFileLimitsV1,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Vec<PrImmutableFileOriginalV1>, ScopedReviewFailureV2> {
    original_files_for_values_v3(&request.intent, &request.values, limits, cancel, deadline)
}
pub(super) fn original_files_for_values_v3(
    intent: &ScopedReviewIntentV2,
    values: &ReviewDraftValuesV2,
    limits: &PrImmutableFileLimitsV1,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Vec<PrImmutableFileOriginalV1>, ScopedReviewFailureV2> {
    let mut seen = BTreeSet::new();
    let mut originals = Vec::new();
    for comment in values.comments() {
        clock(cancel, deadline)?;
        if !seen.insert(comment.anchor.server_path.as_str()) {
            continue;
        }
        let mut found = None;
        for (index, file) in intent.files.observed.files().iter().enumerate() {
            clock(cancel, deadline)?;
            if file.path == comment.anchor.server_path && found.replace(index).is_some() {
                return Err(refused("Original review file is ambiguous"));
            }
        }
        let index=found.ok_or_else(||refused("A saved selected file is absent from this actual Files source; refresh/re-anchor explicitly"))?;
        originals.push(
            PrImmutableFileOriginalV1::from_observed(
                intent.scope(),
                intent.files.observed.clone(),
                index,
                limits,
            )
            .map_err(source_failure)?,
        );
    }
    clock(cancel, deadline)?;
    Ok(originals)
}
#[allow(clippy::too_many_arguments)]
fn rehydrate(
    request: &ScopedReviewRequestV2,
    originals: &[PrImmutableFileOriginalV1],
    provider: &GithubProvider,
    current: &crate::providers::gh::CurrentCatalogueRepository,
    identity: &HostIdentity,
    local: &ResourceLocalGuard,
    database: &Path,
    limits: &PrImmutableFileLimitsV1,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Vec<InlineComment>, ScopedReviewFailureV2> {
    rehydrate_values_v3(
        &request.intent,
        &request.values,
        originals,
        provider,
        current,
        identity,
        local,
        database,
        limits,
        cancel,
        deadline,
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn rehydrate_values_v3(
    intent: &ScopedReviewIntentV2,
    values: &ReviewDraftValuesV2,
    originals: &[PrImmutableFileOriginalV1],
    provider: &GithubProvider,
    current: &crate::providers::gh::CurrentCatalogueRepository,
    identity: &HostIdentity,
    local: &ResourceLocalGuard,
    database: &Path,
    limits: &PrImmutableFileLimitsV1,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Vec<InlineComment>, ScopedReviewFailureV2> {
    let mut comments = (0..values.comments().len())
        .map(|_| None)
        .collect::<Vec<Option<InlineComment>>>();
    for original in originals {
        clock(cancel, deadline)?;
        let observed = provider
            .pr_immutable_file_images_v1(current, identity, original, limits, cancel, |_| {
                private_clock(cancel, deadline)?;
                local.validate(database, &intent.source)?;
                private_clock(cancel, deadline)
            })
            .map_err(source_failure)?;
        let file = intent
            .files
            .observed
            .files()
            .get(original.file_index())
            .ok_or_else(|| refused("Original file member unavailable"))?;
        for (index, comment) in values
            .comments()
            .iter()
            .enumerate()
            .filter(|(_, comment)| comment.anchor.server_path == file.path)
        {
            clock(cancel, deadline)?;
            let anchor = &comment.anchor;
            let side = if anchor.side == crate::providers::gh::DiffSide::Left {
                PrImageSideV1::Old
            } else {
                PrImageSideV1::New
            };
            if observed.parent().head_sha() != anchor.head
                || observed.merge_base() != anchor.merge_base
                || observed.patch_digest() != anchor.patch_digest
                || observed.image(side).path() != anchor.image_path
                || !observed.range_admitted(side, anchor.first, anchor.last)
            {
                return Err(refused("Original H/M/path/range/digest changed; preserve text and re-anchor explicitly"));
            }
            let canonical = intent
                .files
                .canonical_diff()
                .ok_or_else(|| refused("Original canonical review patch unavailable"))?;
            let mut endpoint = false;
            for file in canonical.files.iter().filter(|file| {
                file.new_path.as_deref().or(file.old_path.as_deref())
                    == Some(anchor.server_path.as_bytes())
            }) {
                for hunk in &file.hunks {
                    clock(cancel, deadline)?;
                    for line in &hunk.lines {
                        clock(cancel, deadline)?;
                        endpoint |= match anchor.side {
                            crate::providers::gh::DiffSide::Left => {
                                line.old_line == Some(anchor.last)
                                    && line.kind == crate::diff::LineKind::Removed
                            }
                            crate::providers::gh::DiffSide::Right => {
                                line.new_line == Some(anchor.last)
                                    && matches!(
                                        line.kind,
                                        crate::diff::LineKind::Added
                                            | crate::diff::LineKind::Context
                                    )
                            }
                        };
                    }
                }
            }
            if !endpoint {
                return Err(refused("Restored range does not end on deleted LEFT or added/context RIGHT content; re-anchor explicitly"));
            }
            comments[index] = Some(
                anchor
                    .comment(comment.body())
                    .map_err(|_| refused("Original review anchor is malformed"))?,
            );
        }
    }
    clock(cancel, deadline)?;
    // Exact original indexes preserve duplicate/nonadjacent user order; a
    // missing member is refusal, never an invented empty placeholder comment.
    let comments = comments
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            refused("An original review comment was not rehydrated; no partial batch")
        })?;
    clock(cancel, deadline)?;
    Ok(comments)
}
fn validate_selected(
    request: &ScopedReviewRequestV2,
    snapshot: &ReviewSnapshotV2,
    identity: &HostIdentity,
) -> Result<(), ScopedReviewFailureV2> {
    if !snapshot.complete() {
        return Err(refused(
            "Complete actual review comments required before Preview; no first page is approval",
        ));
    }
    let review = snapshot.review();
    if request.action == ReviewRequestedActionV2::Dismiss {
        if !matches!(
            review.state(),
            ObservedReviewStateV2::Approved | ObservedReviewStateV2::ChangesRequested
        ) || !review.has_observed_submission_v2()
        {
            return Err(refused("Select an actual approval/change request; dismissal eligibility remains UNKNOWN until server response"));
        }
        return Ok(());
    }
    if !review.is_observed_own_pending()
        || review.author_account_id() != identity.account_id
        || snapshot
            .comments()
            .iter()
            .any(|comment| comment.author_account_id() != identity.account_id)
    {
        return Err(refused(
            "Select a complete server pending review owned by this actual numeric account",
        ));
    }
    if request.action.event().is_some()
        && (review.commit() != request.intent.parent().head_sha()
            || snapshot
                .comments()
                .iter()
                .any(|comment| comment.commit() != request.intent.parent().head_sha()))
    {
        return Err(refused("Pending review/comments have an older head; retain text and explicitly re-anchor before Submit"));
    }
    if matches!(
        request.action,
        ReviewRequestedActionV2::Approve | ReviewRequestedActionV2::RequestChanges
    ) && request
        .intent
        .parent()
        .author_account_id()
        .is_none_or(|uid| Some(uid) == identity.account_id)
    {
        return Err(refused("Actual numeric PR author is unknown or is this reviewer; approval/change request disabled"));
    }
    Ok(())
}
fn snapshot_digest(
    snapshot: &ReviewSnapshotV2,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<String, ScopedReviewFailureV2> {
    let mut digest = Sha256::new();
    let review = snapshot.review();
    digest.update(
        serde_json::to_vec(&(
            review.id(),
            review.node(),
            review.author_account_id(),
            review.body(),
            review.state().label(),
            review.commit(),
            review.submitted_at(),
        ))
        .map_err(|_| refused("Actual review fingerprint unavailable"))?,
    );
    for comment in snapshot.comments() {
        clock(cancel, deadline)?;
        digest.update(
            serde_json::to_vec(&(
                comment.id(),
                comment.node(),
                comment.review_id(),
                comment.author_account_id(),
                comment.body(),
                comment.path(),
                comment.commit(),
                comment.side(),
                comment.line(),
                comment.start_line(),
                comment.start_side(),
                comment.in_reply_to(),
            ))
            .map_err(|_| refused("Actual pending-comment fingerprint unavailable"))?,
        );
    }
    clock(cancel, deadline)?;
    Ok(format!("{:x}", digest.finalize()))
}
