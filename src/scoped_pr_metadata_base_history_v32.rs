//! Fresh original Summary/Conversation inspection. No Source is issued,
//! protected text is never restored, and ACK never publishes or deletes.
use super::*;
use crate::{
    pr_write_journal::{
        metadata_base_origin_v32::history_v32::{BaseHistoryV32, BaseRecoveryWitnessV32},
        PrWriteJournal, PrWriteState, PrWriteTarget,
    },
    providers::gh::BaseRecoveryCurrentV32,
    scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard,
};
use std::sync::Mutex;
pub(crate) struct BaseHistoryViewV32 {
    pub(crate) original: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    pub(crate) snapshot: Arc<BaseHistoryV32>,
    pub(crate) identity: HostIdentity,
    pub(crate) key: Arc<str>,
    pub(crate) lines: Arc<[String]>,
    pub(crate) display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    pub(crate) author: crate::scoped_pr_collaboration_draft_v17::ConversationActorV17,
    pub(crate) current: Option<Arc<BaseInspectionV32>>,
}
pub(crate) struct BaseInspectionV32 {
    pub(crate) cancel: CancellationToken,
    pub(crate) deadline: Instant,
    pub(crate) reference: PrPublicationReference,
    useful: AtomicBool,
    observation: Mutex<Option<(BaseRecoveryWitnessV32, BaseRecoveryCurrentV32)>>,
}
impl BaseInspectionV32 {
    pub(crate) fn painted(&self, useful: bool) {
        self.useful.store(useful, Ordering::Release);
    }
    pub(crate) fn ready(&self) -> bool {
        !self.cancel.is_cancelled()
            && Instant::now() < self.deadline
            && self.useful.load(Ordering::Acquire)
            && self.observation.lock().is_ok_and(|v| v.is_some())
    }
}
#[derive(Clone)]
pub(crate) struct BaseHistoryStateV32 {
    pub(crate) view: Arc<BaseHistoryViewV32>,
    pub(crate) ticket: Arc<()>,
}
pub(crate) struct BaseHistoryRequestV32 {
    pub(crate) view: Arc<BaseHistoryViewV32>,
    row: usize,
    ticket: Arc<()>,
    presentation: Arc<()>,
    epoch: u64,
}
pub(crate) struct BaseHistoryOpenRequestV32 {
    pub(crate) original: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    ticket: Arc<()>,
    presentation: Arc<()>,
    epoch: u64,
}
pub(crate) fn state(form: &Form) -> Option<&BaseHistoryStateV32> {
    match &form.purpose {
        FormPurpose::PrCollaborationV17(s) => match &s.mode {
            ComposerModeV17::BaseHistoryV32(v) => Some(v),
            _ => None,
        },
        _ => None,
    }
}
pub(in crate::scoped_pr_collaboration_ui_v17) struct HistoryCloseIdentityV296 {
    view: Arc<BaseHistoryViewV32>,
    ticket: Arc<()>,
}
pub(in crate::scoped_pr_collaboration_ui_v17) fn capture_close_v296(
    form: &Form,
) -> Option<HistoryCloseIdentityV296> {
    let state = state(form)?;
    if !form
        .preview_display_v1
        .document
        .as_ref()
        .is_some_and(|doc| Arc::ptr_eq(doc, &state.view.display))
    {
        return None;
    }
    Some(HistoryCloseIdentityV296 {
        view: Arc::clone(&state.view),
        ticket: Arc::clone(&state.ticket),
    })
}
impl HistoryCloseIdentityV296 {
    pub(in crate::scoped_pr_collaboration_ui_v17) fn matches_form(&self, form: &Form) -> bool {
        state(form).is_some_and(|state| {
            Arc::ptr_eq(&state.view, &self.view) && Arc::ptr_eq(&state.ticket, &self.ticket)
        })
    }
    pub(in crate::scoped_pr_collaboration_ui_v17) fn owns_original(
        &self,
        app: &crate::app::App,
    ) -> bool {
        self.view.original.owns_presentation(app)
    }
}
impl BaseHistoryRequestV32 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        state(form).is_some_and(|s| {
            Arc::ptr_eq(&s.view, &self.view) && Arc::ptr_eq(&s.ticket, &self.ticket)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        self.owns(form) && form.presentation_v4().1 == Some(self.epoch) && form.fields.is_empty()
    }
}
impl BaseHistoryOpenRequestV32 {
    pub(crate) fn matches(&self, form: &Form) -> bool {
        super::state(form).is_some_and(|s| {
            Arc::ptr_eq(&s.ticket, &self.ticket)
                && Arc::ptr_eq(s.intent.source.opening(), &self.original)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
            && form.presentation_v4().1 == Some(self.epoch)
            && form.publication.is_some()
    }
}
pub(crate) fn capture_open(form: &Form) -> Result<Arc<BaseHistoryOpenRequestV32>, String> {
    let s = super::state(form).ok_or("Not protected original metadata Form")?;
    if form.publication.is_none() {
        return Err("Exact protected metadata reference required".into());
    }
    Ok(Arc::new(BaseHistoryOpenRequestV32 {
        original: s.intent.source.opening().clone(),
        ticket: s.ticket.clone(),
        presentation: form.presentation_v4().0.clone(),
        epoch: form
            .presentation_v4()
            .1
            .ok_or("Metadata history input retired")?,
    }))
}
pub(crate) fn request(form: &Form) -> Result<Arc<BaseHistoryRequestV32>, String> {
    let s = state(form).ok_or("Not original metadata history")?;
    let mut unresolved = s.view.snapshot.rows.iter().enumerate().filter(|(_, r)| {
        matches!(
            r.marker.state,
            PrWriteState::Pending | PrWriteState::Uncertain
        )
    });
    let row = unresolved
        .next()
        .map(|(i, _)| i)
        .ok_or("No supported unresolved metadata attempt; history stays read only")?;
    if unresolved.next().is_some() || !form.fields.is_empty() {
        return Err("Exact single metadata blocker unavailable".into());
    }
    Ok(Arc::new(BaseHistoryRequestV32 {
        view: s.view.clone(),
        row,
        ticket: s.ticket.clone(),
        presentation: form.presentation_v4().0.clone(),
        epoch: form
            .presentation_v4()
            .1
            .ok_or("Metadata history input retired")?,
    }))
}
pub(crate) fn ack_ready(form: &Form) -> bool {
    form.preview
        && !form.preview_display_v1.geometry_rejected
        && request(form).is_ok_and(|r| {
            r.view.current.as_ref().is_some_and(|current| {
                current.ready()
                    && current.reference
                        == r.view.snapshot.rows[r.row].marker.publication_reference()
            })
        })
}
fn line(lines: &mut Vec<String>, value: String) -> GithubResult<()> {
    if lines.len() >= 50_000
        || lines
            .iter()
            .try_fold(value.len(), |n, s| n.checked_add(s.len()))
            .is_none_or(|n| n > 8 * 1024 * 1024)
    {
        return Err(failure());
    }
    lines.push(value);
    Ok(())
}
fn prepare_lines(
    history: &BaseHistoryV32,
    current: &BaseRecoveryCurrentV32,
) -> GithubResult<Vec<String>> {
    let parent = current.actual().parent();
    let mut lines = vec![
        "ORIGINAL BASE HISTORY • inspect only; no selection restoration or resend".into(),
        format!(
            "{} bounded original rows • {}",
            history.rows.len(),
            if history.complete {
                "complete observed window"
            } else {
                "older records omitted"
            }
        ),
        format!(
            "Currently observed Base {} at {} • head {}",
            crate::git::display_bytes(parent.summary().base_ref.as_bytes()),
            parent.base_tip_sha(),
            parent.head_sha()
        ),
    ];
    for row in &history.rows {
        line(
            &mut lines,
            format!(
                "Attempt #{} • {} • historical H {} (not current authority)",
                row.marker.attempt_id,
                match row.marker.state {
                    PrWriteState::Pending => "Pending",
                    PrWriteState::Uncertain => "Unknown outcome",
                    PrWriteState::Confirmed => "Confirmed",
                    PrWriteState::Acknowledged => "Locally acknowledged",
                },
                row.marker.observed_head
            ),
        )?;
        line(
            &mut lines,
            format!(
                "Historical Base {} at {}",
                crate::git::display_bytes(row.origin.seed.historical_ref().as_bytes()),
                row.origin.seed.historical_base()
            ),
        )?;
        line(
            &mut lines,
            format!(
                "Requested Base {} at {} • exact fingerprint {}",
                crate::git::display_bytes(row.origin.seed.requested_ref().as_bytes()),
                row.origin.seed.requested_tip(),
                row.marker.payload_sha256
            ),
        )?;
        line(
            &mut lines,
            if row.body.is_some() {
                "Protected exact draft/reference retained; NOT editable or eligible".into()
            } else {
                "Original draft absent; journal reference retained without inference".into()
            },
        )?;
    }
    line(&mut lines,"Explicit LOCAL ACK releases only this exact freshly inspected uncertainty. It never resends, deletes a draft, restores eligibility or proves causality.".into())?;
    Ok(lines)
}
pub(crate) fn open(
    original: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    expected: Option<&BaseHistoryRequestV32>,
) -> GithubResult<Form> {
    let ((snapshot, lines, display, mut form, current, author), identity) =
        source::with_base_history_v32(
            original.clone(),
            provider,
            database,
            cancel,
            deadline,
            |observation, boundary| {
                let actual = observation.actual();
                let parent = actual.parent().clone();
                let target = PrWriteTarget::new(actual.identity(), parent.summary())
                    .map_err(|_| failure())?;
                let journal = PrWriteJournal::open(database).map_err(|_| failure())?;
                let snapshot = if let Some(expected) = expected {
                    journal
                        .validate_base_history_v32(&target, &parent, &expected.view.snapshot)
                        .map_err(|_| failure())?;
                    expected.view.snapshot.clone()
                } else {
                    Arc::new(
                        journal
                            .base_history_v32(&target, &parent)
                            .map_err(|_| failure())?,
                    )
                };
                ResourceLocalGuard::validate_base_history_v32(
                    database, boundary, &target, &parent, &snapshot,
                )?;
                let witness = expected
                    .map(|expected| {
                        let row = snapshot.rows.get(expected.row).ok_or_else(failure)?;
                        journal
                            .prepare_base_inspection_v32(
                                &target,
                                &row.marker.publication_reference(),
                            )
                            .map_err(|_| failure())
                    })
                    .transpose()?;
                let lines: Arc<[String]> = prepare_lines(&snapshot, &observation)?.into();
                let display = crate::prepared_form_text_v1::PreparedFormTextV1::prepare(
                    &lines, cancel, deadline,
                )
                .map_err(|_| failure())?;
                let author = crate::scoped_pr_collaboration_draft_v17::prepare_actor_v17(
                    &actual.identity().host,
                    &parent.summary().pr.author,
                    parent.author_account_id(),
                    cancel,
                    deadline,
                )?;
                let mut form = Form::pr(PrIntent {
                    kind: PrFormKind::Base,
                    target: parent.summary().clone(),
                    identity: actual.identity().clone(),
                })
                .map_err(|_| failure())?;
                form.title = format!(
                    "Original {} history • {}#{}",
                    "Base",
                    parent.repository(),
                    parent.number()
                );
                form.fields.clear();
                form.dirty = false;
                form.preview = true;
                Ok((
                    snapshot,
                    lines,
                    display,
                    form,
                    observation,
                    witness,
                    author,
                    target,
                    parent,
                ))
            },
            |(snapshot, _, _, _, current, witness, _, target, parent), boundary| {
                ResourceLocalGuard::validate_base_history_v32(
                    database, boundary, target, parent, snapshot,
                )?;
                if let Some(witness) = witness {
                    PrWriteJournal::open(database)
                        .map_err(|_| failure())?
                        .validate_base_inspection_v32(witness)
                        .map_err(|_| failure())?;
                }
                provider.validate_base_inspection_v32(current, cancel, deadline)
            },
        )
        .and_then(
            |((snapshot, lines, display, form, observation, witness, author, _, _), identity)| {
                provider.validate_base_inspection_v32(&observation, cancel, deadline)?;
                let current = witness.map(|witness| {
                    Arc::new(BaseInspectionV32 {
                        cancel: observation.cancellation().clone(),
                        deadline: deadline.min(observation.deadline()),
                        reference: witness.origin.reference.clone(),
                        useful: AtomicBool::new(false),
                        observation: Mutex::new(Some((witness, observation))),
                    })
                });
                Ok(((snapshot, lines, display, form, current, author), identity))
            },
        )?;
    let view = Arc::new(BaseHistoryViewV32 {
        original,
        snapshot,
        identity,
        key: Arc::from("metadata-base-history-v32"),
        lines,
        display,
        author,
        current,
    });
    form.install_prepared_preview_display_v1(view.display.clone());
    form.purpose = FormPurpose::PrCollaborationV17(Box::new(ConversationComposerStateV17 {
        mode: ComposerModeV17::BaseHistoryV32(BaseHistoryStateV32 {
            view,
            ticket: Arc::new(()),
        }),
    }));
    Ok(form)
}
pub(crate) fn acknowledge(
    request: &BaseHistoryRequestV32,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<(PrPublicationReference, usize)> {
    let current = request
        .view
        .current
        .as_ref()
        .filter(|v| v.ready())
        .ok_or_else(failure)?;
    if !cancel.same_instance(&current.cancel) || deadline > current.deadline {
        return Err(failure());
    }
    check(cancel, deadline)?;
    let (witness, observation) = current
        .observation
        .lock()
        .map_err(|_| failure())?
        .take()
        .ok_or_else(failure)?;
    let changed = PrWriteJournal::open(database)
        .map_err(|_| failure())?
        .acknowledge_base_inspection_v32(provider, witness, observation, cancel, deadline)
        .map_err(|_| failure())?;
    Ok((current.reference.clone(), changed))
}
