//! Exactly one fresh Confirm attempt. A closed Preview is display material,
//! never a reusable source/access/SQL/write permit.
use super::*;
use crate::{
    drafts::{DraftClient, DraftPublicationFenceV2},
    pr_write_journal::{
        PrConfirmedEffect, PrEffectKind, PrWriteMarker, PrWriteState, PrWriteTarget,
    },
    providers::gh::ReviewPublicationFailureV2,
    scoped_pr_observed_tabs_v1::resources::file_identity,
};
use serde::ser::{SerializeSeq, SerializeStruct};
use std::{io::Write, sync::atomic::Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedReviewPublicationStateV2 {
    NotStarted,
    Uncertain,
    ConfirmedSnapshotRetired,
    ConfirmedButJournalFailed,
    ConfirmedButSourceUnavailable,
}
/// Only safe receipt metadata survives terminal source retirement. This never
/// exposes a review/comment body, actor capability or restored publication proof.
pub struct ScopedReviewPublicationV2 {
    state: ScopedReviewPublicationStateV2,
    reference: PrPublicationReference,
    review_id: Option<u64>,
    source_retired: bool,
    draft_retained: bool,
}
impl std::fmt::Debug for ScopedReviewPublicationV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopedReviewPublicationV2")
            .field("state", &self.state)
            .field("source_retired", &self.source_retired)
            .finish_non_exhaustive()
    }
}
impl ScopedReviewPublicationV2 {
    pub fn state(&self) -> ScopedReviewPublicationStateV2 {
        self.state
    }
    pub fn reference(&self) -> &PrPublicationReference {
        &self.reference
    }
    pub fn confirmed_review_id(&self) -> Option<u64> {
        self.review_id
    }
    pub fn source_retired(&self) -> bool {
        self.source_retired
    }
    pub fn hide_private(&self) -> bool {
        true
    }
    /// False after an unknown ACK is NOT proof that no draft was persisted.
    pub fn draft_retained(&self) -> bool {
        self.draft_retained
    }
}
fn pending(
    reference: PrPublicationReference,
    state: ScopedReviewPublicationStateV2,
) -> ScopedReviewPublicationV2 {
    ScopedReviewPublicationV2 {
        state,
        reference,
        review_id: None,
        source_retired: false,
        draft_retained: true,
    }
}

