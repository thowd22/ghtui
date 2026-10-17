//! Title/description facade on the SAME actual original Core and source.
//! Three measured children plus the fixed prepaid cursor, not a provider API.
use super::*;
use crate::{
    drafts::{DraftClient, DraftPublicationAckV2},
    providers::gh::{
        OriginalMetadataTextV25, OriginalPrMembershipRefV13, PostWriteCaptureAllowanceV2,
        PrMetadataTextFailureV25, PrMetadataTextFieldV25, PrMetadataTextReceiptV25,
        METADATA_TAB_TEXT_POST_CAPS_V51, METADATA_TEXT_POST_CAPS_V25,
    },
};

pub(crate) struct MetadataTextPhaseV25<'a> {
    source: &'a PrCollaborationSourceV17,
    session: OriginalPrAccessSessionV13<'a>,
    measured: GithubProvider,
    actual: Arc<OriginalMetadataTextV25>,
    cancel: &'a CancellationToken,
    deadline: Instant,
    captures: Option<PostWriteCaptureAllowanceV2>,
}
impl PrCollaborationSourceV17 {
    #[expect(
        clippy::too_many_arguments,
        reason = "One original source/Core, closed selected field and explicit phase clock/token; no provider escape"
    )]
    pub(crate) fn with_metadata_text_phase_v25<T>(
        &self,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        field: PrMetadataTextFieldV25,
        postwrite: bool,
        prepare: impl FnOnce(MetadataTextPhaseV25<'_>) -> GithubResult<T>,
    ) -> GithubResult<T> {
        if database != self.opening.database() || !self.opening.admits_metadata_field_v51(field) {
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
        let captures = if postwrite {
            let caps: &[usize] = if self.opening.metadata_tab_v51() {
                &METADATA_TAB_TEXT_POST_CAPS_V51
            } else {
                &METADATA_TEXT_POST_CAPS_V25
            };
            Some(session.reserve_postwrite_capture_v2(caps, 1, cancel)?)
        } else {
            None
        };
        let window =
            session.reserve_measured_external_reads(3, 128 * 1024, 8 * 1024 * 1024, 1, cancel)?;
        let measured = provider.clone().with_measured_read_window(window)?;
        let observed = measured.pr_original_metadata_text_v25(
            session
                .observation_v1(self.current()?.original().id)
                .ok_or_else(failure)?,
            self.opening.parent(),
            field,
            cancel,
        )?;
        if !GithubProvider::same_thread_history_parent_v1(&self.parent, observed.parent()) {
            return Err(failure());
        }
        let actual = Arc::new(measured.observe_original_metadata_text_v25(
            observed,
            session.identity(),
            cancel,
        )?);
        prepare(MetadataTextPhaseV25 {
            source: self,
            session,
            measured,
            actual,
            cancel,
            deadline,
            captures,
        })
    }
}
impl MetadataTextPhaseV25<'_> {
    pub(crate) fn effective_deadline_v48(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn actual(&self) -> &Arc<OriginalMetadataTextV25> {
        &self.actual
    }
    pub(crate) fn local(&self, database: &Path) -> GithubResult<ResourceLocalGuard> {
        ResourceLocalGuard::capture_boundary_v13(
            database,
            self.source.local_boundary(),
            self.actual.identity(),
            self.actual.observed().parent().summary(),
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
        let closing = self.measured.pr_original_metadata_text_v25(
            self.session
                .observation_v1(self.source.current()?.original().id)
                .ok_or_else(failure)?,
            self.source.opening.parent(),
            self.actual.observed().field(),
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
        live(self.cancel, self.deadline)?;
        self.measured.validate_original_metadata_generation_v25(
            &self.actual,
            self.cancel,
            self.deadline,
        )?;
        Ok(closed.identity().clone())
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Exact original ACK/body key/cursor and final SQL witness close before the one typed field write"
    )]
    pub(crate) fn publish(
        self,
        ordinary: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        requested: &str,
        ack: &DraftPublicationAckV2,
        drafts: &DraftClient,
        database: &Path,
        key: &str,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> Result<PrMetadataTextReceiptV25, PrMetadataTextFailureV25> {
        let measured = self.measured.clone();
        let actual = Arc::clone(&self.actual);
        let source = self.source;
        let cancel = self.cancel;
        let current = source
            .current()
            .map_err(PrMetadataTextFailureV25::NotStarted)?;
        ordinary.publish_original_metadata_text_v25(
            &measured,
            captures,
            &actual,
            current,
            source.opening.parent(),
            requested,
            cancel,
            || {
                let identity = self.finish(guard)?;
                ack.check_current(drafts, database, source.opening.scope(), key, cancel)
                    .map_err(|_| failure())?;
                Ok(identity)
            },
        )
    }
}
