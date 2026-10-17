//! Original Issue chooser/selection on the SAME Collaboration Form. Typed
//! choices remain navigation; only the actual selected Core creates Preview.
use super::*;
use crate::{
    forms::{Field, FormAction, PrFormKind, PrIntent},
    pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
    pr_issue_links_v87::IssueLinkCommandV87,
    pr_metadata_picker::{issue_link_v87::IssueLinkPickerV87, PrMetadataPickerOutcome},
    providers::{
        gh::{GithubErrorKind, GithubProvider, GithubResult, IssueLinkCatalogueV87},
        runner::CancellationToken,
    },
    scoped_pr_collaboration_source_v17::{
        issue_links_phase_v87::{self, IssueLinkOpenV87},
        PrCollaborationSourceV17,
    },
    scoped_pr_issue_links_v87::{failure, IssueLinkIntentV87, IssueLinkPreviewV87},
};
use std::{
    path::Path,
    sync::{atomic::Ordering, Weak},
    time::Instant,
};
#[path = "scoped_pr_issue_link_history_ui_v87.rs"]
pub(crate) mod history;

pub(crate) struct IssueLinkFormIntentV87 {
    pub(crate) source: Arc<PrCollaborationSourceV17>,
    pub(crate) catalogue: Arc<IssueLinkCatalogueV87>,
    pub(crate) command: IssueLinkCommandV87,
    pub(crate) author: crate::scoped_pr_collaboration_draft_v17::ConversationActorV17,
    pub(crate) lines: Arc<[String]>,
}
#[derive(Clone)]
pub(crate) struct IssueLinkStateV87 {
    pub(crate) intent: Arc<IssueLinkFormIntentV87>,
    pub(crate) identity: HostIdentity,
    pub(crate) selection: Option<Arc<IssueLinkIntentV87>>,
    selected_value: Option<Arc<str>>,
    pub(crate) key: Option<Arc<str>>,
    pub(crate) preview: Option<Arc<IssueLinkPreviewV87>>,
    pub(crate) ticket: Arc<()>,
    pub(crate) revision: Option<u64>,
    picker: IssueLinkPickerV87,
    picker_open: bool,
    initial_save: Option<InitialIssueSaveV87>,
}
#[derive(Clone)]
struct InitialIssueSaveV87 {
    preview: Weak<IssueLinkPreviewV87>,
    presentation: Weak<()>,
    ticket: Weak<()>,
    epoch: u64,
    revision: u64,
}
pub(crate) struct IssueLinkRequestV87 {
    pub(crate) intent: Arc<IssueLinkFormIntentV87>,
    pub(crate) selection: Arc<IssueLinkIntentV87>,
    pub(crate) values: Arc<BTreeMap<String, String>>,
    pub(crate) preview: Option<Arc<IssueLinkPreviewV87>>,
    presentation: Arc<()>,
    epoch: u64,
    ticket: Arc<()>,
    revision: u64,
}
pub(crate) fn state(form: &Form) -> Option<&IssueLinkStateV87> {
    match &form.purpose {
        FormPurpose::PrCollaborationV17(s) => match &s.mode {
            ComposerModeV17::IssueLinksV87(v) => Some(v),
            _ => None,
        },
        _ => None,
    }
}
fn state_mut(form: &mut Form) -> Option<&mut IssueLinkStateV87> {
    match &mut form.purpose {
        FormPurpose::PrCollaborationV17(s) => match &mut s.mode {
            ComposerModeV17::IssueLinksV87(v) => Some(v),
            _ => None,
        },
        _ => None,
    }
}
impl IssueLinkStateV87 {
    pub(crate) fn invalidate(&mut self) {
        self.initial_save = None;
        if let Some(preview) = self.preview.take() {
            preview.painted(false);
        }
        self.revision = self.revision.and_then(|r| r.checked_add(1));
    }
    pub(crate) fn lines(&self) -> &[String] {
        self.preview
            .as_ref()
            .map_or(&self.intent.lines, |preview| &preview.lines)
    }
    pub(crate) fn needs_new(&self) -> bool {
        self.preview
            .as_ref()
            .is_some_and(|preview| preview.needs_new)
    }
}
#[cfg(test)]
pub(crate) fn open(
    opening: Arc<PrCollaborationOpenIntentV17>,
    command: IssueLinkCommandV87,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Form> {
    match open_result_v102(opening,command,provider,database,cancel,deadline,IssueUiOpenPolicyV102::Legacy)? {
        crate::scoped_pr_collaboration_source_v17::initial_v17::IssueOpenCoreResultV102::Closed(form)=>Ok(form),
        crate::scoped_pr_collaboration_source_v17::initial_v17::IssueOpenCoreResultV102::Unsupported(_)=>Err(failure()),
    }
}
#[derive(Clone, Copy)]
enum IssueUiOpenPolicyV102 {
    #[cfg(test)]
    Legacy,
    NativeSchema,
}
pub(crate) fn open_native_v102(
    opening: Arc<PrCollaborationOpenIntentV17>,
    command: IssueLinkCommandV87,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<
    crate::scoped_pr_collaboration_source_v17::initial_v17::IssueOpenCoreResultV102<Form>,
> {
    open_result_v102(
        opening,
        command,
        provider,
        database,
        cancel,
        deadline,
        IssueUiOpenPolicyV102::NativeSchema,
    )
}
fn open_result_v102(
    opening: Arc<PrCollaborationOpenIntentV17>,
    command: IssueLinkCommandV87,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    policy: IssueUiOpenPolicyV102,
) -> GithubResult<
    crate::scoped_pr_collaboration_source_v17::initial_v17::IssueOpenCoreResultV102<Form>,
> {
    use crate::scoped_pr_collaboration_source_v17::initial_v17::IssueOpenCoreResultV102;
    let prepare =
        |catalogue: &Arc<IssueLinkCatalogueV87>,
         _: &crate::scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard| {
            let parent = catalogue.parent();
            catalogue
                .retained_bound_v87()
                .checked_add(256 * 1024)
                .filter(|n| *n <= 4 * 1024 * 1024)
                .ok_or_else(failure)?;
            let picker = IssueLinkPickerV87::prepare(Arc::clone(catalogue), cancel, deadline)
                .map_err(|_| failure())?;
            let mut form = Form::pr(PrIntent {
                kind: PrFormKind::Title,
                target: parent.summary().clone(),
                identity: opening.parent().identity().clone(),
            })
            .map_err(|_| failure())?;
            form.title = format!(
                "{} • {}#{}",
                command.label(),
                parent.repository(),
                parent.number()
            );
            form.fields =
                vec![Field::text("body", "Selected actual Issue", "", false)
                    .map_err(|_| failure())?];
            form.focus = form.fields.len();
            form.dirty = false;
            let author = crate::scoped_pr_collaboration_draft_v17::prepare_actor_v17(
                &opening.scope().host,
                &parent.summary().pr.author,
                parent.author_account_id(),
                cancel,
                deadline,
            )?;
            let lines:Arc<[String]>=vec![format!("{} • manual development relationship",command.label()),
                format!("Original PR {}#{} • repository #{} / PR #{}",parent.repository(),parent.number(),parent.repository_id(),parent.database_id()),
                format!("Actual Issues {} • membership {}",catalogue.len(),if catalogue.complete(){"complete"}else{"partial: unseen Issues remain unknown"}),
                "Choose an observed Issue. Names, numbers, saved values and a picker index do not grant permission.".into(),
                "Fresh Preview checks BOTH selected objects and all six complete sets. One explicit Confirm attempts one mutation.".into(),
                "No body-keyword edit, automatic closing promise or Projects change. Local Git remains unchanged.".into()].into();
            Ok((form, picker, author, lines))
        };
    let request = IssueLinkOpenV87 {
        opening: Arc::clone(&opening),
        command,
        repository: None,
        max_pages: 4,
    };
    let opened = match policy {
        #[cfg(test)]
        IssueUiOpenPolicyV102::Legacy => {
            IssueOpenCoreResultV102::Closed(issue_links_phase_v87::open_issue_links_v87(
                request, provider, database, cancel, deadline, prepare,
            )?)
        }
        IssueUiOpenPolicyV102::NativeSchema => issue_links_phase_v87::open_issue_links_native_v102(
            request, provider, database, cancel, deadline, prepare,
        )?,
    };
    let (source, catalogue, (mut form, picker, author, lines)) = match opened {
        IssueOpenCoreResultV102::Closed(opened) => opened,
        IssueOpenCoreResultV102::Unsupported(proof) => {
            return Ok(IssueOpenCoreResultV102::Unsupported(proof))
        }
    };
    let identity = source.identity().clone();
    let intent = Arc::new(IssueLinkFormIntentV87 {
        source,
        catalogue,
        command,
        author,
        lines,
    });
    form.purpose = FormPurpose::PrCollaborationV17(Box::new(ConversationComposerStateV17 {
        mode: ComposerModeV17::IssueLinksV87(IssueLinkStateV87 {
            intent,
            identity,
            selection: None,
            selected_value: None,
            key: None,
            preview: None,
            ticket: Arc::new(()),
            revision: Some(0),
            picker,
            picker_open: false,
            initial_save: None,
        }),
    }));
    Ok(IssueOpenCoreResultV102::Closed(form))
}
pub(crate) fn picker(form: &Form) -> Option<&IssueLinkPickerV87> {
    state(form).filter(|s| s.picker_open).map(|s| &s.picker)
}
pub(crate) fn draw_picker(
    frame: &mut ratatui::Frame,
    form: &mut Form,
    area: ratatui::layout::Rect,
    theme: crate::theme::Theme,
) -> bool {
    let Some(state) = state_mut(form).filter(|s| s.picker_open) else {
        return false;
    };
    let hits = state.picker.draw(frame, area, theme);
    form.metadata_picker_hits = hits;
    true
}
pub(crate) fn request(form: &Form) -> GithubResult<Arc<IssueLinkRequestV87>> {
    let state = state(form).ok_or_else(failure)?;
    let selection = state
        .selection
        .as_ref()
        .ok_or_else(|| crate::providers::gh::GithubError {
            kind: GithubErrorKind::InvalidTarget,
            message: "Choose an actual eligible Issue before Preview".into(),
        })?;
    if state.picker_open
        || form.fields.len() != 1
        || form.fields[0].key != "body"
        || form.fields[0].editor.text.len() > 8192
        || state.selected_value.as_deref() != Some(form.fields[0].editor.text.as_str())
    {
        return Err(failure());
    }
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
    crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(&values, form.publication.as_ref())
        .map_err(|_| failure())?;
    Ok(Arc::new(IssueLinkRequestV87 {
        intent: Arc::clone(&state.intent),
        selection: Arc::clone(selection),
        values,
        preview: state.preview.clone(),
        presentation: Arc::clone(form.presentation_v4().0),
        epoch: form.presentation_v4().1.ok_or_else(failure)?,
        ticket: Arc::clone(&state.ticket),
        revision: state.revision.ok_or_else(failure)?,
    }))
}
impl IssueLinkRequestV87 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        state(form).is_some_and(|s| {
            Arc::ptr_eq(&s.intent, &self.intent) && Arc::ptr_eq(&s.ticket, &self.ticket)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        self.owns(form)
            && form.presentation_v4().1 == Some(self.epoch)
            && state(form).is_some_and(|s| {
                !s.picker_open
                    && s.revision == Some(self.revision)
                    && s.selection
                        .as_ref()
                        .is_some_and(|selection| Arc::ptr_eq(selection, &self.selection))
                    && match (&s.preview, &self.preview) {
                        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                        (None, None) => true,
                        _ => false,
                    }
            })
            && form.fields.len() == 1
            && self.values.get("body").map(String::as_str)
                == Some(form.fields[0].editor.text.as_str())
    }
    fn confirm_profile(&self, form: &Form) -> bool {
        self.matches(form)
            && form.publication.is_none()
            && form.preview
            && !form.preview_display_v1.geometry_rejected
            && self.preview.as_ref().is_some_and(|preview| {
                !preview.needs_new
                    && preview.useful.load(Ordering::Acquire)
                    && Arc::ptr_eq(&preview.intent, &self.selection)
                    && Arc::ptr_eq(&preview.values, &self.values)
                    && state(form)
                        .and_then(|s| s.key.as_ref())
                        .is_some_and(|key| Arc::ptr_eq(key, &preview.key))
            })
    }
    pub(crate) fn matches_confirm(&self, form: &Form) -> bool {
        self.confirm_profile(form)
            && self
                .preview
                .as_ref()
                .is_some_and(|p| !p.used.load(Ordering::Acquire))
    }
    pub(crate) fn matches_inflight(&self, form: &Form) -> bool {
        form.submitting && self.confirm_profile(form)
    }
    pub(crate) fn prepare(
        &self,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        new_action: bool,
    ) -> GithubResult<Arc<IssueLinkPreviewV87>> {
        let revision = if new_action {
            Some(
                self.preview
                    .as_ref()
                    .filter(|p| {
                        p.needs_new
                            && p.useful.load(Ordering::Acquire)
                            && !p.used.load(Ordering::Acquire)
                    })
                    .and_then(|p| p.lineage.as_ref())
                    .ok_or_else(failure)?,
            )
        } else {
            None
        };
        if !new_action && self.preview.as_ref().is_some_and(|p| p.needs_new) {
            return Err(failure());
        }
        let preview = IssueLinkPreviewV87::prepare(
            &self.selection,
            provider,
            database,
            cancel,
            deadline,
            revision,
        )?;
        if *preview.values != *self.values {
            return Err(failure());
        }
        Ok(preview)
    }
    pub(crate) fn prepare_native_v102(
        &self,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<crate::scoped_pr_issue_links_v87::IssuePreviewResultV102> {
        if self.preview.as_ref().is_some_and(|p| p.needs_new) {
            return Err(failure());
        }
        let result = IssueLinkPreviewV87::prepare_native_v102(
            &self.selection,
            provider,
            database,
            cancel,
            deadline,
        )?;
        if let crate::scoped_pr_issue_links_v87::IssuePreviewResultV102::Preview(preview) = &result
        {
            if *preview.values != *self.values {
                return Err(failure());
            }
        }
        Ok(result)
    }
}
pub(crate) fn new_ready(form: &Form) -> bool {
    request(form).is_ok_and(|r| {
        r.matches(form)
            && form.preview
            && !form.preview_display_v1.geometry_rejected
            && r.preview.as_ref().is_some_and(|p| {
                p.needs_new
                    && p.useful.load(Ordering::Acquire)
                    && !p.used.load(Ordering::Acquire)
                    && form.publication.as_ref() == p.lineage.as_ref().map(|r| r.reference())
            })
    })
}
pub(crate) fn accept_preview(
    form: &mut Form,
    request: &Arc<IssueLinkRequestV87>,
    preview: Arc<IssueLinkPreviewV87>,
) -> bool {
    if !request.matches(form)
        || !Arc::ptr_eq(&preview.intent, &request.selection)
        || *preview.values != *request.values
    {
        return false;
    }
    let display = Arc::clone(&preview.display);
    let register = !preview.needs_new;
    let notice = preview
        .needs_new
        .then(|| preview.lineage.as_ref().map(|r| r.reference().clone()))
        .flatten();
    let state = state_mut(form).unwrap();
    state.identity = preview.identity.clone();
    state.key = Some(Arc::clone(&preview.key));
    state.initial_save = register.then(|| InitialIssueSaveV87 {
        preview: Arc::downgrade(&preview),
        presentation: Arc::downgrade(&request.presentation),
        ticket: Arc::downgrade(&request.ticket),
        epoch: request.epoch,
        revision: request.revision,
    });
    state.preview = Some(preview);
    form.publication = notice;
    form.preview = true;
    form.preview_requested = false;
    form.submit_requested = false;
    form.dirty = register;
    form.preview_scroll = 0;
    form.focus = form.fields.len();
    form.error.clear();
    form.install_prepared_preview_display_v1(display);
    true
}
pub(crate) fn pending_initial_save(form: &Form) -> bool {
    let Some(s) = state(form) else {
        return false;
    };
    let (Some(save), Some(preview), Some(selection), Some(key)) =
        (&s.initial_save, &s.preview, &s.selection, &s.key)
    else {
        return false;
    };
    form.dirty
        && form.preview
        && !form.submitting
        && form.publication.is_none()
        && !s.picker_open
        && !preview.needs_new
        && !preview.used.load(Ordering::Acquire)
        && save.preview.ptr_eq(&Arc::downgrade(preview))
        && save
            .presentation
            .ptr_eq(&Arc::downgrade(form.presentation_v4().0))
        && save.ticket.ptr_eq(&Arc::downgrade(&s.ticket))
        && form.presentation_v4().1 == Some(save.epoch)
        && s.revision == Some(save.revision)
        && Arc::ptr_eq(&preview.intent, selection)
        && Arc::ptr_eq(&preview.key, key)
        && form.fields.len() == 1
        && form.fields[0].key == "body"
        && preview.values.len() == 1
        && preview.values.get("body").map(String::as_str)
            == Some(form.fields[0].editor.text.as_str())
        && s.selected_value.as_deref() == Some(form.fields[0].editor.text.as_str())
}
pub(crate) fn settle_initial_save(form: &mut Form, accepted: bool) {
    if !pending_initial_save(form) {
        return;
    }
    state_mut(form).unwrap().initial_save = None;
    if !accepted {
        form.invalidate_plan();
    }
}
pub(crate) fn dispatch(form: &mut Form, action: &FormAction) -> bool {
    if state(form).is_none() {
        return false;
    }
    match action {
        FormAction::NewIntent if state(form).is_some_and(|s| s.needs_new()) => {
            if new_ready(form) {
                form.submit_requested = true;
            } else {
                form.error =
                    "Paint the exact Issue terminal notice before explicit New action".into();
            }
            true
        }
        FormAction::OpenPicker(_)
            if !form.preview && !form.submitting && form.publication.is_none() =>
        {
            form.invalidate_plan();
            state_mut(form).unwrap().picker_open = true;
            form.metadata_picker_hits.clear();
            true
        }
        FormAction::MetadataPicker(action) if picker(form).is_some() => {
            let outcome = state_mut(form).unwrap().picker.dispatch(action.clone());
            match outcome {
                PrMetadataPickerOutcome::Chosen(index) => {
                    let state = state(form).unwrap();
                    let result = (|| {
                        let row = state.intent.catalogue.row_v87(index).ok_or_else(failure)?;
                        if !row.enabled {
                            return Err(failure());
                        }
                        let selection = IssueLinkIntentV87::choose(
                            Arc::clone(&state.intent.source),
                            Arc::clone(&state.intent.catalogue),
                            index,
                            state.intent.command,
                        )?;
                        // A bounded desired value only. Exact producer seed must
                        // corroborate all seven entries during the next Core.
                        let value = serde_json::to_string(&(
                            87,
                            state.intent.command.command(),
                            row.repository_id,
                            row.repository,
                            row.id,
                            row.node,
                            row.number,
                        ))
                        .map_err(|_| failure())?;
                        if value.len() > 8192 {
                            return Err(failure());
                        }
                        let editor =
                            crate::editor::TextBuffer::new(&value).map_err(|_| failure())?;
                        Ok((selection, value, editor))
                    })();
                    match result {
                        Ok((selection, value, editor)) => {
                            form.invalidate_plan();
                            form.fields[0].editor = editor;
                            let state = state_mut(form).unwrap();
                            state.selection = Some(selection);
                            state.selected_value = Some(Arc::from(value));
                            state.key = None;
                            state.picker_open = false;
                            form.dirty = false;
                            form.error.clear();
                            form.focus = form.fields.len();
                        }
                        Err(error) => form.error = error.message,
                    }
                }
                PrMetadataPickerOutcome::Closed => state_mut(form).unwrap().picker_open = false,
                PrMetadataPickerOutcome::Changed => {}
            }
            form.metadata_picker_hits.clear();
            true
        }
        FormAction::Preview if form.publication.is_none() => {
            if request(form).is_ok() {
                form.preview_requested = true;
            } else {
                form.error = "Choose an actual Issue before fresh Preview".into();
            }
            true
        }
        FormAction::Submit => {
            if request(form).is_ok_and(|request| request.matches_confirm(form)) {
                form.submit_requested = true;
            } else {
                form.error = "Fresh useful exact Issue Preview required before one Confirm".into();
            }
            true
        }
        FormAction::Editor | FormAction::Choice(_, _) => true,
        _ => false,
    }
}