struct BorrowedComments<'a>(&'a [ReviewDraftCommentV2]);
impl Serialize for BorrowedComments<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Comment<'a> {
            body: &'a str,
            anchor: &'a ReviewAnchorMetadataV2,
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for comment in self.0 {
            sequence.serialize_element(&Comment {
                body: comment.body(),
                anchor: comment.anchor(),
            })?;
        }
        sequence.end()
    }
}
struct BorrowedValues<'a>(&'a ReviewDraftValuesV2);
impl Serialize for BorrowedValues<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value = serializer.serialize_struct("ReviewValues", 2)?;
        value.serialize_field("summary", self.0.summary())?;
        value.serialize_field("comments", &BorrowedComments(self.0.comments()))?;
        value.end()
    }
}
struct BoundedEncoding<'a> {
    bytes: usize,
    output: Option<Vec<u8>>,
    cancel: &'a CancellationToken,
    deadline: Instant,
}
fn preflight_reference_envelope(
    values: &ReviewDraftValuesV2,
    node: &str,
    head: &str,
    digest: &str,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<(), ScopedReviewFailureV2> {
    // Serialization-only upper bound, NOT a PrPublicationReference, review ID,
    // journal receipt or future attempt claim. All actual string metadata is
    // borrowed; only the positive SQLite integer width is conservatively maxed.
    #[derive(Serialize)]
    struct ReferenceWidth<'a> {
        attempt_id: i64,
        pr_node_id: &'a str,
        observed_head: &'a str,
        payload_sha256: &'a str,
    }
    #[derive(Serialize)]
    struct Envelope<'a> {
        version: u8,
        values: BorrowedValues<'a>,
        publication: ReferenceWidth<'a>,
    }
    let value = Envelope {
        version: 3,
        values: BorrowedValues(values),
        publication: ReferenceWidth {
            attempt_id: i64::MAX,
            pr_node_id: node,
            observed_head: head,
            payload_sha256: digest,
        },
    };
    let mut count = BoundedEncoding {
        bytes: 0,
        output: None,
        cancel,
        deadline,
    };
    serde_json::to_writer(&mut count, &value).map_err(|_| {
        refused("Exact escaped review values/reference leave no128KiB envelope; no journal claim")
    })?;
    clock(cancel, deadline)
}
impl Write for BoundedEncoding<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.cancel.is_cancelled() || Instant::now() >= self.deadline {
            return Err(std::io::Error::other(
                "Original review draft deadline/token ended",
            ));
        }
        let next = self
            .bytes
            .checked_add(bytes.len())
            .filter(|size| *size <= REVIEW_DRAFT_BYTES_V2)
            .ok_or_else(|| std::io::Error::other("Review draft exceeds128KiB before allocation"))?;
        if let Some(output) = &mut self.output {
            output.extend_from_slice(bytes);
        }
        self.bytes = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) fn encode_review_draft(
    values: &ReviewDraftValuesV2,
    reference: Option<&PrPublicationReference>,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<String, ScopedReviewFailureV2> {
    clock(cancel, deadline)?;
    values
        .validate()
        .map_err(|_| refused("Review values exceed bounded text/anchor contract"))?;
    #[derive(Serialize)]
    struct Envelope<'a> {
        version: u8,
        values: BorrowedValues<'a>,
        publication: Option<&'a PrPublicationReference>,
    }
    let envelope = Envelope {
        version: 3,
        values: BorrowedValues(values),
        publication: reference,
    };
    // Count escaped bytes using borrowed input BEFORE allocating the actual
    // output. The second pass receives exactly that bounded capacity.
    let mut count = BoundedEncoding {
        bytes: 0,
        output: None,
        cancel,
        deadline,
    };
    serde_json::to_writer(&mut count, &envelope).map_err(|_| {
        refused("Actual encoded review draft exceeds128KiB or original phase ended")
    })?;
    let mut output = BoundedEncoding {
        bytes: 0,
        output: Some(Vec::with_capacity(count.bytes)),
        cancel,
        deadline,
    };
    serde_json::to_writer(&mut output, &envelope)
        .map_err(|_| refused("Original bounded review draft encoding ended"))?;
    clock(cancel, deadline)?;
    String::from_utf8(output.output.unwrap_or_default())
        .map_err(|_| refused("Review draft encoding unavailable"))
}
impl ScopedReviewRequestV2 {
    /// Bounded local encoding only. This does not create/renew a provider phase
    /// or grant authority; publication separately consumes its original clock.
    pub fn encoded_draft(
        &self,
        reference: Option<&PrPublicationReference>,
    ) -> Result<String, String> {
        encode_review_draft(
            &self.values,
            reference,
            &CancellationToken::default(),
            Instant::now() + std::time::Duration::from_secs(1),
        )
        .map_err(|error| error.to_string())
    }
}

