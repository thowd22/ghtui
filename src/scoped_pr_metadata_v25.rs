//! Closed title/description purpose on the existing Collaboration Form and
//! original source. A restored value/ticket is never source or write authority.
use super::*;
use crate::{
    forms::{Field, PrIntent},
    pr_write_journal::{
        metadata_origin_v25::{draft_key_v25, MetadataOriginSeedV25},
        PrPublicationReference,
    },
    providers::{
        gh::{
            GithubError, GithubErrorKind, GithubProvider, GithubResult,
            PrMetadataTextFieldV25 as FieldKind,
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

#[path = "scoped_pr_metadata_history_v25.rs"]
pub(crate) mod history;
#[path = "scoped_pr_metadata_publication_v25.rs"]
pub(crate) mod publication;
pub(crate) struct MetadataIntentV25 {
    pub(crate) source: Arc<PrCollaborationSourceV17>,
    pub(crate) field: FieldKind,
    pub(crate) key: Arc<str>,
    before: Arc<str>,
    pub(crate) lines: Arc<[String]>,
    pub(crate) author: crate::scoped_pr_collaboration_draft_v17::ConversationActorV17,
}
#[derive(Clone)]
pub(crate) struct MetadataStateV25 {
    pub(crate) intent: Arc<MetadataIntentV25>,
    pub(crate) identity: HostIdentity,
    pub(crate) ticket: Arc<()>,
    pub(crate) revision: Option<u64>,
    pub(crate) preview: Option<Arc<MetadataPreviewV25>>,
}
pub(crate) struct MetadataRequestV25 {
    pub(crate) intent: Arc<MetadataIntentV25>,
    pub(crate) values: Arc<BTreeMap<String, String>>,
    pub(crate) preview: Option<Arc<MetadataPreviewV25>>,
    presentation: Arc<()>,
    epoch: u64,
    ticket: Arc<()>,
    revision: u64,
}
pub(crate) struct MetadataPreviewV25 {
    pub(crate) intent: Arc<MetadataIntentV25>,
    pub(crate) values: Arc<BTreeMap<String, String>>,
    pub(crate) seed: MetadataOriginSeedV25,
    pub(crate) identity: HostIdentity,
    pub(crate) lines: Arc<[String]>,
    pub(crate) display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    useful: AtomicBool,
    pub(crate) used: AtomicBool,
}
pub(crate) const METADATA_NO_CHANGE_V65: &str =
    "No change: requested metadata exactly matches current state";
pub(crate) enum MetadataPreviewDispositionV65 {
    Preview(Arc<MetadataPreviewV25>),
    NoChange,
}
pub(crate) fn failure() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Exact original metadata field/source/draft changed; nothing resent".into(),
    }
}
pub(crate) fn check(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(failure())
    } else {
        Ok(())
    }
}
pub(crate) fn state(form: &Form) -> Option<&MetadataStateV25> {
    match &form.purpose {
        FormPurpose::PrCollaborationV17(s) => match &s.mode {
            ComposerModeV17::MetadataV25(s) => Some(s),
            _ => None,
        },
        _ => None,
    }
}
fn state_mut(form: &mut Form) -> Option<&mut MetadataStateV25> {
    match &mut form.purpose {
        FormPurpose::PrCollaborationV17(s) => match &mut s.mode {
            ComposerModeV17::MetadataV25(s) => Some(s),
            _ => None,
        },
        _ => None,
    }
}
pub(crate) fn open(
    opening: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    field: FieldKind,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Form> {
    let (source, _, (mut form, key, before, lines, author)) = source::open_metadata_text_v25(
        Arc::clone(&opening),
        field,
        provider,
        database,
        cancel,
        deadline,
        |actual, local| {
            check(cancel, deadline)?;
            let parent = actual.observed().parent();
            let key: Arc<str> = draft_key_v25(opening.scope(), parent, field)
                .map_err(|_| failure())?
                .into();
            let before: Arc<str> = Arc::from(actual.observed().value());
            let saved = Cache::open(database)
                .map_err(|_| failure())?
                .bounded_draft(opening.scope(), &key, 128 * 1024)
                .map_err(|_| failure())?;
            let mut value = before.to_string();
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
                    || saved.values.len() != 1
                    || saved.values.get("body").is_none_or(|s| s.len() > 65_536)
                {
                    return Err(failure());
                }
                value = saved.values["body"].clone();
                publication = saved.publication;
                if let Some(reference) = &publication {
                    let journal = crate::pr_write_journal::PrWriteJournal::open(database)
                        .map_err(|_| failure())?;
                    let marker = journal
                        .original_publication(local.target_v17(), reference)
                        .map_err(|_| failure())?
                        .ok_or_else(failure)?;
                    let record = journal
                        .metadata_origin_v25(local.target_v17(), &marker)
                        .map_err(|_| failure())?;
                    if record.seed.field != field || marker.selected_resource != key.as_ref() {
                        return Err(failure());
                    }
                }
            }
            // This clone contains the SAME actually read selected value. The
            // raw producer, not a stripped Description DTO, supplies it.
            let mut target = parent.summary().clone();
            match field {
                FieldKind::Title => target.pr.title = before.to_string(),
                FieldKind::Description => target.pr.body = before.to_string(),
            }
            let mut form = Form::pr(PrIntent {
                kind: field.form_kind(),
                target,
                identity: actual.identity().clone(),
            })
            .map_err(|_| failure())?;
            form.title = format!(
                "Edit PR {} • {}#{}",
                field.label(),
                parent.repository(),
                parent.number()
            );
            form.fields = vec![Field::text(
                "body",
                field.label(),
                &value,
                field == FieldKind::Description,
            )
            .map_err(|_| failure())?];
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
                format!("Edit PR {} • exact one-field effect", field.label()),
                format!("{}#{} • repository #{} / PR #{}", parent.repository(), parent.number(), parent.repository_id(), parent.database_id()),
                format!("Current permission: viewerCanUpdate={} • fresh recheck required", actual.can_update()),
                format!("Current {}: {} UTF-8 bytes{}", field.label(), before.len(), if actual.observed().explicitly_null() { " (explicit server null)" } else { "" }),
                format!("Original H {} • base {}", parent.head_sha(), parent.base_tip_sha()),
                "Preview is read only; only explicit Confirm attempts this one field. Local Git remains unchanged.".into(),
                "Protected references are inspect-only; no restored approval or automatic resend.".into(),
            ].into();
            check(cancel, deadline)?;
            Ok((form, key, before, lines, author))
        },
        |_| Ok(()),
    )?;
    // All body/actor/SQL preparation completed before final original UID.
    let identity = source.identity().clone();
    let intent = Arc::new(MetadataIntentV25 {
        source,
        field,
        key,
        before,
        lines,
        author,
    });
    form.purpose = FormPurpose::PrCollaborationV17(Box::new(ConversationComposerStateV17 {
        mode: ComposerModeV17::MetadataV25(MetadataStateV25 {
            intent,
            identity,
            ticket: Arc::new(()),
            revision: Some(0),
            preview: None,
        }),
    }));
    Ok(form)
}
impl MetadataStateV25 {
    pub(crate) fn invalidate(&mut self) {
        if let Some(preview) = self.preview.take() {
            preview.painted(false);
        }
        self.revision = self.revision.and_then(|v| v.checked_add(1));
    }
}
impl MetadataRequestV25 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        state(form).is_some_and(|s| {
            Arc::ptr_eq(&s.ticket, &self.ticket) && Arc::ptr_eq(&s.intent, &self.intent)
        }) && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
            && form.presentation_v4().1 == Some(self.epoch)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        self.owns(form)
            && form.presentation_v4().1 == Some(self.epoch)
            && form.publication.is_none()
            && state(form).is_some_and(|s| {
                s.revision == Some(self.revision)
                    && match (&s.preview, &self.preview) {
                        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                        (None, None) => true,
                        _ => false,
                    }
            })
            && form.fields.len() == 1
            && form.fields[0].key == "body"
            && self.values.get("body").map(String::as_str)
                == Some(form.fields[0].editor.text.as_str())
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
pub(crate) fn request(form: &Form) -> Result<Arc<MetadataRequestV25>, String> {
    let s = state(form).ok_or("Not exact original metadata Form")?;
    if form.publication.is_some()
        || form.fields.len() != 1
        || form.fields[0].key != "body"
        || form.fields[0].editor.text.len() > 65_536
    {
        return Err("Exact unprotected metadata field required; never resend".into());
    }
    let values = Arc::new(form.values());
    encode_draft_v17(&values, None)?;
    s.intent
        .field
        .validate_requested(&values["body"])
        .map_err(|e| e.to_string())?;
    Ok(Arc::new(MetadataRequestV25 {
        intent: s.intent.clone(),
        values,
        preview: s.preview.clone(),
        presentation: form.presentation_v4().0.clone(),
        epoch: form.presentation_v4().1.ok_or("Metadata input retired")?,
        ticket: s.ticket.clone(),
        revision: s.revision.ok_or("Metadata revision retired")?,
    }))
}
impl MetadataPreviewV25 {
    pub(crate) fn painted(&self, useful: bool) {
        self.useful.store(useful, Ordering::Release);
    }
    #[cfg(test)]
    pub(crate) fn prepare(
        request: &Arc<MetadataRequestV25>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        match Self::prepare_native_v65(request, provider, database, cancel, deadline)? {
            MetadataPreviewDispositionV65::Preview(preview) => Ok(preview),
            MetadataPreviewDispositionV65::NoChange => Err(GithubError {
                kind: GithubErrorKind::InvalidTarget,
                message: METADATA_NO_CHANGE_V65.into(),
            }),
        }
    }
    pub(crate) fn prepare_native_v65(
        request: &Arc<MetadataRequestV25>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<MetadataPreviewDispositionV65> {
        check(cancel, deadline)?;
        let intent = &request.intent;
        let prepared = intent.source.with_metadata_text_phase_v25(
            provider,
            database,
            cancel,
            deadline,
            intent.field,
            false,
            |phase| {
                let local = phase.local(database)?;
                let actual = phase.actual();
                if actual.observed().value() != intent.before.as_ref() {
                    return Err(failure());
                }
                // Source/clock/structural refusal remains fail-closed. Only
                // the actual producer's typed local value/capability refusal
                // survives before the origin encoder's structural defense.
                actual.validate_live_source_v25().map_err(|_| failure())?;
                // This local tag is not inferred from a provider error or its
                // message. After valid input, this exact pure producer call
                // can return InvalidTarget only for unchanged actual bytes.
                intent.field.validate_requested(&request.values["body"])?;
                if let Err(error) = actual.prepare_requested(&request.values["body"]) {
                    if matches!(error.kind, GithubErrorKind::InvalidTarget) {
                        return Ok(None);
                    }
                    return Err(error);
                }
                let seed = MetadataOriginSeedV25::from_producer(
                    &intent.source,
                    actual,
                    &request.values["body"],
                    database,
                )
                .map_err(|_| failure())?;
                if seed.draft_key().map_err(|_| failure())? != intent.key.as_ref() {
                    return Err(failure());
                }
                let mut target = actual.observed().parent().summary().clone();
                match intent.field {
                    FieldKind::Title => target.pr.title = intent.before.to_string(),
                    FieldKind::Description => target.pr.body = intent.before.to_string(),
                }
                let pure = crate::pr_metadata_preview::PrMetadataPreview::prepare(
                    &PrIntent {
                        kind: intent.field.form_kind(),
                        target,
                        identity: actual.identity().clone(),
                    },
                    &request.values,
                )
                .map_err(|_| failure())?;
                let lines: Arc<[String]> = pure.preview().to_vec().into();
                let display = crate::prepared_form_text_v1::PreparedFormTextV1::prepare(
                    &lines, cancel, deadline,
                )
                .map_err(|_| failure())?;
                let identity = phase.finish(|| {
                    local.validate_boundary_v13(database, intent.source.local_boundary())
                })?;
                Ok(Some((seed, identity, lines, display)))
            },
        )?;
        let Some((seed, identity, lines, display)) = prepared else {
            return Ok(MetadataPreviewDispositionV65::NoChange);
        };
        Ok(MetadataPreviewDispositionV65::Preview(Arc::new(Self {
            intent: intent.clone(),
            values: request.values.clone(),
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
    request: &Arc<MetadataRequestV25>,
    preview: Arc<MetadataPreviewV25>,
) -> bool {
    if !request.matches(form)
        || !Arc::ptr_eq(&preview.intent, &request.intent)
        || !Arc::ptr_eq(&preview.values, &request.values)
    {
        return false;
    }
    let display = preview.display.clone();
    let Some(state) = state_mut(form) else {
        return false;
    };
    state.identity = preview.identity.clone();
    state.preview = Some(preview);
    form.preview = true;
    form.preview_requested = false;
    form.submit_requested = false;
    form.preview_scroll = 0;
    form.focus = form.fields.len();
    form.error.clear();
    form.install_prepared_preview_display_v1(display);
    true
}
