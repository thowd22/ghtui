//! Explicit new action presentation and worker-only original SQL lineage.
//! No protected body, old preview or approval is restored by this module.
use super::*;
use crate::pr_write_journal::review_action_lineage_v12::{
    ReviewAcknowledgedRevisionV12, ReviewActionBaseV12,
};
use crate::providers::gh::HostIdentity;

pub(crate) enum ActionContextV12 {
    Review {
        draft: Arc<ScopedReviewDraftV2>,
        selection: Option<Arc<ScopedReviewSelectionV2>>,
        action: ReviewRequestedActionV2,
    },
    Operation(Arc<operations_v3::ScopedReviewOperationSourceV3>),
}
pub(crate) struct NeedsNewActionV12 {
    pub context: ActionContextV12,
    pub revision: Arc<ReviewAcknowledgedRevisionV12>,
    pub display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    pub deadline: Instant,
    pub opened: std::sync::atomic::AtomicBool,
    pub requester: avatars_v7::PreparedActorV7,
}
impl NeedsNewActionV12 {
    pub fn intent(&self) -> &Arc<ScopedReviewIntentV2> {
        match &self.context {
            ActionContextV12::Review { draft, .. } => draft.intent(),
            ActionContextV12::Operation(source) => source.intent().original(),
        }
    }
    pub fn prepare(
        context: ActionContextV12,
        revision: Arc<ReviewAcknowledgedRevisionV12>,
        display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, String> {
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err("Original new-action phase ended".into());
        }
        let identity = match &context {
            ActionContextV12::Review { draft, .. } => draft.identity(),
            ActionContextV12::Operation(source) => source.identity(),
        };
        let requester = avatars_v7::PreparedActorV7::requester(identity, cancel, deadline)?;
        Ok(Arc::new(Self {
            context,
            revision,
            display,
            deadline,
            opened: std::sync::atomic::AtomicBool::new(false),
            requester,
        }))
    }
}
/// Worker-only, BEFORE the original opener's final raw parent/repo/SQL/UID.
/// Final construction merely retains this Arc and actual closing requester.
pub(crate) fn prepare_notice(
    intent: &Arc<ScopedReviewIntentV2>,
    label: &str,
    identity: &HostIdentity,
    revision: &Arc<ReviewAcknowledgedRevisionV12>,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Arc<crate::prepared_form_text_v1::PreparedFormTextV1>, String> {
    let parent = intent.parent();
    let lines=vec![format!("CURRENT requested action: {label}"),
        format!("{}#{} • repo #{} • PR #{} • current numeric UID {:?}",parent.repository(),parent.number(),parent.repository_id(),parent.database_id(),identity.account_id),
        format!("{} #{}",revision.label(),revision.reference().attempt_id),
        "Old protected text and durable history remain unchanged. This is NOT retry, reconciliation, restored approval or proof of no effects.".into(),
        "Choose New action explicitly to open fresh CURRENT editable text (edit only), or EMPTY text. Then fresh Preview and explicit Confirm are still required.".into(),
        "Pending/uncertain attempts must first be freshly inspected and explicitly acknowledged. Confirmed receipt history is labeled Confirmed, never acknowledged.".into()];
    crate::prepared_form_text_v1::PreparedFormTextV1::prepare(&lines, cancel, deadline)
        .map_err(|_| "New-action notice exceeds bounded original display".into())
}
pub(crate) struct OpenChoiceV12 {
    pub key: Arc<str>,
    pub revision: Option<Arc<ReviewAcknowledgedRevisionV12>>,
    pub needs_new: bool,
}
pub(crate) fn choose(
    database: &Path,
    base: &ReviewActionBaseV12,
    explicit: Option<&Arc<ReviewAcknowledgedRevisionV12>>,
) -> Result<OpenChoiceV12, String> {
    let journal =
        PrWriteJournal::open(database).map_err(|_| "Original action journal unavailable")?;
    let current = journal.action_revision_v12(base).map_err(|_| {
        "Original action is protected or its exact lineage is unavailable; inspect history first"
    })?;
    match (explicit, current) {
        (Some(expected), Some(current)) => {
            expected
                .validate(database, base)
                .map_err(|_| "Original terminal action/source/SQL changed; nothing reopened")?;
            if expected.reference() != current.reference() || expected.key() != current.key() {
                return Err("A newer terminal action exists; inspect it explicitly".into());
            }
            Ok(OpenChoiceV12 {
                key: Arc::from(current.key()),
                revision: Some(current),
                needs_new: false,
            })
        }
        (None, Some(current)) => {
            // A previously explicit unprotected revision may resume its text;
            // no original protected text/ref is copied into it.
            let saved = Cache::open(database)
                .map_err(|_| "Original draft unavailable")?
                .bounded_draft(base.scope(), current.key(), REVIEW_DRAFT_BYTES_V2)
                .map_err(|_| "Original draft unavailable")?;
            let resume = saved.is_some();
            Ok(OpenChoiceV12 {
                key: Arc::from(current.key()),
                revision: Some(current),
                needs_new: !resume,
            })
        }
        (Some(_), None) => {
            Err("Original terminal lineage disappeared; no arbitrary new revision".into())
        }
        (None, None) => Ok(OpenChoiceV12 {
            key: Arc::from(base.key()),
            revision: None,
            needs_new: false,
        }),
    }
}
