//! Fresh explicit outcome inspection, then a SEPARATE local acknowledgement.
//! SAME Summary source/Review queue/journal/runner; no restored approval,
//! resend, server mutation, inferred causality or body deletion.
use super::*;
use crate::{
    pr_write_journal::{
        review_recovery_v11::ReviewRecoveryJournalWitnessV11, PrPublicationReference,
    },
    providers::gh::{ReviewReadLimitsV2, ReviewRecoveryCurrentV11},
};
use std::sync::Mutex;

pub struct ReviewRecoveryRequestV11 {
    history: Arc<PreparedReviewOperationHistoryV3>,
    row: usize,
}
impl std::fmt::Debug for ReviewRecoveryRequestV11 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReviewRecoveryRequestV11(<chosen genuine historical blocker; no permission>)")
    }
}
impl ReviewRecoveryRequestV11 {
    pub(crate) fn capture(
        history: Arc<PreparedReviewOperationHistoryV3>,
        row: usize,
    ) -> Result<Arc<Self>, String> {
        if history.opened != Some(row)
            || history.snapshot.rows.get(row).is_none_or(|row| {
                !matches!(
                    row.marker.state,
                    PrWriteState::Pending | PrWriteState::Uncertain
                )
            })
        {
            return Err("Open the exact unresolved original Review history row first".into());
        }
        Ok(Arc::new(Self { history, row }))
    }
    pub(crate) fn intent(&self) -> &Arc<ReviewOperationHistoryIntentV3> {
        self.history.intent()
    }
    pub(crate) fn history(&self) -> &Arc<PreparedReviewOperationHistoryV3> {
        &self.history
    }
    pub(crate) fn row(&self) -> usize {
        self.row
    }
}
/// The consume slot is never reconstructed from a draft, marker, Doc, history
/// JSON or a successful UI click. ONE genuine fresh measured observation only.
pub struct PreparedReviewRecoveryV11 {
    request: Arc<ReviewRecoveryRequestV11>,
    identity: HostIdentity,
    deadline: Instant,
    reference: PrPublicationReference,
    cancellation: CancellationToken,
    display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    actor: crate::identity::ActorIdentity,
    observation: Mutex<Option<(ReviewRecoveryJournalWitnessV11, ReviewRecoveryCurrentV11)>>,
}
impl std::fmt::Debug for PreparedReviewRecoveryV11 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedReviewRecoveryV11(<fresh outcome; one local ACK; never publication>)")
    }
}
impl PreparedReviewRecoveryV11 {
    pub(crate) fn request(&self) -> &Arc<ReviewRecoveryRequestV11> {
        &self.request
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn display(&self) -> &Arc<crate::prepared_form_text_v1::PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn actor(&self) -> &crate::identity::ActorIdentity {
        &self.actor
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn reference(&self) -> &PrPublicationReference {
        &self.reference
    }
    pub(crate) fn available(&self) -> bool {
        !self.cancellation.is_cancelled()
            && Instant::now() < self.deadline
            && self.observation.lock().is_ok_and(|slot| slot.is_some())
    }
}
pub struct ReviewRecoveryAcknowledgedV11 {
    pub(crate) reference: PrPublicationReference,
    pub(crate) changed: usize,
}

fn source_identity(
    intent: &ReviewOperationHistoryIntentV3,
    identity: &HostIdentity,
) -> Result<(), ScopedReviewFailureV2> {
    intent.source_identity_v15(identity)
}
fn local(
    database: &Path,
    intent: &ReviewOperationHistoryIntentV3,
    witness: &ReviewRecoveryJournalWitnessV11,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<(), GithubError> {
    private_clock(cancel, deadline)?;
    intent.validate_local_source_v15(database)?;
    PrWriteJournal::open(database)
        .map_err(|_| safe_local())?
        .validate_review_inspection_v11(witness)
        .map_err(|_| safe_local())?;
    private_clock(cancel, deadline)
}

pub(crate) fn inspect(
    request: Arc<ReviewRecoveryRequestV11>,
    provider: &GithubProvider,
    database: &Path,
    reviews: &ReviewReadLimitsV2,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Arc<PreparedReviewRecoveryV11>, ScopedReviewFailureV2> {
    clock(cancel, deadline)?;
    reviews.validate().map_err(source_failure)?;
    let intent = request.intent();
    let old = request
        .history
        .snapshot
        .rows
        .get(request.row)
        .ok_or_else(|| refused("Original selected history row unavailable"))?;
    let issuer = provider
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .map_err(source_failure)?;
    let phase = ReviewHistoryReadPhaseV15::begin(
        intent,
        &issuer,
        database,
        cancel,
        deadline,
        Some(reviews),
    )?;
    let identity = phase.identity().clone();
    source_identity(intent, &identity)?;
    let parent = phase.parent().clone();
    let journal = PrWriteJournal::open(database).map_err(|_| source_failure(safe_local()))?;
    let target = PrWriteTarget::new(&identity, parent.summary())
        .map_err(|_| source_failure(safe_local()))?;
    journal
        .validate_review_operation_history_v3(
            &target,
            parent.repository_id(),
            parent.repository_node(),
            parent.database_id(),
            &request.history.snapshot,
        )
        .map_err(|_| source_failure(safe_local()))?;
    let witness=journal.prepare_review_inspection_v11(&target,&old.marker.publication_reference()).map_err(|_|refused("Selected original Review is not the exact supported current blocker; inspect history afresh"))?;
    if witness.target() != &target {
        return Err(source_failure(safe_local()));
    }
    let original = old.origin.as_ref().and_then(|origin| origin.metadata());
    let selected = original
        .and_then(|metadata| metadata.recovery_review_v11())
        .map(|(id, node)| (id, Some(node)))
        .or_else(|| {
            witness
                .marker()
                .selected_resource
                .strip_prefix("review/")
                .and_then(|id| id.parse().ok())
                .map(|id| (id, None))
        });
    let observation = phase.inspect_review_outcome_v15(
        selected,
        witness.marker().command == "review-rerequest-v3",
        reviews,
    )?;
    let deadline = deadline.min(observation.deadline());
    let mut lines=vec![format!("Original attempt #{} • {} • {:?}",old.marker.attempt_id,old.marker.command,old.marker.state),
        format!("HISTORICAL requested H {} • NOT current truth",old.marker.observed_head),
        "Inspect current observations carefully. Acknowledge is LOCAL ONLY: accept observed uncertainty and release this exact old barrier. It never resends or restores approval.".into()];
    lines.extend(
        observation
            .lines()
            .iter()
            .map(|line| crate::git::display_bytes(line.as_bytes())),
    );
    let display =
        crate::prepared_form_text_v1::PreparedFormTextV1::prepare(&lines, cancel, deadline)
            .map_err(|_| {
                refused("Fresh recovery observation cannot fit bounded original-phase display")
            })?;
    let closing_identity = phase.finish(|| {
        local(database, intent, &witness, cancel, deadline)?;
        issuer.validate_review_inspection_v11(&observation, &identity, &parent, cancel, deadline)
    })?;
    source_identity(intent, &closing_identity)?;
    if closing_identity.login.len() > 4096 {
        return Err(refused("Actual recovery inspector metadata exceeds bounds"));
    }
    let identity = closing_identity;
    let actor = crate::identity::ActorIdentity::github(
        &identity.host,
        &Actor {
            login: identity.login.clone(),
            ..Default::default()
        },
    );
    clock(cancel, deadline)?;
    let reference = witness.marker().publication_reference();
    let prepared = Arc::new(PreparedReviewRecoveryV11 {
        request: Arc::clone(&request),
        identity,
        deadline,
        reference,
        cancellation: cancel.clone(),
        display,
        actor,
        observation: Mutex::new(Some((witness, observation))),
    });
    clock(cancel, deadline)?;
    Ok(prepared)
}

pub(crate) fn acknowledge(
    prepared: &Arc<PreparedReviewRecoveryV11>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    requested_deadline: Instant,
) -> Result<ReviewRecoveryAcknowledgedV11, ScopedReviewFailureV2> {
    let deadline = requested_deadline.min(prepared.deadline);
    clock(cancel, deadline)?;
    clock(&prepared.cancellation, deadline)?;
    // Worker-only one-take. A failed/cancelled attempt requires a NEW explicit
    // inspection, never replaying this retained presentation/observation.
    let (witness, observation) = prepared
        .observation
        .lock()
        .map_err(|_| refused("Original recovery observation unavailable"))?
        .take()
        .ok_or_else(|| {
            refused("Original recovery observation consumed; inspect current outcome again")
        })?;
    let intent = prepared.request.intent();
    local(database, intent, &witness, cancel, deadline).map_err(source_failure)?;
    observation
        .validate(provider, cancel, deadline)
        .map_err(source_failure)?;
    let issuer = provider
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .map_err(source_failure)?;
    let phase =
        ReviewHistoryReadPhaseV15::begin(intent, &issuer, database, cancel, deadline, None)?;
    let identity = phase.identity().clone();
    source_identity(intent, &identity)?;
    if identity.host != prepared.identity.host
        || identity.account_id != prepared.identity.account_id
    {
        return Err(source_failure(safe_local()));
    }
    let parent = phase.parent().clone();
    issuer
        .validate_review_inspection_v11(&observation, &identity, &parent, cancel, deadline)
        .map_err(source_failure)?;
    let closing_identity = phase.finish(|| {
        clock(&prepared.cancellation, deadline).map_err(|_| safe_local())?;
        local(database, intent, &witness, cancel, deadline)?;
        issuer.validate_review_inspection_v11(&observation, &identity, &parent, cancel, deadline)
    })?;
    source_identity(intent, &closing_identity)?;
    clock(cancel, deadline)?;
    clock(&prepared.cancellation, deadline)?;
    let reference = witness.marker().publication_reference();
    let mut journal = PrWriteJournal::open(database).map_err(|_| source_failure(safe_local()))?;
    let changed = journal
        .acknowledge_review_inspection_v11(
            &issuer,
            witness,
            observation,
            &closing_identity,
            cancel,
            deadline,
        )
        .map_err(|_| source_failure(safe_local()))?;
    // Acknowledgement intentionally advanced the original journal generation.
    // NO source/private SQL/body read or draft cleanup after this retirement.
    Ok(ReviewRecoveryAcknowledgedV11 { reference, changed })
}
