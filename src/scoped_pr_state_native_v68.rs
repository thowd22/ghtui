//! Closed State arms in the existing Collaboration work/result/owner slot.
//! No queue, fallback client or result-from-DTO authority is introduced.
use super::*;
use ui::state_v68 as state;

pub(super) enum StateWorkV68 {
    Open {
        original: Arc<PrCollaborationOpenIntentV17>,
        command: crate::pr_state_model::PrStateCommand,
        closed_draft_v20: Option<Arc<ui::pure_close_v20::ClosedDraftTicketV20>>,
    },
    Preview(Arc<state::StateRequestV68>),
    Confirm(Arc<state::StateRequestV68>),
    New(Arc<state::StateNewRequestV68>),
    History {
        original: Arc<PrCollaborationOpenIntentV17>,
        command: crate::pr_state_model::PrStateCommand,
        from: Option<Arc<state::history_v74::StateHistoryOpenRequestV74>>,
    },
    Inspect(Arc<state::history_v74::StateHistoryRequestV74>),
    Acknowledge(Arc<state::history_v74::StateHistoryRequestV74>),
}
pub(super) enum StateResultV68 {
    Open(Box<Form>),
    Preview(Arc<state::StateRequestV68>, Arc<state::StatePreviewV68>),
    SaveRefused(Arc<state::StateRequestV68>),
    New(Arc<state::StateNewRequestV68>, Arc<state::StatePreviewV68>),
    History(Box<Form>),
    Acknowledged(
        Arc<state::history_v74::StateHistoryRequestV74>,
        crate::pr_write_journal::PrPublicationReference,
        usize,
    ),
}
impl StateWorkV68 {
    pub(super) fn closed_draft_ticket_v75(
        &self,
    ) -> Option<&Arc<ui::pure_close_v20::ClosedDraftTicketV20>> {
        match self {
            Self::Open {
                closed_draft_v20, ..
            } => closed_draft_v20.as_ref(),
            _ => None,
        }
    }
    pub(super) fn matches_owned_inflight_v74(
        &self,
        owner: &ScopedPrCollaborationOwnerV17,
        app: &App,
    ) -> Option<bool> {
        if !matches!(self, Self::Confirm(_) | Self::Acknowledge(_)) {
            return None;
        }
        if !app
            .scoped_pr_collaboration_owner_v17
            .as_ref()
            .is_some_and(|current| std::ptr::eq(current.as_ref(), owner))
        {
            return None;
        }
        Some(
            !owner.expired()
                && app.view == View::Inbox
                && matches!(owner.original(),CollaborationWorkOriginV78::PrPage(original) if original.owns_presentation(app))
                && !app.form_dismissed
                && app.overlay == Some(Overlay::Form)
                && app.form.as_ref().is_some_and(|form| match self {
                    Self::Confirm(request) => request.matches_inflight_confirm_v74(form),
                    Self::Acknowledge(request) => request.matches_inflight_ack_v77(form),
                    _ => false,
                }),
        )
    }
    pub(super) fn phase(&self) -> ConversationPhaseKindV17 {
        match self {
            Self::Open { .. } => ConversationPhaseKindV17::Open,
            Self::Preview(_) => ConversationPhaseKindV17::Preview,
            Self::Confirm(_) => ConversationPhaseKindV17::Confirm,
            Self::New(_) => ConversationPhaseKindV17::NewAction,
            Self::History { .. } => ConversationPhaseKindV17::History,
            Self::Inspect(_) => ConversationPhaseKindV17::Inspect,
            Self::Acknowledge(_) => ConversationPhaseKindV17::Acknowledge,
        }
    }
    pub(super) fn original(&self) -> &Arc<PrCollaborationOpenIntentV17> {
        match self {
            Self::Open { original, .. } => original,
            Self::Preview(r) | Self::Confirm(r) => r.intent.source.opening(),
            Self::New(r) => r.original_request.intent.source.opening(),
            Self::History { original, .. } => original,
            Self::Inspect(r) | Self::Acknowledge(r) => &r.view.original,
        }
    }
    pub(super) fn owns_form(&self, form: &Form) -> bool {
        match self {
            Self::Open { .. } => false,
            Self::Preview(r) | Self::Confirm(r) => r.owns(form),
            Self::New(r) => r.owns(form),
            Self::History { from, .. } => from.as_ref().is_some_and(|r| r.owns(form)),
            Self::Inspect(r) | Self::Acknowledge(r) => r.owns(form),
        }
    }
    pub(super) fn matches_retiring_confirm_v74(&self, form: &Form) -> bool {
        match self {
            Self::Confirm(request) => request.matches(form),
            _ => false,
        }
    }
    pub(super) fn matches(&self, app: &App) -> bool {
        match self {
            Self::Open { .. } => app.form.is_none() && app.overlay.is_none(),
            Self::History { from: None, .. } => app.form.is_none() && app.overlay.is_none(),
            _ => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|form| match self {
                        Self::Preview(r) => r.matches(form),
                        Self::Confirm(r) => r.matches_confirm(form),
                        Self::New(r) => r.matches(form),
                        Self::Open { .. } => false,
                        Self::History { from, .. } => {
                            from.as_ref().is_some_and(|r| r.matches(form))
                        }
                        Self::Inspect(r) => r.matches(form),
                        Self::Acknowledge(r) => r.matches_ack_v77(form),
                    })
            }
        }
    }
    pub(super) fn confirm_key(&self) -> Option<&str> {
        match self {
            Self::Confirm(r) => Some(&r.key),
            _ => None,
        }
    }
    pub(super) fn result_matches(&self, result: &ResultKindV17) -> bool {
        match (self, result) {
            (_, ResultKindV17::Failure { .. }) => true,
            (Self::Open { .. }, ResultKindV17::LocalDraftReadFailureV20(_)) => true,
            (
                Self::Open {
                    original, command, ..
                },
                ResultKindV17::StateV68(StateResultV68::Open(form)),
            ) => {
                state::state(form).is_some_and(|s| {
                    s.intent.command == *command && Arc::ptr_eq(s.intent.source.opening(), original)
                }) || state::history_v74::state(form).is_some_and(|s| {
                    s.view.command == *command && Arc::ptr_eq(&s.view.original, original)
                })
            }
            (
                Self::Preview(expected),
                ResultKindV17::StateV68(StateResultV68::Preview(request, preview)),
            ) => {
                Arc::ptr_eq(expected, request)
                    && Arc::ptr_eq(&request.intent, &preview.intent)
                    && Arc::ptr_eq(&request.values, &preview.values)
            }
            (
                Self::Preview(expected),
                ResultKindV17::StateV68(StateResultV68::SaveRefused(request)),
            ) => Arc::ptr_eq(expected, request),
            (
                Self::New(expected),
                ResultKindV17::StateV68(StateResultV68::New(request, preview)),
            ) => Arc::ptr_eq(expected, request) && preview.is_new_result_v68(request),
            (Self::Confirm(request), ResultKindV17::Publication(outcome)) => {
                request.preview.as_ref().is_some_and(|preview| {
                    preview
                        .seed
                        .write(&preview.values)
                        .and_then(|w| w.digest())
                        .is_ok_and(|hash| hash == outcome.reference.payload_sha256)
                })
            }
            (
                Self::History {
                    original, command, ..
                },
                ResultKindV17::StateV68(StateResultV68::History(form)),
            ) => state::history_v74::state(form).is_some_and(|s| {
                Arc::ptr_eq(&s.view.original, original) && s.view.command == *command
            }),
            (Self::Inspect(request), ResultKindV17::StateV68(StateResultV68::History(form))) => {
                state::history_v74::state(form).is_some_and(|s| {
                    Arc::ptr_eq(&s.view.original, &request.view.original)
                        && Arc::ptr_eq(&s.view.snapshot, &request.view.snapshot)
                        && s.view.command == request.view.command
                })
            }
            (
                Self::Acknowledge(expected),
                ResultKindV17::StateV68(StateResultV68::Acknowledged(request, reference, count)),
            ) => {
                Arc::ptr_eq(expected, request)
                    && request.inspected_reference() == Some(reference)
                    && *count == 1
            }
            _ => false,
        }
    }
    pub(super) fn execute(
        &self,
        owner: &ScopedPrCollaborationOwnerV17,
        provider: &GithubProvider,
        database: &Path,
        drafts: &DraftClient,
    ) -> crate::providers::gh::GithubResult<ResultKindV17> {
        let cancel = owner.cancellation();
        let deadline = owner.deadline;
        match self {
            Self::Open {
                original, command, ..
            } => {
                let cutoff = owner
                    .open_cutoff_v20
                    .lock()
                    .ok()
                    .and_then(|c| c.clone())
                    .ok_or_else(state::failure)?;
                if let Err(e) = drafts.wait_conversation_open_cutoff_v20(&cutoff, cancel) {
                    if e == ReadCutoffFailureV20::SaveFailed {
                        if let Some(ticket) = self.closed_draft_ticket_v75() {
                            ui::pure_close_v20::validate_local_recovery(
                                ticket, database, cancel, deadline,
                            )?;
                        }
                    }
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(e));
                }
                let form = state::open(
                    Arc::clone(original),
                    *command,
                    provider,
                    database,
                    cancel,
                    deadline,
                )?;
                if let Err(e) = cutoff.check(drafts, cancel) {
                    if e == ReadCutoffFailureV20::SaveFailed {
                        if let Some(ticket) = self.closed_draft_ticket_v75() {
                            ui::pure_close_v20::validate_local_recovery(
                                ticket, database, cancel, deadline,
                            )?;
                        }
                    }
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(e));
                }
                Ok(ResultKindV17::StateV68(StateResultV68::Open(Box::new(
                    form,
                ))))
            }
            Self::Preview(r) => {
                let cutoff = owner
                    .open_cutoff_v20
                    .lock()
                    .ok()
                    .and_then(|slot| slot.clone());
                let Some(cutoff) = cutoff else {
                    return Ok(ResultKindV17::StateV68(StateResultV68::SaveRefused(
                        Arc::clone(r),
                    )));
                };
                if drafts
                    .wait_conversation_open_cutoff_v20(&cutoff, cancel)
                    .is_err()
                {
                    return Ok(ResultKindV17::StateV68(StateResultV68::SaveRefused(
                        Arc::clone(r),
                    )));
                }
                state::StatePreviewV68::prepare_with_save_disposition_v95(
                    r,
                    provider,
                    database,
                    cancel,
                    deadline,
                    &state::StatePreviewSaveV78 {
                        drafts,
                        cutoff: &cutoff,
                    },
                )
                .map(|prepared| {
                    ResultKindV17::StateV68(match prepared {
                        state::StatePreviewPreparationV95::Ready(p) => {
                            StateResultV68::Preview(Arc::clone(r), p)
                        }
                        state::StatePreviewPreparationV95::SaveRefused => {
                            StateResultV68::SaveRefused(Arc::clone(r))
                        }
                    })
                })
            }
            Self::New(r) => r
                .prepare(provider, database, cancel, deadline)
                .map(|p| ResultKindV17::StateV68(StateResultV68::New(Arc::clone(r), p))),
            Self::Confirm(r) => {
                let preview = r.preview.as_ref().ok_or_else(state::failure)?;
                let fence = owner
                    .publication
                    .lock()
                    .ok()
                    .and_then(|mut p| p.take())
                    .ok_or_else(state::failure)?;
                PrPlan::execute_original_state_v68(
                    preview, provider, database, drafts, fence, cancel, deadline,
                )
                .map(ResultKindV17::Publication)
            }
            Self::History {
                original, command, ..
            } => state::history_v74::open(
                crate::scoped_pr_collaboration_source_v17::state_v68::StateOpenRequestV68 {
                    opening: Arc::clone(original),
                    command: *command,
                },
                provider,
                database,
                cancel,
                deadline,
                None,
            )
            .map(|f| ResultKindV17::StateV68(StateResultV68::History(Box::new(f)))),
            Self::Inspect(r) => state::history_v74::open(
                crate::scoped_pr_collaboration_source_v17::state_v68::StateOpenRequestV68 {
                    opening: Arc::clone(&r.view.original),
                    command: r.view.command,
                },
                provider,
                database,
                cancel,
                deadline,
                Some(r),
            )
            .map(|f| ResultKindV17::StateV68(StateResultV68::History(Box::new(f)))),
            Self::Acknowledge(r) => state::history_v74::acknowledge(
                r, provider, database, cancel, deadline,
            )
            .map(|(reference, count)| {
                ResultKindV17::StateV68(StateResultV68::Acknowledged(
                    Arc::clone(r),
                    reference,
                    count,
                ))
            }),
        }
    }
}
impl StateResultV68 {
    pub(super) fn apply(self, app: &mut App) {
        match self {
            Self::Open(form) => {
                let history = state::history_v74::state(&form).is_some();
                app.form = Some(*form);
                app.form_dismissed = false;
                app.overlay = Some(Overlay::Form);
                app.status = if history {
                    "Original State history • unresolved original remains protected • inspect current before LOCAL ACK".into()
                } else {
                    "Original PR State intent • save first • fresh Preview and explicit Confirm required".into()
                };
            }
            Self::Preview(r, p) => {
                if let Some(form) = &mut app.form {
                    if state::accept_preview(form, &r, p) {
                        form.submitting = false;
                        app.status =
                            "PR State Preview • read only • inspect exact effect before Confirm"
                                .into();
                    }
                }
            }
            Self::SaveRefused(request) => {
                let Some(form) = app
                    .form
                    .as_mut()
                    .filter(|form| form.submitting && request.matches(form))
                else {
                    return;
                };
                form.submitting = false;
                form.preview_requested = false;
                form.submit_requested = false;
                form.error = state::save_refusal_v78().message;
                app.status = form.error.clone();
            }
            Self::New(r, p) => {
                if let Some(form) = &mut app.form {
                    if state::accept_new(form, &r, p) {
                        form.submitting = false;
                        app.status="New current State intent • bodyless Save and fresh Preview required • old original inspect-only".into();
                    }
                }
            }
            Self::History(form) => {
                app.form = Some(*form);
                app.form_dismissed = false;
                app.overlay = Some(Overlay::Form);
                app.status="Original State history • read only • inspect exact current uncertainty before LOCAL ACK".into();
            }
            Self::Acknowledged(_, _, _) => {
                app.form = None;
                app.form_dismissed = true;
                app.overlay = None;
                app.hits.clear();
                // LOCAL ACK is neither a current-page read nor proof that an
                // old open-filter queue is still complete. Do not reveal the
                // old detail/tickets as if the inspection issued a Source.
                if let Some(workspace) = &mut app.github {
                    workspace.pr = None;
                    workspace.selected = None;
                    workspace.personal_pr_page_ticket_v13 = None;
                    workspace.notification_pr_page_ticket_v63 = None;
                    workspace.scoped_pr_navigation = None;
                    workspace.scoped_pr_resource = None;
                    workspace.retire_state_membership_v68();
                }
                app.pr_file_images = None;
                app.pr_native_diff_v1 = None;
                app.pr_native_diff_layout_v1 = Default::default();
                app.pr_reactions = None;
                app.pr_reaction_display.clear();
                app.pr_inspection = None;
                app.avatar_states.clear();
                app.status="LOCAL State ACK recorded • no resend, source renewal or membership hydration • reopen a fresh actual PR before a new action".into();
            }
        }
    }
}
pub(super) fn requested(
    form: &Form,
    confirm: bool,
    deadline: Instant,
) -> Result<(StateWorkV68, CancellationToken, Instant), String> {
    if state::history_v74::state(form).is_some() {
        let request = state::history_v74::request(form)?;
        if confirm {
            if !state::history_v74::ack_ready(form) {
                return Err("Fresh useful exact State inspection required before LOCAL ACK".into());
            }
            let current = request
                .view
                .current
                .as_ref()
                .ok_or("Exact State inspection absent")?;
            let cancel = current.cancel.clone();
            let deadline = current.deadline;
            return Ok((StateWorkV68::Acknowledge(request), cancel, deadline));
        }
        return Ok((
            StateWorkV68::Inspect(request),
            CancellationToken::default(),
            deadline,
        ));
    }
    if form.publication.is_some() && form.preview_requested {
        let from = state::history_v74::capture_open(form)?;
        return Ok((
            StateWorkV68::History {
                original: Arc::clone(&from.original),
                command: from.command,
                from: Some(from),
            },
            CancellationToken::default(),
            deadline,
        ));
    }
    let request = state::request(form).map_err(|e| e.message)?;
    if confirm && state::state(form).is_some_and(|s| s.needs_new) {
        return state::new_request(form)
            .map(|r| (StateWorkV68::New(r), CancellationToken::default(), deadline))
            .map_err(|e| e.message);
    }
    if confirm && !request.matches_confirm(form) {
        return Err("Paint the useful exact bodyless State Preview before Confirm".into());
    }
    Ok((
        if confirm {
            StateWorkV68::Confirm(request)
        } else {
            StateWorkV68::Preview(request)
        },
        CancellationToken::default(),
        deadline,
    ))
}
