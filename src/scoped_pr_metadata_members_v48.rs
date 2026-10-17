//! ALL ten member purposes on the existing original Collaboration Form. The
//! picker/index/value is descriptive data; every Preview/Confirm re-reads the
//! original Core and actual selected metadata. No legacy publisher fallback.
use super::*;
use crate::{
    forms::{Field, FormAction},
    pr_metadata_actions::{PrMetadataCommand, PrMetadataPlan},
    pr_metadata_catalogue::LoadedPrMetadataChoices,
    pr_metadata_picker::{PrMetadataPicker, PrMetadataPickerOutcome},
    pr_write_journal::metadata_members_v48::lineage_v52::{
        MembersActionBaseV48, MembersTerminalRevisionV48,
    },
    pr_write_journal::metadata_members_v48::MetadataMembersOriginSeedV48,
    providers::{
        gh::{
            GithubError, GithubErrorKind, GithubProvider, GithubResult, OriginalMetadataMembersV48,
            PrMetadataReadLimits,
        },
        runner::CancellationToken,
    },
    scoped_pr_collaboration_source_v17::{
        metadata_members_phase_v48::{
            self as phase, MetadataMembersOpenV48, MetadataMembersPhasePurposeV48,
        },
        PrCollaborationSourceV17,
    },
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Weak,
    },
    time::Instant,
};
#[path = "scoped_pr_metadata_members_history_v48.rs"]
pub(crate) mod history;

