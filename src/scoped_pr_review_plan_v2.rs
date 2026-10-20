//! Original-source pending-review workers. No restored draft/reference/proof is
//! a publication permit; private results leave ONLY after actual source close.
use super::*;
use crate::{
    pr_write_journal::{PrPublicationReference, PrWriteJournal},
    providers::{
        gh::{
            GithubError, GithubErrorKind, GithubProvider, HostIdentity, PrResourceReadLimitsV1,
            RepositoryCatalogueAccessLimits, ReviewCatalogueV2, ReviewReadLimitsV2, ReviewRecordV2,
            ReviewSnapshotV2,
        },
        runner::CancellationToken,
    },
    scoped_pr_observed_tabs_v1::resources::{safe_local, ResourceLocalGuard},
    storage::Cache,
};
use std::{path::Path, time::Instant};
#[path = "scoped_pr_review_operations_v3.rs"]
pub mod operations_v3;
#[path = "scoped_pr_review_preview_v2.rs"]
mod preview;
pub use preview::{
    ReviewRequestedActionV2, ScopedReviewPreviewV2, ScopedReviewPublicationStateV2,
    ScopedReviewPublicationV2, ScopedReviewRequestV2,
};

pub struct ScopedReviewFailureV2 {
    pub kind: Option<GithubErrorKind>,
    pub hide_private: bool,
    message: &'static str,
}
impl std::fmt::Debug for ScopedReviewFailureV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopedReviewFailureV2")
            .field("kind", &self.kind)
            .field("hide_private", &self.hide_private)
            .finish()
    }
}
impl std::fmt::Display for ScopedReviewFailureV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}
impl std::error::Error for ScopedReviewFailureV2 {}
fn refused(message: &'static str) -> ScopedReviewFailureV2 {
    ScopedReviewFailureV2 {
        kind: None,
        hide_private: false,
        message,
    }
}
fn source_failure(error: GithubError) -> ScopedReviewFailureV2 {
    ScopedReviewFailureV2 {kind:Some(error.kind),hide_private:true,message:"Original review source/account/storage unavailable; private content hidden, no fallback"}
}
fn clock(cancel: &CancellationToken, deadline: Instant) -> Result<(), ScopedReviewFailureV2> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err(refused(
            "Original queued review phase ended; no new clock or private result",
        ));
    }
    Ok(())
}
fn private_clock(cancel: &CancellationToken, deadline: Instant) -> Result<(), GithubError> {
    clock(cancel, deadline).map_err(|_| GithubError {
        kind: GithubErrorKind::Transport,
        message: "Original review phase unavailable".into(),
    })
}
fn original_scope(
    intent: &ScopedReviewIntentV2,
    identity: &HostIdentity,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<(), ScopedReviewFailureV2> {
    clock(cancel, deadline)?;
    if Scope::github(&identity.host, identity.account_id)
        .ok()
        .as_ref()
        != Some(intent.scope())
    {
        return Err(source_failure(safe_local()));
    }
    if !intent
        .source
        .admits_review_navigation_v14(&intent.active, identity, &intent.navigation)
    {
        return Err(source_failure(safe_local()));
    }
    clock(cancel, deadline)?;
    Ok(())
}

/// Actual closed draft/catalogue. Getters carry data only; none can recreate a
/// source, approval, measured operation or publication token after restart.
pub struct ScopedReviewDraftV2 {
    intent: Arc<ScopedReviewIntentV2>,
    values: Arc<ReviewDraftValuesV2>,
    key: Arc<str>,
    identity: HostIdentity,
    publication: Option<PrPublicationReference>,
    catalogue: Arc<ReviewCatalogueV2>,
    restored_values: bool,
    pub(super) lineage_v12: Option<
        Arc<crate::pr_write_journal::review_action_lineage_v12::ReviewAcknowledgedRevisionV12>,
    >,
    pub(super) needs_new_v12: bool,
    pub(crate) new_notice_v12: Option<Arc<crate::prepared_form_text_v1::PreparedFormTextV1>>,
    /// Repository permission flags from this SAME source session's own
    /// repository observation; missing flags stay unknown.
    repository_permissions_v20: crate::providers::gh::RepositoryCataloguePermissions,
}
impl std::fmt::Debug for ScopedReviewDraftV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedReviewDraftV2(<actual closed private text; no approval>)")
    }
}
impl ScopedReviewDraftV2 {
    pub fn intent(&self) -> &Arc<ScopedReviewIntentV2> {
        &self.intent
    }
    pub fn values(&self) -> &Arc<ReviewDraftValuesV2> {
        &self.values
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn publication(&self) -> Option<&PrPublicationReference> {
        self.publication.as_ref()
    }
    pub fn catalogue(&self) -> &Arc<ReviewCatalogueV2> {
        &self.catalogue
    }
    pub fn restored_values(&self) -> bool {
        self.restored_values
    }
    pub fn repository_permissions_v20(
        &self,
    ) -> &crate::providers::gh::RepositoryCataloguePermissions {
        &self.repository_permissions_v20
    }
    pub(crate) fn new_action_revision_v12(
        &self,
    ) -> Option<
        &Arc<crate::pr_write_journal::review_action_lineage_v12::ReviewAcknowledgedRevisionV12>,
    > {
        self.needs_new_v12
            .then_some(self.lineage_v12.as_ref())
            .flatten()
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open_new_v12(
        intent: Arc<ScopedReviewIntentV2>,
        selection: Option<&Arc<ScopedReviewSelectionV2>>,
        action: ReviewRequestedActionV2,
        revision: &Arc<
            crate::pr_write_journal::review_action_lineage_v12::ReviewAcknowledgedRevisionV12,
        >,
        provider: &GithubProvider,
        database: &Path,
        reviews: &ReviewReadLimitsV2,
        resources: &PrResourceReadLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, ScopedReviewFailureV2> {
        let result = Self::open_key(
            intent,
            selection.map(|s| (s, action)),
            Some(revision),
            provider,
            database,
            reviews,
            resources,
            access,
            cancel,
            deadline,
        )?;
        if result.needs_new_v12 || result.lineage_v12.is_none() {
            return Err(refused("Original explicit new-action revision unavailable"));
        }
        if let Some(selection) = selection {
            if !result.catalogue().rows().iter().any(|row| {
                row.id() == selection.snapshot().review().id()
                    && row.node() == selection.snapshot().review().node()
            }) || !result.values().comments().is_empty()
            {
                return Err(refused(
                    "Actual new-action selected review changed; nothing reopened",
                ));
            }
        }
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn open(
        intent: Arc<ScopedReviewIntentV2>,
        provider: &GithubProvider,
        database: &Path,
        reviews: &ReviewReadLimitsV2,
        resources: &PrResourceReadLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, ScopedReviewFailureV2> {
        Self::open_key(
            intent, None, None, provider, database, reviews, resources, access, cancel, deadline,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn open_selected(
        selection: &Arc<ScopedReviewSelectionV2>,
        action: ReviewRequestedActionV2,
        provider: &GithubProvider,
        database: &Path,
        reviews: &ReviewReadLimitsV2,
        resources: &PrResourceReadLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, ScopedReviewFailureV2> {
        let result = Self::open_key(
            selection.intent.clone(),
            Some((selection, action)),
            None,
            provider,
            database,
            reviews,
            resources,
            access,
            cancel,
            deadline,
        )?;
        if !result.values.comments().is_empty() {
            return Err(refused("Selected-action draft contains batch comments; no partial restore or batch overwrite"));
        }
        if !result.catalogue.rows().iter().any(|review| {
            review.id() == selection.snapshot.review().id()
                && review.node() == selection.snapshot.review().node()
        }) {
            return Err(refused("Original selected review no longer occurs in current observed catalogue; refresh explicitly"));
        }
        clock(cancel, deadline)?;
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)]
    fn open_key(
        intent: Arc<ScopedReviewIntentV2>,
        selected: Option<(&Arc<ScopedReviewSelectionV2>, ReviewRequestedActionV2)>,
        explicit_v12: Option<
            &Arc<crate::pr_write_journal::review_action_lineage_v12::ReviewAcknowledgedRevisionV12>,
        >,
        provider: &GithubProvider,
        database: &Path,
        reviews: &ReviewReadLimitsV2,
        resources: &PrResourceReadLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, ScopedReviewFailureV2> {
        clock(cancel, deadline)?;
        reviews.validate().map_err(source_failure)?;
        resources.validate().map_err(source_failure)?;
        let provider = provider.clone().with_catalogue_read_deadline(deadline);
        let mut session = intent
            .source
            .begin_access(&provider, access, cancel)
            .map_err(source_failure)?;
        let identity = session.identity().clone();
        original_scope(&intent, &identity, cancel, deadline)?;
        let window = session
            .reserve_measured_external_reads(
                reviews.max_pages + 2,
                reviews.max_document_bytes.max(resources.max_document_bytes),
                resources.max_pool_bytes,
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
        let observation = session
            .observation_v1(intent.parent().repository_id())
            .ok_or_else(|| source_failure(safe_local()))?;
        let repository_permissions_v20 = observation.repository().permissions.clone();
        let parent = measured
            .pr_inline_original_parent_v1(observation, &intent.files.observed, resources, cancel)
            .map_err(source_failure)?;
        let local = ResourceLocalGuard::capture_boundary_v13(
            database,
            intent.source.local_boundary(),
            &identity,
            parent.summary(),
        )
        .map_err(source_failure)?;
        let base = crate::pr_write_journal::review_action_lineage_v12::ReviewActionBaseV12::review(
            &intent,
            selected.map(|(selection, _)| selection),
            selected.map_or(ReviewRequestedActionV2::CreatePending, |(_, action)| action),
        )
        .map_err(|_| source_failure(safe_local()))?;
        let choice = super::action_lineage_v12::choose(database, &base, explicit_v12)
            .map_err(|_| source_failure(safe_local()))?;
        let key = choice.key.clone();
        let encoded = if choice.needs_new {
            None
        } else {
            Cache::open(database)
                .map_err(|_| source_failure(safe_local()))?
                .bounded_draft(intent.scope(), &key, REVIEW_DRAFT_BYTES_V2)
                .map_err(|_| source_failure(safe_local()))?
        };
        let restored_values = encoded.is_some();
        let mut values = ReviewDraftValuesV2::empty();
        let mut publication = None;
        if let Some(encoded) = encoded {
            let raw: serde_json::Value = serde_json::from_str(&encoded)
                .map_err(|_| refused("Malformed review draft; no proof/approval restored"))?;
            let object = raw
                .as_object()
                .filter(|object| {
                    object.len() == 3
                        && object.contains_key("version")
                        && object.contains_key("values")
                        && object.contains_key("publication")
                })
                .ok_or_else(|| {
                    refused("Review draft contains unknown or missing fields; no partial restore")
                })?;
            if object["version"].as_u64() != Some(3) {
                return Err(refused(
                    "Unsupported saved review draft version; no approval restored",
                ));
            }
            values = ReviewDraftValuesV2::decode_values(&object["values"].to_string()).map_err(
                |_| refused("Saved review values exceed their bounded text-only recovery contract"),
            )?;
            if !object["publication"].is_null() {
                let reference = object["publication"]
                    .as_object()
                    .filter(|reference| {
                        reference.len() == 4
                            && reference
                                .get("attempt_id")
                                .and_then(serde_json::Value::as_i64)
                                .is_some_and(|id| id > 0)
                    })
                    .ok_or_else(|| refused("Malformed original review publication metadata"))?;
                for (field, sizes) in [
                    ("pr_node_id", &[1024usize][..]),
                    ("observed_head", &[40usize, 64][..]),
                    ("payload_sha256", &[64usize][..]),
                ] {
                    reference
                        .get(field)
                        .and_then(serde_json::Value::as_str)
                        .filter(|value| {
                            if field == "pr_node_id" {
                                !value.is_empty()
                                    && value.len() <= sizes[0]
                                    && !value.chars().any(char::is_control)
                            } else {
                                sizes.contains(&value.len())
                                    && value.bytes().all(|byte| byte.is_ascii_hexdigit())
                            }
                        })
                        .ok_or_else(|| {
                            refused(
                                "Original review publication metadata exceeds bound before clone",
                            )
                        })?;
                }
            }
            publication = serde_json::from_value(object["publication"].clone())
                .map_err(|_| refused("Malformed original review publication reference"))?;
            if let Some(reference) = &publication {
                let marker = PrWriteJournal::open(database)
                    .map_err(|_| source_failure(safe_local()))?
                    .original_publication(&local.target, reference)
                    .map_err(|_| source_failure(safe_local()))?
                    .ok_or_else(|| source_failure(safe_local()))?;
                if !matches!(
                    marker.command.as_str(),
                    "review-create-pending-v2"
                        | "review-submit-v2"
                        | "review-discard-v2"
                        | "review-update-pending-v2"
                        | "review-dismiss-v2"
                ) || marker.state != crate::pr_write_journal::PrWriteState::Confirmed
                {
                    return Err(source_failure(safe_local()));
                }
            }
        }
        clock(cancel, deadline)?;
        let catalogue = measured
            .review_catalogue_v2(&identity, &parent, reviews, cancel, deadline)
            .map_err(source_failure)?;
        let new_notice_v12 = if choice.needs_new {
            Some(
                super::action_lineage_v12::prepare_notice(
                    &intent,
                    selected
                        .map_or(ReviewRequestedActionV2::CreatePending, |(_, action)| action)
                        .label(),
                    &identity,
                    choice
                        .revision
                        .as_ref()
                        .ok_or_else(|| source_failure(safe_local()))?,
                    cancel,
                    deadline,
                )
                .map_err(|_| source_failure(safe_local()))?,
            )
        } else {
            None
        };
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
                local.validate_boundary_v13(database, intent.source.local_boundary())?;
                if let Some(revision) = &choice.revision {
                    revision
                        .validate(database, &base)
                        .map_err(|_| safe_local())?;
                }
                private_clock(cancel, deadline)
            })
            .map_err(source_failure)?;
        clock(cancel, deadline)?;
        let result = Arc::new(Self {
            intent,
            values: Arc::new(values),
            key,
            identity: closed.identity().clone(),
            publication,
            catalogue: Arc::new(catalogue),
            restored_values,
            lineage_v12: choice.revision,
            needs_new_v12: choice.needs_new,
            new_notice_v12,
            repository_permissions_v20,
        });
        clock(cancel, deadline)?;
        Ok(result)
    }
}

/// Genuine selected server review plus all observed comments after its OWN
/// source close. No Deserialize constructor or pending-review login inference.
pub struct ScopedReviewSelectionV2 {
    intent: Arc<ScopedReviewIntentV2>,
    snapshot: Arc<ReviewSnapshotV2>,
    identity: HostIdentity,
    draft_prefix: Arc<str>,
}
impl std::fmt::Debug for ScopedReviewSelectionV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedReviewSelectionV2(<actual closed review; no mutation permit>)")
    }
}
impl ScopedReviewSelectionV2 {
    pub fn intent(&self) -> &Arc<ScopedReviewIntentV2> {
        &self.intent
    }
    pub fn snapshot(&self) -> &Arc<ReviewSnapshotV2> {
        &self.snapshot
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn action_key(&self, action: ReviewRequestedActionV2) -> Result<String, String> {
        if action == ReviewRequestedActionV2::CreatePending {
            return Err("Actual server review cannot create a batch action key".into());
        }
        Ok(format!(
            "{}/review/{}/{}",
            self.draft_prefix,
            self.snapshot.review().id(),
            action.draft_tag()
        ))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn read(
        draft: &Arc<ScopedReviewDraftV2>,
        row: usize,
        provider: &GithubProvider,
        database: &Path,
        reviews: &ReviewReadLimitsV2,
        resources: &PrResourceReadLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, ScopedReviewFailureV2> {
        clock(cancel, deadline)?;
        reviews.validate().map_err(source_failure)?;
        resources.validate().map_err(source_failure)?;
        let selected: &Arc<ReviewRecordV2> = draft
            .catalogue
            .rows()
            .get(row)
            .ok_or_else(|| refused("Select an actual loaded review, not a restored ID"))?;
        let intent = &draft.intent;
        let provider = provider.clone().with_catalogue_read_deadline(deadline);
        let mut session = intent
            .source
            .begin_access(&provider, access, cancel)
            .map_err(source_failure)?;
        let identity = session.identity().clone();
        original_scope(intent, &identity, cancel, deadline)?;
        let window = session
            .reserve_measured_external_reads(
                reviews.max_pages + 4,
                reviews.max_document_bytes.max(resources.max_document_bytes),
                resources.max_pool_bytes,
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
        let local = ResourceLocalGuard::capture_boundary_v13(
            database,
            intent.source.local_boundary(),
            &identity,
            parent.summary(),
        )
        .map_err(source_failure)?;
        let snapshot = measured
            .review_snapshot_v2(&identity, &parent, selected, reviews, cancel, deadline)
            .map_err(source_failure)?;
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
                local.validate_boundary_v13(database, intent.source.local_boundary())?;
                private_clock(cancel, deadline)
            })
            .map_err(source_failure)?;
        let result = Arc::new(Self {
            intent: intent.clone(),
            snapshot: Arc::new(snapshot),
            identity: closed.identity().clone(),
            draft_prefix: Arc::from(
                intent
                    .draft_key()
                    .map_err(|_| refused("Original review base key unavailable"))?,
            ),
        });
        clock(cancel, deadline)?;
        Ok(result)
    }
}
