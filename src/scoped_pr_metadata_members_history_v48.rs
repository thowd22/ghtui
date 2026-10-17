//! Reachable all-ten protected history. List8 and Inspect10 use the SAME
//! original initial Core, never a synthetic Files/source or saved authority.
use super::*;
use crate::{
    pr_write_journal::{
        metadata_members_v48::history_v48::{
            MetadataMembersHistoryV48, MetadataMembersRecoveryWitnessV48,
        },
        PrPublicationReference, PrWriteJournal, PrWriteState, PrWriteTarget,
    },
    providers::gh::MetadataMembersRecoveryCurrentV48,
    scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard,
};
use std::sync::Mutex;
pub(crate) struct MembersHistoryViewV48 {
    pub(crate) original: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    pub(crate) snapshot: Arc<MetadataMembersHistoryV48>,
    pub(crate) identity: HostIdentity,
    pub(crate) key: Arc<str>,
    pub(crate) lines: Arc<[String]>,
    pub(crate) display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    pub(crate) author: crate::scoped_pr_collaboration_draft_v17::ConversationActorV17,
    pub(crate) current: Option<Arc<MembersInspectionV48>>,
}
pub(crate) struct MembersInspectionV48 {
    pub(crate) cancel: CancellationToken,
    pub(crate) deadline: Instant,
    reference: PrPublicationReference,
    useful: AtomicBool,
    observation: Mutex<
        Option<(
            MetadataMembersRecoveryWitnessV48,
            MetadataMembersRecoveryCurrentV48,
        )>,
    >,
}
impl MembersInspectionV48 {
    pub(crate) fn reference(&self) -> &PrPublicationReference {
        &self.reference
    }
    pub(crate) fn painted(&self, useful: bool) {
        self.useful.store(useful, Ordering::Release);
    }
    pub(crate) fn ready(&self) -> bool {
        !self.cancel.is_cancelled()
            && Instant::now() < self.deadline
            && self.useful.load(Ordering::Acquire)
            && self.observation.lock().is_ok_and(|slot| slot.is_some())
    }
}
#[derive(Clone)]
pub(crate) struct MembersHistoryStateV48 {
    pub(crate) view: Arc<MembersHistoryViewV48>,
    ticket: Arc<()>,
}
pub(crate) struct MembersHistoryRequestV48 {
    pub(crate) view: Arc<MembersHistoryViewV48>,
    row: usize,
    ticket: Arc<()>,
    presentation: Arc<()>,
    epoch: u64,
}
pub(crate) struct MembersHistoryOpenRequestV48 {
    pub(crate) original: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    ticket: Arc<()>,
    presentation: Arc<()>,
    epoch: u64,
}
pub(crate) fn state(form: &Form) -> Option<&MembersHistoryStateV48> {
    match &form.purpose {
        FormPurpose::PrCollaborationV17(state) => match &state.mode {
            ComposerModeV17::MembersHistoryV48(state) => Some(state),
            _ => None,
        },
        _ => None,
    }
}
pub(in crate::scoped_pr_collaboration_ui_v17) struct HistoryCloseIdentityV296 {
    view: Arc<MembersHistoryViewV48>,
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
impl MembersHistoryRequestV48 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        state(form).is_some_and(|state| {
            Arc::ptr_eq(&state.view, &self.view) && Arc::ptr_eq(&state.ticket, &self.ticket)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        self.owns(form) && form.presentation_v4().1 == Some(self.epoch) && form.fields.is_empty()
    }
}
impl MembersHistoryOpenRequestV48 {
    pub(crate) fn matches(&self, form: &Form) -> bool {
        super::state(form).is_some_and(|state| {
            Arc::ptr_eq(state.intent.source.opening(), &self.original)
                && Arc::ptr_eq(&state.ticket, &self.ticket)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
            && form.presentation_v4().1 == Some(self.epoch)
            && form.publication.is_some()
    }
}
pub(crate) fn capture_open(form: &Form) -> Result<Arc<MembersHistoryOpenRequestV48>, String> {
    let state = super::state(form).ok_or("Not original member metadata Form")?;
    if form.publication.is_none() {
        return Err("Exact protected member reference required".into());
    }
    Ok(Arc::new(MembersHistoryOpenRequestV48 {
        original: state.intent.source.opening().clone(),
        ticket: state.ticket.clone(),
        presentation: form.presentation_v4().0.clone(),
        epoch: form.presentation_v4().1.ok_or("Member input retired")?,
    }))
}
pub(crate) fn request(form: &Form) -> Result<Arc<MembersHistoryRequestV48>, String> {
    let state = state(form).ok_or("Not original member history")?;
    let mut unresolved = state
        .view
        .snapshot
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            matches!(
                row.marker.state,
                PrWriteState::Pending | PrWriteState::Uncertain
            )
        });
    let row = unresolved
        .next()
        .map(|(i, _)| i)
        .ok_or("No unresolved member blocker; history remains read only")?;
    if unresolved.next().is_some() || !form.fields.is_empty() {
        return Err("Exact single member blocker required".into());
    }
    Ok(Arc::new(MembersHistoryRequestV48 {
        view: state.view.clone(),
        row,
        ticket: state.ticket.clone(),
        presentation: form.presentation_v4().0.clone(),
        epoch: form.presentation_v4().1.ok_or("History input retired")?,
    }))
}
pub(crate) fn ack_ready(form: &Form) -> bool {
    form.preview
        && !form.preview_display_v1.geometry_rejected
        && request(form).is_ok_and(|request| {
            request.view.current.as_ref().is_some_and(|current| {
                current.ready()
                    && current.reference
                        == request.view.snapshot.rows[request.row]
                            .marker
                            .publication_reference()
            })
        })
}
fn prepare_lines(
    history: &MetadataMembersHistoryV48,
    current: Option<&MetadataMembersRecoveryCurrentV48>,
) -> GithubResult<Vec<String>> {
    let mut lines=vec!["ORIGINAL MEMBER METADATA HISTORY • all ten purposes • inspect only".into(),
        "Saved bodies, references and old choices are NOT current permission, editable values or resend authority.".into(),
        format!("{} bounded rows • {}",history.rows.len(),if history.complete {"complete observed window"}else{"older rows omitted"})];
    if let Some(current) = current {
        lines.push(format!(
            "Fresh actual {} • H {} • base {} • observation, not causal proof",
            current.actual().command().label(),
            current.actual().parent().head_sha(),
            current.actual().parent().base_tip_sha()
        ));
        for member in crate::pr_metadata_actions::current_members(
            current.actual().read(),
            current.actual().command(),
        )
        .map_err(local_failure)?
        {
            lines.push(format!(
                "Currently observed {} • #{} • {}",
                crate::git::display_bytes(member.name().as_bytes()),
                member.id(),
                member.node()
            ));
        }
    }
    for row in &history.rows {
        lines.push(format!(
            "Attempt #{} • {} • {:?}",
            row.marker.attempt_id,
            row.origin.seed.command().map_err(|_| failure())?.label(),
            row.marker.state
        ));
        lines.push(format!(
            "Historical H {} • exact ref {} • NOT current authority",
            row.marker.observed_head, row.marker.payload_sha256
        ));
        if let Some((_, values)) = &row.body {
            lines.push("Protected requested selection (NOT editable/restored):".into());
            for value in values["body"].lines() {
                lines.push(crate::git::display_bytes(value.as_bytes()));
            }
        } else {
            lines.push("Original saved body absent; exact reference remains inspect-only".into());
        }
    }
    lines.push("LOCAL ACK only releases this exact freshly inspected uncertainty; no write, draft deletion, restored approval or causal claim.".into());
    if lines.len() > 50_000
        || lines
            .iter()
            .try_fold(0usize, |n, line| n.checked_add(line.capacity()))
            .is_none_or(|n| n > 1024 * 1024)
    {
        return Err(failure());
    }
    Ok(lines)
}
pub(crate) fn open(
    original: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    expected: Option<&MembersHistoryRequestV48>,
) -> GithubResult<Form> {
    let inspect = expected
        .map(|request| {
            request.view.snapshot.rows[request.row]
                .origin
                .seed
                .command()
                .map_err(|_| failure())
        })
        .transpose()?;
    let ((snapshot, lines, display, mut form, current, author), identity) =
        phase::with_metadata_members_history_v48(
            phase::MetadataMembersHistoryOpenV48 {
                opening: original.clone(),
                inspect,
            },
            provider,
            database,
            cancel,
            deadline,
            |parent, identity, boundary, observed| {
                let target =
                    PrWriteTarget::new(identity, parent.summary()).map_err(|_| failure())?;
                let journal = PrWriteJournal::open(database).map_err(|_| failure())?;
                let snapshot = if let Some(expected) = expected {
                    journal
                        .validate_metadata_members_history_v48(
                            &target,
                            parent,
                            &expected.view.snapshot,
                        )
                        .map_err(|_| failure())?;
                    expected.view.snapshot.clone()
                } else {
                    Arc::new(
                        journal
                            .metadata_members_history_v48(&target, parent)
                            .map_err(|_| failure())?,
                    )
                };
                ResourceLocalGuard::validate_metadata_members_history_v48(
                    database, boundary, &target, parent, &snapshot,
                )?;
                let witness = expected
                    .map(|request| {
                        journal
                            .prepare_metadata_members_inspection_v48(
                                &target,
                                &snapshot.rows[request.row].marker.publication_reference(),
                            )
                            .map_err(|_| failure())
                    })
                    .transpose()?;
                let lines: Arc<[String]> = prepare_lines(&snapshot, observed.as_ref())?.into();
                let effective = observed
                    .as_ref()
                    .map_or(deadline, |value| deadline.min(value.deadline()));
                let display = crate::prepared_form_text_v1::PreparedFormTextV1::prepare(
                    &lines, cancel, effective,
                )
                .map_err(|_| failure())?;
                let author = crate::scoped_pr_collaboration_draft_v17::prepare_actor_v17(
                    &identity.host,
                    &parent.summary().pr.author,
                    parent.author_account_id(),
                    cancel,
                    effective,
                )?;
                let mut form = crate::scoped_pr_collaboration_history_v17::form(parent, identity)?;
                form.title = format!(
                    "Original member metadata history • {}#{}",
                    parent.repository(),
                    parent.number()
                );
                Ok((
                    snapshot,
                    lines,
                    display,
                    form,
                    observed,
                    witness,
                    author,
                    target,
                    parent.clone(),
                ))
            },
            |(snapshot, _, _, _, observed, witness, _, target, parent), boundary| {
                ResourceLocalGuard::validate_metadata_members_history_v48(
                    database, boundary, target, parent, snapshot,
                )?;
                if let Some(witness) = witness {
                    PrWriteJournal::open(database)
                        .map_err(|_| failure())?
                        .validate_metadata_members_inspection_v48(witness)
                        .map_err(|_| failure())?;
                }
                if let Some(observed) = observed {
                    provider
                        .validate_metadata_members_inspection_v48(observed, cancel, deadline)?;
                }
                Ok(())
            },
        )
        .and_then(
            |((snapshot, lines, display, form, observed, witness, author, _, _), identity)| {
                let current = match (observed, witness) {
                    (Some(observed), Some(witness)) => {
                        provider.validate_metadata_members_inspection_v48(
                            &observed, cancel, deadline,
                        )?;
                        Some(Arc::new(MembersInspectionV48 {
                            cancel: observed.cancellation().clone(),
                            deadline: deadline.min(observed.deadline()),
                            reference: witness.origin.reference.clone(),
                            useful: AtomicBool::new(false),
                            observation: Mutex::new(Some((witness, observed))),
                        }))
                    }
                    (None, None) => None,
                    _ => return Err(failure()),
                };
                Ok(((snapshot, lines, display, form, current, author), identity))
            },
        )?;
    let view = Arc::new(MembersHistoryViewV48 {
        original,
        snapshot,
        identity,
        key: "metadata-members-history-v48".into(),
        lines,
        display,
        author,
        current,
    });
    form.install_prepared_preview_display_v1(view.display.clone());
    form.purpose = FormPurpose::PrCollaborationV17(Box::new(ConversationComposerStateV17 {
        mode: ComposerModeV17::MembersHistoryV48(MembersHistoryStateV48 {
            view,
            ticket: Arc::new(()),
        }),
    }));
    Ok(form)
}
pub(crate) fn acknowledge(
    request: &MembersHistoryRequestV48,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<(PrPublicationReference, usize)> {
    let current = request
        .view
        .current
        .as_ref()
        .filter(|current| current.ready())
        .ok_or_else(failure)?;
    if !cancel.same_instance(&current.cancel) || deadline > current.deadline {
        return Err(failure());
    }
    let (witness, observed) = current
        .observation
        .lock()
        .map_err(|_| failure())?
        .take()
        .ok_or_else(failure)?;
    let changed = PrWriteJournal::open(database)
        .map_err(|_| failure())?
        .acknowledge_metadata_members_inspection_v48(provider, witness, observed, cancel, deadline)
        .map_err(|_| failure())?;
    Ok((current.reference.clone(), changed))
}