pub(crate) struct MetadataMembersIntentV48 {
    pub(crate) source: Arc<PrCollaborationSourceV17>,
    pub(crate) actual: Arc<OriginalMetadataMembersV48>,
    pub(crate) loaded: Arc<LoadedPrMetadataChoices>,
    pub(crate) command: PrMetadataCommand,
    pub(crate) author: crate::scoped_pr_collaboration_draft_v17::ConversationActorV17,
    lines: Arc<[String]>,
}
#[derive(Clone)]
pub(crate) struct MetadataMembersStateV48 {
    pub(crate) intent: Arc<MetadataMembersIntentV48>,
    pub(crate) identity: HostIdentity,
    pub(crate) key: Option<Arc<str>>,
    selected_value: Option<Arc<str>>,
    pub(crate) selected: Option<Arc<PrMetadataPlan>>,
    pub(crate) ticket: Arc<()>,
    pub(crate) revision: Option<u64>,
    pub(crate) preview: Option<Arc<MetadataMembersPreviewV48>>,
    pub(crate) lineage_v52: Option<Arc<MembersTerminalRevisionV48>>,
    pub(crate) needs_new_v52: bool,
    initial_save_v77: Option<InitialSaveV77>,
}
/// One local save registration only, issued by accepting the actual matching
/// fresh Preview. This is NOT a durable ACK, source or publication permission.
#[derive(Clone)]
struct InitialSaveV77 {
    preview: Weak<MetadataMembersPreviewV48>,
    presentation: Weak<()>,
    ticket: Weak<()>,
    epoch: u64,
    revision: u64,
}
pub(crate) struct MetadataMembersRequestV48 {
    pub(crate) intent: Arc<MetadataMembersIntentV48>,
    pub(crate) plan: Arc<PrMetadataPlan>,
    // No durable key until a fresh Preview has corroborated selected Team
    // organization in the SAME original Core response. Local selection is data.
    pub(crate) key: Option<Arc<str>>,
    pub(crate) values: Arc<BTreeMap<String, String>>,
    pub(crate) preview: Option<Arc<MetadataMembersPreviewV48>>,
    lineage_v52: Option<Arc<MembersTerminalRevisionV48>>,
    needs_new_v52: bool,
    presentation: Arc<()>,
    epoch: u64,
    ticket: Arc<()>,
    revision: u64,
}
pub(crate) struct MetadataMembersPreviewV48 {
    pub(crate) intent: Arc<MetadataMembersIntentV48>,
    pub(crate) plan: Arc<PrMetadataPlan>,
    pub(crate) values: Arc<BTreeMap<String, String>>,
    pub(crate) seed: MetadataMembersOriginSeedV48,
    pub(crate) actual: Arc<OriginalMetadataMembersV48>,
    pub(crate) identity: HostIdentity,
    pub(crate) key: Arc<str>,
    pub(crate) lines: Arc<[String]>,
    pub(crate) display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    pub(crate) useful: AtomicBool,
    pub(crate) used: AtomicBool,
    base_v52: MembersActionBaseV48,
    pub(crate) lineage_v52: Option<Arc<MembersTerminalRevisionV48>>,
    pub(crate) notice_v52: bool,
}
pub(crate) fn failure() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Exact original metadata members/source/input changed; no fallback or resend"
            .into(),
    }
}
fn local_failure(message: impl Into<String>) -> GithubError {
    GithubError {
        kind: GithubErrorKind::InvalidTarget,
        message: message.into(),
    }
}
pub(crate) fn state(form: &Form) -> Option<&MetadataMembersStateV48> {
    match &form.purpose {
        FormPurpose::PrCollaborationV17(state) => match &state.mode {
            ComposerModeV17::MetadataMembersV48(value) => Some(value),
            _ => None,
        },
        _ => None,
    }
}
fn state_mut(form: &mut Form) -> Option<&mut MetadataMembersStateV48> {
    match &mut form.purpose {
        FormPurpose::PrCollaborationV17(state) => match &mut state.mode {
            ComposerModeV17::MetadataMembersV48(value) => Some(value),
            _ => None,
        },
        _ => None,
    }
}
impl MetadataMembersStateV48 {
    pub(crate) fn invalidate(&mut self) {
        self.initial_save_v77 = None;
        if let Some(preview) = self.preview.take() {
            preview.painted(false);
        }
        self.revision = self.revision.and_then(|revision| revision.checked_add(1));
    }
    pub(crate) fn lines(&self) -> &[String] {
        self.preview
            .as_ref()
            .map_or(&self.intent.lines, |preview| &preview.lines)
    }
}
pub(crate) fn open(
    opening: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    command: PrMetadataCommand,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Form> {
    let limits = PrMetadataReadLimits::default();
    let (source, actual, (mut form, loaded, selected, key, author, lines)) =
        phase::open_metadata_members_v48(
            MetadataMembersOpenV48 {
                opening,
                command,
                limits: &limits,
            },
            provider,
            database,
            cancel,
            deadline,
            |actual, _local| {
                let loaded = Arc::new(LoadedPrMetadataChoices::from_original_members_v48(
                    Arc::clone(actual),
                )?);
                let legacy = crate::pr_metadata_native::PrMetadataIntent::new(
                    command,
                    actual.identity().clone(),
                    actual.read().target().clone(),
                )
                .map_err(local_failure)?;
                let mut form = Form::pr_metadata(legacy).map_err(local_failure)?;
                let (selected, key, value) = if command == PrMetadataCommand::ClearMilestone {
                    let plan = Arc::new(
                        PrMetadataPlan::prepare_original_members_v48(actual, None)
                            .map_err(local_failure)?,
                    );
                    (
                        Some(plan),
                        None,
                        "Clear the actual current milestone".to_owned(),
                    )
                } else {
                    (None, None, String::new())
                };
                form.fields = vec![Field::text("body", "Selected actual member", &value, false)
                    .map_err(local_failure)?];
                form.focus = form.fields.len();
                // No editable remote body: never ordinary-autosave over an unknown
                // protected key on selection. A freshly proved regular Preview
                // registers the real initial autosave; SAME begin then allocates
                // the ref and saves/acknowledges that bound envelope before claim.
                form.dirty = false;
                let parent = actual.parent();
                let author = crate::scoped_pr_collaboration_draft_v17::prepare_actor_v17(
                    &actual.identity().host,
                    &parent.summary().pr.author,
                    parent.author_account_id(),
                    cancel,
                    deadline.min(actual.deadline()),
                )?;
                let lines: Arc<[String]> = vec![
                format!("{} • exact one-field metadata effect", command.label()),
                format!("{}#{} • repository #{} / PR #{}", parent.repository(), parent.number(), parent.repository_id(), parent.database_id()),
                format!("Actual eligible pages {}/{} • only observed choices; unseen membership remains unknown", actual.read().fetched_pages(), actual.read().requested_pages()),
                "Choose an actual eligible member or current member; no name-only or position-derived ID.".into(),
                "Preview freshly checks complete current membership and preserves every unmodified User/Team member.".into(),
                "Only an explicit useful Confirm attempts one write. Local Git is unchanged; protected history never replays.".into(),
            ].into();
                Ok((form, loaded, selected, key, author, lines))
            },
        )?;
    let identity = source.identity().clone();
    let intent = Arc::new(MetadataMembersIntentV48 {
        source,
        actual,
        loaded,
        command,
        author,
        lines,
    });
    form.purpose = FormPurpose::PrCollaborationV17(Box::new(ConversationComposerStateV17 {
        mode: ComposerModeV17::MetadataMembersV48(MetadataMembersStateV48 {
            intent,
            identity,
            key,
            selected_value: selected
                .as_ref()
                .map(|_| Arc::from(form.fields[0].editor.text.as_str())),
            selected,
            ticket: Arc::new(()),
            revision: Some(0),
            preview: None,
            lineage_v52: None,
            needs_new_v52: false,
            initial_save_v77: None,
        }),
    }));
    Ok(form)
}

fn selection(form: &Form, index: usize) -> GithubResult<(Arc<PrMetadataPlan>, String)> {
    let state = state(form).ok_or_else(failure)?;
    let picker = form.metadata_picker.as_ref().ok_or_else(failure)?;
    if !Arc::ptr_eq(picker.source(), &state.intent.loaded)
        || !picker
            .source()
            .matches_original_members_v48(&state.intent.actual)
    {
        return Err(failure());
    }
    let command = state.intent.command;
    if command.adds() {
        if picker.current_command().is_some() {
            return Err(failure());
        }
        picker.source().choose(index)?;
    } else if picker.current_command() != Some(command) || picker.current_member(index).is_none() {
        return Err(failure());
    }
    let plan = Arc::new(
        PrMetadataPlan::prepare_original_members_v48(&state.intent.actual, Some(index))
            .map_err(local_failure)?,
    );
    let member = plan.selected().ok_or_else(failure)?;
    let value = format!(
        "{} • actual #{} • {}",
        crate::git::display_bytes(member.name().as_bytes()),
        member.id(),
        member.node()
    );
    if value.len() > 8192 {
        return Err(local_failure("Selected member label exceeds bounded field"));
    }
    Ok((plan, value))
}
pub(crate) fn dispatch(form: &mut Form, action: &FormAction) -> bool {
    if state(form).is_none() {
        return false;
    }
    match action {
        FormAction::NewIntent if state(form).is_some_and(|s| s.needs_new_v52) => {
            match new_request_v52(form) {
                Ok(_) => form.submit_requested = true,
                Err(error) => form.error = error.message,
            };
            true
        }
        FormAction::OpenPicker(_)
            if !form.preview && !form.submitting && form.publication.is_none() =>
        {
            let value = state(form).unwrap();
            if value.intent.command == PrMetadataCommand::ClearMilestone {
                return true;
            }
            let picker = if value.intent.command.adds() {
                Ok(PrMetadataPicker::new(Arc::clone(&value.intent.loaded)))
            } else {
                PrMetadataPicker::current(Arc::clone(&value.intent.loaded), value.intent.command)
            };
            match picker {
                Ok(picker) => form.metadata_picker = Some(picker),
                Err(error) => form.error = error,
            }
            form.metadata_picker_hits.clear();
            true
        }
        FormAction::MetadataPicker(action) if form.metadata_picker.is_some() => {
            let outcome = form
                .metadata_picker
                .as_mut()
                .unwrap()
                .dispatch(action.clone());
            match outcome {
                PrMetadataPickerOutcome::Chosen(index) => match selection(form, index) {
                    Ok((plan, value)) => match crate::editor::TextBuffer::new(&value) {
                        Ok(editor) => {
                            form.fields[0].editor = editor;
                            let current = state_mut(form).unwrap();
                            current.selected = Some(plan);
                            current.key = None;
                            current.selected_value = Some(value.into());
                            current.lineage_v52 = None;
                            current.needs_new_v52 = false;
                            form.metadata_picker = None;
                            form.metadata_picker_hits.clear();
                            form.dirty = false;
                            form.invalidate_plan();
                            form.focus = form.fields.len();
                        }
                        Err(error) => form.error = error,
                    },
                    Err(error) => form.error = error.message,
                },
                PrMetadataPickerOutcome::Closed => {
                    form.metadata_picker = None;
                    form.metadata_picker_hits.clear();
                }
                PrMetadataPickerOutcome::Changed => form.metadata_picker_hits.clear(),
            }
            true
        }
        FormAction::Preview if !form.preview && form.publication.is_none() => {
            match request(form) {
                Ok(_) => form.preview_requested = true,
                Err(error) => form.error = error.message,
            }
            true
        }
        FormAction::Submit => {
            if request(form).is_ok_and(|request| request.matches_confirm(form)) {
                form.submit_requested = true;
            } else {
                form.error = "Fresh useful exact member Preview required before Confirm".into();
            }
            true
        }
        FormAction::Editor | FormAction::Choice(_, _) => true,
        _ => false,
    }
}
pub(crate) fn request(form: &Form) -> GithubResult<Arc<MetadataMembersRequestV48>> {
    let state = state(form).ok_or_else(failure)?;
    let plan = state
        .selected
        .as_ref()
        .ok_or_else(|| local_failure("Choose an actual indexed member before Preview"))?;
    if form.fields.len() != 1
        || form.fields[0].key != "body"
        || form.fields[0].editor.text.len() > 8192
        || state.selected_value.as_deref() != Some(form.fields[0].editor.text.as_str())
        || form.metadata_picker.is_some()
        || form.publication.is_some()
    {
        return Err(local_failure(
            "Close the genuine member picker before fresh Preview",
        ));
    }
    let revision = state.revision.ok_or_else(failure)?;
    let (presentation, epoch) = form.presentation_v4();
    let values = state
        .preview
        .as_ref()
        .filter(|preview| {
            preview.values.get("body").map(String::as_str)
                == Some(form.fields[0].editor.text.as_str())
        })
        .map_or_else(
            || Arc::new(form.values()),
            |preview| Arc::clone(&preview.values),
        );
    Ok(Arc::new(MetadataMembersRequestV48 {
        intent: Arc::clone(&state.intent),
        plan: Arc::clone(plan),
        key: state.key.clone(),
        values,
        preview: state.preview.clone(),
        lineage_v52: state.lineage_v52.clone(),
        needs_new_v52: state.needs_new_v52,
        presentation: Arc::clone(presentation),
        epoch: epoch.ok_or_else(failure)?,
        ticket: Arc::clone(&state.ticket),
        revision,
    }))
}
impl MetadataMembersRequestV48 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        state(form).is_some_and(|state| {
            Arc::ptr_eq(&state.intent, &self.intent) && Arc::ptr_eq(&state.ticket, &self.ticket)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
            && form.presentation_v4().1 == Some(self.epoch)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        self.owns(form)
            && form.publication.is_none()
            && form.metadata_picker.is_none()
            && state(form).is_some_and(|state| {
                state.revision == Some(self.revision)
                    && state
                        .selected
                        .as_ref()
                        .is_some_and(|plan| Arc::ptr_eq(plan, &self.plan))
                    && match (&state.key, &self.key) {
                        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                        (None, None) => true,
                        _ => false,
                    }
                    && match (&state.preview, &self.preview) {
                        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                        (None, None) => true,
                        _ => false,
                    }
            })
            && form.fields.len() == 1
            && self.values.get("body").map(String::as_str)
                == Some(form.fields[0].editor.text.as_str())
    }
    pub(crate) fn matches_confirm(&self, form: &Form) -> bool {
        self.matches_confirm_profile_v81(form)
            && self
                .preview
                .as_ref()
                .is_some_and(|preview| !preview.used.load(Ordering::Acquire))
    }
    /// Only the actual accepted current owner may call this after admission.
    /// Consuming its one-take Preview is not loss of the SAME busy Form. This
    /// does not admit a second Confirm: matches_confirm and consume stay strict.
    pub(crate) fn matches_inflight_confirm_v81(&self, form: &Form) -> bool {
        form.submitting && self.matches_confirm_profile_v81(form)
    }
    fn matches_confirm_profile_v81(&self, form: &Form) -> bool {
        self.matches(form)
            && !self.needs_new_v52
            && form.preview
            && !form.preview_display_v1.geometry_rejected
            && self.preview.as_ref().is_some_and(|preview| {
                self.key
                    .as_ref()
                    .is_some_and(|key| Arc::ptr_eq(key, &preview.key))
                    && Arc::ptr_eq(&preview.intent, &self.intent)
                    && Arc::ptr_eq(&preview.plan, &self.plan)
                    && Arc::ptr_eq(&preview.values, &self.values)
                    && preview.useful.load(Ordering::Acquire)
                    && !preview.notice_v52
            })
    }
}
#[derive(Clone, Copy)]
enum PreviewPhaseV52 {
    Preview,
    ExplicitNew,
}
impl MetadataMembersPreviewV48 {
    pub(crate) fn painted(&self, useful: bool) {
        self.useful.store(useful, Ordering::Release);
    }
    pub(crate) fn prepare(
        request: &Arc<MetadataMembersRequestV48>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        if request.needs_new_v52 {
            return Err(local_failure(
                "Choose explicit New action before another fresh Preview",
            ));
        }
        Self::prepare_phase_v52(
            request,
            provider,
            database,
            cancel,
            deadline,
            PreviewPhaseV52::Preview,
        )
    }
    fn prepare_phase_v52(
        request: &Arc<MetadataMembersRequestV48>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        purpose: PreviewPhaseV52,
    ) -> GithubResult<Arc<Self>> {
        let intent = &request.intent;
        let phase_purpose = if intent
            .source
            .opening()
            .metadata_members_tab_v61(request.plan.command())
        {
            MetadataMembersPhasePurposeV48::MetadataPreviewV62(&request.plan, &intent.actual)
        } else {
            MetadataMembersPhasePurposeV48::Preview(&request.plan, intent.actual.limits())
        };
        let (actual,seed,key,identity,lines,display,base_v52,lineage_v52,notice_v52)=intent.source.with_metadata_members_phase_v48(provider,database,cancel,deadline,phase_purpose,|phase|{
            let deadline=phase.deadline();
            let local=phase.local(database)?;
            let actual=Arc::clone(phase.actual());
            let current=MetadataMembersOriginSeedV48::from_producer(&intent.source,&actual,&request.plan,&request.values,database).map_err(|_|failure())?;
            let base=MembersActionBaseV48::from_producer(&current,&phase,&request.plan,database).map_err(|_|failure())?;
            let journal=crate::pr_write_journal::PrWriteJournal::open(database).map_err(|_|failure())?;
            let revision=journal.members_action_revision_v52(&base).map_err(|_|failure())?;
            let (seed,notice)=match (&request.lineage_v52,&revision,purpose){
                (Some(expected),Some(current_revision),PreviewPhaseV52::Preview) if !request.needs_new_v52=>{
                    journal.validate_members_revision_v52(&base,expected).map_err(|_|failure())?;
                    (MetadataMembersOriginSeedV48::from_revision_v52(current,&base,current_revision).map_err(|_|failure())?,false)
                }
                (Some(expected),Some(current_revision),PreviewPhaseV52::ExplicitNew) if request.needs_new_v52=>{
                    journal.validate_members_revision_v52(&base,expected).map_err(|_|failure())?;
                    (MetadataMembersOriginSeedV48::from_revision_v52(current,&base,current_revision).map_err(|_|failure())?,false)
                }
                (None,Some(_),PreviewPhaseV52::Preview)=>(current,true),
                (None,None,PreviewPhaseV52::Preview)=>(current,false),
                _=>return Err(failure()),
            };
            if !notice&&request.key.as_ref().is_some_and(|key|seed.draft_key()!=key.as_ref())&&!matches!(purpose,PreviewPhaseV52::ExplicitNew){return Err(failure());}
            let lines:Arc<[String]>=if notice{
                let revision=revision.as_ref().ok_or_else(failure)?;
                vec!["NEEDS NEW ACTION • no write or body replay".into(),revision.label().into(),format!("Exact terminal attempt #{} • old protected body/ref/origin remain inspect-only",revision.reference().attempt_id),"Choose New action explicitly for a NEW current read; it does not restore approval or resend.".into()].into()
            }else{crate::pr_metadata_native::preview(&request.plan).map_err(local_failure)?.into()};
            let display=crate::prepared_form_text_v1::PreparedFormTextV1::prepare(&lines,cancel,deadline).map_err(|_|failure())?;
            let key=Arc::from(seed.draft_key());
            let identity=phase.finish(||{
                local.validate_boundary_v13(database,intent.source.local_boundary())?;
                if let Some(revision)=&revision {journal.validate_members_revision_v52(&base,revision).map_err(|_|failure())?;}
                Ok(())
            })?;
            Ok((actual,seed,key,identity,lines,display,base,revision,notice))
        })?;
        Ok(Arc::new(Self {
            intent: Arc::clone(intent),
            plan: Arc::clone(&request.plan),
            values: Arc::clone(&request.values),
            seed,
            actual,
            identity,
            key,
            lines,
            display,
            useful: AtomicBool::new(false),
            used: AtomicBool::new(false),
            base_v52,
            lineage_v52,
            notice_v52,
        }))
    }
    pub(crate) fn bind_fresh_seed_v52(
        &self,
        current: MetadataMembersOriginSeedV48,
        phase:&crate::scoped_pr_collaboration_source_v17::metadata_members_phase_v48::MetadataMembersPhaseV48<'_>,
        database: &Path,
    ) -> GithubResult<MetadataMembersOriginSeedV48> {
        let base = MembersActionBaseV48::from_producer(&current, phase, &self.plan, database)
            .map_err(|_| failure())?;
        match &self.lineage_v52 {
            Some(revision) if !self.notice_v52 => {
                crate::pr_write_journal::PrWriteJournal::open(database)
                    .map_err(|_| failure())?
                    .validate_members_revision_v52(&self.base_v52, revision)
                    .map_err(|_| failure())?;
                crate::pr_write_journal::PrWriteJournal::open(database)
                    .map_err(|_| failure())?
                    .validate_members_revision_v52(&base, revision)
                    .map_err(|_| failure())?;
                MetadataMembersOriginSeedV48::from_revision_v52(current, &base, revision)
                    .map_err(|_| failure())
            }
            None if !self.notice_v52 => Ok(current),
            _ => Err(failure()),
        }
    }
}

pub(crate) fn accept_preview(
    form: &mut Form,
    request: &Arc<MetadataMembersRequestV48>,
    preview: Arc<MetadataMembersPreviewV48>,
) -> bool {
    if !request.matches(form)
        || !Arc::ptr_eq(&preview.intent, &request.intent)
        || !Arc::ptr_eq(&preview.plan, &request.plan)
        || !Arc::ptr_eq(&preview.values, &request.values)
    {
        return false;
    }
    let display = Arc::clone(&preview.display);
    let state = state_mut(form).unwrap();
    state.identity = preview.identity.clone();
    state.key = Some(Arc::clone(&preview.key));
    state.lineage_v52 = preview.lineage_v52.clone();
    state.needs_new_v52 = preview.notice_v52;
    let notice_reference = preview
        .notice_v52
        .then(|| preview.lineage_v52.as_ref().unwrap().reference().clone());
    let register_initial_save = !preview.notice_v52;
    state.initial_save_v77 = register_initial_save.then(|| InitialSaveV77 {
        preview: Arc::downgrade(&preview),
        presentation: Arc::downgrade(&request.presentation),
        ticket: Arc::downgrade(&request.ticket),
        epoch: request.epoch,
        revision: request.revision,
    });
    state.preview = Some(preview);
    form.publication = notice_reference;
    form.preview = true;
    form.preview_requested = false;
    form.submit_requested = false;
    // Only this genuine fresh lineage/no-blocker/key/selection proof may
    // register the current key with the SAME DraftWriter before fence capture.
    // An explicit-New notice never overwrites its protected predecessor.
    form.dirty = register_initial_save;
    form.preview_scroll = 0;
    form.focus = form.fields.len();
    form.error.clear();
    form.install_prepared_preview_display_v1(display);
    true
}

/// Borrowed pointer/scalar checks and one already bounded raw field comparison.
/// A real edit retires this ticket through invalidate(); raw equality also
/// refuses changed/grafted values before save. No body clone or I/O.
pub(crate) fn pending_initial_save_v77(form: &Form) -> bool {
    let Some(state) = state(form) else {
        return false;
    };
    let (Some(save), Some(preview), Some(plan), Some(key)) = (
        state.initial_save_v77.as_ref(),
        state.preview.as_ref(),
        state.selected.as_ref(),
        state.key.as_ref(),
    ) else {
        return false;
    };
    form.dirty
        && form.preview
        && !form.submitting
        && form.publication.is_none()
        && form.metadata_picker.is_none()
        && !state.needs_new_v52
        && !preview.notice_v52
        && !preview.used.load(Ordering::Acquire)
        && save.preview.ptr_eq(&Arc::downgrade(preview))
        && save
            .presentation
            .ptr_eq(&Arc::downgrade(form.presentation_v4().0))
        && form.presentation_v4().1 == Some(save.epoch)
        && save.ticket.ptr_eq(&Arc::downgrade(&state.ticket))
        && state.revision == Some(save.revision)
        && Arc::ptr_eq(&preview.intent, &state.intent)
        && Arc::ptr_eq(&preview.plan, plan)
        && Arc::ptr_eq(&preview.key, key)
        && form.fields.len() == 1
        && form.fields[0].key == "body"
        && preview.values.len() == 1
        && preview.values.get("body").map(String::as_str)
            == Some(form.fields[0].editor.text.as_str())
        && state.selected_value.as_deref() == Some(form.fields[0].editor.text.as_str())
}
pub(crate) fn settle_initial_save_v77(form: &mut Form, accepted: bool) {
    if !pending_initial_save_v77(form) {
        return;
    }
    state_mut(form).unwrap().initial_save_v77 = None;
    if !accepted {
        // Rejected registration is not a saved draft. Keep body/dirty state,
        // retire approval, and require another genuine explicit Preview.
        form.invalidate_plan();
    }
}

pub(crate) struct MetadataMembersNewRequestV52 {
    pub(crate) original_request: Arc<MetadataMembersRequestV48>,
}
impl MetadataMembersNewRequestV52 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        self.original_request.owns(form)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        let r = &self.original_request;
        r.owns(form)
            && form.preview
            && !form.preview_display_v1.geometry_rejected
            && form.metadata_picker.is_none()
            && r.preview.as_ref().is_some_and(|preview| {
                preview.notice_v52
                    && preview.useful.load(Ordering::Acquire)
                    && !preview.used.load(Ordering::Acquire)
                    && state(form).is_some_and(|s| {
                        s.needs_new_v52
                            && s.revision == Some(r.revision)
                            && s.preview
                                .as_ref()
                                .is_some_and(|actual| Arc::ptr_eq(actual, preview))
                    })
                    && form.publication.as_ref()
                        == preview
                            .lineage_v52
                            .as_ref()
                            .map(|revision| revision.reference())
            })
    }
    pub(crate) fn prepare(
        &self,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<MetadataMembersPreviewV48>> {
        MetadataMembersPreviewV48::prepare_phase_v52(
            &self.original_request,
            provider,
            database,
            cancel,
            deadline,
            PreviewPhaseV52::ExplicitNew,
        )
    }
}
pub(crate) fn new_request_v52(form: &Form) -> GithubResult<Arc<MetadataMembersNewRequestV52>> {
    let state = state(form)
        .filter(|s| s.needs_new_v52)
        .ok_or_else(failure)?;
    let preview = state
        .preview
        .as_ref()
        .filter(|p| p.notice_v52 && p.lineage_v52.is_some())
        .ok_or_else(failure)?;
    let (presentation, epoch) = form.presentation_v4();
    let original_request = Arc::new(MetadataMembersRequestV48 {
        intent: Arc::clone(&state.intent),
        plan: Arc::clone(&preview.plan),
        key: state.key.clone(),
        values: Arc::clone(&preview.values),
        preview: Some(Arc::clone(preview)),
        lineage_v52: state.lineage_v52.clone(),
        needs_new_v52: true,
        presentation: Arc::clone(presentation),
        epoch: epoch.ok_or_else(failure)?,
        ticket: Arc::clone(&state.ticket),
        revision: state.revision.ok_or_else(failure)?,
    });
    let request = Arc::new(MetadataMembersNewRequestV52 { original_request });
    if !request.matches(form) {
        return Err(local_failure(
            "Paint the useful exact Members terminal notice before explicit New action",
        ));
    }
    Ok(request)
}
pub(crate) fn accept_new_v52(
    form: &mut Form,
    request: &Arc<MetadataMembersNewRequestV52>,
    fresh: Arc<MetadataMembersPreviewV48>,
) -> bool {
    if !request.matches(form)
        || fresh.notice_v52
        || !Arc::ptr_eq(&fresh.intent, &request.original_request.intent)
        || !Arc::ptr_eq(&fresh.plan, &request.original_request.plan)
        || !Arc::ptr_eq(&fresh.values, &request.original_request.values)
        || fresh.lineage_v52.is_none()
    {
        return false;
    }
    // Only this exact explicit-New result detaches the OLD protected reference
    // from the new live Form. No saved body/ref/origin is modified or replayed.
    form.invalidate_plan();
    let state = state_mut(form).unwrap();
    state.identity = fresh.identity.clone();
    state.key = Some(Arc::clone(&fresh.key));
    state.lineage_v52 = fresh.lineage_v52.clone();
    state.needs_new_v52 = false;
    state.preview = None;
    form.publication = None;
    form.preview = false;
    form.dirty = false;
    form.error.clear();
    form.focus = form.fields.len();
    true
}
