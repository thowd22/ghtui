//! Protected manual-link history on the original Collaboration Form/worker.
//! Inspect never restores an editable body or manufactures a new Source.
use super::*;
use crate::{
    pr_write_journal::{
        issue_link_origin_v87::history_v87::{IssueLinkHistoryV87, IssueLinkRecoveryWitnessV87},
        PrPublicationReference, PrWriteJournal, PrWriteState, PrWriteTarget,
    },
    providers::gh::IssueLinkRecoveryCurrentV87,
    scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard,
};
use std::sync::{atomic::AtomicBool, Mutex};

pub(crate) struct IssueHistoryViewV87 {
    pub(crate) original: Arc<PrCollaborationOpenIntentV17>,
    pub(crate) command: IssueLinkCommandV87,
    pub(crate) snapshot: Arc<IssueLinkHistoryV87>,
    pub(crate) identity: HostIdentity,
    pub(crate) key: Arc<str>,
    pub(crate) lines: Arc<[String]>,
    pub(crate) display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    pub(crate) author: crate::scoped_pr_collaboration_draft_v17::ConversationActorV17,
    pub(crate) current: Option<Arc<IssueInspectionV87>>,
}
pub(crate) struct IssueInspectionV87 {
    pub(crate) cancel: CancellationToken,
    pub(crate) deadline: Instant,
    reference: PrPublicationReference,
    useful: AtomicBool,
    observation: Mutex<Option<(IssueLinkRecoveryWitnessV87, IssueLinkRecoveryCurrentV87)>>,
}
impl IssueInspectionV87 {
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
            && self.observation.lock().is_ok_and(|s| s.is_some())
    }
}
#[derive(Clone)]
pub(crate) struct IssueHistoryStateV87 {
    pub(crate) view: Arc<IssueHistoryViewV87>,
    ticket: Arc<()>,
}
pub(crate) struct IssueHistoryRequestV87 {
    pub(crate) view: Arc<IssueHistoryViewV87>,
    row: usize,
    ticket: Arc<()>,
    presentation: Arc<()>,
    epoch: u64,
}
pub(crate) struct IssueHistoryOpenRequestV87 {
    pub(crate) original: Arc<PrCollaborationOpenIntentV17>,
    pub(crate) command: IssueLinkCommandV87,
    ticket: Arc<()>,
    presentation: Arc<()>,
    epoch: u64,
    model: usize,
    prepared: usize,
}
pub(crate) enum IssueHistoryAdmissionV87<'a> {
    Current,
    Form(&'a IssueHistoryOpenRequestV87),
    Inspection(&'a IssueHistoryRequestV87),
}
pub(crate) fn state(form: &Form) -> Option<&IssueHistoryStateV87> {
    match &form.purpose {
        FormPurpose::PrCollaborationV17(s) => match &s.mode {
            ComposerModeV17::IssueHistoryV87(v) => Some(v),
            _ => None,
        },
        _ => None,
    }
}
impl IssueHistoryRequestV87 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        state(form).is_some_and(|s| {
            Arc::ptr_eq(&s.view, &self.view) && Arc::ptr_eq(&s.ticket, &self.ticket)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        self.owns(form) && form.presentation_v4().1 == Some(self.epoch) && form.fields.is_empty()
    }
}
impl IssueHistoryOpenRequestV87 {
    pub(crate) fn matches(&self, form: &Form) -> bool {
        super::state(form).is_some_and(|s| {
            Arc::ptr_eq(s.intent.source.opening(), &self.original)
                && s.intent.command == self.command
                && Arc::ptr_eq(&s.ticket, &self.ticket)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
            && form.presentation_v4().1 == Some(self.epoch)
            && form.publication.is_some()
    }
}
pub(crate) fn capture_open(form: &Form) -> Result<Arc<IssueHistoryOpenRequestV87>, String> {
    let s = super::state(form).ok_or("Not original manual Issue Form")?;
    if form.publication.is_none() {
        return Err("Exact protected Issue reference required".into());
    }
    let model = s
        .intent
        .catalogue
        .retained_bound_v87()
        .checked_add(
            s.preview
                .as_ref()
                .map_or(0, |p| p.actual.retained_bound_v87()),
        )
        .ok_or("Issue history inherited model overflow")?;
    // Picker's existing <=2MiB prepared bound remains live in the old Form;
    // shared Arc models above are not copied or discounted by this ticket.
    let prepared = (2 * 1024 * 1024usize)
        .checked_add(s.intent.lines.iter().map(String::capacity).sum::<usize>())
        .and_then(|n| {
            n.checked_add(s.preview.as_ref().map_or(0, |p| {
                p.display.retained_bytes() + p.lines.iter().map(String::capacity).sum::<usize>()
            }))
        })
        .and_then(|n| {
            n.checked_add(
                form.fields
                    .iter()
                    .map(|f| f.editor.text.capacity() + std::mem::size_of_val(f))
                    .sum::<usize>(),
            )
        })
        .filter(|n| *n <= 8 * 1024 * 1024)
        .ok_or("Issue history inherited prepared bound overflow")?;
    Ok(Arc::new(IssueHistoryOpenRequestV87 {
        original: Arc::clone(s.intent.source.opening()),
        command: s.intent.command,
        ticket: Arc::clone(&s.ticket),
        presentation: Arc::clone(form.presentation_v4().0),
        epoch: form.presentation_v4().1.ok_or("Issue Form retired")?,
        model,
        prepared,
    }))
}
pub(crate) fn request(form: &Form) -> Result<Arc<IssueHistoryRequestV87>, String> {
    let s = state(form).ok_or("Not original Issue history")?;
    let mut unresolved = s.view.snapshot.rows.iter().enumerate().filter(|(_, r)| {
        matches!(
            r.marker.state,
            PrWriteState::Pending | PrWriteState::Uncertain
        )
    });
    let row = unresolved
        .next()
        .map(|(i, _)| i)
        .ok_or("No unresolved Issue blocker; history remains read only")?;
    if unresolved.next().is_some() || !form.fields.is_empty() {
        return Err("Exact single Issue blocker required".into());
    }
    Ok(Arc::new(IssueHistoryRequestV87 {
        view: Arc::clone(&s.view),
        row,
        ticket: Arc::clone(&s.ticket),
        presentation: Arc::clone(form.presentation_v4().0),
        epoch: form.presentation_v4().1.ok_or("History retired")?,
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
fn lines(
    history: &IssueLinkHistoryV87,
    current: Option<&IssueLinkRecoveryCurrentV87>,
    inherited: usize,
) -> GithubResult<Vec<String>> {
    let mut bytes = 0usize;
    for row in &history.rows {
        bytes = bytes
            .checked_add(
                1024 + row.marker.selected_resource.len()
                    + row.marker.observed_head.len()
                    + row.marker.payload_sha256.len(),
            )
            .and_then(|n| n.checked_add(row.body.as_ref().map_or(0, |(body, _)| body.len())))
            .ok_or_else(failure)?;
    }
    // Actual source-line construction plus sanitizer/Arc text, two immutable
    // row-index layouts and maximum in-progress Vec overlap. A physical row
    // consumes >=one UTF8 byte or one empty source line;64x bytes +512/line
    // dominates all actual Rust row/header/move layouts before allocation.
    let rows = history
        .rows
        .len()
        .checked_mul(8)
        .and_then(|n| n.checked_add(16))
        .ok_or_else(failure)?;
    history
        .retained_bound_v87()
        .checked_add(current.map_or(0, |c| c.actual().retained_bound_v87()))
        .and_then(|n| n.checked_add(inherited))
        .and_then(|n| n.checked_add(bytes.checked_mul(64)?))
        .and_then(|n| n.checked_add(rows.checked_mul(512)?))
        .filter(|n| *n <= 8 * 1024 * 1024)
        .ok_or_else(failure)?;
    let mut lines = vec![
        "ORIGINAL MANUAL ISSUE-LINK HISTORY • inspect, never resend".into(),
        "Saved selections, receipt IDs and old permissions are NOT editable/restored approval."
            .into(),
        format!(
            "{} bounded original rows • {}",
            history.rows.len(),
            if history.complete {
                "complete observed window"
            } else {
                "older rows omitted"
            }
        ),
    ];
    if let Some(c) = current {
        let other = history
            .retained_bound_v87()
            .checked_add(inherited)
            .and_then(|n| n.checked_add(bytes.checked_mul(64)?))
            .and_then(|n| n.checked_add(rows.checked_mul(512)?))
            .ok_or_else(failure)?;
        lines.extend(c.prepare_inspection_lines_v87(other)?);
    }
    for row in &history.rows {
        lines.push(format!(
            "Attempt #{} • {} • {:?}",
            row.marker.attempt_id,
            history.command.label(),
            row.marker.state
        ));
        lines.push(format!(
            "Historical H {} • exact reference {}",
            row.marker.observed_head, row.marker.payload_sha256
        ));
        lines.push(format!(
            "Original draft key {}",
            row.marker.selected_resource
        ));
        if let Some((_, values)) = &row.body {
            lines.push(format!(
                "Protected requested Issue identity (NOT editable): {}",
                crate::git::display_bytes(values["body"].as_bytes())
            ));
        } else {
            lines.push("Original body absent; exact receipt remains read only".into());
        }
    }
    lines.push("LOCAL ACK releases only this exact freshly inspected PR+Issue uncertainty. It sends no mutation, deletes no draft and grants no retry.".into());
    Ok(lines)
}
pub(crate) fn open(
    original: Arc<PrCollaborationOpenIntentV17>,
    command: IssueLinkCommandV87,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    admission: IssueHistoryAdmissionV87<'_>,
) -> GithubResult<Form> {
    let (expected, inherited, prepared) = match admission {
        IssueHistoryAdmissionV87::Current => (None, 0, 0),
        IssueHistoryAdmissionV87::Form(from) => (None, from.model, from.prepared),
        IssueHistoryAdmissionV87::Inspection(request) => {
            let view = &request.view;
            let observed = if let Some(c) = &view.current {
                c.observation
                    .lock()
                    .map_err(|_| failure())?
                    .as_ref()
                    .map_or(0, |(_, o)| o.actual().retained_bound_v87())
            } else {
                0
            };
            let model = view
                .snapshot
                .retained_bound_v87()
                .checked_add(observed)
                .ok_or_else(failure)?;
            let prepared = view
                .display
                .retained_bytes()
                .checked_add(view.lines.iter().map(String::capacity).sum::<usize>())
                .ok_or_else(failure)?;
            (Some(request), model, prepared)
        }
    };
    let expected_witness = expected
        .map(|r| {
            PrWriteJournal::open(database)
                .and_then(|j| {
                    j.prepare_issue_link_inspection_v87(
                        &r.view.snapshot.rows[r.row].marker.target,
                        &r.view.snapshot.rows[r.row].marker.publication_reference(),
                    )
                })
                .map_err(|_| failure())
        })
        .transpose()?;
    let ((snapshot, lines, display, mut form, current, author), identity) =
        issue_links_phase_v87::with_issue_link_history_v87(
            issue_links_phase_v87::IssueLinkHistoryOpenV87 {
                opening: Arc::clone(&original),
                inspect: expected_witness.as_ref(),
                inherited,
            },
            provider,
            database,
            cancel,
            deadline,
            |parent, identity, boundary, current| {
                let target =
                    PrWriteTarget::new(identity, parent.summary()).map_err(|_| failure())?;
                let journal = PrWriteJournal::open(database).map_err(|_| failure())?;
                let snapshot = if let Some(expected) = expected {
                    if expected.view.command != command
                        || !Arc::ptr_eq(&expected.view.original, &original)
                    {
                        return Err(failure());
                    }
                    journal
                        .validate_issue_link_history_v87(&target, parent, &expected.view.snapshot)
                        .map_err(|_| failure())?;
                    Arc::clone(&expected.view.snapshot)
                } else {
                    Arc::new(
                        journal
                            .issue_link_history_with_overlap_v87(
                                &target,
                                parent,
                                command,
                                inherited
                                    .checked_add(parent.retained_heap_bytes_v2())
                                    .and_then(|n| {
                                        n.checked_add(
                                            current
                                                .as_ref()
                                                .map_or(0, |c| c.actual().retained_bound_v87()),
                                        )
                                    })
                                    .ok_or_else(failure)?,
                            )
                            .map_err(|_| failure())?,
                    )
                };
                ResourceLocalGuard::validate_issue_link_history_v87(
                    database, boundary, &target, parent, &snapshot,
                )?;
                let lines: Arc<[String]> = lines(
                    &snapshot,
                    current.as_ref(),
                    inherited.checked_add(prepared).ok_or_else(failure)?,
                )?
                .into();
                let effective = current
                    .as_ref()
                    .map_or(deadline, |c| deadline.min(c.deadline()));
                let display = crate::prepared_form_text_v1::PreparedFormTextV1::prepare(
                    &lines, cancel, effective,
                )
                .map_err(|_| failure())?;
                if display
                    .preparation_peak_bound()
                    .checked_add(snapshot.retained_bound_v87())
                    .and_then(|n| n.checked_add(inherited))
                    .and_then(|n| n.checked_add(prepared))
                    .is_none_or(|n| n > 8 * 1024 * 1024)
                {
                    return Err(failure());
                }
                let author = crate::scoped_pr_collaboration_draft_v17::prepare_actor_v17(
                    &identity.host,
                    &parent.summary().pr.author,
                    parent.author_account_id(),
                    cancel,
                    effective,
                )?;
                let mut form = crate::scoped_pr_collaboration_history_v17::form(parent, identity)?;
                form.title = format!(
                    "Original manual Issue history • {}#{}",
                    parent.repository(),
                    parent.number()
                );
                Ok((
                    snapshot,
                    lines,
                    display,
                    form,
                    current,
                    author,
                    target,
                    Arc::clone(parent),
                ))
            },
            |(snapshot, _, _, _, current, _, target, parent), boundary| {
                ResourceLocalGuard::validate_issue_link_history_v87(
                    database, boundary, target, parent, snapshot,
                )?;
                if let Some(w) = &expected_witness {
                    PrWriteJournal::open(database)
                        .map_err(|_| failure())?
                        .validate_issue_link_inspection_v87(w)
                        .map_err(|_| failure())?;
                }
                if let Some(c) = current {
                    provider.validate_issue_link_inspection_v87(c, cancel, deadline)?;
                }
                Ok(())
            },
        )
        .and_then(
            |((snapshot, lines, display, form, current, author, _, _), identity)| {
                let current = match (current, expected_witness) {
                    (Some(c), Some(w)) => {
                        provider.validate_issue_link_inspection_v87(&c, cancel, deadline)?;
                        Some(Arc::new(IssueInspectionV87 {
                            cancel: c.cancellation().clone(),
                            deadline: deadline.min(c.deadline()),
                            reference: w.reference().clone(),
                            useful: AtomicBool::new(false),
                            observation: Mutex::new(Some((w, c))),
                        }))
                    }
                    (None, None) => None,
                    _ => return Err(failure()),
                };
                Ok(((snapshot, lines, display, form, current, author), identity))
            },
        )?;
    let view = Arc::new(IssueHistoryViewV87 {
        original,
        command,
        snapshot,
        identity,
        key: "manual-issue-history-v87".into(),
        lines,
        display,
        author,
        current,
    });
    form.install_prepared_preview_display_v1(Arc::clone(&view.display));
    form.purpose = FormPurpose::PrCollaborationV17(Box::new(ConversationComposerStateV17 {
        mode: ComposerModeV17::IssueHistoryV87(IssueHistoryStateV87 {
            view,
            ticket: Arc::new(()),
        }),
    }));
    Ok(form)
}
pub(crate) fn acknowledge(
    request: &IssueHistoryRequestV87,
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
    let (witness, observed) = current
        .observation
        .lock()
        .map_err(|_| failure())?
        .take()
        .ok_or_else(failure)?;
    let changed = PrWriteJournal::open(database)
        .map_err(|_| failure())?
        .acknowledge_issue_link_inspection_v87(provider, witness, observed, cancel, deadline)
        .map_err(|_| failure())?;
    Ok((current.reference.clone(), changed))
}
