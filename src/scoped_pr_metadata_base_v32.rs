//! Closed Base member Form on the existing Collaboration owner and publisher.
//! Stored names/tips are inert draft hints, never selectable-member authority.
use super::*;
use crate::{
    forms::{Field, FormAction, PrFormKind, PrIntent},
    pr_metadata_picker::{base_v32::BasePickerV32, PrMetadataPickerOutcome},
    pr_write_journal::{
        metadata_base_origin_v32::{draft_key_v32, BaseOriginSeedV32},
        PrPublicationReference,
    },
    providers::{
        gh::{
            BaseCatalogueLimitsV32, BaseCatalogueV32, BaseSelectionV32, GithubError,
            GithubErrorKind, GithubProvider, GithubResult,
        },
        runner::CancellationToken,
    },
    scoped_pr_collaboration_source_v17::{self as source, PrCollaborationSourceV17},
    storage::Cache,
};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

#[path = "scoped_pr_metadata_base_history_v32.rs"]
pub(crate) mod history;
#[path = "scoped_pr_metadata_base_publication_v32.rs"]
pub(crate) mod publication;

pub(crate) struct BaseIntentV32 {
    pub(crate) source: Arc<PrCollaborationSourceV17>,
    pub(crate) catalogue: Arc<BaseCatalogueV32>,
    pub(crate) key: Arc<str>,
    pub(crate) lines: Arc<[String]>,
    pub(crate) author: crate::scoped_pr_collaboration_draft_v17::ConversationActorV17,
}
#[derive(Clone)]
pub(crate) struct BaseStateV32 {
    pub(crate) intent: Arc<BaseIntentV32>,
    pub(crate) identity: HostIdentity,
    pub(crate) ticket: Arc<()>,
    pub(crate) revision: Option<u64>,
    pub(crate) preview: Option<Arc<BasePreviewV32>>,
    pub(crate) selection: Option<Arc<BaseSelectionV32>>,
    picker: BasePickerV32,
    picker_open: bool,
}
pub(crate) struct BaseRequestV32 {
    pub(crate) intent: Arc<BaseIntentV32>,
    pub(crate) values: Arc<BTreeMap<String, String>>,
    pub(crate) selection: Arc<BaseSelectionV32>,
    pub(crate) preview: Option<Arc<BasePreviewV32>>,
    ticket: Arc<()>,
    presentation: Arc<()>,
    epoch: u64,
    revision: u64,
}
pub(crate) struct BasePreviewV32 {
    pub(crate) intent: Arc<BaseIntentV32>,
    pub(crate) values: Arc<BTreeMap<String, String>>,
    pub(crate) selection: Arc<BaseSelectionV32>,
    pub(crate) seed: BaseOriginSeedV32,
    pub(crate) identity: HostIdentity,
    pub(crate) lines: Arc<[String]>,
    pub(crate) display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    useful: AtomicBool,
    pub(crate) used: AtomicBool,
}
pub(crate) enum BasePreviewResultV39 {
    Prepared(Arc<BasePreviewV32>),
    NoChange(Arc<source::metadata_base_phase_v32::BaseNoChangeV39>),
}
pub(crate) fn failure() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Exact original Base member/source/draft changed; nothing resent".into(),
    }
}
pub(crate) fn check(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(failure())
    } else {
        Ok(())
    }
}
pub(crate) fn state(form: &Form) -> Option<&BaseStateV32> {
    match &form.purpose {
        FormPurpose::PrCollaborationV17(s) => match &s.mode {
            ComposerModeV17::MetadataBaseV32(s) => Some(s),
            _ => None,
        },
        _ => None,
    }
}
fn state_mut(form: &mut Form) -> Option<&mut BaseStateV32> {
    match &mut form.purpose {
        FormPurpose::PrCollaborationV17(s) => match &mut s.mode {
            ComposerModeV17::MetadataBaseV32(s) => Some(s),
            _ => None,
        },
        _ => None,
    }
}
pub(crate) fn open(
    opening: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Form> {
    let (source, catalogue, (mut form, key, lines, author, picker)) =
        source::metadata_base_phase_v32::open_base_catalogue_v32(
            Arc::clone(&opening),
            provider,
            database,
            cancel,
            deadline,
            BaseCatalogueLimitsV32::default(),
            |catalogue, local| {
                check(cancel, deadline)?;
                let actual = catalogue.actual();
                let parent = actual.parent();
                let key: Arc<str> = draft_key_v32(opening.scope(), parent)
                    .map_err(|_| failure())?
                    .into();
                let saved = Cache::open(database)
                    .map_err(|_| failure())?
                    .bounded_draft(opening.scope(), &key, 128 * 1024)
                    .map_err(|_| failure())?;
                let mut values = BTreeMap::from([
                    ("base".into(), String::new()),
                    ("tip".into(), String::new()),
                ]);
                let mut publication = None;
                if let Some(saved) = &saved {
                    #[derive(serde::Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Envelope {
                        version: u8,
                        values: BTreeMap<String, String>,
                        publication: Option<PrPublicationReference>,
                    }
                    let saved: Envelope = serde_json::from_str(saved).map_err(|_| failure())?;
                    if saved.version != 17
                        || saved.values.len() != 2
                        || saved.values.get("base").is_none_or(|s| s.len() > 1024)
                        || saved.values.get("tip").is_none_or(|s| s.len() > 64)
                    {
                        return Err(failure());
                    }
                    if let Some(reference) = &saved.publication {
                        let journal = crate::pr_write_journal::PrWriteJournal::open(database)
                            .map_err(|_| failure())?;
                        let marker = journal
                            .original_publication(local.target_v17(), reference)
                            .map_err(|_| failure())?
                            .ok_or_else(failure)?;
                        journal
                            .metadata_base_origin_v32(local.target_v17(), &marker)
                            .map_err(|_| failure())?;
                        if marker.selected_resource != key.as_ref() {
                            return Err(failure());
                        }
                    }
                    values = saved.values;
                    publication = saved.publication;
                }
                let mut form = Form::pr(PrIntent {
                    kind: PrFormKind::Base,
                    target: parent.summary().clone(),
                    identity: actual.identity().clone(),
                })
                .map_err(|_| failure())?;
                form.title = format!(
                    "Change PR Base • {}#{}",
                    parent.repository(),
                    parent.number()
                );
                form.fields = vec![
                    Field::text(
                        "base",
                        "Selected existing base branch (F2 Choose)",
                        &values["base"],
                        false,
                    )
                    .map_err(|_| failure())?,
                    Field::text(
                        "tip",
                        "Exact observed selected tip (read only)",
                        &values["tip"],
                        false,
                    )
                    .map_err(|_| failure())?,
                ];
                form.publication = publication;
                form.restored_draft = saved.is_some();
                form.dirty = form.publication.is_none();
                let author = crate::scoped_pr_collaboration_draft_v17::prepare_actor_v17(
                    &actual.identity().host,
                    &parent.summary().pr.author,
                    parent.author_account_id(),
                    cancel,
                    deadline,
                )?;
                let lines: Arc<[String]> = vec![
                "Change PR Base • actual existing branch in the SAME base repository".into(),
                format!("{}#{} • repository #{} / PR #{}", parent.repository(), parent.number(), parent.repository_id(), parent.database_id()),
                format!("Current Base: {} at {}", parent.summary().base_ref, parent.base_tip_sha()),
                format!("Current permission: viewerCanUpdate={} • fresh recheck required", actual.can_update()),
                format!("Original head {} stays unchanged; base comparison will change", parent.head_sha()),
                "F2 Choose selects an actually observed branch; restored text is NOT a selection.".into(),
                "Preview/Confirm recheck the exact tip. Protection is descriptive, not permission. Local Git remains unchanged.".into(),
            ].into();
                let picker = BasePickerV32::prepare(Arc::clone(catalogue), cancel, deadline)
                    .map_err(|_| failure())?;
                check(cancel, deadline)?;
                Ok((form, key, lines, author, picker))
            },
            |_| Ok(()),
        )?;
    let identity = source.identity().clone();
    let protected = form.publication.is_some();
    let intent = Arc::new(BaseIntentV32 {
        source,
        catalogue,
        key,
        lines,
        author,
    });
    form.purpose = FormPurpose::PrCollaborationV17(Box::new(ConversationComposerStateV17 {
        mode: ComposerModeV17::MetadataBaseV32(BaseStateV32 {
            intent,
            identity,
            ticket: Arc::new(()),
            revision: Some(0),
            preview: None,
            selection: None,
            picker,
            picker_open: !protected,
        }),
    }));
    Ok(form)
}
impl BaseStateV32 {
    pub(crate) fn invalidate(&mut self) {
        if let Some(p) = self.preview.take() {
            p.painted(false);
        }
        self.revision = self.revision.and_then(|v| v.checked_add(1));
    }
}
impl BaseRequestV32 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        state(form).is_some_and(|s| {
            Arc::ptr_eq(&s.ticket, &self.ticket) && Arc::ptr_eq(&s.intent, &self.intent)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        self.owns(form)
            && form.presentation_v4().1 == Some(self.epoch)
            && form.publication.is_none()
            && state(form).is_some_and(|s| {
                !s.picker_open
                    && s.revision == Some(self.revision)
                    && s.selection
                        .as_ref()
                        .is_some_and(|v| Arc::ptr_eq(v, &self.selection))
                    && match (&s.preview, &self.preview) {
                        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                        (None, None) => true,
                        _ => false,
                    }
            })
            && form.fields.len() == 2
            && form.fields.iter().all(|f| {
                self.values.get(&f.key).map(String::as_str) == Some(f.editor.text.as_str())
            })
    }
    pub(crate) fn matches_confirm(&self, form: &Form) -> bool {
        self.matches(form)
            && form.preview
            && !form.preview_display_v1.geometry_rejected
            && self
                .preview
                .as_ref()
                .is_some_and(|p| p.useful.load(Ordering::Acquire))
    }
}
pub(crate) fn request(form: &Form) -> Result<Arc<BaseRequestV32>, String> {
    let s = state(form).ok_or("Not exact original Base Form")?;
    if form.publication.is_some() || s.picker_open || form.fields.len() != 2 {
        return Err("Choose a fresh actual Base member; protected attempts never resend".into());
    }
    let selection = s
        .selection
        .as_ref()
        .ok_or("Choose an actual member with F2; stored text cannot grant eligibility")?;
    let values = Arc::new(bounded_values_v32(form)?);
    if values.len() != 2
        || values.get("base").map(String::as_str) != Some(selection.name())
        || values.get("tip").map(String::as_str) != Some(selection.tip())
    {
        return Err("Exact selected Base member differs from Form".into());
    }
    crate::scoped_pr_collaboration_draft_v17::encode_base_draft_v32(&values, None)?;
    Ok(Arc::new(BaseRequestV32 {
        intent: s.intent.clone(),
        values,
        selection: selection.clone(),
        preview: s.preview.clone(),
        ticket: s.ticket.clone(),
        presentation: form.presentation_v4().0.clone(),
        epoch: form.presentation_v4().1.ok_or("Base input retired")?,
        revision: s.revision.ok_or("Base revision retired")?,
    }))
}
/// Validate the genuine two-field Form on borrowed text BEFORE values() copies
/// any raw field. This is shared by request capture and ordinary autosave.
pub(crate) fn bounded_values_v32(form: &Form) -> Result<BTreeMap<String, String>, String> {
    let [base, tip] = form.fields.as_slice() else {
        return Err("Exact bounded Base draft fields required".into());
    };
    if state(form).is_none()
        || base.key != "base"
        || tip.key != "tip"
        || base.editor.text.len() > 1024
        || tip.editor.text.len() > 64
    {
        return Err("Exact bounded Base draft fields required".into());
    }
    Ok(form.values())
}
impl BasePreviewV32 {
    pub(crate) fn painted(&self, useful: bool) {
        self.useful.store(useful, Ordering::Release);
    }
    pub(crate) fn prepare(
        request: &Arc<BaseRequestV32>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<BasePreviewResultV39> {
        check(cancel, deadline)?;
        let intent = &request.intent;
        let prepared = intent.source.with_base_preview_v39(provider, database, cancel, deadline, &request.selection, |phase| {
            let local = phase.local(database)?;
            // Preserve typed local NoChange/Permission after the genuine raw
            // producer, never convert source/identity drift to a local error.
            phase.actual().require_change(&request.selection)?;
            let seed = BaseOriginSeedV32::from_producer(&intent.source, phase.actual(), &request.selection, database).map_err(|_| failure())?;
            if seed.draft_key().map_err(|_| failure())? != intent.key.as_ref() { return Err(failure()); }
            let parent = phase.actual().parent();
            let lines: Arc<[String]> = vec![
                "BASE PREVIEW • one explicit Confirm attempts this comparison change".into(),
                format!("{}#{} • PR node {} / repository #{}", parent.repository(), parent.number(), parent.node_id(), parent.repository_id()),
                format!("BEFORE Base {} at {}", parent.summary().base_ref, parent.base_tip_sha()),
                format!("AFTER Base {} at {}", crate::git::display_bytes(request.selection.name().as_bytes()), request.selection.tip()),
                format!("Head {} remains unchanged. Review/Inline comparison freshness will retire together.", parent.head_sha()),
                "Only the base field is sent. Current branch tip and viewerCanUpdate were independently rechecked.".into(),
                "No local Git mutation. Receipt is observation, not causality; uncertainty is inspect-only, never resend.".into(),
            ].into();
            let display = crate::prepared_form_text_v1::PreparedFormTextV1::prepare(&lines, cancel, deadline).map_err(|_| failure())?;
            let identity = phase.finish(|| local.validate_boundary_v13(database, intent.source.local_boundary()))?;
            Ok((seed, identity, lines, display))
        })?;
        let (seed, identity, lines, display) = match prepared {
            source::metadata_base_phase_v32::BasePhaseResultV39::Prepared(value) => value,
            source::metadata_base_phase_v32::BasePhaseResultV39::NoChange(proof) => {
                return Ok(BasePreviewResultV39::NoChange(proof))
            }
        };
        Ok(BasePreviewResultV39::Prepared(Arc::new(Self {
            intent: intent.clone(),
            values: request.values.clone(),
            selection: request.selection.clone(),
            seed,
            identity,
            lines,
            display,
            useful: AtomicBool::new(false),
            used: AtomicBool::new(false),
        })))
    }
}
pub(crate) fn accept_preview(
    form: &mut Form,
    request: &Arc<BaseRequestV32>,
    preview: Arc<BasePreviewV32>,
) -> bool {
    if !request.matches(form)
        || !Arc::ptr_eq(&preview.intent, &request.intent)
        || !Arc::ptr_eq(&preview.values, &request.values)
        || !Arc::ptr_eq(&preview.selection, &request.selection)
    {
        return false;
    }
    let display = preview.display.clone();
    let Some(s) = state_mut(form) else {
        return false;
    };
    s.identity = preview.identity.clone();
    s.preview = Some(preview);
    form.preview = true;
    form.preview_requested = false;
    form.submit_requested = false;
    form.preview_scroll = 0;
    form.focus = form.fields.len();
    form.error.clear();
    form.install_prepared_preview_display_v1(display);
    true
}
pub(crate) fn picker(form: &Form) -> Option<&BasePickerV32> {
    state(form).filter(|s| s.picker_open).map(|s| &s.picker)
}
pub(crate) fn draw_picker(
    frame: &mut ratatui::Frame,
    form: &mut Form,
    area: ratatui::layout::Rect,
    theme: crate::theme::Theme,
) -> bool {
    let Some(s) = state_mut(form).filter(|s| s.picker_open) else {
        return false;
    };
    let hits = s.picker.draw(frame, area, theme);
    form.metadata_picker_hits = hits;
    true
}
pub(crate) fn dispatch(form: &mut Form, action: &FormAction) -> bool {
    if state(form).is_none() {
        return false;
    }
    match action {
        FormAction::OpenPicker(_)
            if !form.preview && !form.submitting && form.publication.is_none() =>
        {
            if let Some(s) = state_mut(form) {
                s.picker_open = true;
                s.invalidate();
            }
            form.metadata_picker_hits.clear();
            true
        }
        FormAction::MetadataPicker(action) if picker(form).is_some() => {
            let outcome = state_mut(form).unwrap().picker.dispatch(action.clone());
            match outcome {
                PrMetadataPickerOutcome::Chosen(index) => {
                    let selected = state(form).unwrap().intent.catalogue.choose(index);
                    match selected {
                        Ok(selection) => {
                            let values: BTreeMap<String, String> = BTreeMap::from([
                                ("base".into(), selection.name().into()),
                                ("tip".into(), selection.tip().into()),
                            ]);
                            let editors = values
                                .iter()
                                .map(|(key, value)| {
                                    crate::editor::TextBuffer::new(value)
                                        .map(|editor| (key, editor))
                                })
                                .collect::<Result<Vec<_>, _>>();
                            match editors {
                                Ok(editors) => {
                                    for (key, editor) in editors {
                                        if let Some(f) =
                                            form.fields.iter_mut().find(|f| &f.key == key)
                                        {
                                            f.editor = editor;
                                        }
                                    }
                                    let s = state_mut(form).unwrap();
                                    s.selection = Some(selection);
                                    s.picker_open = false;
                                    s.invalidate();
                                    form.dirty = true;
                                    form.invalidate_plan();
                                    form.metadata_picker_hits.clear();
                                }
                                Err(error) => form.error = error,
                            }
                        }
                        Err(error) => form.error = error.message,
                    }
                }
                PrMetadataPickerOutcome::Closed => {
                    state_mut(form).unwrap().picker_open = false;
                    form.metadata_picker_hits.clear();
                }
                PrMetadataPickerOutcome::Changed => form.metadata_picker_hits.clear(),
            }
            true
        }
        FormAction::Submit => {
            if request(form).is_ok_and(|r| r.matches_confirm(form)) {
                form.submit_requested = true;
            } else {
                form.error = "Paint the exact fresh useful Base Preview before Confirm".into();
            }
            true
        }
        FormAction::Editor | FormAction::Choice(_, _) => true,
        _ => false,
    }
}
