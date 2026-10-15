//! Genuine pending-comment/re-request selection and fresh source preparation.
//! Old closed records are selection INTENT only. Every phase reissues the
//! selected observation through ONE original source session/pool/clock.
use super::*;
use crate::providers::gh::{
    InlineComment, ObservedReviewStateV2, PendingCommentOriginalV3, PendingReviewOriginalV3,
    PrImmutableFileLimitsV1, RerequestOriginalV3, ReviewRequestTeamsV3,
};
use sha2::{Digest, Sha256};
#[path = "scoped_pr_review_operation_draft_v3.rs"]
pub mod draft_v3;
#[path = "scoped_pr_review_operation_execute_v3.rs"]
pub mod execute_v3;
#[path = "scoped_pr_review_operation_history_v3.rs"]
pub mod history_v3;
#[path = "scoped_pr_review_operation_metadata_v3.rs"]
pub(crate) mod metadata_v3;
#[path = "scoped_pr_review_operation_preview_v3.rs"]
pub mod preview_v3;
#[path = "scoped_pr_review_operation_teams_v3.rs"]
pub mod teams_v3;
#[path = "scoped_pr_review_operation_ui_v3.rs"]
pub mod ui_v3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewOperationActionV3 {
    AddPendingComment,
    EditPendingComment,
    DeletePendingComment,
    RerequestReview,
}
impl ReviewOperationActionV3 {
    pub fn label(self) -> &'static str {
        match self {
            Self::AddPendingComment => "Add comment to exact server PENDING review — not submitted",
            Self::EditPendingComment => "Edit exact server PENDING comment — not submitted",
            Self::DeletePendingComment => "Delete exact server PENDING comment",
            Self::RerequestReview => "Re-request actual reviewer — eligibility UNKNOWN",
        }
    }
    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::AddPendingComment => "review-pending-comment-add-v3",
            Self::EditPendingComment => "review-pending-comment-edit-v3",
            Self::DeletePendingComment => "review-pending-comment-delete-v3",
            Self::RerequestReview => "review-rerequest-v3",
        }
    }
    pub fn needs_body(self) -> bool {
        matches!(self, Self::AddPendingComment | Self::EditPendingComment)
    }
}
enum ChoiceV3 {
    Add {
        selection: Arc<ScopedReviewSelectionV2>,
        anchor: Arc<ReviewAnchorMetadataV2>,
        original_anchor: Arc<ScopedReviewAnchorIntentV2>,
    },
    Edit {
        selection: Arc<ScopedReviewSelectionV2>,
        index: usize,
    },
    Delete {
        selection: Arc<ScopedReviewSelectionV2>,
        index: usize,
    },
    Reviewer(Arc<ScopedReviewSelectionV2>),
    Team {
        catalogue: Arc<ReviewRequestTeamsV3>,
        index: usize,
    },
}
pub struct ScopedReviewOperationIntentV3 {
    original: Arc<ScopedReviewIntentV2>,
    choice: ChoiceV3,
}
impl std::fmt::Debug for ScopedReviewOperationIntentV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "ScopedReviewOperationIntentV3(<actual chosen source/member; no write permission>)",
        )
    }
}
impl ScopedReviewOperationIntentV3 {
    fn own(selection: &ScopedReviewSelectionV2) -> Result<(), String> {
        let snapshot = selection.snapshot();
        if !snapshot.complete()
            || !snapshot.review().is_observed_own_pending()
            || snapshot.review().author_account_id() != selection.identity().account_id
        {
            return Err(
                "Choose a complete actual server PENDING review owned by this numeric account"
                    .into(),
            );
        }
        Ok(())
    }
    /// WORKER-ONLY scalar metadata preparation from a genuine chosen Images
    /// range. Native/decoded ranges cannot invoke this constructor.
    pub fn add(
        selection: Arc<ScopedReviewSelectionV2>,
        anchor: &Arc<ScopedReviewAnchorIntentV2>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, String> {
        Self::own(&selection)?;
        let metadata = anchor.prepare_metadata(selection.intent(), cancel, deadline)?;
        if metadata.last() > i32::MAX as u32
            || metadata.first() > i32::MAX as u32
            || selection.snapshot().review().commit() != selection.intent().parent().head_sha()
        {
            return Err(
                "Pending add requires current H and actual signed-32-bit canonical coordinates"
                    .into(),
            );
        }
        Ok(Arc::new(Self {
            original: selection.intent.clone(),
            choice: ChoiceV3::Add {
                selection,
                anchor: Arc::new(metadata),
                original_anchor: anchor.clone(),
            },
        }))
    }
    /// O(1) selected actual member; capabilities are freshly read in worker.
    pub fn edit(
        selection: Arc<ScopedReviewSelectionV2>,
        index: usize,
        delete: bool,
    ) -> Result<Arc<Self>, String> {
        Self::own(&selection)?;
        if selection
            .snapshot()
            .comments()
            .get(index)
            .is_none_or(|comment| comment.author_account_id() != selection.identity().account_id)
        {
            return Err("Choose an actual loaded PENDING comment, not a restored ID".into());
        }
        let original = selection.intent.clone();
        Ok(Arc::new(Self {
            original,
            choice: if delete {
                ChoiceV3::Delete { selection, index }
            } else {
                ChoiceV3::Edit { selection, index }
            },
        }))
    }
    pub fn reviewer(selection: Arc<ScopedReviewSelectionV2>) -> Result<Arc<Self>, String> {
        let review = selection.snapshot().review();
        if !selection.snapshot().complete()
            || !review.has_observed_submission_v2()
            || review.author_account_id().is_none()
            || !matches!(
                review.state(),
                ObservedReviewStateV2::Approved
                    | ObservedReviewStateV2::ChangesRequested
                    | ObservedReviewStateV2::Commented
                    | ObservedReviewStateV2::Dismissed
            )
        {
            return Err("Choose an actual submitted review with observed numeric reviewer; unknown remains readable".into());
        }
        Ok(Arc::new(Self {
            original: selection.intent.clone(),
            choice: ChoiceV3::Reviewer(selection),
        }))
    }
    pub fn team(
        original: Arc<ScopedReviewIntentV2>,
        catalogue: Arc<ReviewRequestTeamsV3>,
        index: usize,
    ) -> Result<Arc<Self>, String> {
        let actual = catalogue
            .teams()
            .get(index)
            .ok_or("Choose an actual repository-team row")?;
        let parent = original.parent();
        if !catalogue.complete()
            || catalogue.parent().repository_id() != parent.repository_id()
            || catalogue.parent().pr_id() != parent.database_id()
            || catalogue.parent().pr_node() != parent.node_id()
            || catalogue.parent().head() != parent.head_sha()
            || catalogue.parent().base() != parent.base_tip_sha()
            || catalogue.parent().host() != original.scope().host
            || Scope::github(
                catalogue.parent().host(),
                Some(catalogue.parent().observed_by()),
            )
            .ok()
            .as_ref()
                != Some(original.scope())
            || actual.node().is_empty()
        {
            return Err(
                "Repository team selection belongs to another/incomplete original numeric source"
                    .into(),
            );
        }
        Ok(Arc::new(Self {
            original,
            choice: ChoiceV3::Team { catalogue, index },
        }))
    }
    pub fn original(&self) -> &Arc<ScopedReviewIntentV2> {
        &self.original
    }
    pub(crate) fn matches_add_work_v3(
        &self,
        selection: &Arc<ScopedReviewSelectionV2>,
        anchor: &Arc<ScopedReviewAnchorIntentV2>,
    ) -> bool {
        matches!(&self.choice,ChoiceV3::Add{selection:actual,original_anchor,..}
            if Arc::ptr_eq(actual,selection)&&Arc::ptr_eq(original_anchor,anchor))
    }
    pub(crate) fn matches_browser_v3(
        &self,
        view: &super::super::browser_v2::ReviewBrowserV2,
    ) -> bool {
        if !Arc::ptr_eq(self.original(), view.intent()) {
            return false;
        }
        match &self.choice {
            ChoiceV3::Edit { selection, index } | ChoiceV3::Delete { selection, index } => {
                view.selection
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, selection))
                    && view.pending_comment_v3 == *index
            }
            ChoiceV3::Reviewer(selection) => view
                .selection
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, selection)),
            ChoiceV3::Team { catalogue, index } => {
                view.teams_v3
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, catalogue))
                    && view.selected_team_v3 == *index
            }
            ChoiceV3::Add { .. } => false, // actual anchored Work has its OWN exact selection/anchor ticket gate
        }
    }
    pub fn action(&self) -> ReviewOperationActionV3 {
        match self.choice {
            ChoiceV3::Add { .. } => ReviewOperationActionV3::AddPendingComment,
            ChoiceV3::Edit { .. } => ReviewOperationActionV3::EditPendingComment,
            ChoiceV3::Delete { .. } => ReviewOperationActionV3::DeletePendingComment,
            ChoiceV3::Reviewer(_) | ChoiceV3::Team { .. } => {
                ReviewOperationActionV3::RerequestReview
            }
        }
    }
    /// WORKER ONLY; bounded original target digest, independent of edited body.
    pub fn draft_key(&self) -> Result<String, String> {
        let parent = self.original.parent();
        let scope = self.original.scope();
        let target = match &self.choice {
            ChoiceV3::Add {
                selection, anchor, ..
            } => serde_json::to_vec(&(
                selection.snapshot().review().id(),
                selection.snapshot().review().node(),
                Some(anchor.as_ref()),
            )),
            ChoiceV3::Edit { selection, index } | ChoiceV3::Delete { selection, index } => {
                let comment = selection
                    .snapshot()
                    .comments()
                    .get(*index)
                    .ok_or("Original pending member unavailable")?;
                serde_json::to_vec(&(
                    selection.snapshot().review().id(),
                    selection.snapshot().review().node(),
                    comment.id(),
                    comment.node(),
                ))
            }
            ChoiceV3::Reviewer(selection) => serde_json::to_vec(&(
                selection.snapshot().review().id(),
                selection.snapshot().review().node(),
                selection.snapshot().review().author_account_id(),
            )),
            ChoiceV3::Team { catalogue, index } => {
                let team = catalogue
                    .teams()
                    .get(*index)
                    .ok_or("Original team member unavailable")?;
                serde_json::to_vec(&(team.kind(), team.id(), team.node()))
            }
        }
        .map_err(|_| "Original review operation target cannot be encoded")?;
        if target.len() > 12 * 1024 {
            return Err(
                "Original review operation metadata exceeds bounded draft-key contract".into(),
            );
        }
        let tuple = serde_json::to_vec(&(
            scope,
            parent.repository_id(),
            parent.repository_node(),
            parent.database_id(),
            parent.node_id(),
            parent.number(),
            parent.head_sha(),
            parent.base_tip_sha(),
            self.action().command(),
            target,
        ))
        .map_err(|_| "Original review operation key unavailable")?;
        if tuple.len() > 16 * 1024 {
            return Err("Original review operation key exceeds16KiB".into());
        }
        Ok(format!("review-operation-v3/{:x}", Sha256::digest(tuple)))
    }
}

