//! Base-only facade on the SAME original Core, measured allocation and cursor.
//! A chosen catalogue member is data; every phase re-observes its exact tip.
use super::*;
use crate::providers::gh::{
    BaseCatalogueLimitsV32, BaseCatalogueV32, BaseSelectionV32, OriginalMetadataBaseV32,
    OriginalPrMembershipRefV13, PostWriteCaptureAllowanceV2, PrMetadataBaseFailureV32,
    PrMetadataBaseReceiptV32, METADATA_BASE_POST_CAPS_V32,
};

#[expect(
    clippy::too_many_arguments,
    reason = "One original queued Core and exact bounded preparation/final SQL closure"
)]
pub(crate) fn open_base_catalogue_v32<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    limits: BaseCatalogueLimitsV32,
    prepare: impl FnOnce(&Arc<BaseCatalogueV32>, &ResourceLocalGuard) -> GithubResult<T>,
    validate: impl Fn(&T) -> GithubResult<()>,
) -> GithubResult<(Arc<PrCollaborationSourceV17>, Arc<BaseCatalogueV32>, T)> {
    let completed = initial_v17::run_metadata_base_catalogue_v32(
        Arc::clone(&opening),
        limits,
        provider,
        database,
        cancel,
        deadline,
        |mut read| {
            let local = ResourceLocalGuard::capture_boundary_v13(
                database,
                read.boundary,
                read.identity,
                read.parent.summary(),
            )?;
            let catalogue = read.base_catalogue_v32(limits)?;
            let prepared = prepare(&catalogue, &local)?;
            Ok((local, catalogue, prepared))
        },
        |(local, _, prepared), boundary| {
            local.validate_boundary_v13(database, boundary)?;
            validate(prepared)
        },
    )?;
    let (local_recovery_v20, catalogue, prepared) = completed.material;
    provider.validate_original_base_generation_v32(catalogue.actual(), cancel, deadline)?;
    let source = Arc::new(PrCollaborationSourceV17 {
        opening,
        parent: completed.parent,
        identity: completed.identity,
        closed: completed.closed,
        local_recovery_v20,
    });
    Ok((source, catalogue, prepared))
}

