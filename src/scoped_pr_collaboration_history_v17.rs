//! All-six original Conversation history, fresh outcome inspection and ONE
//! explicit local ACK. No publication source, body restore, replay or retry.
use crate::{
    forms::{Form, PrFormKind, PrIntent},
    pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
    pr_write_journal::{
        conversation_history_v17::{ConversationHistoryV17, ConversationRecoveryWitnessV17},
        PrPublicationReference, PrWriteJournal, PrWriteState, PrWriteTarget,
    },
    providers::{
        gh::{
            ConversationRecoveryCurrentV17, GithubError, GithubErrorKind, GithubProvider,
            GithubResult, HostIdentity, PrResourceParentV1,
        },
        runner::CancellationToken,
    },
    scoped_pr_collaboration_source_v17::with_history_read_v17,
    scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard,
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};
pub(crate) struct ConversationHistoryViewV17 {
    pub original: Arc<PrCollaborationOpenIntentV17>,
    pub snapshot: Arc<ConversationHistoryV17>,
    pub identity: HostIdentity,
    pub key: Arc<str>,
    pub lines: Arc<[String]>,
    pub display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    pub current: Option<Arc<PreparedConversationRecoveryV17>>,
}
pub(crate) struct OpenedConversationHistoryV17 {
    pub view: Arc<ConversationHistoryViewV17>,
    pub form: Form,
}
pub(crate) struct ConversationHistoryRequestV17 {
    pub view: Arc<ConversationHistoryViewV17>,
    pub row: usize,
    pub ticket: Arc<()>,
    pub presentation: Arc<()>,
    pub input_epoch: u64,
}
pub(crate) struct PreparedConversationRecoveryV17 {
    pub deadline: Instant,
    pub cancel: CancellationToken,
    pub reference: PrPublicationReference,
    useful: AtomicBool,
    observed: Mutex<
        Option<(
            ConversationRecoveryWitnessV17,
            ConversationRecoveryCurrentV17,
        )>,
    >,
}
impl PreparedConversationRecoveryV17 {
    pub(crate) fn painted(&self, useful: bool) {
        self.useful.store(useful, Ordering::Release);
    }
    pub(crate) fn available(&self) -> bool {
        !self.cancel.is_cancelled()
            && Instant::now() < self.deadline
            && self.useful.load(Ordering::Acquire)
            && self.observed.lock().is_ok_and(|s| s.is_some())
    }
}
fn refusal() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Exact original Conversation history/inspection changed; no release or resend"
            .into(),
    }
}
fn clock(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(refusal())
    } else {
        Ok(())
    }
}
pub(crate) fn form(parent: &PrResourceParentV1, identity: &HostIdentity) -> GithubResult<Form> {
    let mut form = Form::pr(PrIntent {
        kind: PrFormKind::Comment,
        target: parent.summary().clone(),
        identity: identity.clone(),
    })
    .map_err(|_| refusal())?;
    form.title = format!(
        "Original Conversation history • {}#{}",
        parent.repository(),
        parent.number()
    );
    form.fields.clear();
    form.dirty = false;
    form.preview = true;
    Ok(form)
}
fn history_lines(history: &ConversationHistoryV17) -> GithubResult<Vec<String>> {
    let mut lines=vec!["ORIGINAL PROTECTED HISTORY • restored records are NOT current permission or approval".into(),
        "Inspect current observations, then explicitly acknowledge LOCAL uncertainty. Neither action resends, restores or deletes the original body.".into(),
        format!("Bounded history: {} rows • {}",history.rows.len(),if history.complete {"complete observed window"} else {"older rows omitted; no complete-history claim"})];
    for row in &history.rows {
        let mut extra = vec![
            format!(
                "Attempt #{} • {} • {:?}",
                row.marker.attempt_id, row.marker.command, row.marker.state
            ),
            format!(
                "Historical H {} • ref {} • NEVER current truth",
                row.marker.observed_head, row.marker.payload_sha256
            ),
        ];
        if let Some(selected) = &row.origin.seed().selected {
            let actor = crate::identity::ActorIdentity::github(
                &row.origin.seed().scope.host,
                &selected.author,
            );
            extra.push(format!(
                "Original comment #{} / {} • [{}] {} • original author UID {:?}",
                selected.id,
                selected.node,
                actor.initials(),
                actor.label(),
                selected.author_account
            ));
        }
        if let Some((_, values)) = &row.body {
            for (key, body) in values {
                extra.push(format!("Protected {key}:"));
                for line in body.lines() {
                    extra.push(crate::git::display_bytes(line.as_bytes()));
                }
            }
        } else {
            extra.push("Original draft absent; metadata retained, no body inferred".into());
        }
        let bytes = lines
            .iter()
            .chain(&extra)
            .try_fold(0usize, |n, s| n.checked_add(s.len()))
            .ok_or_else(refusal)?;
        if bytes > 8 * 1024 * 1024 || lines.len().saturating_add(extra.len()) > 50_000 {
            return Err(refusal());
        }
        lines.extend(extra);
    }
    Ok(lines)
}
pub(crate) fn open(
    original: Arc<PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<OpenedConversationHistoryV17> {
    let ((snapshot, lines, display, form, key), identity) = with_history_read_v17(
        original.clone(),
        provider,
        database,
        cancel,
        deadline,
        |read| {
            let target =
                PrWriteTarget::new(read.identity, read.parent.summary()).map_err(|_| refusal())?;
            let journal = PrWriteJournal::open(database).map_err(|_| refusal())?;
            let history = Arc::new(
                journal
                    .conversation_history_v17(&target, read.parent)
                    .map_err(|_| refusal())?,
            );
            ResourceLocalGuard::validate_conversation_history_v17(
                database,
                read.boundary,
                &target,
                read.parent,
                &history,
            )?;
            let lines: Arc<[String]> = history_lines(&history)?.into();
            let display =
                crate::prepared_form_text_v1::PreparedFormTextV1::prepare(&lines, cancel, deadline)
                    .map_err(|_| refusal())?;
            let form = form(read.parent, read.identity)?;
            let key: Arc<str> =
                format!("conversation-history-v17/{}", read.parent.database_id()).into();
            Ok((
                history,
                lines,
                display,
                form,
                key,
                target,
                read.parent.clone(),
            ))
        },
        |(history, _, _, _, _, target, parent), boundary| {
            ResourceLocalGuard::validate_conversation_history_v17(
                database, boundary, target, parent, history,
            )
        },
    )
    .map(|((history, lines, display, form, key, _, _), identity)| {
        ((history, lines, display, form, key), identity)
    })?;
    let view = Arc::new(ConversationHistoryViewV17 {
        original,
        snapshot,
        identity,
        key,
        lines,
        display,
        current: None,
    });
    Ok(OpenedConversationHistoryV17 { view, form })
}
pub(crate) fn inspect(
    request: &ConversationHistoryRequestV17,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<OpenedConversationHistoryV17> {
    let original = &request.view.original;
    let ((snapshot, lines, display, form, current), identity) = with_history_read_v17(
        original.clone(),
        provider,
        database,
        cancel,
        deadline,
        |read| {
            let target =
                PrWriteTarget::new(read.identity, read.parent.summary()).map_err(|_| refusal())?;
            let journal = PrWriteJournal::open(database).map_err(|_| refusal())?;
            journal
                .validate_conversation_history_v17(&target, read.parent, &request.view.snapshot)
                .map_err(|_| refusal())?;
            let row = request
                .view
                .snapshot
                .rows
                .get(request.row)
                .ok_or_else(refusal)?;
            if !matches!(
                row.marker.state,
                PrWriteState::Pending | PrWriteState::Uncertain
            ) {
                return Err(refusal());
            }
            let witness = journal
                .prepare_conversation_inspection_v17(&target, &row.marker.publication_reference())
                .map_err(|_| refusal())?;
            let observation = read.inspect(witness.origin.seed())?;
            let deadline = deadline.min(observation.deadline());
            let mut lines = history_lines(&request.view.snapshot)?;
            lines.extend(observation.lines().iter().cloned());
            let display =
                crate::prepared_form_text_v1::PreparedFormTextV1::prepare(&lines, cancel, deadline)
                    .map_err(|_| refusal())?;
            let lines: Arc<[String]> = lines.into();
            let form = form(read.parent, read.identity)?;
            Ok((
                request.view.snapshot.clone(),
                lines,
                display,
                form,
                witness,
                observation,
                target,
                read.parent.clone(),
                read.identity.clone(),
                deadline,
            ))
        },
        |(snapshot, _, _, _, witness, observation, target, parent, identity, deadline),
         boundary| {
            ResourceLocalGuard::validate_conversation_history_v17(
                database, boundary, target, parent, snapshot,
            )?;
            PrWriteJournal::open(database)
                .map_err(|_| refusal())?
                .validate_conversation_inspection_v17(witness)
                .map_err(|_| refusal())?;
            provider.validate_conversation_inspection_v17(
                observation,
                identity,
                parent,
                cancel,
                *deadline,
            )
        },
    )
    .map(
        |((snapshot, lines, display, form, witness, observation, _, _, _, deadline), identity)| {
            let reference = witness.origin.reference().clone();
            (
                (
                    snapshot,
                    lines,
                    display,
                    form,
                    Arc::new(PreparedConversationRecoveryV17 {
                        deadline,
                        cancel: cancel.clone(),
                        reference,
                        useful: AtomicBool::new(false),
                        observed: Mutex::new(Some((witness, observation))),
                    }),
                ),
                identity,
            )
        },
    )?;
    let view = Arc::new(ConversationHistoryViewV17 {
        original: original.clone(),
        snapshot,
        identity,
        key: request.view.key.clone(),
        lines,
        display,
        current: Some(current),
    });
    Ok(OpenedConversationHistoryV17 { view, form })
}
pub(crate) fn acknowledge(
    request: &ConversationHistoryRequestV17,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<(PrPublicationReference, usize)> {
    let current = request
        .view
        .current
        .as_ref()
        .filter(|v| v.available())
        .ok_or_else(refusal)?;
    if !cancel.same_instance(&current.cancel) || deadline > current.deadline {
        return Err(refusal());
    }
    clock(cancel, deadline)?;
    let (witness, observation) = current
        .observed
        .lock()
        .map_err(|_| refusal())?
        .take()
        .ok_or_else(refusal)?;
    if !observation.matches_target(witness.target())
        || crate::storage::Scope::github(
            &request.view.identity.host,
            request.view.identity.account_id,
        )
        .map_err(|_| refusal())?
            != *witness.target().scope()
    {
        return Err(refusal());
    }
    let mut journal = PrWriteJournal::open(database).map_err(|_| refusal())?;
    // No JSON/body/source read after final UID: exact byte witness only, then
    // SAME existing runner generation reconciliation + sole SQL transition.
    let changed = journal
        .acknowledge_conversation_inspection_v17(provider, witness, observation, cancel, deadline)
        .map_err(|_| refusal())?;
    Ok((current.reference.clone(), changed))
}
