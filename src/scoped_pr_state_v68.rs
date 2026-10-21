//! Bodyless Close/Reopen on the SAME Collaboration Form. Source, current
//! capabilities and exact journal lineage are always actual producer data.
use super::*;
#[path = "scoped_pr_state_history_v74.rs"]
pub(crate) mod history_v74;
use crate::{
    forms::FormAction,
    pr_state_model::PrStateCommand,
    pr_write_journal::{
        state_origin_v68::{
            lineage_v68::{StateActionBaseV68, StateTerminalRevisionV68},
            state_draft_key_v68, StateOriginSeedV68,
        },
        PrPublicationReference, PrWriteJournal,
    },
    providers::{
        gh::{GithubError, GithubErrorKind, GithubProvider, GithubResult, StateCapabilityV68},
        runner::CancellationToken,
    },
    scoped_pr_collaboration_source_v17::{
        self as source, state_phase_v68::StatePhaseRequestV68, PrCollaborationSourceV17,
    },
    storage::Cache,
};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
pub(crate) struct StateIntentV68 {
    pub(crate) source: Arc<PrCollaborationSourceV17>,
    pub(crate) command: PrStateCommand,
    pub(crate) author: crate::scoped_pr_collaboration_draft_v17::ConversationActorV17,
    pub(crate) title: String,
    lines: Arc<[String]>,
    base: Option<StateActionBaseV68>,
    capability: StateCapabilityV68,
    blocked: bool,
    /// Shared lifecycle ink for the observed-state row (TASK-25 AC6).
    observed_v25: crate::theme::SemanticToken,
}
#[derive(Clone)]
pub(crate) struct StateComposerStateV68 {
    pub(crate) intent: Arc<StateIntentV68>,
    pub(crate) identity: HostIdentity,
    pub(crate) key: Arc<str>,
    pub(super) ticket: Arc<()>,
    revision: Option<u64>,
    pub(crate) preview: Option<Arc<StatePreviewV68>>,
    pub(crate) needs_new: bool,
    lineage: Option<Arc<StateTerminalRevisionV68>>,
}
pub(crate) struct StateRequestV68 {
    pub(crate) intent: Arc<StateIntentV68>,
    pub(crate) key: Arc<str>,
    pub(crate) values: Arc<BTreeMap<String, String>>,
    pub(crate) preview: Option<Arc<StatePreviewV68>>,
    presentation: Arc<()>,
    epoch: u64,
    ticket: Arc<()>,
    revision: u64,
    lineage: Option<Arc<StateTerminalRevisionV68>>,
    needs_new: bool,
}
pub(crate) struct StatePreviewV68 {
    pub(crate) intent: Arc<StateIntentV68>,
    pub(crate) key: Arc<str>,
    pub(crate) values: Arc<BTreeMap<String, String>>,
    pub(crate) seed: StateOriginSeedV68,
    pub(crate) identity: HostIdentity,
    pub(crate) lines: Arc<[String]>,
    pub(crate) display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    pub(crate) useful: AtomicBool,
    pub(crate) used: AtomicBool,
    lineage: Option<Arc<StateTerminalRevisionV68>>,
    notice: bool,
}
/// A local Save refusal is not a provider/source failure and grants no plan.
pub(crate) enum StatePreviewPreparationV95 {
    Ready(Arc<StatePreviewV68>),
    SaveRefused,
}
pub(crate) fn failure() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Exact original State/source/input changed; no fallback or resend".into(),
    }
}
fn local(message: &str) -> GithubError {
    GithubError {
        kind: GithubErrorKind::InvalidTarget,
        message: message.into(),
    }
}
pub(crate) fn save_refusal_v78() -> GithubError {
    local(
        "Save the exact bodyless State intent successfully before Preview; nothing queried or sent",
    )
}
/// SAME real DraftWriter producer cutoff, never the logical Form revision or
/// saved flags. Only the actual owner captures this prequeue token.
pub(crate) struct StatePreviewSaveV78<'a> {
    pub(crate) drafts: &'a crate::drafts::DraftClient,
    pub(crate) cutoff: &'a crate::drafts::ConversationOpenReadCutoffV20,
}
impl StatePreviewSaveV78<'_> {
    fn check(&self, cancel: &CancellationToken) -> GithubResult<()> {
        self.cutoff
            .check(self.drafts, cancel)
            .map_err(|_| save_refusal_v78())
    }
    fn check_body(
        &self,
        request: &StateRequestV68,
        database: &Path,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.check(cancel)?;
        let body = Cache::open(database)
            .map_err(|_| save_refusal_v78())?
            .bounded_draft(request.intent.source.opening().scope(), &request.key, 8192)
            .map_err(|_| save_refusal_v78())?
            .ok_or_else(save_refusal_v78)?;
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Envelope {
            version: u8,
            values: BTreeMap<String, String>,
            publication: Option<PrPublicationReference>,
        }
        let parsed: Envelope = serde_json::from_str(&body).map_err(|_| save_refusal_v78())?;
        if parsed.version != 17 || !parsed.values.is_empty() || parsed.publication.is_some() {
            return Err(save_refusal_v78());
        }
        self.check(cancel)
    }
}
const ACCOUNT_PREFIX_V25: &str = "Account: ";
const ACCOUNT_ROW_V25: usize = 1;
const OBSERVED_PREFIX_V25: &str = "Observed ";
const OBSERVED_ROW_V25: usize = 4;
const HEAD_PREFIX_V25: &str = "Current H ";
const HEAD_ROW_V25: usize = 5;
/// TASK-25 AC6: the State composer names the requesting account on its own
/// preview row with the same filled initials chip as the PR action previews
/// (`[AB] login` under NO_COLOR). Display only; it grants nothing.
pub(crate) fn account_paint_v25(form: &Form) -> Option<crate::pr_actions::PrPreviewPaintV1> {
    let state = state(form)?;
    Some(crate::pr_actions::PrPreviewPaintV1 {
        host: state.identity.host.clone(),
        actors: vec![crate::pr_actions::PrPreviewActorRowV1 {
            row: ACCOUNT_ROW_V25,
            prefix: ACCOUNT_PREFIX_V25,
            actor: crate::providers::gh::Actor {
                login: state.identity.login.clone(),
                actor_type: "User".into(),
                ..Default::default()
            },
        }],
        // The observed PR state keeps its text label and takes the shared
        // lifecycle ink; the current head takes the Head ink. A NEEDS NEW
        // ACTION notice has other rows, so only matching rows are painted.
        states: [
            (
                OBSERVED_ROW_V25,
                OBSERVED_PREFIX_V25,
                state.intent.observed_v25,
            ),
            (
                HEAD_ROW_V25,
                HEAD_PREFIX_V25,
                crate::theme::SemanticToken::Head,
            ),
        ]
        .into_iter()
        .filter(|(row, prefix, _)| {
            state
                .lines()
                .get(*row)
                .is_some_and(|line| line.starts_with(prefix))
        })
        .map(|(row, _, token)| (row, token))
        .collect(),
    })
}
pub(crate) fn state(form: &Form) -> Option<&StateComposerStateV68> {
    match &form.purpose {
        FormPurpose::PrCollaborationV17(s) => match &s.mode {
            ComposerModeV17::StateV68(v) => Some(v),
            _ => None,
        },
        _ => None,
    }
}
fn state_mut(form: &mut Form) -> Option<&mut StateComposerStateV68> {
    match &mut form.purpose {
        FormPurpose::PrCollaborationV17(s) => match &mut s.mode {
            ComposerModeV17::StateV68(v) => Some(v),
            _ => None,
        },
        _ => None,
    }
}
impl StateComposerStateV68 {
    pub(crate) fn invalidate(&mut self) {
        if let Some(preview) = self.preview.take() {
            preview.painted(false);
        }
        self.revision = self.revision.and_then(|n| n.checked_add(1));
    }
    pub(crate) fn ready(&self) -> bool {
        self.revision.is_some() && self.preview.is_some() && !self.needs_new
    }
    pub(crate) fn lines(&self) -> &[String] {
        self.preview
            .as_ref()
            .map_or(&self.intent.lines, |p| &p.lines)
    }
}
pub(crate) fn open(
    opening: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    command: PrStateCommand,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Form> {
    if PrWriteJournal::open(database)
        .map_err(|_| failure())?
        .state_open_requires_history_v75(&opening, command)
        .map_err(|_| failure())?
    {
        return history_v74::open(
            source::state_v68::StateOpenRequestV68 { opening, command },
            provider,
            database,
            cancel,
            deadline,
            None,
        );
    }
    let (
        source,
        _,
        (key, base, capability, publication, restored, blocked, title, lines, author, observed_v25),
    ) = source::state_v68::open_state_v68(
        source::state_v68::StateOpenRequestV68 {
            opening: Arc::clone(&opening),
            command,
        },
        provider,
        database,
        cancel,
        deadline,
        |actual, local_guard, base| {
            let parent = actual.observed().parent();
            let key =
                state_draft_key_v68(opening.scope(), parent, command).map_err(|_| failure())?;
            let saved = Cache::open(database)
                .map_err(|_| failure())?
                .bounded_draft(opening.scope(), &key, 128 * 1024)
                .map_err(|_| failure())?;
            let journal = PrWriteJournal::open(database).map_err(|_| failure())?;
            let blocked = journal
                .blocking(local_guard.target_v17())
                .map_err(|_| failure())?
                .is_some();
            let publication = if let Some(saved) = &saved {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Envelope {
                    version: u8,
                    values: BTreeMap<String, String>,
                    publication: Option<PrPublicationReference>,
                }
                let saved: Envelope = serde_json::from_str(saved).map_err(|_| failure())?;
                if saved.version != 17 || !saved.values.is_empty() {
                    return Err(failure());
                }
                if let Some(reference) = &saved.publication {
                    let marker = journal
                        .original_publication(local_guard.target_v17(), reference)
                        .map_err(|_| failure())?
                        .ok_or_else(failure)?;
                    journal
                        .state_origin_v68(local_guard.target_v17(), &marker)
                        .map_err(|_| failure())?;
                }
                saved.publication
            } else {
                None
            };
            let title = format!(
                "{} PR • {}#{}",
                command.label(),
                parent.repository(),
                parent.number()
            );
            let lines:Arc<[String]>=vec![
                format!("{} • exact one PR lifecycle effect",command.label()),
                // Row ACCOUNT_ROW_V25: painted with the shared actor chip.
                format!("{ACCOUNT_PREFIX_V25}{} on {} • numeric identity {}",
                    crate::git::display_bytes(actual.identity().login.as_bytes()),
                    actual.identity().host,
                    actual.identity().account_id.map_or_else(|| "unavailable".into(), |id| id.to_string())),
                format!("{}#{} • repository #{} / PR #{}",parent.repository(),parent.number(),parent.repository_id(),parent.database_id()),
                format!("Independent issue #{} • {}",actual.observed().issue_id(),actual.capability().label()),
                format!("Observed {} • requested {} • draft {} • merged {}",parent.summary().pr.state,
                    if command==PrStateCommand::Close {"closed"} else {"open"},parent.summary().pr.draft,parent.summary().merged),
                format!("Current H {} • B {}",parent.head_sha(),parent.base_tip_sha()),
                "Bodyless action • no comment, merge, branch deletion or local Git changes.".into(),
                "Preview is read only. Confirm rechecks current access and sends one attempt; no expected-head CAS is claimed.".into(),
                "Protected old refs are inspect-only; local ACK never proves remote success.".into(),
            ].into();
            let author = crate::scoped_pr_collaboration_draft_v17::prepare_actor_v17(
                &actual.identity().host,
                &parent.summary().pr.author,
                parent.author_account_id(),
                cancel,
                actual.publication_cutoff_v68(),
            )?;
            Ok((
                Arc::<str>::from(key),
                base.cloned(),
                actual.capability(),
                publication,
                saved.is_some(),
                blocked,
                title,
                lines,
                author,
                crate::pr_actions::pr_lifecycle_state_token(
                    &parent.summary().pr.state,
                    parent.summary().pr.draft,
                    parent.summary().merged,
                ),
            ))
        },
        |_| Ok(()),
    )?;
    let identity = source.identity().clone();
    let eligible = base.is_some();
    let intent = Arc::new(StateIntentV68 {
        source,
        command,
        author,
        title,
        lines,
        base,
        capability,
        blocked,
        observed_v25,
    });
    let mut form = Form::pr_state_v68(StateComposerStateV68 {
        intent,
        identity,
        key,
        ticket: Arc::new(()),
        revision: Some(0),
        preview: None,
        needs_new: false,
        lineage: None,
    });
    form.publication = publication;
    form.restored_draft = restored;
    form.dirty = form.publication.is_none() && !blocked && eligible;
    Ok(form)
}
pub(crate) fn request(form: &Form) -> GithubResult<Arc<StateRequestV68>> {
    let s = state(form).ok_or_else(failure)?;
    if !form.fields.is_empty()
        || form.metadata_picker.is_some()
        || form.picker.is_some()
        || form.publication.is_some() && !s.needs_new
    {
        return Err(local(
            "State original is bodyless and protected refs are inspect-only",
        ));
    }
    if s.intent.blocked && !s.needs_new {
        return Err(local("Inspect the unresolved PR publication first"));
    }
    let (presentation, epoch) = form.presentation_v4();
    Ok(Arc::new(StateRequestV68 {
        intent: Arc::clone(&s.intent),
        key: Arc::clone(&s.key),
        values: s
            .preview
            .as_ref()
            .map_or_else(|| Arc::new(BTreeMap::new()), |p| Arc::clone(&p.values)),
        preview: s.preview.clone(),
        presentation: Arc::clone(presentation),
        epoch: epoch.ok_or_else(failure)?,
        ticket: Arc::clone(&s.ticket),
        revision: s.revision.ok_or_else(failure)?,
        lineage: s.lineage.clone(),
        needs_new: s.needs_new,
    }))
}
impl StateRequestV68 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        state(form).is_some_and(|s| {
            Arc::ptr_eq(&s.intent, &self.intent) && Arc::ptr_eq(&s.ticket, &self.ticket)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
            && form.presentation_v4().1 == Some(self.epoch)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        self.owns(form)
            && form.fields.is_empty()
            && self.values.is_empty()
            && form.publication.is_none()
            && form.metadata_picker.is_none()
            && form.picker.is_none()
            && state(form).is_some_and(|s| {
                s.revision == Some(self.revision)
                    && Arc::ptr_eq(&s.key, &self.key)
                    && match (&s.preview, &self.preview) {
                        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                        (None, None) => true,
                        _ => false,
                    }
            })
    }
    pub(crate) fn matches_confirm(&self, form: &Form) -> bool {
        self.matches_confirm_profile_v74(form)
            && self
                .preview
                .as_ref()
                .is_some_and(|p| !p.used.load(Ordering::Acquire))
    }
    pub(crate) fn matches_inflight_confirm_v74(&self, form: &Form) -> bool {
        form.submitting && self.matches_confirm_profile_v74(form)
    }
    fn matches_confirm_profile_v74(&self, form: &Form) -> bool {
        self.matches(form)
            && !self.needs_new
            && form.preview
            && !form.preview_display_v1.geometry_rejected
            && self.preview.as_ref().is_some_and(|p| {
                Arc::ptr_eq(&p.intent, &self.intent)
                    && Arc::ptr_eq(&p.key, &self.key)
                    && Arc::ptr_eq(&p.values, &self.values)
                    && !p.notice
                    && p.useful.load(Ordering::Acquire)
            })
    }
}
impl StatePreviewV68 {
    pub(crate) fn is_new_result_v68(&self, request: &StateNewRequestV68) -> bool {
        !self.notice
            && self.lineage.is_some()
            && Arc::ptr_eq(&self.intent, &request.original_request.intent)
            && Arc::ptr_eq(&self.values, &request.original_request.values)
    }
    pub(crate) fn painted(&self, useful: bool) {
        self.useful.store(useful, Ordering::Release);
    }
    #[cfg(test)]
    pub(crate) fn prepare(
        request: &Arc<StateRequestV68>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        saved: &StatePreviewSaveV78<'_>,
    ) -> GithubResult<Arc<Self>> {
        match Self::prepare_with_save_disposition_v95(
            request, provider, database, cancel, deadline, saved,
        )? {
            StatePreviewPreparationV95::Ready(preview) => Ok(preview),
            StatePreviewPreparationV95::SaveRefused => Err(save_refusal_v78()),
        }
    }
    pub(crate) fn prepare_with_save_disposition_v95(
        request: &Arc<StateRequestV68>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        saved: &StatePreviewSaveV78<'_>,
    ) -> GithubResult<StatePreviewPreparationV95> {
        if request.needs_new {
            return Err(local(
                "Choose explicit New action before another State Preview",
            ));
        }
        request
            .intent
            .source
            .validate_local_draft_recovery_v20(database, cancel, deadline)?;
        if saved.check_body(request, database, cancel).is_err() {
            return Ok(StatePreviewPreparationV95::SaveRefused);
        }
        Self::prepare_phase(
            request,
            provider,
            database,
            cancel,
            deadline,
            false,
            Some(saved),
        )
        .map(StatePreviewPreparationV95::Ready)
    }
    fn prepare_phase(
        request: &Arc<StateRequestV68>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        explicit_new: bool,
        saved: Option<&StatePreviewSaveV78<'_>>,
    ) -> GithubResult<Arc<Self>> {
        let intent = &request.intent;
        let base = intent
            .base
            .as_ref()
            .ok_or_else(|| local("Actual State capability unavailable; no new write is offered"))?;
        let (seed,key,identity,lines,display,lineage,notice)=intent.source.with_state_phase_v68(
            StatePhaseRequestV68 {provider,database,cancel,deadline,command:intent.command,postwrite:false},|phase| {
                let actual=phase.actual();actual.require()?;
                let local_guard=phase.local(database)?;
                let seed=StateOriginSeedV68::from_producer(&intent.source,actual,database).map_err(|_|failure())?;
                let journal=PrWriteJournal::open(database).map_err(|_|failure())?;
                let revision=journal.state_action_revision_v68(base).map_err(|_|failure())?;
                let (seed,notice)=match (&request.lineage,&revision,explicit_new) {
                    (Some(expected),Some(current),true) if request.needs_new=>{
                        journal.validate_state_revision_v68(base,expected).map_err(|_|failure())?;
                        (StateOriginSeedV68::from_revision_v68(seed,base,current).map_err(|_|failure())?,false)
                    }
                    (Some(expected),Some(current),false) if !request.needs_new=>{
                        journal.validate_state_revision_v68(base,expected).map_err(|_|failure())?;
                        (StateOriginSeedV68::from_revision_v68(seed,base,current).map_err(|_|failure())?,false)
                    }
                    (None,Some(_),false)=>(seed,true),
                    (None,None,false)=>(seed,false),
                    _=>return Err(failure()),
                };
                let key:Arc<str>=seed.draft_key_v68().map_err(|_|failure())?.into();
                if !notice&&!explicit_new&&key.as_ref()!=request.key.as_ref() {return Err(failure());}
                let lines:Arc<[String]>=if notice {let r=revision.as_ref().ok_or_else(failure)?;
                    vec!["NEEDS NEW ACTION • no write or resend".into(),r.label().into(),
                        format!("Exact terminal attempt #{} • old ref/body/origin remain inspect-only",r.reference().attempt_id),
                        "Choose New action for a NEW current read; this never restores approval.".into()].into()
                } else {intent.lines.clone()};
                let display=crate::prepared_form_text_v1::PreparedFormTextV1::prepare(&lines,cancel,phase.deadline()).map_err(|_|failure())?;
                let identity=phase.finish(|| {
                    if let Some(saved)=saved {saved.check(cancel)?;}
                    local_guard.validate_boundary_v13(database,intent.source.local_boundary())?;
                    if let Some(r)=&revision {journal.validate_state_revision_v68(base,r).map_err(|_|failure())?;}
                    Ok(())
                })?;
                Ok((seed,key,identity,lines,display,revision,notice))
            })?;
        if let Some(saved) = saved {
            saved.check(cancel)?;
        }
        Ok(Arc::new(Self {
            intent: Arc::clone(intent),
            key,
            values: Arc::clone(&request.values),
            seed,
            identity,
            lines,
            display,
            useful: AtomicBool::new(false),
            used: AtomicBool::new(false),
            lineage,
            notice,
        }))
    }
    pub(crate) fn bind_fresh_seed(
        &self,
        current: StateOriginSeedV68,
        database: &Path,
    ) -> GithubResult<StateOriginSeedV68> {
        if self.notice {
            return Err(failure());
        }
        match (&self.intent.base, &self.lineage) {
            (Some(base), Some(revision)) => {
                PrWriteJournal::open(database)
                    .map_err(|_| failure())?
                    .validate_state_revision_v68(base, revision)
                    .map_err(|_| failure())?;
                StateOriginSeedV68::from_revision_v68(current, base, revision)
                    .map_err(|_| failure())
            }
            (Some(_), None) => Ok(current),
            _ => Err(failure()),
        }
    }
}
pub(crate) fn accept_preview(
    form: &mut Form,
    request: &Arc<StateRequestV68>,
    preview: Arc<StatePreviewV68>,
) -> bool {
    if !request.matches(form)
        || !Arc::ptr_eq(&request.intent, &preview.intent)
        || !Arc::ptr_eq(&request.values, &preview.values)
    {
        return false;
    }
    install(form, preview);
    true
}
fn install(form: &mut Form, preview: Arc<StatePreviewV68>) {
    let display = Arc::clone(&preview.display);
    let s = state_mut(form).expect("Closed State result admitted by exact request");
    s.identity = preview.identity.clone();
    s.key = Arc::clone(&preview.key);
    s.lineage = preview.lineage.clone();
    s.needs_new = preview.notice;
    let reference = preview.notice.then(|| {
        preview
            .lineage
            .as_ref()
            .expect("Typed notice has exact terminal")
            .reference()
            .clone()
    });
    s.preview = Some(preview);
    form.publication = reference;
    form.preview = true;
    form.preview_requested = false;
    form.submit_requested = false;
    // The actual root Open registered its bodyless key before this Preview.
    // Do not dirty a ready Preview: Main's ordinary save invalidation would
    // correctly retire it. Explicit-New successor registration has its own
    // ordinary unapproved-Intent save contract; no generic dirty bypass.
    form.dirty = false;
    form.preview_scroll = 0;
    form.focus = form.fields.len();
    form.error.clear();
    form.install_prepared_preview_display_v1(display);
}
pub(crate) fn dispatch(form: &mut Form, action: &FormAction) -> bool {
    if state(form).is_none() {
        return false;
    }
    match action {
        FormAction::NewIntent if state(form).is_some_and(|s| s.needs_new) => {
            match new_request(form) {
                Ok(_) => form.submit_requested = true,
                Err(e) => form.error = e.message,
            }
            true
        }
        FormAction::Editor | FormAction::Choice(_, _) | FormAction::OpenPicker(_) => true,
        FormAction::Preview if !form.preview && form.publication.is_none() => {
            if state(form)
                .is_some_and(|s| s.intent.capability != StateCapabilityV68::Observed(true))
            {
                form.error =
                    "Actual command capability unavailable or denied; no State work queued".into();
            } else {
                match request(form) {
                    Ok(_) => form.preview_requested = true,
                    Err(e) => form.error = e.message,
                }
            }
            true
        }
        FormAction::Submit => {
            if request(form).is_ok_and(|r| r.matches_confirm(form)) {
                form.submit_requested = true;
            } else {
                form.error =
                    "Fresh useful exact bodyless State Preview required before Confirm".into();
            }
            true
        }
        _ => false,
    }
}
pub(crate) struct StateNewRequestV68 {
    pub(crate) original_request: Arc<StateRequestV68>,
}
impl StateNewRequestV68 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        self.original_request.owns(form)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        let r = &self.original_request;
        r.owns(form)
            && form.preview
            && !form.preview_display_v1.geometry_rejected
            && form.fields.is_empty()
            && r.values.is_empty()
            && r.needs_new
            && r.preview.as_ref().is_some_and(|p| {
                p.notice
                    && p.useful.load(Ordering::Acquire)
                    && !p.used.load(Ordering::Acquire)
                    && p.lineage.as_ref().map(|l| l.reference()) == form.publication.as_ref()
            })
            && state(form).is_some_and(|s| {
                s.needs_new
                    && s.revision == Some(r.revision)
                    && s.preview
                        .as_ref()
                        .zip(r.preview.as_ref())
                        .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
            })
    }
    pub(crate) fn prepare(
        &self,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<StatePreviewV68>> {
        StatePreviewV68::prepare_phase(
            &self.original_request,
            provider,
            database,
            cancel,
            deadline,
            true,
            None,
        )
    }
}
pub(crate) fn new_request(form: &Form) -> GithubResult<Arc<StateNewRequestV68>> {
    let request = Arc::new(StateNewRequestV68 {
        original_request: request(form)?,
    });
    if !request.matches(form) {
        return Err(local(
            "Paint the exact State terminal notice before explicit New action",
        ));
    }
    Ok(request)
}
pub(crate) fn accept_new(
    form: &mut Form,
    request: &Arc<StateNewRequestV68>,
    fresh: Arc<StatePreviewV68>,
) -> bool {
    if !request.matches(form)
        || fresh.notice
        || fresh.lineage.is_none()
        || !Arc::ptr_eq(&fresh.intent, &request.original_request.intent)
        || !Arc::ptr_eq(&fresh.values, &request.original_request.values)
    {
        return false;
    }
    form.invalidate_plan();
    let s = state_mut(form).expect("Exact State New request owns this Form");
    s.identity = fresh.identity.clone();
    s.key = Arc::clone(&fresh.key);
    s.lineage = fresh.lineage.clone();
    s.needs_new = false;
    s.preview = None;
    form.publication = None;
    form.preview = false;
    // Fresh New proved this successor key before UID, but did NOT approve a
    // Preview. Ordinary Main invalidation/Save is safe and required here.
    form.dirty = true;
    form.error.clear();
    form.focus = form.fields.len();
    true
}
