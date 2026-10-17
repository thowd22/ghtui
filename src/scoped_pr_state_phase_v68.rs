//! Fresh closed State phase over the SAME original source/Core. The facade
//! exposes neither its measured provider nor a reusable access window.
use super::*;
use crate::{
    pr_state_model::PrStateCommand,
    providers::gh::{
        OriginalPrMembershipRefV13, OriginalPrStateV68, PostWriteCaptureAllowanceV2,
        STATE_POST_CAPS_V68,
    },
};
pub(crate) struct StatePhaseV68<'a> {
    source: &'a PrCollaborationSourceV17,
    session: OriginalPrAccessSessionV13<'a>,
    measured: GithubProvider,
    actual: Arc<OriginalPrStateV68>,
    cancel: &'a CancellationToken,
    deadline: Instant,
    captures: Option<PostWriteCaptureAllowanceV2>,
}
pub(crate) struct StatePhaseRequestV68<'a> {
    pub(crate) provider: &'a GithubProvider,
    pub(crate) database: &'a Path,
    pub(crate) cancel: &'a CancellationToken,
    pub(crate) deadline: Instant,
    pub(crate) command: PrStateCommand,
    pub(crate) postwrite: bool,
}
pub(crate) struct StatePublicationAckV68<'a> {
    pub(crate) permit: &'a crate::pr_write_journal::PrWritePermit,
    pub(crate) ack: &'a crate::drafts::DraftPublicationAckV2,
    pub(crate) drafts: &'a crate::drafts::DraftClient,
    pub(crate) key: &'a str,
}
impl PrCollaborationSourceV17 {
    pub(crate) fn with_state_phase_v68<T>(
        &self,
        request: StatePhaseRequestV68<'_>,
        prepare: impl FnOnce(StatePhaseV68<'_>) -> GithubResult<T>,
    ) -> GithubResult<T> {
        let StatePhaseRequestV68 {
            provider,
            database,
            cancel,
            deadline,
            command,
            postwrite,
        } = request;
        if database != self.opening.database() || !self.opening.admits_state_v68(command) {
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
            Some(session.reserve_postwrite_capture_v2(&STATE_POST_CAPS_V68, 1, cancel)?)
        } else {
            None
        };
        let window =
            session.reserve_measured_external_reads(4, 128 * 1024, 8 * 1024 * 1024, 1, cancel)?;
        let measured = provider.clone().with_measured_read_window(window)?;
        let observed = measured.pr_original_state_read_v68(
            session
                .observation_v1(self.current()?.original().id)
                .ok_or_else(failure)?,
            self.opening.parent(),
            command,
            cancel,
        )?;
        if !GithubProvider::same_thread_history_parent_v1(&self.parent, observed.parent()) {
            return Err(failure());
        }
        let actual =
            Arc::new(measured.observe_original_state_v68(observed, session.identity(), cancel)?);
        // A narrower actual producer I stays the single cutoff everywhere.
        let deadline = deadline
            .min(actual.publication_cutoff_v68())
            .min(captures.as_ref().map_or(deadline, |c| c.deadline()));
        live(cancel, deadline)?;
        prepare(StatePhaseV68 {
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
impl StatePhaseV68<'_> {
    pub(crate) fn actual(&self) -> &Arc<OriginalPrStateV68> {
        &self.actual
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
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
    pub(crate) fn publish(
        self,
        ordinary: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        publication: StatePublicationAckV68<'_>,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> Result<
        crate::providers::gh::StateReceiptV68,
        crate::providers::gh::StatePublicationFailureV68,
    > {
        use crate::providers::gh::{StatePublicationFailureV68, StatePublishInputV68};
        let measured = self.measured.clone();
        let actual = Arc::clone(&self.actual);
        let source = self.source;
        let cancel = self.cancel;
        let current = source
            .current()
            .map_err(StatePublicationFailureV68::NotStarted)?;
        let StatePublicationAckV68 {
            permit,
            ack,
            drafts,
            key,
        } = publication;
        let reference = permit.marker().publication_reference();
        if permit.marker().target.scope() != source.opening.scope()
            || permit.marker().target.node_id() != actual.observed().parent().node_id()
            || permit.marker().selected_resource != key
            || permit.marker().command != actual.observed().command().key()
        {
            return Err(StatePublicationFailureV68::NotStarted(failure()));
        }
        ordinary.publish_original_state_v68(
            StatePublishInputV68 {
                measured: &measured,
                captures,
                actual: &actual,
                current,
                cancel,
                reference: &reference,
            },
            || {
                let identity = self.finish(guard)?;
                ack.check_current(
                    drafts,
                    source.opening.database(),
                    source.opening.scope(),
                    key,
                    cancel,
                )
                .map_err(|_| failure())?;
                Ok(identity)
            },
        )
    }
    /// One closing raw PR role, original repository and final numeric UID.
    /// All SQL/body/actor/Doc/preencoded request preparation precedes this.
    pub(crate) fn finish(
        self,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<HostIdentity> {
        live(self.cancel, self.deadline)?;
        let closing = self.measured.pr_original_detail_parent_v17(
            self.session
                .observation_v1(self.source.current()?.original().id)
                .ok_or_else(failure)?,
            self.source.opening.parent(),
            self.cancel,
        )?;
        if !GithubProvider::same_thread_history_parent_v1(self.actual.observed().parent(), &closing)
        {
            return Err(failure());
        }
        let mut session = self.session;
        session.verify(self.source.current()?.original(), self.cancel)?;
        let closed = session.close_with_guard(self.cancel, || {
            live(self.cancel, self.deadline)?;
            guard()
        })?;
        self.measured.validate_original_state_generation_v68(
            &self.actual,
            self.cancel,
            self.deadline,
        )?;
        live(self.cancel, self.deadline)?;
        Ok(closed.identity().clone())
    }
}
