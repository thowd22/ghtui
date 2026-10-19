//! ONE actual Workspace retained on App, hidden from generic drawing/capture.
//! The terminal descriptor exists only from a native durable confirmed source
//! and its actual own-fence commit. It is read-only, nonserialized and cannot
//! mint a source, write/ACK permit, restored approval or saved body replay.
use super::*;
use crate::{
    forms::Form,
    github_workspace::GithubWorkspace,
    prepared_form_text_v1::PreparedFormTextV1,
    scoped_pr_collaboration_ui_v17 as ui,
    storage::conversation_refresh_v19::{
        ConversationOwnFenceV19, PersonalConversationReadAttemptV20,
        PersonalConversationReservationWitnessV20,
    },
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock,
};
#[cfg(test)]
#[path = "../scoped_pr_collaboration_refresh_retry_v20_tests.rs"]
mod retry_v20_tests;
enum PersonalReadStateV20 {
    Ready(Option<Arc<PersonalConversationReservationWitnessV20>>),
    InFlight(Arc<()>),
    Unavailable,
    Completed,
}

pub(crate) struct ConversationReturnDescriptorV19 {
    source: Arc<PrCollaborationSourceV17>,
    terminal: Arc<ConversationTerminalWitnessV19>,
    identity: crate::providers::gh::HostIdentity,
    lines: Arc<[String]>,
    display: Arc<PreparedFormTextV1>,
    own: OnceLock<Arc<ConversationOwnFenceV19>>,
    painted: AtomicBool,
    attempted: AtomicBool,
    personal_read_v20: Mutex<PersonalReadStateV20>,
}
pub(crate) struct PreparedConversationReturnV19 {
    descriptor: Arc<ConversationReturnDescriptorV19>,
    form: Box<Form>,
}
pub struct ConversationReturnStateV19 {
    descriptor: Arc<ConversationReturnDescriptorV19>,
    presentation: Arc<()>,
    workspace: GithubWorkspace,
}
impl ConversationReturnStateV19 {
    pub(super) fn into_workspace_v19(self) -> GithubWorkspace {
        self.workspace
    }
}
pub(crate) struct ConversationRefreshRequestV19 {
    descriptor: Arc<ConversationReturnDescriptorV19>,
    presentation: Arc<()>,
    epoch: u64,
}
// The worker, not an early UI Cancel, owns settlement. Drop changes only this
// exact private descriptor; no App/slot/newer page/status can be touched.
struct PersonalReadSettlementV20 {
    descriptor: Arc<ConversationReturnDescriptorV19>,
    ticket: Arc<()>,
    previous: Option<Arc<PersonalConversationReservationWitnessV20>>,
    attempt: Option<Arc<PersonalConversationReadAttemptV20>>,
    unavailable: bool,
}
impl Drop for PersonalReadSettlementV20 {
    fn drop(&mut self) {
        let Ok(mut state) = self.descriptor.personal_read_v20.lock() else {
            return;
        };
        if !matches!(&*state,PersonalReadStateV20::InFlight(ticket) if Arc::ptr_eq(ticket,&self.ticket))
        {
            return;
        }
        *state = if self.attempt.as_ref().is_some_and(|a| a.page_completed()) {
            PersonalReadStateV20::Completed
        } else if self.unavailable || self.attempt.as_ref().is_some_and(|a| !a.can_settle()) {
            PersonalReadStateV20::Unavailable
        } else {
            PersonalReadStateV20::Ready(
                self.attempt
                    .as_ref()
                    .and_then(|a| a.last_committed())
                    .or_else(|| self.previous.take()),
            )
        };
    }
}
impl ConversationReturnDescriptorV19 {
    pub(crate) fn source(&self) -> &Arc<PrCollaborationSourceV17> {
        &self.source
    }
    pub(crate) fn original(&self) -> &Arc<PrCollaborationOpenIntentV17> {
        self.source.opening()
    }
    pub(crate) fn identity(&self) -> &crate::providers::gh::HostIdentity {
        &self.identity
    }
    pub(crate) fn lines(&self) -> &[String] {
        &self.lines
    }
    pub(crate) fn display(&self) -> &Arc<PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn painted(&self, useful: bool) {
        self.painted.store(useful, Ordering::Release);
    }
    pub(crate) fn useful(&self) -> bool {
        self.painted.load(Ordering::Acquire) && self.read_ready_v20()
    }
    pub(crate) fn consumed_v19(&self) -> bool {
        if self.own.get().is_some_and(|own| own.is_personal_v20()) {
            self.personal_read_v20.lock().map_or(true, |state| {
                matches!(
                    *state,
                    PersonalReadStateV20::Unavailable | PersonalReadStateV20::Completed
                )
            })
        } else {
            self.attempted.load(Ordering::Acquire)
        }
    }
    fn read_ready_v20(&self) -> bool {
        if self.own.get().is_some_and(|own| own.is_personal_v20()) {
            self.personal_read_v20
                .lock()
                .is_ok_and(|state| matches!(*state, PersonalReadStateV20::Ready(_)))
        } else {
            !self.attempted.load(Ordering::Acquire)
        }
    }
    fn current(&self, app: &App) -> bool {
        self.own.get().is_some()
            && app.view == View::Inbox
            && app.github.is_none()
            && app
                .scoped_pr_collaboration_return_v19
                .as_ref()
                .is_some_and(|state| {
                    std::ptr::eq(state.descriptor.as_ref(), self)
                        && app.form.as_ref().is_some_and(|form| {
                            Arc::ptr_eq(form.presentation_v4().0, &state.presentation)
                        })
                        && self
                            .original()
                            .matches_retained_return_v19(app, &state.workspace)
                })
    }
}
impl PreparedConversationReturnV19 {
    /// ALL lines/Doc/Form/identity allocations happen inside the real original
    /// phase BEFORE its final postwrite UID, including failure-return material.
    pub(crate) fn prepare(
        source: &Arc<PrCollaborationSourceV17>,
        terminal: &Arc<ConversationTerminalWitnessV19>,
        identity: &crate::providers::gh::HostIdentity,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Self> {
        if !Arc::ptr_eq(source, terminal.source_v19())
            || source.opening().scope() != terminal.target().scope()
        {
            return Err(refused());
        }
        let lines:Arc<[String]>=vec![
            "DURABLY CONFIRMED • fresh private view unavailable • NEVER resend".into(),
            format!("Original attempt #{} • exact original PR • current read required",terminal.reference().attempt_id),
            "Refresh PR starts a NEW bounded read. It does not revive the old operation clock, source access or approval.".into(),
            if terminal.state_transition_v68().is_some() {"Back hides old queue membership/counts as unavailable, with old PR details and page tickets removed.".into()}
            else {"Back returns only the previous cached Inbox, with old PR details and page tickets removed.".into()},
            "Old body/reference/history remain inspect-only. No write, ACK, draft restore or automatic retry occurs here.".into(),
        ].into();
        let display =
            PreparedFormTextV1::prepare(&lines, cancel, deadline).map_err(|_| refused())?;
        let descriptor = Arc::new(ConversationReturnDescriptorV19 {
            source: Arc::clone(source),
            terminal: Arc::clone(terminal),
            identity: identity.clone(),
            lines,
            display,
            own: OnceLock::new(),
            painted: AtomicBool::new(false),
            attempted: AtomicBool::new(false),
            personal_read_v20: Mutex::new(PersonalReadStateV20::Ready(None)),
        });
        let form = Box::new(ui::form_from_return_v19(Arc::clone(&descriptor))?);
        Ok(Self { descriptor, form })
    }
    /// Only a successful SAME actual native terminal+privacy transaction may
    /// activate this prepared display. Pointer moves/OnceLock set only.
    pub(crate) fn bind(self, own: Arc<ConversationOwnFenceV19>) -> GithubResult<Self> {
        if !Arc::ptr_eq(own.terminal(), &self.descriptor.terminal)
            || !Arc::ptr_eq(own.source(), &self.descriptor.source)
        {
            return Err(refused());
        }
        self.descriptor.own.set(own).map_err(|_| refused())?;
        Ok(self)
    }
    pub(crate) fn install(
        self,
        app: &mut App,
        owner: &crate::scoped_pr_collaboration_native_v17::ScopedPrCollaborationOwnerV17,
    ) -> bool {
        if self.descriptor.own.get().is_none()
            || app.scoped_pr_collaboration_return_v19.is_some()
            || app.overlay != Some(Overlay::Form)
            || !self.descriptor.original().owns_presentation(app)
            || !owner.owns(app)
        {
            return false;
        }
        let Some(workspace) = app.github.take() else {
            return false;
        };
        app.scoped_pr_collaboration_return_v19 = Some(ConversationReturnStateV19 {
            descriptor: self.descriptor,
            presentation: Arc::clone(self.form.presentation_v4().0),
            workspace,
        });
        app.form = Some(*self.form);
        app.form_dismissed = false;
        app.overlay = Some(Overlay::Form);
        app.context =
            "Confirmed original PR • old private underlay hidden • fresh read only".into();
        app.content[View::Inbox.index()] = vec![app.context.clone()];
        app.pr_file_images = None;
        app.pr_native_diff_v1 = None;
        app.pr_native_diff_layout_v1 = Default::default();
        app.pr_reactions = None;
        app.pr_reaction_display.clear();
        app.pr_inspection = None;
        app.avatar_states.clear();
        app.hits.clear();
        true
    }
}
impl ConversationRefreshRequestV19 {
    pub(crate) fn original(&self) -> &Arc<PrCollaborationOpenIntentV17> {
        self.descriptor.original()
    }
    pub(crate) fn owns_form(&self, form: &Form) -> bool {
        ui::return_descriptor_v19(form).is_some_and(|d| Arc::ptr_eq(d, &self.descriptor))
            && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
    }
    pub(crate) fn owns(&self, app: &App) -> bool {
        self.descriptor.current(app)
            && app.overlay == Some(Overlay::Form)
            && !app.form_dismissed
            && app.form.as_ref().is_some_and(|form| {
                self.owns_form(form) && form.presentation_v4().1 == Some(self.epoch)
            })
    }
    pub(crate) fn matches_capture(&self, app: &App) -> bool {
        self.owns(app)
            && app.form.as_ref().is_some_and(|f| {
                f.preview
                    && !f.preview_display_v1.geometry_rejected
                    && f.fields.is_empty()
                    && !f.dirty
                    && f.publication.is_none()
            })
            && self.descriptor.painted.load(Ordering::Acquire)
    }
    /// Pending terminal read exposes ONLY Esc. Keyboard Esc advances the
    /// existing Form input epoch once before its Close action is delivered;
    /// a clicked Close may retain it. This CLOSED cancellation predicate is
    /// never used to admit a result/read or a different Form/newer intent.
    pub(crate) fn owns_cancel_v19(&self, app: &App) -> bool {
        self.descriptor.current(app)
            && app.overlay == Some(Overlay::Form)
            && !app.form_dismissed
            && app.form.as_ref().is_some_and(|form| {
                self.owns_form(form)
                    && form.submitting
                    && form.presentation_v4().1.is_some_and(|epoch| {
                        epoch == self.epoch || self.epoch.checked_add(1) == Some(epoch)
                    })
            })
    }
    pub(crate) fn capture(app: &App) -> Result<Arc<Self>, String> {
        let form = app
            .form
            .as_ref()
            .ok_or("Exact read-only terminal Form required")?;
        let descriptor =
            ui::return_descriptor_v19(form).ok_or("Not the closed original terminal read Form")?;
        if !descriptor.read_ready_v20() {
            return Err("Retained fresh-read attempt consumed • Back and reopen from a genuine current source; no automatic retry".into());
        }
        let request = Arc::new(Self {
            descriptor: Arc::clone(descriptor),
            presentation: Arc::clone(form.presentation_v4().0),
            epoch: form.presentation_v4().1.ok_or("Terminal input retired")?,
        });
        if !request.matches_capture(app) {
            return Err("Paint the exact current useful terminal read Form first".into());
        }
        Ok(request)
    }
    pub(crate) fn execute(
        &self,
        provider: &crate::providers::gh::GithubProvider,
        database: &std::path::Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<AdmittedConversationPageV19> {
        let own = self.descriptor.own.get().ok_or_else(refused)?;
        if own.is_personal_v20() {
            return self.execute_personal_v20(provider, database, cancel, deadline, own);
        }
        if self.descriptor.attempted.swap(true, Ordering::AcqRel) {
            return Err(refused());
        }
        own.validate_before_read(cancel, deadline)
            .map_err(|_| refresh_local_unavailable_v19())?;
        if own.terminal().state_transition_v68().is_some() {
            let closed = crate::scoped_pr_collaboration_source_v17::with_state_refresh_read_v68(
                crate::scoped_pr_collaboration_source_v17::StateRefreshReadV68 {
                    own,
                    attempt: None,
                },
                provider,
                database,
                cancel,
                deadline,
                |read| PreparedConversationPageV19::prepare_read_v19(own, read, cancel),
                |_, boundary| {
                    let cache = crate::storage::Cache::open(database).map_err(|_| refused())?;
                    own.validate_during_read(&cache, boundary)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    let journal = crate::pr_write_journal::PrWriteJournal::open(database)
                        .map_err(|_| refused())?;
                    self.descriptor
                        .terminal
                        .validate_current_v19(&journal)
                        .map_err(|_| refresh_local_unavailable_v19())
                },
            )?;
            return PreparedConversationPageV19::admit_read_v19(closed, cancel, deadline);
        }
        if own.terminal().base_transition_v32().is_some() {
            let closed = crate::scoped_pr_collaboration_source_v17::with_base_refresh_read_v36(
                own,
                None,
                provider,
                database,
                cancel,
                deadline,
                |read| PreparedConversationPageV19::prepare_read_v19(own, read, cancel),
                |_, boundary| {
                    let cache = crate::storage::Cache::open(database).map_err(|_| refused())?;
                    own.validate_during_read(&cache, boundary)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    let journal = crate::pr_write_journal::PrWriteJournal::open(database)
                        .map_err(|_| refused())?;
                    self.descriptor
                        .terminal
                        .validate_current_v19(&journal)
                        .map_err(|_| refresh_local_unavailable_v19())
                },
            )?;
            return PreparedConversationPageV19::admit_read_v19(closed, cancel, deadline);
        }
        if own.terminal().issue_link_command_v87().is_some() {
            return self.execute_issue_link_v94(provider, database, cancel, deadline, own, None);
        }
        if own.terminal().members_command_v48().is_some() {
            let closed = crate::scoped_pr_collaboration_source_v17::metadata_members_phase_v48::with_metadata_members_manual_refresh_v48(
                crate::scoped_pr_collaboration_source_v17::metadata_members_phase_v48::MetadataMembersManualPhaseV48 {own,attempt:None},
                provider,
                database,
                cancel,
                deadline,
                |read| PreparedConversationPageV19::prepare_read_v19(own, read, cancel),
                |_, boundary| {
                    let cache = crate::storage::Cache::open(database).map_err(|_| refused())?;
                    own.validate_during_read(&cache, boundary)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    let journal = crate::pr_write_journal::PrWriteJournal::open(database)
                        .map_err(|_| refused())?;
                    self.descriptor
                        .terminal
                        .validate_current_v19(&journal)
                        .map_err(|_| refresh_local_unavailable_v19())
                },
            )?;
            return PreparedConversationPageV19::admit_read_v19(closed, cancel, deadline);
        }
        if self.original().metadata_tab_v51() {
            let closed = crate::scoped_pr_collaboration_source_v17::with_metadata_tab_refresh_v51(
                own,
                None,
                provider,
                database,
                cancel,
                deadline,
                |read| PreparedConversationPageV19::prepare_read_v19(own, read, cancel),
                |_, boundary| {
                    let cache = crate::storage::Cache::open(database)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    own.validate_during_read(&cache, boundary)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    let journal = crate::pr_write_journal::PrWriteJournal::open(database)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    self.descriptor
                        .terminal
                        .validate_current_v19(&journal)
                        .map_err(|_| refresh_local_unavailable_v19())
                },
            )?;
            return PreparedConversationPageV19::admit_read_v19(closed, cancel, deadline);
        }
        let closed = crate::scoped_pr_collaboration_source_v17::with_refresh_read_v19(
            self.original().clone(),
            provider,
            database,
            cancel,
            deadline,
            |read| PreparedConversationPageV19::prepare_read_v19(own, read, cancel),
            |_, boundary| {
                let cache = crate::storage::Cache::open(database).map_err(|_| refused())?;
                own.validate_during_read(&cache, boundary)
                    .map_err(|_| refresh_local_unavailable_v19())?;
                let journal = crate::pr_write_journal::PrWriteJournal::open(database)
                    .map_err(|_| refused())?;
                self.descriptor
                    .terminal
                    .validate_current_v19(&journal)
                    .map_err(|_| refresh_local_unavailable_v19())
            },
        )?;
        PreparedConversationPageV19::admit_read_v19(closed, cancel, deadline)
    }
    fn execute_issue_link_v94(
        &self,
        provider: &crate::providers::gh::GithubProvider,
        database: &std::path::Path,
        cancel: &CancellationToken,
        deadline: Instant,
        own: &Arc<ConversationOwnFenceV19>,
        attempt: Option<
            &Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
        >,
    ) -> GithubResult<AdmittedConversationPageV19> {
        let closed = crate::scoped_pr_collaboration_source_v17::issue_links_phase_v87::with_issue_link_manual_refresh_v94(
            crate::scoped_pr_collaboration_source_v17::issue_links_phase_v87::IssueLinkManualPhaseV94 { own, attempt },
            provider, database, cancel, deadline,
            |read| PreparedConversationPageV19::prepare_read_v19(own, read, cancel),
            |_, boundary| {
                let cache = crate::storage::Cache::open(database).map_err(|_| refresh_local_unavailable_v19())?;
                if let Some(attempt) = attempt {
                    attempt.validate_during_read(&cache, boundary)
                } else {
                    own.validate_during_read(&cache, boundary)
                }.map_err(|_| refresh_local_unavailable_v19())?;
                let journal = crate::pr_write_journal::PrWriteJournal::open(database).map_err(|_| refresh_local_unavailable_v19())?;
                self.descriptor.terminal.validate_current_v19(&journal).map_err(|_| refresh_local_unavailable_v19())
            },
        )?;
        PreparedConversationPageV19::admit_read_v19(closed, cancel, deadline)
    }
    fn execute_personal_v20(
        &self,
        provider: &crate::providers::gh::GithubProvider,
        database: &std::path::Path,
        cancel: &CancellationToken,
        deadline: Instant,
        own: &Arc<ConversationOwnFenceV19>,
    ) -> GithubResult<AdmittedConversationPageV19> {
        let ticket = Arc::new(());
        let previous = {
            let mut state = self
                .descriptor
                .personal_read_v20
                .lock()
                .map_err(|_| refused())?;
            let PersonalReadStateV20::Ready(previous) = &*state else {
                return Err(refresh_local_unavailable_v19());
            };
            let previous = previous.clone();
            *state = PersonalReadStateV20::InFlight(Arc::clone(&ticket));
            previous
        };
        let mut settle = PersonalReadSettlementV20 {
            descriptor: Arc::clone(&self.descriptor),
            ticket,
            previous,
            attempt: None,
            unavailable: false,
        };
        let attempt = match own.begin_personal_read_attempt_v20(
            settle.previous.as_ref(),
            provider,
            cancel,
            deadline,
        ) {
            Ok(attempt) => attempt,
            Err(_) => {
                settle.unavailable = !cancel.is_cancelled() && Instant::now() < deadline;
                return Err(refresh_local_unavailable_v19());
            }
        };
        settle.attempt = Some(Arc::clone(&attempt));
        if own.terminal().issue_link_command_v87().is_some() {
            return self.execute_issue_link_v94(
                provider,
                database,
                cancel,
                deadline,
                own,
                Some(&attempt),
            );
        }
        if own.terminal().state_transition_v68().is_some() {
            let closed = crate::scoped_pr_collaboration_source_v17::with_state_refresh_read_v68(
                crate::scoped_pr_collaboration_source_v17::StateRefreshReadV68 {
                    own,
                    attempt: Some(&attempt),
                },
                provider,
                database,
                cancel,
                deadline,
                |read| PreparedConversationPageV19::prepare_read_v19(own, read, cancel),
                |_, boundary| {
                    let cache = crate::storage::Cache::open(database)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    attempt
                        .validate_during_read(&cache, boundary)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    let journal = crate::pr_write_journal::PrWriteJournal::open(database)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    self.descriptor
                        .terminal
                        .validate_current_v19(&journal)
                        .map_err(|_| refresh_local_unavailable_v19())
                },
            )?;
            return PreparedConversationPageV19::admit_read_v19(closed, cancel, deadline);
        }
        if own.terminal().base_transition_v32().is_some() {
            let closed = crate::scoped_pr_collaboration_source_v17::with_base_refresh_read_v36(
                own,
                Some(&attempt),
                provider,
                database,
                cancel,
                deadline,
                |read| PreparedConversationPageV19::prepare_read_v19(own, read, cancel),
                |_, boundary| {
                    let cache = crate::storage::Cache::open(database)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    attempt
                        .validate_during_read(&cache, boundary)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    let journal = crate::pr_write_journal::PrWriteJournal::open(database)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    self.descriptor
                        .terminal
                        .validate_current_v19(&journal)
                        .map_err(|_| refresh_local_unavailable_v19())
                },
            )?;
            return PreparedConversationPageV19::admit_read_v19(closed, cancel, deadline);
        }
        if own.terminal().members_command_v48().is_some() {
            let closed = crate::scoped_pr_collaboration_source_v17::metadata_members_phase_v48::with_metadata_members_manual_refresh_v48(
                crate::scoped_pr_collaboration_source_v17::metadata_members_phase_v48::MetadataMembersManualPhaseV48 {own,attempt:Some(&attempt)},
                provider,
                database,
                cancel,
                deadline,
                |read| PreparedConversationPageV19::prepare_read_v19(own, read, cancel),
                |_, boundary| {
                    let cache = crate::storage::Cache::open(database).map_err(|_| refused())?;
                    attempt.validate_during_read(&cache, boundary)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    let journal = crate::pr_write_journal::PrWriteJournal::open(database)
                        .map_err(|_| refused())?;
                    self.descriptor
                        .terminal
                        .validate_current_v19(&journal)
                        .map_err(|_| refresh_local_unavailable_v19())
                },
            )?;
            return PreparedConversationPageV19::admit_read_v19(closed, cancel, deadline);
        }
        if self.original().metadata_tab_v51() {
            let closed = crate::scoped_pr_collaboration_source_v17::with_metadata_tab_refresh_v51(
                own,
                Some(&attempt),
                provider,
                database,
                cancel,
                deadline,
                |read| PreparedConversationPageV19::prepare_read_v19(own, read, cancel),
                |_, boundary| {
                    let cache = crate::storage::Cache::open(database)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    attempt
                        .validate_during_read(&cache, boundary)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    let journal = crate::pr_write_journal::PrWriteJournal::open(database)
                        .map_err(|_| refresh_local_unavailable_v19())?;
                    self.descriptor
                        .terminal
                        .validate_current_v19(&journal)
                        .map_err(|_| refresh_local_unavailable_v19())
                },
            )?;
            return PreparedConversationPageV19::admit_read_v19(closed, cancel, deadline);
        }
        let closed = crate::scoped_pr_collaboration_source_v17::with_personal_refresh_read_v20(
            self.original().clone(),
            provider,
            database,
            cancel,
            deadline,
            &attempt,
            |read| PreparedConversationPageV19::prepare_read_v19(own, read, cancel),
            |_, boundary| {
                let cache = crate::storage::Cache::open(database)
                    .map_err(|_| refresh_local_unavailable_v19())?;
                attempt
                    .validate_during_read(&cache, boundary)
                    .map_err(|_| refresh_local_unavailable_v19())?;
                let journal = crate::pr_write_journal::PrWriteJournal::open(database)
                    .map_err(|_| refresh_local_unavailable_v19())?;
                self.descriptor
                    .terminal
                    .validate_current_v19(&journal)
                    .map_err(|_| refresh_local_unavailable_v19())
            },
        )?;
        PreparedConversationPageV19::admit_read_v19(closed, cancel, deadline)
    }
}
pub(crate) fn capture_request(app: &App) -> Result<Arc<ConversationRefreshRequestV19>, String> {
    ConversationRefreshRequestV19::capture(app)
}
pub(crate) fn back(app: &mut App) -> bool {
    let Some(form) = app.form.as_ref() else {
        return false;
    };
    let Some(descriptor) = ui::return_descriptor_v19(form).cloned() else {
        return false;
    };
    if !descriptor.current(app) || app.overlay != Some(Overlay::Form) || app.form_dismissed {
        return false;
    }
    let Some(mut state) = app.scoped_pr_collaboration_return_v19.take() else {
        return false;
    };
    // Strip ALL old PR/read tickets BEFORE the real cached Workspace is ever
    // paintable again. No provider/SQL/read/source issuance or write/ACK path.
    state.workspace.pr = None;
    state.workspace.scoped_pr_navigation = None;
    state.workspace.scoped_pr_resource = None;
    state.workspace.personal_pr_page_ticket_v13 = None;
    let state_v68 = descriptor.terminal.state_transition_v68().is_some();
    if state_v68 {
        state.workspace.retire_state_membership_v68();
    }
    app.context = state.workspace.context();
    app.github = Some(state.workspace);
    app.form = None;
    app.form_dismissed = true;
    app.overlay = None;
    app.inbox_list.selected = app.github.as_ref().unwrap().selected_index();
    app.inbox_list.top = 0;
    app.focus = 0;
    app.github_content_focused = true;
    app.hits.clear();
    app.status = if state_v68 {
        "PR queue membership/counts unavailable • no reads or resend".into()
    } else {
        "Cached Inbox only • old PR details removed • no reads or resend".into()
    };
    true
}
pub(crate) fn retire_stale(app: &mut App) -> bool {
    let Some(state) = &app.scoped_pr_collaboration_return_v19 else {
        return false;
    };
    let descriptor = Arc::clone(&state.descriptor);
    let presentation = Arc::clone(&state.presentation);
    if descriptor.current(app) {
        return false;
    }
    let old_form = app.form.as_ref().is_some_and(|f| {
        Arc::ptr_eq(f.presentation_v4().0, &presentation)
            && ui::return_descriptor_v19(f).is_some_and(|d| Arc::ptr_eq(d, &descriptor))
    });
    app.scoped_pr_collaboration_return_v19 = None;
    if old_form {
        app.form = None;
        app.form_dismissed = true;
        if app.overlay == Some(Overlay::Form) {
            app.overlay = None;
            app.hits.clear();
        }
    }
    true
}
/// Generic-slot replacement retires ONLY this original hidden Return and
/// exact old Form. It never cancels/removes the replacement physical slot or
/// changes its token/generation/loading/status/Notice/hits. A genuine newer
/// source/navigation/Form cannot satisfy the original ticket comparison.
pub(crate) fn retire_orphan_v19(app: &mut App, request: &ConversationRefreshRequestV19) {
    let old_state = app
        .scoped_pr_collaboration_return_v19
        .as_ref()
        .is_some_and(|state| {
            Arc::ptr_eq(&state.descriptor, &request.descriptor)
                && Arc::ptr_eq(&state.presentation, &request.presentation)
        });
    let old_form = app
        .form
        .as_ref()
        .is_some_and(|form| request.owns_form(form));
    let old_overlay = old_form && app.overlay == Some(Overlay::Form);
    if old_state {
        app.scoped_pr_collaboration_return_v19 = None;
    }
    if old_form {
        app.form = None;
        app.form_dismissed = true;
    }
    if old_overlay {
        app.overlay = None;
        app.hits.clear();
    }
}
pub(crate) fn retire_account_failure(
    app: &mut App,
    request: &ConversationRefreshRequestV19,
) -> bool {
    if !request.owns(app) {
        return false;
    }
    app.scoped_pr_collaboration_return_v19 = None;
    app.form = None;
    app.form_dismissed = true;
    if app.overlay == Some(Overlay::Form) {
        app.overlay = None;
        app.hits.clear();
    }
    app.context = "Original account/PR unavailable • retained private page discarded".into();
    app.content[View::Inbox.index()] = vec![app.context.clone()];
    app.avatar_states.clear();
    true
}
pub(crate) fn failed_guidance(app: &mut App, request: &ConversationRefreshRequestV19) {
    if request.owns(app) || request.owns_cancel_v19(app) {
        if let Some(form) = &mut app.form {
            form.error=if request.descriptor.own.get().is_some_and(|own|own.is_personal_v20()) && !request.descriptor.consumed_v19() {
                if request.descriptor.read_ready_v20() {"Read failed • Refresh PR starts a NEW explicit read • no write resent or draft restored"}
                else {"Original read is still settling • wait before a NEW explicit Refresh • no write resent; Back is safe"}
            } else {"Fresh read unavailable; Back and reopen from a genuine current source. No epoch renewed, write resent or protected body restored."}.into();
        }
    }
}
pub(crate) fn forward_refresh(app: &mut App) -> bool {
    let Some(form) = app.form.as_ref() else {
        return false;
    };
    let Some(descriptor) = ui::return_descriptor_v19(form) else {
        return false;
    };
    if !descriptor.current(app) || app.overlay != Some(Overlay::Form) || app.form_dismissed {
        return true;
    }
    let ready = ui::return_ready_v19(form) && !form.submitting;
    if ready {
        app.form.as_mut().unwrap().preview_requested = true;
    } else {
        app.form.as_mut().unwrap().error="Fresh read unavailable/consumed • inspect the useful document, or Back and reopen; nothing queued".into();
    }
    true
}
