//! One Confirm on the SAME Review queue, original token/absolute deadline,
//! measured prewrite pool, prepaid cursor and existing DraftClient/ACK. No
//! source/permission is restored from the saved body, origin or Preview.
use super::super::preview::execute::{original_draft_saved, validate_owned};
use super::*;
use crate::{
    drafts::{DraftClient, DraftPublicationFenceV2},
    pr_write_journal::{PrConfirmedEffect, PrEffectKind, PrWriteState, PrWriteTarget},
    providers::gh::{
        PendingCommentMutationV3, ReviewPublicationFailureV2, ReviewRequestActorKindV3,
    },
};
use preview_v3::ReviewOperationPreviewV3;
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewOperationSafeEffectV3 {
    PendingComment {
        id: u64,
        original_review: u64,
        deleted: bool,
        review_and_comment_absent: bool,
    },
    ReviewerRequested {
        kind: ReviewRequestActorKindV3,
        id: u64,
    },
}
pub struct ReviewOperationPublicationV3 {
    state: ScopedReviewPublicationStateV2,
    reference: PrPublicationReference,
    effect: Option<ReviewOperationSafeEffectV3>,
    source_retired: bool,
    draft_retained: bool,
}
impl std::fmt::Debug for ReviewOperationPublicationV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReviewOperationPublicationV3")
            .field("state", &self.state)
            .field("effect", &self.effect)
            .field("source_retired", &self.source_retired)
            .finish_non_exhaustive()
    }
}
impl ReviewOperationPublicationV3 {
    pub fn state(&self) -> ScopedReviewPublicationStateV2 {
        self.state
    }
    pub fn reference(&self) -> &PrPublicationReference {
        &self.reference
    }
    pub fn safe_effect(&self) -> Option<ReviewOperationSafeEffectV3> {
        self.effect
    }
    pub fn source_retired(&self) -> bool {
        self.source_retired
    }
    pub fn draft_retained(&self) -> bool {
        self.draft_retained
    }
    pub fn hide_private(&self) -> bool {
        true
    }
}
fn pending(
    reference: PrPublicationReference,
    state: ScopedReviewPublicationStateV2,
) -> ReviewOperationPublicationV3 {
    ReviewOperationPublicationV3 {
        state,
        reference,
        effect: None,
        source_retired: false,
        draft_retained: true,
    }
}
fn same_inline(a: &InlineComment, b: &InlineComment) -> bool {
    a.path == b.path
        && a.body == b.body
        && a.side == b.side
        && a.line == b.line
        && a.start_line == b.start_line
        && a.start_side == b.start_side
}
fn same_observation(a: &ObservationV3, b: &ObservationV3) -> bool {
    match (a, b) {
        (
            ObservationV3::Pending {
                review: a,
                comment: x,
                inline: i,
            },
            ObservationV3::Pending {
                review: b,
                comment: y,
                inline: j,
            },
        ) => {
            a.snapshot().same_observation(b.snapshot())
                && match (x, y) {
                    (None, None) => true,
                    (Some(x), Some(y)) => x.same_observation(y),
                    _ => false,
                }
                && match (i, j) {
                    (None, None) => true,
                    (Some(i), Some(j)) => same_inline(i, j),
                    _ => false,
                }
        }
        (ObservationV3::Rerequest(a), ObservationV3::Rerequest(b)) => a.same_observation(b),
        _ => false,
    }
}
fn pending_mutation<'a>(
    action: ReviewOperationActionV3,
    observation: &'a ObservationV3,
    body: &'a str,
) -> Result<PendingCommentMutationV3<'a>, ScopedReviewFailureV2> {
    let ObservationV3::Pending {
        review,
        comment,
        inline,
    } = observation
    else {
        return Err(refused("Original pending-only observation unavailable"));
    };
    match action {
        ReviewOperationActionV3::AddPendingComment => Ok(PendingCommentMutationV3::Add {
            review,
            comment: inline
                .as_ref()
                .ok_or_else(|| refused("Actual canonical pending-add range unavailable"))?,
        }),
        ReviewOperationActionV3::EditPendingComment => Ok(PendingCommentMutationV3::Edit {
            comment: comment
                .as_ref()
                .ok_or_else(|| refused("Actual pending edit member unavailable"))?,
            body,
        }),
        ReviewOperationActionV3::DeletePendingComment => Ok(PendingCommentMutationV3::Delete {
            comment: comment
                .as_ref()
                .ok_or_else(|| refused("Actual pending delete member unavailable"))?,
        }),
        ReviewOperationActionV3::RerequestReview => Err(refused(
            "A reviewer request is not a pending-comment mutation",
        )),
    }
}
fn retirement(
    database: &Path,
    identity: &HostIdentity,
    parent: &crate::providers::gh::PrResourceParentV1,
    reference: &PrPublicationReference,
    action: ReviewOperationActionV3,
    epoch: i64,
    confirmed: bool,
) -> bool {
    if action == ReviewOperationActionV3::RerequestReview {
        // Closed structured impact only. Original source journal epoch is
        // actually advanced; no discussion/body/label-filter fence is written.
        crate::pr_projection::review_rerequest_impact_v3::structured_impact(
            database, identity, parent, reference, epoch, confirmed,
        )
        .is_ok()
    } else if confirmed {
        matches!(
            crate::pr_projection::invalidate_confirmed_discussions(
                database,
                identity,
                parent.summary(),
                reference
            ),
            Ok(Some(_))
        )
    } else {
        matches!(
            crate::pr_projection::invalidate_unresolved_discussions(
                database,
                identity,
                parent.summary(),
                reference
            ),
            Ok(Some(_))
        )
    }
}
impl ReviewOperationPreviewV3 {
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
    ) -> Result<ReviewOperationPublicationV3, ScopedReviewFailureV2> {
        let deadline = deadline.min(publication.deadline());
        clock(cancel, deadline)?;
        if !confirmed || restored.is_some() {
            return Err(refused(
                "Fresh explicit Confirm only; restored original history is never resend permission",
            ));
        }
        if self
            .started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(refused(
                "Exact operation Preview already attempted; inspect durable original outcome",
            ));
        }
        let request = self.request();
        let original = request.intent().original();
        let action = request.intent().action();
        let preview = self.source();
        if request.body() != preview.body() || request.draft_key() != preview.key() {
            return Err(refused("Original prepared body/key changed; nothing sent"));
        }
        images.validate().map_err(source_failure)?;
        reviews.validate().map_err(source_failure)?;
        resources.validate().map_err(source_failure)?;
        let write = preview.prepared_write()?;
        let digest = write
            .digest()
            .map_err(|_| refused("Actual original operation fingerprint unavailable"))?;
        draft_v3::preflight_publication_envelope(
            request.body(),
            original.parent().node_id(),
            original.parent().head_sha(),
            &digest,
        )
        .map_err(|_| {
            refused("Actual escaped operation and original reference exceed128KiB; nothing claimed")
        })?;
        clock(cancel, deadline)?;
        preview
            .local
            .validate(database, &original.source)
            .map_err(source_failure)?;
        let originals = original_images_v3(request.intent(), images, cancel, deadline)?;
        let receipt_slots = match action {
            ReviewOperationActionV3::RerequestReview => 2,
            ReviewOperationActionV3::DeletePendingComment => reviews
                .max_pages
                .checked_add(4)
                .ok_or_else(|| refused("Delete receipt count overflow"))?,
            _ => reviews
                .max_pages
                .checked_add(3)
                .ok_or_else(|| refused("Pending receipt count overflow"))?,
        };
        let slots = receipt_slots
            .checked_add(4)
            .filter(|slots| *slots <= 32)
            .ok_or_else(|| {
                refused("Original receipt plus visibility exceeds32 prepaid captures")
            })?;
        let count = originals
            .iter()
            .try_fold(
                reviews.max_pages.saturating_mul(2).saturating_add(18),
                |count, original| count.checked_add(original.max_external_requests()),
            )
            .ok_or_else(|| refused("Original operation measured request count overflow"))?;
        let issuer = provider.clone().with_catalogue_read_deadline(deadline);
        let mut session = issuer
            .begin_repository_catalogue_access(original.source.source(), access, cancel)
            .map_err(source_failure)?;
        let identity = session.identity().clone();
        original_scope(original, &identity, cancel, deadline)?;
        if identity.host != preview.identity().host
            || identity.account_id != preview.identity().account_id
        {
            return Err(source_failure(safe_local()));
        }
        let mut captures = session
            .reserve_postwrite_capture_v2(&vec![1024 * 1024; slots], 2, cancel)
            .map_err(source_failure)?;
        let window = session
            .reserve_measured_external_reads(count, 1024 * 1024, images.max_pool_bytes, 2, cancel)
            .map_err(source_failure)?;
        let measured = issuer
            .clone()
            .with_measured_read_window(window)
            .map_err(source_failure)?;
        session
            .verify(original.navigation.original(), cancel)
            .map_err(source_failure)?;
        let parent = measured
            .pr_inline_original_parent_v1(
                session
                    .observation_v1(original.parent().repository_id())
                    .ok_or_else(|| source_failure(safe_local()))?,
                &original.files.observed,
                resources,
                cancel,
            )
            .map_err(source_failure)?;
        preview
            .local
            .validate(database, &original.source)
            .map_err(source_failure)?;
        let values = operation_values_v3(request.intent(), Arc::from(request.body()))?;
        let inline = if values.comments().is_empty() {
            Vec::new()
        } else {
            preview::rehydrate_values_v3(
                original,
                &values,
                &originals,
                &measured,
                session
                    .observation_v1(parent.repository_id())
                    .ok_or_else(|| source_failure(safe_local()))?,
                &identity,
                &preview.local,
                database,
                images,
                cancel,
                deadline,
            )?
        };
        let fresh = read_observation(
            request.intent(),
            &measured,
            &identity,
            &parent,
            reviews,
            inline.into_iter().next(),
            cancel,
            deadline,
        )?;
        if !same_observation(&preview.observation, &fresh) {
            return Err(refused("Actual selected source/capability/body/range/membership changed after Preview; no write"));
        }
        let target = PrWriteTarget::new(&identity, parent.summary())
            .map_err(|_| source_failure(safe_local()))?;
        if target != preview.local.target {
            return Err(source_failure(safe_local()));
        }
        preview
            .local
            .validate(database, &original.source)
            .map_err(source_failure)?;
        let mut journal =
            PrWriteJournal::open(database).map_err(|_| source_failure(safe_local()))?;
        let seed_v12 = if let Some(revision) = &preview.lineage_v12 {
            let base =
                crate::pr_write_journal::review_action_lineage_v12::ReviewActionBaseV12::operation(
                    request.intent(),
                )
                .map_err(|_| source_failure(safe_local()))?;
            revision
                .validate(database, &base)
                .map_err(|_| source_failure(safe_local()))?;
            Some(crate::pr_write_journal::review_action_lineage_v12::ReviewActionOriginSeedV12::from_revision(revision,Some(preview.origin_seed()?),write).map_err(|_|source_failure(safe_local()))?)
        } else {
            None
        };
        let permit = if let Some(seed) = &seed_v12 {
            journal.begin_review_action_v12(&target, write, seed)
        } else {
            journal.begin_review_operation_v3(&target, write, preview.origin_seed()?)
        }
        .map_err(|_| source_failure(safe_local()))?;
        let marker = permit.marker();
        let reference = marker.publication_reference();
        let encoded = match draft_v3::encode(request.body(), Some(&reference)) {
            Ok(encoded) => encoded,
            Err(_) => {
                return Ok(pending(
                    reference,
                    ScopedReviewPublicationStateV2::NotStarted,
                ))
            }
        };
        let begun = preview
            .local
            .generation
            .checked_add(1)
            .ok_or_else(|| source_failure(safe_local()))?;
        let mut ack =
            match drafts.save_publication_and_wait_v2(publication, encoded.clone(), cancel) {
                Ok(ack) => Some(ack),
                Err(_) => {
                    let mut result = pending(reference, ScopedReviewPublicationStateV2::NotStarted);
                    result.draft_retained = false;
                    return Ok(result);
                }
            };
        if validate_owned(
            &preview.local,
            database,
            original,
            marker,
            begun,
            PrWriteState::Pending,
        )
        .is_err()
            || original_draft_saved(database, original, preview.key(), &encoded).is_err()
            || clock(cancel, deadline).is_err()
            || ack.as_ref().is_none_or(|ack| {
                ack.check_current(drafts, database, original.scope(), preview.key(), cancel)
                    .is_err()
            })
            || journal.claim(&permit, &target, write).is_err()
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
            ack.as_ref()
                .ok_or_else(safe_local)?
                .check_current(drafts, database, original.scope(), preview.key(), cancel)
                .map_err(|_| safe_local())?;
            measured.pr_inline_original_parent_v1(
                session
                    .observation_v1(parent.repository_id())
                    .ok_or_else(safe_local)?,
                &original.files.observed,
                resources,
                cancel,
            )?;
            session.verify(original.navigation.original(), cancel)?;
            let closed = session.close_with_guard(cancel, || {
                private_clock(cancel, deadline)?;
                ack.as_ref()
                    .ok_or_else(safe_local)?
                    .check_current(drafts, database, original.scope(), preview.key(), cancel)
                    .map_err(|_| safe_local())?;
                validate_owned(
                    &preview.local,
                    database,
                    original,
                    marker,
                    claimed,
                    PrWriteState::Pending,
                )?;
                original_draft_saved(database, original, preview.key(), &encoded)?;
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
            ack.as_ref()
                .ok_or_else(safe_local)?
                .check_current(drafts, database, original.scope(), preview.key(), cancel)
                .map_err(|_| safe_local())?;
            private_clock(cancel, deadline)
        };
        let receipt: Result<
            (PrConfirmedEffect, ReviewOperationSafeEffectV3),
            ReviewPublicationFailureV2,
        > = if action == ReviewOperationActionV3::RerequestReview {
            let (ObservationV3::Rerequest(expected), ObservationV3::Rerequest(fresh)) =
                (&preview.observation, &fresh)
            else {
                return Err(refused("Exact re-request observations missing"));
            };
            issuer
                .rerequest_review_guarded_v3(
                    &measured,
                    &mut captures,
                    &identity,
                    &parent,
                    expected,
                    fresh,
                    reviews,
                    cancel,
                    deadline,
                    before_write,
                )
                .map(|receipt| {
                    let target = receipt.target();
                    let kind = match target.kind() {
                        ReviewRequestActorKindV3::User => "User",
                        ReviewRequestActorKindV3::Bot => "Bot",
                        ReviewRequestActorKindV3::Team => "Team",
                    };
                    (
                        PrConfirmedEffect {
                            kind: PrEffectKind::Metadata,
                            resource_id: format!("reviewer-request/{kind}/{}", target.id()),
                            node_id: Some(target.node().into()),
                        },
                        ReviewOperationSafeEffectV3::ReviewerRequested {
                            kind: target.kind(),
                            id: target.id(),
                        },
                    )
                })
        } else {
            let expected = pending_mutation(action, &preview.observation, request.body())?;
            let fresh = pending_mutation(action, &fresh, request.body())?;
            issuer
                .mutate_pending_comment_guarded_v3(
                    &measured,
                    &mut captures,
                    &identity,
                    &parent,
                    expected,
                    fresh,
                    reviews,
                    cancel,
                    deadline,
                    before_write,
                )
                .map(|receipt| {
                    (
                        PrConfirmedEffect {
                            kind: if receipt.deleted() {
                                PrEffectKind::CommentDeleted
                            } else {
                                PrEffectKind::Comment
                            },
                            resource_id: receipt.comment_id().to_string(),
                            node_id: Some(receipt.comment_node().into()),
                        },
                        ReviewOperationSafeEffectV3::PendingComment {
                            id: receipt.comment_id(),
                            original_review: receipt.original_review_id(),
                            deleted: receipt.deleted(),
                            review_and_comment_absent: receipt.review_and_comment_absent(),
                        },
                    )
                })
        };
        let (effect, safe) = match receipt {
            Err(ReviewPublicationFailureV2::NotStarted(_)) => {
                return Ok(pending(
                    reference,
                    ScopedReviewPublicationStateV2::NotStarted,
                ))
            }
            Err(ReviewPublicationFailureV2::WriteMayHaveStarted(_)) => {
                let _ = journal.mark_uncertain(&permit);
                let epoch = journal.source_generation(&target).unwrap_or(-1);
                let retired = retirement(
                    database, &identity, &parent, &reference, action, epoch, false,
                );
                let mut result = pending(reference, ScopedReviewPublicationStateV2::Uncertain);
                result.source_retired = retired;
                return Ok(result);
            }
            Ok(receipt) => receipt,
        };
        if journal.mark_confirmed(&permit, &effect).is_err() {
            let epoch = journal.source_generation(&target).unwrap_or(-1);
            let retired = retirement(
                database, &identity, &parent, &reference, action, epoch, false,
            );
            return Ok(ReviewOperationPublicationV3 {
                state: ScopedReviewPublicationStateV2::ConfirmedButJournalFailed,
                reference,
                effect: Some(safe),
                source_retired: retired,
                draft_retained: true,
            });
        }
        let confirmed_epoch = claimed
            .checked_add(1)
            .ok_or_else(|| source_failure(safe_local()))?;
        let mut draft_retained = true;
        let visible = original_closed
            .as_ref()
            .and_then(|closed| closed.observation(parent.repository_id()))
            .is_some_and(|current| {
                issuer
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
                                &preview.local,
                                database,
                                original,
                                marker,
                                confirmed_epoch,
                                PrWriteState::Confirmed,
                            )?;
                            original_draft_saved(database, original, preview.key(), &encoded)?;
                            if let Some(seed) = &seed_v12 {
                                seed.validate_owned_basis(database, &target, marker)
                                    .map_err(|_| safe_local())?;
                            }
                            let acknowledged = ack.take().ok_or_else(safe_local)?;
                            acknowledged
                                .check_current(
                                    drafts,
                                    database,
                                    original.scope(),
                                    preview.key(),
                                    cancel,
                                )
                                .map_err(|_| safe_local())?;
                            drafts
                                .remove_publication_and_wait_v2(acknowledged, cancel)
                                .map_err(|_| safe_local())?;
                            draft_retained = false;
                            validate_owned(
                                &preview.local,
                                database,
                                original,
                                marker,
                                confirmed_epoch,
                                PrWriteState::Confirmed,
                            )?;
                            private_clock(cancel, deadline)
                        },
                    )
                    .is_ok()
            })
            && clock(cancel, deadline).is_ok();
        // Final intentional retirement only. No private SQL/read/cleanup or
        // receipt actor/body leaves after this effect; result is safe IDs only.
        let retired = retirement(
            database,
            &identity,
            &parent,
            &reference,
            action,
            confirmed_epoch,
            true,
        );
        let state = if visible && retired && clock(cancel, deadline).is_ok() {
            ScopedReviewPublicationStateV2::ConfirmedSnapshotRetired
        } else {
            ScopedReviewPublicationStateV2::ConfirmedButSourceUnavailable
        };
        Ok(ReviewOperationPublicationV3 {
            state,
            reference,
            effect: Some(safe),
            source_retired: retired,
            draft_retained,
        })
    }
}