enum ObservationV3 {
    Pending {
        review: Arc<PendingReviewOriginalV3>,
        comment: Option<Arc<PendingCommentOriginalV3>>,
        inline: Option<InlineComment>,
    },
    Rerequest(Arc<RerequestOriginalV3>),
}
/// Closed worker result for rendering/Preview only. Its measured pool has
/// already sealed; publication must perform an entirely fresh original phase.
pub struct ScopedReviewOperationSourceV3 {
    intent: Arc<ScopedReviewOperationIntentV3>,
    identity: HostIdentity,
    observation: ObservationV3,
    key: Arc<str>,
    body: Arc<str>,
    lines: Arc<[String]>,
    local: ResourceLocalGuard,
    restored: bool,
    origin:
        Option<crate::pr_write_journal::review_operation_history_v3::ReviewOperationOriginSeedV3>,
    write: Option<crate::pr_write_journal::PrWriteIntent>,
    actors: Arc<[super::super::avatars_v7::PreparedActorV7]>,
    requester: Option<super::super::avatars_v7::PreparedActorV7>,
    display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    pub(super) lineage_v12: Option<
        Arc<crate::pr_write_journal::review_action_lineage_v12::ReviewAcknowledgedRevisionV12>,
    >,
    pub(super) needs_new_v12: bool,
    pub(crate) new_notice_v12: Option<Arc<crate::prepared_form_text_v1::PreparedFormTextV1>>,
}
impl std::fmt::Debug for ScopedReviewOperationSourceV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedReviewOperationSourceV3(<closed original observation; no approval>)")
    }
}
impl ScopedReviewOperationSourceV3 {
    pub fn intent(&self) -> &Arc<ScopedReviewOperationIntentV3> {
        &self.intent
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn restored_body(&self) -> bool {
        self.restored
    }
    pub(crate) fn new_action_revision_v12(
        &self,
    ) -> Option<
        &Arc<crate::pr_write_journal::review_action_lineage_v12::ReviewAcknowledgedRevisionV12>,
    > {
        if self.needs_new_v12 {
            self.lineage_v12.as_ref()
        } else {
            None
        }
    }
    pub fn lines(&self) -> &Arc<[String]> {
        &self.lines
    }
    pub fn pending_review(&self) -> Option<&Arc<PendingReviewOriginalV3>> {
        match &self.observation {
            ObservationV3::Pending { review, .. } => Some(review),
            _ => None,
        }
    }
    pub fn pending_comment(&self) -> Option<&Arc<PendingCommentOriginalV3>> {
        match &self.observation {
            ObservationV3::Pending { comment, .. } => comment.as_ref(),
            _ => None,
        }
    }
    pub fn pending_inline(&self) -> Option<&InlineComment> {
        match &self.observation {
            ObservationV3::Pending { inline, .. } => inline.as_ref(),
            _ => None,
        }
    }
    pub fn rerequest(&self) -> Option<&Arc<RerequestOriginalV3>> {
        match &self.observation {
            ObservationV3::Rerequest(source) => Some(source),
            _ => None,
        }
    }
    pub(crate) fn origin_seed(
        &self,
    ) -> Result<
        &crate::pr_write_journal::review_operation_history_v3::ReviewOperationOriginSeedV3,
        ScopedReviewFailureV2,
    > {
        self.origin
            .as_ref()
            .ok_or_else(|| refused("Original producer metadata unavailable; no journal begin"))
    }
    pub(crate) fn prepared_write(
        &self,
    ) -> Result<&crate::pr_write_journal::PrWriteIntent, ScopedReviewFailureV2> {
        self.write
            .as_ref()
            .ok_or_else(|| refused("Actual original operation fingerprint unavailable"))
    }
    pub(crate) fn actors(&self) -> &[super::super::avatars_v7::PreparedActorV7] {
        &self.actors
    }
    pub(crate) fn requester(&self) -> Result<&super::super::avatars_v7::PreparedActorV7, String> {
        self.requester
            .as_ref()
            .ok_or_else(|| "Actual closing requester unavailable".into())
    }
    pub(crate) fn display(&self) -> &Arc<crate::prepared_form_text_v1::PreparedFormTextV1> {
        &self.display
    }
    /// Local diagnostic validation only; not source/principal closure and not
    /// publication permission. Real Confirm must reissue the entire phase.
    pub fn validate_local(&self, database: &Path) -> Result<(), ScopedReviewFailureV2> {
        self.local
            .validate(database, &self.intent.original.source)
            .map_err(source_failure)
    }
    pub fn disabled_reason(&self) -> Option<&'static str> {
        match &self.observation {
        ObservationV3::Pending{inline:None,..} if self.intent.action()==ReviewOperationActionV3::AddPendingComment=>Some("Write the exact pending comment, then freshly Preview its canonical range"),
        ObservationV3::Pending{comment:Some(comment),..}=>match self.intent.action() {
            ReviewOperationActionV3::EditPendingComment if comment.viewer_can_update()!=Some(true)=>Some("Actual comment update capability unavailable; observed UNKNOWN/denied stays disabled"),
            ReviewOperationActionV3::DeletePendingComment if comment.viewer_can_delete()!=Some(true)=>Some("Actual comment delete capability unavailable; observed UNKNOWN/denied stays disabled"),
            _=>None,
        },
        ObservationV3::Rerequest(source) if source.already_requested()=>Some("Actual reviewer is already requested; no blind remove/re-add"),
        _=>None,
    }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn read(
        intent: Arc<ScopedReviewOperationIntentV3>,
        body: Arc<str>,
        provider: &GithubProvider,
        database: &Path,
        images: &PrImmutableFileLimitsV1,
        reviews: &ReviewReadLimitsV2,
        resources: &PrResourceReadLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, ScopedReviewFailureV2> {
        Self::read_original(
            intent,
            Some(body),
            None,
            provider,
            database,
            images,
            reviews,
            resources,
            access,
            cancel,
            deadline,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        intent: Arc<ScopedReviewOperationIntentV3>,
        provider: &GithubProvider,
        database: &Path,
        images: &PrImmutableFileLimitsV1,
        reviews: &ReviewReadLimitsV2,
        resources: &PrResourceReadLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, ScopedReviewFailureV2> {
        Self::read_original(
            intent, None, None, provider, database, images, reviews, resources, access, cancel,
            deadline,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn read_lineage_v12(
        intent: Arc<ScopedReviewOperationIntentV3>,
        body: Option<Arc<str>>,
        revision: &Arc<
            crate::pr_write_journal::review_action_lineage_v12::ReviewAcknowledgedRevisionV12,
        >,
        provider: &GithubProvider,
        database: &Path,
        images: &PrImmutableFileLimitsV1,
        reviews: &ReviewReadLimitsV2,
        resources: &PrResourceReadLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, ScopedReviewFailureV2> {
        Self::read_original(
            intent,
            body,
            Some(revision),
            provider,
            database,
            images,
            reviews,
            resources,
            access,
            cancel,
            deadline,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn read_original(
        intent: Arc<ScopedReviewOperationIntentV3>,
        supplied: Option<Arc<str>>,
        explicit_v12: Option<
            &Arc<crate::pr_write_journal::review_action_lineage_v12::ReviewAcknowledgedRevisionV12>,
        >,
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
        images.validate().map_err(source_failure)?;
        reviews.validate().map_err(source_failure)?;
        resources.validate().map_err(source_failure)?;
        if supplied.as_ref().is_some_and(|body| {
            body.len() > 65_536 || (!intent.action().needs_body() && !body.is_empty())
        }) {
            return Err(refused(
                "Review operation body must match its exact bounded requested effect",
            ));
        }
        // A body-independent key never restores an old source/approval.
        let key: Arc<str> = Arc::from(
            intent
                .draft_key()
                .map_err(|_| refused("Original operation key unavailable"))?,
        );
        let originals = original_images_v3(&intent, images, cancel, deadline)?;
        let count = originals
            .iter()
            .try_fold(
                reviews.max_pages.saturating_mul(2).saturating_add(9),
                |count, original| count.checked_add(original.max_external_requests()),
            )
            .ok_or_else(|| refused("Original operation read count exceeds bounds"))?;
        let issuer = provider.clone().with_catalogue_read_deadline(deadline);
        let mut session = issuer
            .begin_repository_catalogue_access(intent.original.source.source(), access, cancel)
            .map_err(source_failure)?;
        let identity = session.identity().clone();
        original_scope(&intent.original, &identity, cancel, deadline)?;
        let window = session
            .reserve_measured_external_reads(count, 1024 * 1024, images.max_pool_bytes, 2, cancel)
            .map_err(source_failure)?;
        let measured = issuer
            .clone()
            .with_measured_read_window(window)
            .map_err(source_failure)?;
        session
            .verify(intent.original.navigation.original(), cancel)
            .map_err(source_failure)?;
        let current = session
            .observation_v1(intent.original.parent().repository_id())
            .ok_or_else(|| source_failure(safe_local()))?;
        let parent = measured
            .pr_inline_original_parent_v1(
                current,
                &intent.original.files.observed,
                resources,
                cancel,
            )
            .map_err(source_failure)?;
        let local = ResourceLocalGuard::capture(
            database,
            &intent.original.source,
            &identity,
            parent.summary(),
        )
        .map_err(source_failure)?;
        let base_v12 =
            crate::pr_write_journal::review_action_lineage_v12::ReviewActionBaseV12::operation(
                &intent,
            )
            .map_err(|_| source_failure(safe_local()))?;
        let choice_v12 =
            super::super::action_lineage_v12::choose(database, &base_v12, explicit_v12)
                .map_err(|_| source_failure(safe_local()))?;
        if supplied.is_some() && choice_v12.needs_new {
            return Err(refused(
                "Protected action requires explicit New action before Preview",
            ));
        }
        let (body, restored) = if choice_v12.needs_new {
            (Arc::from(""), false)
        } else if let Some(body) = supplied {
            (body, false)
        } else {
            let saved = Cache::open(database)
                .map_err(|_| source_failure(safe_local()))?
                .bounded_draft(
                    intent.original.scope(),
                    &choice_v12.key,
                    REVIEW_DRAFT_BYTES_V2,
                )
                .map_err(|_| source_failure(safe_local()))?;
            match saved {
                Some(encoded) => {
                    let (body, reference) = draft_v3::decode(&encoded).map_err(|_| {
                        refused(
                            "Original operation draft malformed; no text/proof partially restored",
                        )
                    })?;
                    if reference.is_some() {
                        return Err(refused("Protected original operation history requires explicit Summary recovery inspection; never restore approval"));
                    }
                    (Arc::from(body), true)
                }
                None => match &intent.choice {
                    ChoiceV3::Edit { selection, index } => {
                        let body = selection
                            .snapshot()
                            .comments()
                            .get(*index)
                            .ok_or_else(|| refused("Original selected comment disappeared"))?
                            .body();
                        if body.len() > 65_536 {
                            return Err(refused(
                                "Original pending body exceeds bounded text contract",
                            ));
                        }
                        (Arc::from(body), false)
                    }
                    _ => (Arc::from(""), false),
                },
            }
        };
        if body.len() > 65_536 || (!intent.action().needs_body() && !body.is_empty()) {
            return Err(refused(
                "Saved operation body does not match exact original purpose",
            ));
        }
        let values = operation_values_v3(&intent, body.clone())?;
        let inline = if values.comments().is_empty() {
            Vec::new()
        } else {
            preview::rehydrate_values_v3(
                &intent.original,
                &values,
                &originals,
                &measured,
                current,
                &identity,
                &local,
                database,
                images,
                cancel,
                deadline,
            )?
        };
        let observation = read_observation(
            &intent,
            &measured,
            &identity,
            &parent,
            reviews,
            inline.into_iter().next(),
            cancel,
            deadline,
        )?;
        let mut lines=vec![intent.action().label().into(),format!("Original {}#{} • repo {} • PR {} • current numeric UID {:?}",
            parent.repository(),parent.number(),parent.repository_id(),parent.database_id(),identity.account_id),
            format!("H {} • base {} • not an expected-head CAS",parent.head_sha(),parent.base_tip_sha()),
            "Fresh Preview then explicit Confirm rechecks original source/account/SQL/target. Restart restores text/history only.".into()];
        match &observation {
            ObservationV3::Pending {
                review, comment, ..
            } => {
                lines.push(format!(
                    "Exact server PENDING review {} • actual owner UID {} • not submitted",
                    review.snapshot().review().id(),
                    review.principal()
                ));
                if let Some(comment) = comment {
                    lines.push(format!(
                        "Exact actual pending comment {} • update {:?} • delete {:?} • observed {}",
                        comment.comment().id(),
                        comment.viewer_can_update(),
                        comment.viewer_can_delete(),
                        comment.updated_at()
                    ));
                }
            }
            ObservationV3::Rerequest(source) => {
                let actual = source.target();
                lines.push(format!(
                    "Actual {:?} reviewer {} • numeric ID {} • opaque {}",
                    actual.kind(),
                    crate::git::display_bytes(actual.actor().login.as_bytes()),
                    actual.id(),
                    crate::git::display_bytes(actual.node().as_bytes())
                ));
                lines.push(source.eligibility_label().into());
                lines.push(
                    "Readback proves requested membership only; notification delivery unknown"
                        .into(),
                );
            }
        }
        if !body.is_empty() {
            lines.push(crate::git::display_bytes(body.as_bytes()));
        }
        if lines
            .iter()
            .try_fold(0usize, |bytes, line| bytes.checked_add(line.len()))
            .is_none_or(|bytes| bytes > 256 * 1024)
        {
            return Err(refused(
                "Original operation presentation exceeds bounded display",
            ));
        }
        // Seed capture includes actual local SQL/database observation and MUST
        // happen before the closing raw parent/repo/SQL/final numeric account.
        // No SQL or seed/source re-opening occurs after that final principal.
        let refs = match &observation {
            ObservationV3::Pending {
                review, comment, ..
            } => {
                let mut refs = vec![(
                    review.snapshot().review().author(),
                    review.snapshot().review().author_account_id(),
                )];
                if let Some(comment) = comment {
                    refs.push((
                        comment.comment().author(),
                        comment.comment().author_account_id(),
                    ));
                }
                refs
            }
            ObservationV3::Rerequest(actual) => vec![(
                actual.target().actor(),
                (actual.target().kind() != crate::providers::gh::ReviewRequestActorKindV3::Team)
                    .then_some(actual.target().id()),
            )],
        };
        let actors = super::super::avatars_v7::batch(&identity.host, &refs, cancel, deadline)
            .map_err(|_| refused("Actual operation actors exceed bounded metadata"))?;
        let display =
            crate::prepared_form_text_v1::PreparedFormTextV1::prepare(&lines, cancel, deadline)
                .map_err(|_| refused("Original operation display exceeds bounded prepared text"))?;
        let mut result = Self {
            intent: intent.clone(),
            identity,
            observation,
            key,
            body,
            lines: lines.into(),
            local,
            restored,
            origin: None,
            write: None,
            actors: actors.into(),
            requester: None,
            display,
            lineage_v12: choice_v12.revision,
            needs_new_v12: choice_v12.needs_new,
            new_notice_v12: None,
        };
        result.origin=Some(crate::pr_write_journal::review_operation_history_v3::ReviewOperationOriginSeedV3::from_source(database,&result)
            .map_err(|_|source_failure(safe_local()))?);
        let mut write = result
            .origin_seed()?
            .metadata()
            .write(result.body())
            .map_err(|_| refused("Actual operation fingerprint exceeds bounded target/body"))?;
        if let Some(revision) = &result.lineage_v12 {
            revision
                .bind_write(&mut write)
                .map_err(|_| refused("Original new-action revision fingerprint unavailable"))?;
        }
        result.write = Some(write);
        result.key = choice_v12.key;
        if result.needs_new_v12 {
            result.new_notice_v12 = Some(
                super::super::action_lineage_v12::prepare_notice(
                    result.intent.original(),
                    result.intent.action().label(),
                    &result.identity,
                    result
                        .lineage_v12
                        .as_ref()
                        .ok_or_else(|| source_failure(safe_local()))?,
                    cancel,
                    deadline,
                )
                .map_err(|_| source_failure(safe_local()))?,
            );
        }
        measured
            .pr_inline_original_parent_v1(
                session
                    .observation_v1(parent.repository_id())
                    .ok_or_else(|| source_failure(safe_local()))?,
                &result.intent.original.files.observed,
                resources,
                cancel,
            )
            .map_err(source_failure)?;
        session
            .verify(result.intent.original.navigation.original(), cancel)
            .map_err(source_failure)?;
        let closed = session
            .close_with_guard(cancel, || {
                private_clock(cancel, deadline)?;
                result
                    .local
                    .validate(database, &result.intent.original.source)?;
                if let Some(revision) = &result.lineage_v12 {
                    revision
                        .validate(database, &base_v12)
                        .map_err(|_| safe_local())?;
                }
                private_clock(cancel, deadline)
            })
            .map_err(source_failure)?;
        clock(cancel, deadline)?;
        result.identity = closed.identity().clone();
        result.requester = Some(
            super::super::avatars_v7::PreparedActorV7::requester(
                closed.identity(),
                cancel,
                deadline,
            )
            .map_err(|_| refused("Actual closing operation requester metadata unavailable"))?,
        );
        let result = Arc::new(result);
        clock(cancel, deadline)?;
        Ok(result)
    }
}
#[allow(clippy::too_many_arguments)]
fn read_observation(
    intent: &ScopedReviewOperationIntentV3,
    measured: &GithubProvider,
    identity: &HostIdentity,
    parent: &crate::providers::gh::PrResourceParentV1,
    reviews: &ReviewReadLimitsV2,
    inline: Option<InlineComment>,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<ObservationV3, ScopedReviewFailureV2> {
    let selected = match &intent.choice {
        ChoiceV3::Add { selection, .. }
        | ChoiceV3::Edit { selection, .. }
        | ChoiceV3::Delete { selection, .. } => Some(selection),
        _ => None,
    };
    if let Some(selection) = selected {
        let review = measured
            .pending_review_original_v3(
                identity,
                parent,
                selection.snapshot(),
                reviews,
                cancel,
                deadline,
            )
            .map_err(source_failure)?;
        let comment = match &intent.choice {
            ChoiceV3::Edit { index, .. } | ChoiceV3::Delete { index, .. } => Some(
                measured
                    .pending_comment_original_v3(review.clone(), *index, cancel, deadline)
                    .map_err(source_failure)?,
            ),
            _ => None,
        };
        return Ok(ObservationV3::Pending {
            review,
            comment,
            inline,
        });
    }
    let source = match &intent.choice {
        ChoiceV3::Reviewer(selection) => measured.rerequest_review_author_original_v3(
            identity,
            parent,
            selection.snapshot(),
            reviews,
            cancel,
            deadline,
        ),
        ChoiceV3::Team { catalogue, index } => measured.rerequest_team_original_v3(
            identity, parent, catalogue, *index, reviews, cancel, deadline,
        ),
        _ => return Err(refused("Original operation selection unavailable")),
    }
    .map_err(source_failure)?;
    Ok(ObservationV3::Rerequest(source))
}
fn operation_values_v3(
    intent: &ScopedReviewOperationIntentV3,
    body: Arc<str>,
) -> Result<Arc<ReviewDraftValuesV2>, ScopedReviewFailureV2> {
    let values = match &intent.choice {
        ChoiceV3::Add { anchor, .. } if !body.trim().is_empty() => ReviewDraftValuesV2::replace(
            Arc::from(""),
            vec![ReviewDraftCommentV2 {
                anchor: anchor.clone(),
                body,
            }]
            .into(),
        ),
        _ => ReviewDraftValuesV2::replace(body, Arc::from([])),
    }
    .map_err(|_| refused("Original operation values exceed exact bounded encoding"))?;
    Ok(Arc::new(values))
}
fn original_images_v3(
    intent: &ScopedReviewOperationIntentV3,
    images: &PrImmutableFileLimitsV1,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Vec<crate::providers::gh::PrImmutableFileOriginalV1>, ScopedReviewFailureV2> {
    let ChoiceV3::Add { anchor, .. } = &intent.choice else {
        return Ok(Vec::new());
    };
    let mut index = None;
    for (candidate, file) in intent.original.files.observed.files().iter().enumerate() {
        clock(cancel, deadline)?;
        if file.path == anchor.server_path && index.replace(candidate).is_some() {
            return Err(refused("Original pending file is ambiguous"));
        }
    }
    let index = index.ok_or_else(|| refused("Original canonical pending file is unavailable"))?;
    let original = crate::providers::gh::PrImmutableFileOriginalV1::from_observed(
        intent.original.scope(),
        intent.original.files.observed.clone(),
        index,
        images,
    )
    .map_err(source_failure)?;
    clock(cancel, deadline)?;
    Ok(vec![original])
}