#[allow(clippy::too_many_arguments)]
pub(in super::super) fn validate_owned(
    local: &ResourceLocalGuard,
    database: &Path,
    intent: &ScopedReviewIntentV2,
    marker: &PrWriteMarker,
    generation: i64,
    state: PrWriteState,
) -> Result<(), GithubError> {
    if std::fs::canonicalize(database).map_err(|_| safe_local())? != local.path
        || file_identity(database)? != local.file
    {
        return Err(safe_local());
    }
    Cache::open(database)
        .map_err(|_| safe_local())?
        .validate_repository_catalogue_intent_source(&intent.source)
        .map_err(|_| safe_local())?;
    let journal = PrWriteJournal::open(database).map_err(|_| safe_local())?;
    if journal
        .source_generation(&local.target)
        .map_err(|_| safe_local())?
        != generation
    {
        return Err(safe_local());
    }
    let actual = journal
        .original_publication(&local.target, &marker.publication_reference())
        .map_err(|_| safe_local())?
        .ok_or_else(safe_local)?;
    if actual.state != state
        || actual.target != marker.target
        || actual.command != marker.command
        || actual.selected_resource != marker.selected_resource
        || actual.payload_sha256 != marker.payload_sha256
        || actual.observed_head != marker.observed_head
    {
        return Err(safe_local());
    }
    let blocking = journal
        .source_blocking(&local.target)
        .map_err(|_| safe_local())?;
    if match state {
        PrWriteState::Pending => blocking.is_none_or(|actual| {
            actual.publication_reference() != marker.publication_reference()
                || actual.state != PrWriteState::Pending
        }),
        PrWriteState::Confirmed => blocking.is_some(),
        _ => true,
    } {
        return Err(safe_local());
    }
    Ok(())
}
pub(in super::super) fn original_draft_saved(
    database: &Path,
    intent: &ScopedReviewIntentV2,
    key: &str,
    encoded: &str,
) -> Result<(), GithubError> {
    let saved = Cache::open(database)
        .map_err(|_| safe_local())?
        .bounded_draft(intent.scope(), key, REVIEW_DRAFT_BYTES_V2)
        .map_err(|_| safe_local())?;
    if saved.as_deref() != Some(encoded) {
        return Err(safe_local());
    }
    Ok(())
}

