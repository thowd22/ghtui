//! Source-less actual State history and useful explicit LOCAL ACK. Historical
//! transitions and protected bodyless requests cannot become another write.
use super::*;
use crate::{
    pr_write_journal::{
        state_origin_v68::history_v74::{StateHistoryV74, StateRecoveryWitnessV74},
        PrWriteState, PrWriteTarget,
    },
    providers::gh::StateRecoveryCurrentV68,
    scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard,
};
use std::sync::Mutex;
pub(crate) struct StateHistoryViewV74 {
    pub(crate) original: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    pub(crate) command: PrStateCommand,
    pub(crate) snapshot: Arc<StateHistoryV74>,
    pub(crate) identity: HostIdentity,
    pub(crate) key: Arc<str>,
    pub(crate) title: String,
    pub(crate) author: crate::scoped_pr_collaboration_draft_v17::ConversationActorV17,
    pub(crate) lines: Arc<[String]>,
    pub(crate) display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    pub(crate) current: Option<Arc<StateInspectionV74>>,
    /// Display-only shared semantic ink per history row (TASK-25 AC6).
    pub(crate) states_v25: Arc<[(usize, crate::theme::SemanticToken)]>,
}
/// The shared ink for a recorded State attempt; its text label stays in the row.
pub(crate) fn attempt_token_v25(state: PrWriteState) -> crate::theme::SemanticToken {
    use crate::theme::SemanticToken;
    match state {
        PrWriteState::Pending => SemanticToken::Pending,
        PrWriteState::Uncertain => SemanticToken::Conflict,
        PrWriteState::Confirmed => SemanticToken::Success,
        PrWriteState::Acknowledged => SemanticToken::Muted,
    }
}
/// Rows of [`visit_lines`] that name a PR or commit state: the fresh
/// observed state (lifecycle ink), each attempt's status and its historical
/// head.
fn states_v25(
    snapshot: &StateHistoryV74,
    fresh: crate::theme::SemanticToken,
) -> Arc<[(usize, crate::theme::SemanticToken)]> {
    let mut states = vec![(2, fresh)];
    let mut row = 5;
    for attempt in &snapshot.rows {
        states.push((row, attempt_token_v25(attempt.marker.state)));
        states.push((row + 1, crate::theme::SemanticToken::Head));
        row += 3 + usize::from(attempt.marker.confirmed.is_some());
    }
    states.into()
}
/// TASK-25 AC6: State history rows keep their text labels and take the
/// shared semantic ink. Display only; it grants nothing.
pub(crate) fn paint_v25(form: &Form) -> Option<crate::pr_actions::PrPreviewPaintV1> {
    let view = &state(form)?.view;
    Some(crate::pr_actions::PrPreviewPaintV1 {
        host: view.identity.host.clone(),
        actors: Vec::new(),
        states: view.states_v25.to_vec(),
    })
}
pub(crate) struct StateInspectionV74 {
    pub(crate) cancel: CancellationToken,
    pub(crate) deadline: Instant,
    reference: PrPublicationReference,
    useful: AtomicBool,
    observation: Mutex<Option<(StateRecoveryWitnessV74, StateRecoveryCurrentV68)>>,
}
impl StateInspectionV74 {
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
pub(crate) struct StateHistoryStateV74 {
    pub(crate) view: Arc<StateHistoryViewV74>,
    ticket: Arc<()>,
}
pub(crate) struct StateHistoryRequestV74 {
    pub(crate) view: Arc<StateHistoryViewV74>,
    row: usize,
    ticket: Arc<()>,
    presentation: Arc<()>,
    epoch: u64,
}
pub(crate) struct StateHistoryOpenRequestV74 {
    pub(crate) original: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    pub(crate) command: PrStateCommand,
    intent: Arc<StateIntentV68>,
    ticket: Arc<()>,
    presentation: Arc<()>,
    epoch: u64,
    reference: Option<PrPublicationReference>,
}
impl StateHistoryOpenRequestV74 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        super::state(form).is_some_and(|s| {
            Arc::ptr_eq(&s.intent, &self.intent) && Arc::ptr_eq(&s.ticket, &self.ticket)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        self.owns(form)
            && form.presentation_v4().1 == Some(self.epoch)
            && form.publication == self.reference
            && form.fields.is_empty()
    }
}
pub(crate) fn capture_open(form: &Form) -> Result<Arc<StateHistoryOpenRequestV74>, String> {
    let state = super::state(form).ok_or("Not protected State intent")?;
    if form.publication.is_none() || !form.fields.is_empty() {
        return Err("Exact protected State reference required".into());
    }
    Ok(Arc::new(StateHistoryOpenRequestV74 {
        original: Arc::clone(state.intent.source.opening()),
        command: state.intent.command,
        intent: Arc::clone(&state.intent),
        ticket: Arc::clone(&state.ticket),
        presentation: Arc::clone(form.presentation_v4().0),
        epoch: form
            .presentation_v4()
            .1
            .ok_or("State intent input retired")?,
        reference: form.publication.clone(),
    }))
}
pub(crate) fn state(form: &Form) -> Option<&StateHistoryStateV74> {
    match &form.purpose {
        FormPurpose::PrCollaborationV17(s) => match &s.mode {
            ComposerModeV17::StateHistoryV74(v) => Some(v),
            _ => None,
        },
        _ => None,
    }
}
impl StateHistoryRequestV74 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        state(form).is_some_and(|s| {
            Arc::ptr_eq(&s.view, &self.view) && Arc::ptr_eq(&s.ticket, &self.ticket)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        self.owns(form) && form.presentation_v4().1 == Some(self.epoch) && form.fields.is_empty()
    }
    pub(crate) fn inspected_reference(&self) -> Option<&PrPublicationReference> {
        self.view.current.as_ref().map(|c| &c.reference)
    }
    pub(crate) fn matches_ack_v77(&self, form: &Form) -> bool {
        self.matches(form) && ack_ready(form)
    }
    /// Installed busy-owner DISPLAY only. The actual worker takes the one
    /// observation; that consumption cannot orphan its own completion. This
    /// never admits a work item and never omits identity/geometry/input checks.
    pub(crate) fn matches_inflight_ack_v77(&self, form: &Form) -> bool {
        self.matches(form)
            && form.submitting
            && form.preview
            && !form.preview_display_v1.geometry_rejected
            && self.view.current.as_ref().is_some_and(|current| {
                !current.cancel.is_cancelled()
                    && Instant::now() < current.deadline
                    && current.useful.load(Ordering::Acquire)
                    && self
                        .view
                        .snapshot
                        .rows
                        .get(self.row)
                        .is_some_and(|row| row.marker.publication_reference() == current.reference)
            })
    }
}
pub(crate) fn request(form: &Form) -> Result<Arc<StateHistoryRequestV74>, String> {
    let s = state(form).ok_or("Not original State history")?;
    let mut rows = s.view.snapshot.rows.iter().enumerate().filter(|(_, r)| {
        matches!(
            r.marker.state,
            PrWriteState::Pending | PrWriteState::Uncertain
        )
    });
    let row = rows
        .next()
        .map(|(i, _)| i)
        .ok_or("No supported original State blocker; history stays read only")?;
    if rows.next().is_some() || !form.fields.is_empty() {
        return Err("Exact single State blocker unavailable".into());
    }
    Ok(Arc::new(StateHistoryRequestV74 {
        view: Arc::clone(&s.view),
        row,
        ticket: Arc::clone(&s.ticket),
        presentation: Arc::clone(form.presentation_v4().0),
        epoch: form
            .presentation_v4()
            .1
            .ok_or("State history input retired")?,
    }))
}
pub(crate) fn ack_ready(form: &Form) -> bool {
    form.preview
        && !form.preview_display_v1.geometry_rejected
        && request(form).is_ok_and(|r| {
            r.view.current.as_ref().is_some_and(|c| {
                c.ready()
                    && c.reference == r.view.snapshot.rows[r.row].marker.publication_reference()
            })
        })
}
fn visit_lines(
    snapshot: &StateHistoryV74,
    current: &StateRecoveryCurrentV68,
    mut line: impl FnMut(std::fmt::Arguments<'_>) -> GithubResult<()>,
) -> GithubResult<()> {
    let actual = current.actual();
    let parent = actual.observed().parent();
    line(format_args!(
        "ORIGINAL PR STATE HISTORY • inspect only • no resend"
    ))?;
    line(format_args!(
        "{} • {} original rows • {}",
        snapshot.command.label(),
        snapshot.rows.len(),
        if snapshot.complete {
            "complete observed window"
        } else {
            "older records omitted"
        }
    ))?;
    line(format_args!(
        "Fresh observed state {} • head {} • base {}",
        parent.summary().pr.state,
        parent.head_sha(),
        parent.base_tip_sha()
    ))?;
    line(format_args!(
        "Actual PR #{} / numeric {} • independent issue #{}",
        parent.number(),
        parent.database_id(),
        actual.observed().issue_id()
    ))?;
    line(format_args!(
        "Current observation is not exclusive causal attribution or write permission."
    ))?;
    for row in &snapshot.rows {
        let status = match row.marker.state {
            PrWriteState::Pending => "Pending",
            PrWriteState::Uncertain => "Uncertain",
            PrWriteState::Confirmed => "Confirmed",
            PrWriteState::Acknowledged => "Locally acknowledged",
        };
        line(format_args!(
            "Attempt #{} • {} • {}",
            row.marker.attempt_id, status, row.marker.command
        ))?;
        line(format_args!(
            "Historical head {} • exact fingerprint {}",
            row.marker.observed_head, row.marker.payload_sha256
        ))?;
        line(format_args!(
            "{}",
            if row.body.is_some() {
                "Protected bodyless request/ref retained • inspect only"
            } else {
                "Original request absent • no reconstruction or replay"
            }
        ))?;
        if let Some(effect) = &row.marker.confirmed {
            line(format_args!("Recorded typed effect {}", effect.resource_id))?;
        }
    }
    line(format_args!("LOCAL ACK releases only this exact inspected uncertainty; no write, draft deletion, approval, source renewal or membership hydration."))
}
fn prepare_lines(
    snapshot: &StateHistoryV74,
    current: &StateRecoveryCurrentV68,
) -> GithubResult<Arc<[String]>> {
    use std::fmt::Write;
    struct Count(usize);
    impl std::fmt::Write for Count {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            self.0 = self.0.checked_add(text.len()).ok_or(std::fmt::Error)?;
            Ok(())
        }
    }
    // The first pass visits only actual borrowed typed facts. It bounds the
    // complete Vec/String/Arc envelope BEFORE allocating any rendered text.
    let mut count = 0usize;
    let mut text_bytes = 0usize;
    visit_lines(snapshot, current, |args| {
        let mut bytes = Count(0);
        bytes.write_fmt(args).map_err(|_| failure())?;
        text_bytes = text_bytes.checked_add(bytes.0).ok_or_else(failure)?;
        count = count.checked_add(1).ok_or_else(failure)?;
        Ok(())
    })?;
    let headers = count
        .checked_mul(std::mem::size_of::<String>())
        .and_then(|n| n.checked_mul(2))
        .and_then(|n| n.checked_add(2 * std::mem::size_of::<usize>()))
        .ok_or_else(failure)?;
    text_bytes
        .checked_add(headers)
        .filter(|n| *n <= 128 * 1024)
        .ok_or_else(failure)?;
    let mut lines = Vec::with_capacity(count);
    if lines.capacity() != count {
        return Err(failure());
    }
    visit_lines(snapshot, current, |args| {
        let mut bytes = Count(0);
        bytes.write_fmt(args).map_err(|_| failure())?;
        let mut text = String::with_capacity(bytes.0);
        if text.capacity() != bytes.0 || lines.len() == count {
            return Err(failure());
        }
        text.write_fmt(args).map_err(|_| failure())?;
        if text.len() != bytes.0 {
            return Err(failure());
        }
        lines.push(text);
        Ok(())
    })?;
    if lines.len() != count {
        return Err(failure());
    }
    Ok(lines.into())
}
pub(crate) fn open(
    request: source::state_v68::StateOpenRequestV68,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    expected: Option<&StateHistoryRequestV74>,
) -> GithubResult<Form> {
    let original = Arc::clone(&request.opening);
    let command = request.command;
    if expected
        .is_some_and(|e| e.view.command != command || !Arc::ptr_eq(&e.view.original, &original))
    {
        return Err(failure());
    }
    // The actual prior history row is inspect-only data, not a Source. Bind
    // its typed journal witness and original queue/Clear boundary BEFORE any
    // new Core read. Never sample an already-cleared epoch as new permission.
    let mut prior_witness = expected
        .map(|r| {
            let row = r.view.snapshot.rows.get(r.row).ok_or_else(failure)?;
            PrWriteJournal::open(database)
                .map_err(|_| failure())?
                .prepare_state_inspection_v74(
                    &row.marker.target,
                    &row.marker.publication_reference(),
                    &original,
                )
                .map_err(|_| failure())
        })
        .transpose()?;
    let inspection_cache = prior_witness
        .as_ref()
        .map(StateRecoveryWitnessV74::cache_guard_v78)
        .transpose()
        .map_err(|_| failure())?;
    let ((mut form, view), identity) = source::state_v68::with_history_v74(
        source::state_v68::StateHistoryReadV78 {
            request,
            inspection_cache: inspection_cache.as_ref(),
        },
        provider,
        database,
        cancel,
        deadline,
        |current, boundary| {
            let actual = current.actual();
            let parent = Arc::clone(actual.observed().parent());
            let actual_validation = Arc::clone(actual);
            let target =
                PrWriteTarget::new(actual.identity(), parent.summary()).map_err(|_| failure())?;
            let journal = PrWriteJournal::open(database).map_err(|_| failure())?;
            let snapshot = if let Some(expected) = expected {
                journal
                    .validate_state_history_v74(&target, &parent, &expected.view.snapshot)
                    .map_err(|_| failure())?;
                Arc::clone(&expected.view.snapshot)
            } else {
                Arc::new(
                    journal
                        .state_history_v74(&target, &parent, command)
                        .map_err(|_| failure())?,
                )
            };
            ResourceLocalGuard::validate_state_history_v74(
                database, boundary, &target, &parent, &snapshot,
            )?;
            let witness = prior_witness
                .take()
                .map(|mut witness| {
                    journal
                        .close_state_inspection_v78(&mut witness)
                        .map_err(|_| failure())?;
                    Ok::<_, GithubError>(witness)
                })
                .transpose()?;
            let lines = prepare_lines(&snapshot, &current)?;
            let states_v25 = states_v25(
                &snapshot,
                crate::pr_actions::pr_lifecycle_state_token(
                    &parent.summary().pr.state,
                    parent.summary().pr.draft,
                    parent.summary().merged,
                ),
            );
            let deadline = deadline.min(current.deadline());
            let display =
                crate::prepared_form_text_v1::PreparedFormTextV1::prepare(&lines, cancel, deadline)
                    .map_err(|_| failure())?;
            let author = crate::scoped_pr_collaboration_draft_v17::prepare_actor_v17(
                &actual.identity().host,
                &parent.summary().pr.author,
                parent.author_account_id(),
                cancel,
                deadline,
            )?;
            let identity = actual.identity().clone();
            let current = witness.map(|w| {
                Arc::new(StateInspectionV74 {
                    cancel: current.cancellation().clone(),
                    deadline,
                    reference: w.origin.reference.clone(),
                    useful: AtomicBool::new(false),
                    observation: Mutex::new(Some((w, current))),
                })
            });
            let view = Arc::new(StateHistoryViewV74 {
                original: Arc::clone(&original),
                command,
                snapshot,
                identity,
                key: format!("state-history-v74/{}", command.key()).into(),
                title: format!(
                    "Original {} history • {}#{}",
                    command.label(),
                    parent.repository(),
                    parent.number()
                ),
                author,
                lines,
                display,
                current,
                states_v25,
            });
            let form = Form::pr_state_history_v74(StateHistoryStateV74 {
                view: Arc::clone(&view),
                ticket: Arc::new(()),
            });
            Ok((form, view, target, parent, actual_validation))
        },
        |(_, view, target, parent, actual_validation), boundary| {
            ResourceLocalGuard::validate_state_history_v74(
                database,
                boundary,
                target,
                parent,
                &view.snapshot,
            )?;
            provider.validate_original_state_generation_v68(actual_validation, cancel, deadline)?;
            if let Some(current) = &view.current {
                let slot = current.observation.lock().map_err(|_| failure())?;
                let (witness, observation) = slot.as_ref().ok_or_else(failure)?;
                PrWriteJournal::open(database)
                    .map_err(|_| failure())?
                    .validate_state_inspection_v74(witness)
                    .map_err(|_| failure())?;
                provider.validate_state_inspection_v68(observation, cancel, deadline)?;
            }
            Ok(())
        },
    )
    .map(|((form, view, _, _, _), identity)| ((form, view), identity))?;
    if view.identity != identity {
        return Err(failure());
    }
    form.install_prepared_preview_display_v1(Arc::clone(&view.display));
    Ok(form)
}
pub(crate) fn acknowledge(
    request: &StateHistoryRequestV74,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<(PrPublicationReference, usize)> {
    let current = request
        .view
        .current
        .as_ref()
        .filter(|c| c.ready())
        .ok_or_else(failure)?;
    if !cancel.same_instance(&current.cancel) || deadline > current.deadline {
        return Err(failure());
    }
    let (witness, observation) = current
        .observation
        .lock()
        .map_err(|_| failure())?
        .take()
        .ok_or_else(failure)?;
    let count = PrWriteJournal::open(database)
        .map_err(|_| failure())?
        .acknowledge_state_inspection_v74(provider, witness, observation, cancel, deadline)
        .map_err(|_| failure())?;
    Ok((current.reference.clone(), count))
}
pub(crate) fn dispatch(form: &mut Form, action: &FormAction) -> bool {
    if state(form).is_none() {
        return false;
    }
    match action {
        FormAction::Preview => {
            form.preview_requested = true;
            true
        }
        FormAction::Submit => {
            if ack_ready(form) {
                form.submit_requested = true;
            } else {
                form.error = "Fresh useful exact State inspection required before LOCAL ACK".into();
            }
            true
        }
        FormAction::Editor | FormAction::NewIntent | FormAction::InspectConversationV17 => true,
        _ => false,
    }
}