pub(crate) struct MetadataBasePhaseV32<'a> {
    source: &'a PrCollaborationSourceV17,
    session: OriginalPrAccessSessionV13<'a>,
    measured: GithubProvider,
    actual: Arc<OriginalMetadataBaseV32>,
    selection: Arc<BaseSelectionV32>,
    cancel: &'a CancellationToken,
    deadline: Instant,
    captures: Option<PostWriteCaptureAllowanceV2>,
}
/// Refusal-only result issued at the actual raw/capability check. No final
/// close/source/Preview/write permission is represented by this observation.
pub(crate) struct BaseNoChangeV39 {
    source: Arc<PrCollaborationSourceV17>,
    actual: Arc<OriginalMetadataBaseV32>,
    selected: Arc<BaseSelectionV32>,
    cancel: CancellationToken,
    deadline: Instant,
}
impl BaseNoChangeV39 {
    pub(crate) fn matches(
        &self,
        source: &Arc<PrCollaborationSourceV17>,
        selected: &Arc<BaseSelectionV32>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> bool {
        Arc::ptr_eq(&self.source, source)
            && Arc::ptr_eq(&self.selected, selected)
            && self.cancel.same_instance(cancel)
            && !cancel.is_cancelled()
            && deadline <= self.deadline
            && Instant::now() < deadline.min(self.deadline).min(self.actual.deadline())
            && GithubProvider::same_thread_history_parent_v1(source.parent(), self.actual.parent())
            && selected.name() == self.actual.parent().summary().base_ref
    }
}
pub(crate) enum BasePhaseResultV39<T> {
    Prepared(T),
    NoChange(Arc<BaseNoChangeV39>),
}
impl PrCollaborationSourceV17 {
    #[expect(
        clippy::too_many_arguments,
        reason = "One source/Core, actual selected member and explicit original queued phase"
    )]
    pub(crate) fn with_metadata_base_phase_v32<T>(
        &self,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        selection: &Arc<BaseSelectionV32>,
        postwrite: bool,
        prepare: impl FnOnce(MetadataBasePhaseV32<'_>) -> GithubResult<T>,
    ) -> GithubResult<T> {
        match self.with_base_phase_v39(
            provider, database, cancel, deadline, selection, postwrite, None, prepare,
        )? {
            BasePhaseResultV39::Prepared(value) => Ok(value),
            BasePhaseResultV39::NoChange(_) => Err(failure()),
        }
    }
    pub(crate) fn with_base_preview_v39<T>(
        self: &Arc<Self>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        selection: &Arc<BaseSelectionV32>,
        prepare: impl FnOnce(MetadataBasePhaseV32<'_>) -> GithubResult<T>,
    ) -> GithubResult<BasePhaseResultV39<T>> {
        self.with_base_phase_v39(
            provider,
            database,
            cancel,
            deadline,
            selection,
            false,
            Some(self),
            prepare,
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Same private phase; only actual Preview may carry its original Arc into a refusal-only NoChange proof"
    )]
    fn with_base_phase_v39<T>(
        &self,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        selection: &Arc<BaseSelectionV32>,
        postwrite: bool,
        preview_source: Option<&Arc<Self>>,
        prepare: impl FnOnce(MetadataBasePhaseV32<'_>) -> GithubResult<T>,
    ) -> GithubResult<BasePhaseResultV39<T>> {
        if preview_source.is_some_and(|source| !std::ptr::eq(self, source.as_ref()))
            || postwrite && preview_source.is_some()
        {
            return Err(failure());
        }
        if database != self.opening.database() || !self.opening.admits_metadata_base_v51() {
            return Err(failure());
        }
        let deadline = deadline.min(
            provider
                .original_pr_read_deadline_v13()
                .ok_or_else(failure)?,
        );
        live(cancel, deadline)?;
        let provider = provider
            .clone()
            .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)?;
        let limits = RepositoryCatalogueAccessLimits {
            transport: crate::providers::gh::RepositoryCatalogueLimits {
                max_document_bytes: 128 * 1024,
                max_response_bytes: 8 * 1024 * 1024,
                ..Default::default()
            },
            max_repository_reads: 2,
            ..Default::default()
        };
        let original = match &self.closed {
            ClosedOriginV17::Catalogue(_) => OriginalPrMembershipRefV13::Catalogue(
                self.opening
                    .catalogue_source()
                    .ok_or_else(failure)?
                    .source(),
            ),
            ClosedOriginV17::Personal(boundary) => {
                if !boundary.admits_producer(&provider) {
                    return Err(failure());
                }
                OriginalPrMembershipRefV13::Personal(boundary.original())
            }
        };
        let mut session = provider.begin_original_pr_access_v13(original, &limits, cancel)?;
        session.verify(self.current()?.original(), cancel)?;
        if crate::storage::Scope::github(&session.identity().host, session.identity().account_id)
            .map_err(|_| failure())?
            != *self.opening.scope()
        {
            return Err(failure());
        }
        // The Metadata-tab-only16 profile retains the original12 for actual
        // Summary/Conversation. Both are carved from the SAME8MiB once.
        let captures = if postwrite {
            let caps: &[usize] = if self.opening.metadata_tab_v51() {
                &crate::providers::gh::METADATA_TAB_TEXT_POST_CAPS_V51
            } else {
                &METADATA_BASE_POST_CAPS_V32
            };
            Some(session.reserve_postwrite_capture_v2(caps, 1, cancel)?)
        } else {
            None
        };
        let window =
            session.reserve_measured_external_reads(5, 128 * 1024, 8 * 1024 * 1024, 1, cancel)?;
        let measured = provider.clone().with_measured_read_window(window)?;
        let observed = measured.pr_original_metadata_base_v32(
            session
                .observation_v1(self.current()?.original().id)
                .ok_or_else(failure)?,
            self.opening.parent(),
            cancel,
        )?;
        if !GithubProvider::same_thread_history_parent_v1(&self.parent, observed.parent()) {
            return Err(failure());
        }
        let actual = Arc::new(measured.observe_original_metadata_base_v32(
            observed,
            session.identity(),
            cancel,
        )?);
        if let Some(source) = preview_source {
            if actual.observed_no_change_v39(selection)? {
                return Ok(BasePhaseResultV39::NoChange(Arc::new(BaseNoChangeV39 {
                    source: source.clone(),
                    actual,
                    selected: selection.clone(),
                    cancel: cancel.clone(),
                    deadline,
                })));
            }
        }
        actual.require_change(selection)?;
        measured.validate_selected_metadata_base_v32(&actual, selection, cancel)?;
        prepare(MetadataBasePhaseV32 {
            source: self,
            session,
            measured,
            actual,
            selection: Arc::clone(selection),
            cancel,
            deadline,
            captures,
        })
        .map(BasePhaseResultV39::Prepared)
    }
}
impl MetadataBasePhaseV32<'_> {
    pub(crate) fn actual(&self) -> &Arc<OriginalMetadataBaseV32> {
        &self.actual
    }
    #[cfg(test)]
    pub(crate) fn selection(&self) -> &Arc<BaseSelectionV32> {
        &self.selection
    }
    pub(crate) fn local(&self, database: &Path) -> GithubResult<ResourceLocalGuard> {
        ResourceLocalGuard::capture_boundary_v13(
            database,
            self.source.local_boundary(),
            self.actual.identity(),
            self.actual.parent().summary(),
        )
    }
    pub(crate) fn take_captures(&mut self) -> GithubResult<PostWriteCaptureAllowanceV2> {
        self.captures.take().ok_or_else(failure)
    }
    pub(crate) fn finish(
        self,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<HostIdentity> {
        live(self.cancel, self.deadline)?;
        self.measured.validate_selected_metadata_base_v32(
            &self.actual,
            &self.selection,
            self.cancel,
        )?;
        let closing = self.measured.pr_original_metadata_base_v32(
            self.session
                .observation_v1(self.source.current()?.original().id)
                .ok_or_else(failure)?,
            self.source.opening.parent(),
            self.cancel,
        )?;
        if !self.actual.observed().same_observation(&closing) {
            return Err(failure());
        }
        let mut session = self.session;
        session.verify(self.source.current()?.original(), self.cancel)?;
        let closed = session.close_with_guard(self.cancel, || {
            live(self.cancel, self.deadline)?;
            guard()
        })?;
        self.measured.validate_original_base_generation_v32(
            &self.actual,
            self.cancel,
            self.deadline,
        )?;
        Ok(closed.identity().clone())
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Same exact ACK/body key/source and original SQL closure before the sole typed Base write"
    )]
    pub(crate) fn publish(
        self,
        ordinary: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        ack: &crate::drafts::DraftPublicationAckV2,
        drafts: &crate::drafts::DraftClient,
        database: &Path,
        key: &str,
        before_close: impl FnOnce() -> GithubResult<()>,
    ) -> Result<PrMetadataBaseReceiptV32, PrMetadataBaseFailureV32> {
        let measured = self.measured.clone();
        let actual = Arc::clone(&self.actual);
        let selection = Arc::clone(&self.selection);
        let source = self.source;
        let cancel = self.cancel;
        ordinary.publish_original_metadata_base_v32(
            &measured,
            captures,
            &actual,
            source
                .current()
                .map_err(PrMetadataBaseFailureV32::NotStarted)?,
            &selection,
            cancel,
            || {
                let identity = self.finish(before_close)?;
                ack.check_current(drafts, database, source.opening.scope(), key, cancel)
                    .map_err(|_| failure())?;
                Ok(identity)
            },
        )
    }
}