impl ScopedReviewPreviewV2 {
    /// One actual queued clock, source session, measured producer and prepaid
    /// post-close cursor. The nonclone same-queue ACK fence is consumed once.
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        provider: &GithubProvider,
        database: &Path,
        drafts: &DraftClient,
        publication: DraftPublicationFenceV2,
        confirmed: bool,
        restored: Option<&PrPublicationReference>,
        images: &PrImmutableFileLimitsV1,
        reviews: &ReviewReadLimitsV2,
        resources: &PrResourceReadLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<ScopedReviewPublicationV2, ScopedReviewFailureV2> {
        let deadline = deadline.min(publication.deadline());
        clock(cancel, deadline)?;
        if !confirmed || restored.is_some() {
            return Err(refused(
                "Fresh explicit Confirm required; restored history is never resend permission",
            ));
        }
        if self
            .started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(refused("This exact review Preview already attempted; inspect the durable result before any new action"));
        }
        self.request.preflight()?;
        images.validate().map_err(source_failure)?;
        reviews.validate().map_err(source_failure)?;
        resources.validate().map_err(source_failure)?;
        let intent = &self.request.intent;
        self.local
            .validate(database, &intent.source)
            .map_err(source_failure)?;
        let originals = original_files(&self.request, images, cancel, deadline)?;
        // Every response slot is a full bounded document (not raw body bytes).
        // Variable actual complete pages consume the SAME next slot. No skip,
        // refund or reset, even if some worst-case slots are never launched.
        let receipt_slots = reviews
            .max_pages
            .checked_add(if self.request.action == ReviewRequestedActionV2::Discard {
                1
            } else {
                3
            })
            .ok_or_else(|| refused("Review receipt slot bound overflow"))?;
        let slots = receipt_slots
            .checked_add(4)
            .filter(|slots| *slots <= 32)
            .ok_or_else(|| {
                refused("Original review receipt and visibility need at most32 prepaid captures")
            })?;
        let caps = vec![1024 * 1024; slots];
        let count = originals
            .iter()
            .try_fold(
                reviews.max_pages.saturating_mul(2).saturating_add(12),
                |count, original| count.checked_add(original.max_external_requests()),
            )
            .ok_or_else(|| refused("Original review read count exceeds bounds"))?;
        let phase_provider = provider.clone().with_catalogue_read_deadline(deadline);
        let mut session = phase_provider
            .begin_repository_catalogue_access(intent.source.source(), access, cancel)
            .map_err(source_failure)?;
        let identity = session.identity().clone();
        original_scope(intent, &identity, cancel, deadline)?;
        if identity.host != self.identity.host || identity.account_id != self.identity.account_id {
            return Err(source_failure(safe_local()));
        }
        let mut captures = session
            .reserve_postwrite_capture_v2(&caps, 2, cancel)
            .map_err(source_failure)?;
        let window = session
            .reserve_measured_external_reads(count, 1024 * 1024, images.max_pool_bytes, 2, cancel)
            .map_err(source_failure)?;
        let measured = phase_provider
            .clone()
            .with_measured_read_window(window)
            .map_err(source_failure)?;
        session
            .verify(intent.navigation.original(), cancel)
            .map_err(source_failure)?;
        let parent = measured
            .pr_inline_original_parent_v1(
                session
                    .observation_v1(intent.parent().repository_id())
                    .ok_or_else(|| source_failure(safe_local()))?,
                &intent.files.observed,
                resources,
                cancel,
            )
            .map_err(source_failure)?;
        self.local
            .validate(database, &intent.source)
            .map_err(source_failure)?;
        let current = session
            .observation_v1(parent.repository_id())
            .ok_or_else(|| source_failure(safe_local()))?;
        let comments = rehydrate(
            &self.request,
            &originals,
            &measured,
            current,
            &identity,
            &self.local,
            database,
            images,
            cancel,
            deadline,
        )?;
        if comments.len() != self.comments.len() {
            return Err(refused(
                "Original batch membership changed after Preview; no partial publication",
            ));
        }
        let observed = if let Some(selected) = &self.observed {
            let fresh = measured
                .review_snapshot_v2(
                    &identity,
                    &parent,
                    selected.review(),
                    reviews,
                    cancel,
                    deadline,
                )
                .map_err(source_failure)?;
            validate_selected(&self.request, &fresh, &identity)?;
            Some(fresh)
        } else {
            None
        };
        let write = write_for_request(
            &self.request,
            observed.as_ref(),
            parent.head_sha(),
            cancel,
            deadline,
        )?;
        if write.digest().map_err(|_| source_failure(safe_local()))?
            != self
                .write
                .digest()
                .map_err(|_| source_failure(safe_local()))?
        {
            return Err(refused(
                "Original selected review/comment content changed after Preview; nothing sent",
            ));
        }
        preflight_reference_envelope(
            &self.request.values,
            intent.parent().node_id(),
            parent.head_sha(),
            &write.digest().map_err(|_| source_failure(safe_local()))?,
            cancel,
            deadline,
        )?;
        let target = PrWriteTarget::new(&identity, parent.summary())
            .map_err(|_| source_failure(safe_local()))?;
        if target != self.local.target {
            return Err(source_failure(safe_local()));
        }
        self.local
            .validate(database, &intent.source)
            .map_err(source_failure)?;
        let mut journal =
            PrWriteJournal::open(database).map_err(|_| source_failure(safe_local()))?;
        self.request
            .validate_lineage_v12(database)
            .map_err(source_failure)?;
        let seed_v12=self.request.lineage_v12.as_ref().map(|revision|
            crate::pr_write_journal::review_action_lineage_v12::ReviewActionOriginSeedV12::from_revision(revision,None,&write))
            .transpose().map_err(|_|source_failure(safe_local()))?;
        let permit = if let Some(seed) = &seed_v12 {
            journal.begin_review_action_v12(&target, &write, seed)
        } else {
            journal.begin(&target, &write)
        }
        .map_err(|_| source_failure(safe_local()))?;
        let marker = permit.marker();
        let reference = marker.publication_reference();
        let encoded =
            match encode_review_draft(&self.request.values, Some(&reference), cancel, deadline) {
                Ok(encoded) => encoded,
                Err(_) => {
                    return Ok(pending(
                        reference,
                        ScopedReviewPublicationStateV2::NotStarted,
                    ))
                }
            };
        let begun = self
            .local
            .generation
            .checked_add(1)
            .ok_or_else(|| source_failure(safe_local()))?;
        let mut acknowledgement =
            match drafts.save_publication_and_wait_v2(publication, encoded.clone(), cancel) {
                Ok(ack) => Some(ack),
                Err(_) => {
                    let mut outcome =
                        pending(reference, ScopedReviewPublicationStateV2::NotStarted);
                    outcome.draft_retained = false;
                    return Ok(outcome);
                }
            };
        let saved = validate_owned(
            &self.local,
            database,
            intent,
            marker,
            begun,
            PrWriteState::Pending,
        )
        .is_ok()
            && original_draft_saved(database, intent, &self.draft_key, &encoded).is_ok();
        if !saved {
            let mut outcome = pending(reference, ScopedReviewPublicationStateV2::NotStarted);
            outcome.draft_retained = false;
            return Ok(outcome);
        }
        if clock(cancel, deadline).is_err()
            || acknowledgement.as_ref().is_none_or(|ack| {
                ack.check_current(drafts, database, intent.scope(), &self.draft_key, cancel)
                    .is_err()
            })
            || journal.claim(&permit, &target, &write).is_err()
        {
            return Ok(pending(
                reference,
                ScopedReviewPublicationStateV2::NotStarted,
            ));
        }
        let claimed = begun
            .checked_add(1)
            .ok_or_else(|| source_failure(safe_local()))?;
        let mut original_closed = None;
        let before_write = || {
            private_clock(cancel, deadline)?;
            acknowledgement
                .as_ref()
                .ok_or_else(safe_local)?
                .check_current(drafts, database, intent.scope(), &self.draft_key, cancel)
                .map_err(|_| safe_local())?;
            measured.pr_inline_original_parent_v1(
                session
                    .observation_v1(parent.repository_id())
                    .ok_or_else(safe_local)?,
                &intent.files.observed,
                resources,
                cancel,
            )?;
            session.verify(intent.navigation.original(), cancel)?;
            let closed = session.close_with_guard(cancel, || {
                private_clock(cancel, deadline)?;
                acknowledgement
                    .as_ref()
                    .ok_or_else(safe_local)?
                    .check_current(drafts, database, intent.scope(), &self.draft_key, cancel)
                    .map_err(|_| safe_local())?;
                validate_owned(
                    &self.local,
                    database,
                    intent,
                    marker,
                    claimed,
                    PrWriteState::Pending,
                )?;
                original_draft_saved(database, intent, &self.draft_key, &encoded)?;
                if let Some(seed) = &seed_v12 {
                    seed.validate_owned_basis(database, &target, marker)
                        .map_err(|_| safe_local())?;
                }
                private_clock(cancel, deadline)
            })?;
            if closed.identity().host != identity.host
                || closed.identity().account_id != identity.account_id
            {
                return Err(safe_local());
            }
            original_closed = Some(closed);
            acknowledgement
                .as_ref()
                .ok_or_else(safe_local)?
                .check_current(drafts, database, intent.scope(), &self.draft_key, cancel)
                .map_err(|_| safe_local())?;
            private_clock(cancel, deadline)
        };
        let receipt = match phase_provider.mutate_review_guarded_v2(
            &measured,
            &mut captures,
            &identity,
            &parent,
            self.mutation_with(observed.as_ref(), &comments),
            reviews,
            cancel,
            deadline,
            before_write,
        ) {
            Err(ReviewPublicationFailureV2::NotStarted(_)) => {
                return Ok(pending(
                    reference,
                    ScopedReviewPublicationStateV2::NotStarted,
                ))
            }
            Err(ReviewPublicationFailureV2::WriteMayHaveStarted(_)) => {
                let _ = journal.mark_uncertain(&permit);
                let retired = matches!(
                    crate::pr_projection::invalidate_unresolved_discussions(
                        database,
                        &identity,
                        parent.summary(),
                        &reference
                    ),
                    Ok(Some(_))
                );
                let mut outcome = pending(reference, ScopedReviewPublicationStateV2::Uncertain);
                outcome.source_retired = retired;
                return Ok(outcome);
            }
            Ok(receipt) => receipt,
        };
        let review_id = receipt.review_id();
        let effect = PrConfirmedEffect {
            kind: if receipt.discarded() {
                PrEffectKind::ReviewDiscarded
            } else {
                PrEffectKind::Review
            },
            resource_id: review_id.to_string(),
            node_id: Some(receipt.review_node().into()),
        };
        if journal.mark_confirmed(&permit, &effect).is_err() {
            let retired = matches!(
                crate::pr_projection::invalidate_unresolved_discussions(
                    database,
                    &identity,
                    parent.summary(),
                    &reference
                ),
                Ok(Some(_))
            );
            return Ok(ScopedReviewPublicationV2 {
                state: ScopedReviewPublicationStateV2::ConfirmedButJournalFailed,
                reference,
                review_id: Some(review_id),
                source_retired: retired,
                draft_retained: true,
            });
        }
        let confirmed_generation = claimed
            .checked_add(1)
            .ok_or_else(|| source_failure(safe_local()))?;
        let mut draft_retained = true;
        let visible = original_closed
            .as_ref()
            .and_then(|closed| closed.observation(parent.repository_id()))
            .is_some_and(|current| {
                phase_provider
                    .close_review_postwrite_visibility_v2(
                        &identity,
                        current,
                        &parent,
                        &mut captures,
                        cancel,
                        deadline,
                        || {
                            private_clock(cancel, deadline)?;
                            validate_owned(
                                &self.local,
                                database,
                                intent,
                                marker,
                                confirmed_generation,
                                PrWriteState::Confirmed,
                            )?;
                            original_draft_saved(database, intent, &self.draft_key, &encoded)?;
                            if let Some(seed) = &seed_v12 {
                                seed.validate_owned_basis(database, &target, marker)
                                    .map_err(|_| safe_local())?;
                            }
                            let ack = acknowledgement.take().ok_or_else(safe_local)?;
                            ack.check_current(
                                drafts,
                                database,
                                intent.scope(),
                                &self.draft_key,
                                cancel,
                            )
                            .map_err(|_| safe_local())?;
                            drafts
                                .remove_publication_and_wait_v2(ack, cancel)
                                .map_err(|_| safe_local())?;
                            draft_retained = false;
                            validate_owned(
                                &self.local,
                                database,
                                intent,
                                marker,
                                confirmed_generation,
                                PrWriteState::Confirmed,
                            )?;
                            private_clock(cancel, deadline)
                        },
                    )
                    .is_ok()
            })
            && clock(cancel, deadline).is_ok();
        // Actual fewer complete pages legitimately leave charged unused slots.
        let visible =
            visible && captures.consumed_requests() >= 6 && captures.consumed_requests() <= slots;
        let source_retired = matches!(
            crate::pr_projection::invalidate_confirmed_discussions(
                database,
                &identity,
                parent.summary(),
                &reference
            ),
            Ok(Some(_))
        );
        let state = if visible && source_retired && clock(cancel, deadline).is_ok() {
            ScopedReviewPublicationStateV2::ConfirmedSnapshotRetired
        } else {
            ScopedReviewPublicationStateV2::ConfirmedButSourceUnavailable
        };
        Ok(ScopedReviewPublicationV2 {
            state,
            reference,
            review_id: Some(review_id),
            source_retired,
            draft_retained,
        })
    }
}
