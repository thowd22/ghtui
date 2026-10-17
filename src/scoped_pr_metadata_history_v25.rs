//! Fresh original Summary/Conversation inspection. No Source is issued,
//! protected text is never restored, and ACK never publishes or deletes.
use super::*;
use crate::{
    pr_write_journal::{
        metadata_origin_v25::history_v25::{MetadataHistoryV25, MetadataRecoveryWitnessV25},
        PrWriteJournal, PrWriteState, PrWriteTarget,
    },
    providers::gh::MetadataRecoveryCurrentV25,
    scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard,
};
use std::sync::Mutex;
pub(crate) struct MetadataHistoryViewV25 {
    pub(crate) original: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    pub(crate) field: FieldKind,
    pub(crate) snapshot: Arc<MetadataHistoryV25>,
    pub(crate) identity: HostIdentity,
    pub(crate) key: Arc<str>,
    pub(crate) lines: Arc<[String]>,
    pub(crate) display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    pub(crate) author: crate::scoped_pr_collaboration_draft_v17::ConversationActorV17,
    pub(crate) current: Option<Arc<MetadataInspectionV25>>,
}
pub(crate) struct MetadataInspectionV25 {
    pub(crate) cancel: CancellationToken,
    pub(crate) deadline: Instant,
    pub(crate) reference: PrPublicationReference,
    useful: AtomicBool,
    observation: Mutex<Option<(MetadataRecoveryWitnessV25, MetadataRecoveryCurrentV25)>>,
}
impl MetadataInspectionV25 {
    pub(crate) fn painted(&self, useful: bool) {
        self.useful.store(useful, Ordering::Release);
    }
    pub(crate) fn ready(&self) -> bool {
        !self.cancel.is_cancelled()
            && Instant::now() < self.deadline
            && self.useful.load(Ordering::Acquire)
            && self.observation.lock().is_ok_and(|v| v.is_some())
    }
    #[cfg(test)]
    pub(crate) fn validates_current_generation_v42(&self, provider: &GithubProvider) -> bool {
        self.observation.lock().is_ok_and(|slot| {
            slot.as_ref().is_some_and(|(_, observation)| {
                provider
                    .validate_metadata_inspection_v25(observation, &self.cancel, self.deadline)
                    .is_ok()
            })
        })
    }
}
#[derive(Clone)]
pub(crate) struct MetadataHistoryStateV25 {
    pub(crate) view: Arc<MetadataHistoryViewV25>,
    pub(crate) ticket: Arc<()>,
}
pub(crate) struct MetadataHistoryRequestV25 {
    pub(crate) view: Arc<MetadataHistoryViewV25>,
    row: usize,
    ticket: Arc<()>,
    presentation: Arc<()>,
    epoch: u64,
}
pub(crate) struct MetadataHistoryOpenRequestV25 {
    pub(crate) original: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    pub(crate) field: FieldKind,
    ticket: Arc<()>,
    presentation: Arc<()>,
    epoch: u64,
}
pub(crate) fn state(form: &Form) -> Option<&MetadataHistoryStateV25> {
    match &form.purpose {
        FormPurpose::PrCollaborationV17(s) => match &s.mode {
            ComposerModeV17::MetadataHistoryV25(v) => Some(v),
            _ => None,
        },
        _ => None,
    }
}
impl MetadataHistoryRequestV25 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        state(form).is_some_and(|s| {
            Arc::ptr_eq(&s.view, &self.view) && Arc::ptr_eq(&s.ticket, &self.ticket)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        self.owns(form) && form.presentation_v4().1 == Some(self.epoch) && form.fields.is_empty()
    }
}
impl MetadataHistoryOpenRequestV25 {
    pub(crate) fn matches(&self, form: &Form) -> bool {
        super::state(form).is_some_and(|s| {
            s.intent.field == self.field
                && Arc::ptr_eq(&s.ticket, &self.ticket)
                && Arc::ptr_eq(s.intent.source.opening(), &self.original)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
            && form.presentation_v4().1 == Some(self.epoch)
            && form.publication.is_some()
    }
}
pub(crate) fn capture_open(form: &Form) -> Result<Arc<MetadataHistoryOpenRequestV25>, String> {
    let s = super::state(form).ok_or("Not protected original metadata Form")?;
    if form.publication.is_none() {
        return Err("Exact protected metadata reference required".into());
    }
    Ok(Arc::new(MetadataHistoryOpenRequestV25 {
        original: s.intent.source.opening().clone(),
        field: s.intent.field,
        ticket: s.ticket.clone(),
        presentation: form.presentation_v4().0.clone(),
        epoch: form
            .presentation_v4()
            .1
            .ok_or("Metadata history input retired")?,
    }))
}
pub(crate) fn request(form: &Form) -> Result<Arc<MetadataHistoryRequestV25>, String> {
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
    Ok(Arc::new(MetadataHistoryRequestV25 {
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
    history: &MetadataHistoryV25,
    current: &MetadataRecoveryCurrentV25,
) -> GithubResult<Vec<String>> {
    let mut lines = vec![
        "ORIGINAL METADATA HISTORY • inspect only; no body restoration or resend".into(),
        format!(
            "{} • {} bounded original rows • {}",
            history.field.label(),
            history.rows.len(),
            if history.complete {
                "complete observed window"
            } else {
                "older records omitted"
            }
        ),
        format!(
            "Current H {} • base {} • fresh observation, not causal proof",
            current.actual().observed().parent().head_sha(),
            current.actual().observed().parent().base_tip_sha()
        ),
        format!("Currently observed {}:", history.field.label()),
    ];
    for value in current.actual().observed().value().lines() {
        line(&mut lines, crate::git::display_bytes(value.as_bytes()))?;
    }
    for row in &history.rows {
        line(
            &mut lines,
            format!(
                "Attempt #{} • {:?} • historical H {} (not current authority)",
                row.marker.attempt_id, row.marker.state, row.marker.observed_head
            ),
        )?;
        line(
            &mut lines,
            format!(
                "Original requester {} • account {} • PR author account {}",
                crate::git::display_bytes(row.origin.seed.observed_requester().as_bytes()),
                row.origin.seed.scope.login,
                row.origin
                    .seed
                    .author_uid()
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "unavailable".into())
            ),
        )?;
        line(
            &mut lines,
            format!(
                "Historical base {} • exact ref {}",
                row.origin.seed.historical_base(),
                row.marker.payload_sha256
            ),
        )?;
        if let Some((_, values)) = &row.body {
            line(
                &mut lines,
                "Protected requested value (NOT editable/restored):".into(),
            )?;
            for value in values["body"].lines() {
                line(&mut lines, crate::git::display_bytes(value.as_bytes()))?;
            }
        } else {
            line(
                &mut lines,
                "Original body absent; reference retained without inference".into(),
            )?;
        }
    }
    line(&mut lines,"Explicit LOCAL ACK only releases the exact inspected uncertainty; never publishes, deletes a draft, restores approval or proves causality.".into())?;
    Ok(lines)
}
pub(crate) fn open(
    original: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    field: FieldKind,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    expected: Option<&MetadataHistoryRequestV25>,
) -> GithubResult<Form> {
    let ((snapshot, lines, display, mut form, current, author), identity) =
        source::with_metadata_history_v25(
            original.clone(),
            field,
            provider,
            database,
            cancel,
            deadline,
            |observation, boundary| {
                let actual = observation.actual();
                let parent = actual.observed().parent().clone();
                let target = PrWriteTarget::new(actual.identity(), parent.summary())
                    .map_err(|_| failure())?;
                let journal = PrWriteJournal::open(database).map_err(|_| failure())?;
                let snapshot = if let Some(expected) = expected {
                    journal
                        .validate_metadata_history_v25(&target, &parent, &expected.view.snapshot)
                        .map_err(|_| failure())?;
                    expected.view.snapshot.clone()
                } else {
                    Arc::new(
                        journal
                            .metadata_history_v25(&target, &parent, field)
                            .map_err(|_| failure())?,
                    )
                };
                ResourceLocalGuard::validate_metadata_history_v25(
                    database, boundary, &target, &parent, &snapshot,
                )?;
                let witness = expected
                    .map(|expected| {
                        let row = snapshot.rows.get(expected.row).ok_or_else(failure)?;
                        journal
                            .prepare_metadata_inspection_v25(
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
                    kind: field.form_kind(),
                    target: parent.summary().clone(),
                    identity: actual.identity().clone(),
                })
                .map_err(|_| failure())?;
                form.title = format!(
                    "Original {} history • {}#{}",
                    field.label(),
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
                ResourceLocalGuard::validate_metadata_history_v25(
                    database, boundary, target, parent, snapshot,
                )?;
                if let Some(witness) = witness {
                    PrWriteJournal::open(database)
                        .map_err(|_| failure())?
                        .validate_metadata_inspection_v25(witness)
                        .map_err(|_| failure())?;
                }
                provider.validate_metadata_inspection_v25(current, cancel, deadline)
            },
        )
        .and_then(
            |((snapshot, lines, display, form, observation, witness, author, _, _), identity)| {
                provider.validate_metadata_inspection_v25(&observation, cancel, deadline)?;
                let current = witness.map(|witness| {
                    Arc::new(MetadataInspectionV25 {
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
    let view = Arc::new(MetadataHistoryViewV25 {
        original,
        field,
        snapshot,
        identity,
        key: format!("metadata-history-v25/{}", field.key()).into(),
        lines,
        display,
        author,
        current,
    });
    form.install_prepared_preview_display_v1(view.display.clone());
    form.purpose = FormPurpose::PrCollaborationV17(Box::new(ConversationComposerStateV17 {
        mode: ComposerModeV17::MetadataHistoryV25(MetadataHistoryStateV25 {
            view,
            ticket: Arc::new(()),
        }),
    }));
    Ok(form)
}
pub(crate) fn acknowledge(
    request: &MetadataHistoryRequestV25,
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
        .acknowledge_metadata_inspection_v25(provider, witness, observation, cancel, deadline)
        .map_err(|_| failure())?;
    Ok((current.reference.clone(), changed))
}
