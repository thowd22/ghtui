//! Caller-bracketed fresh phase on the SAME actual original Core. This does
//! not mint another source, queue, clock, provider getter or read fallback.
use super::*;
use crate::providers::gh::{OriginalPrMembershipRefV13, PostWriteCaptureAllowanceV2};

pub(crate) struct ConversationPhaseV17<'a> {
    source: &'a PrCollaborationSourceV17,
    session: OriginalPrAccessSessionV13<'a>,
    measured: GithubProvider,
    parent: Arc<PrResourceParentV1>,
    identity: HostIdentity,
    cancel: &'a CancellationToken,
    deadline: Instant,
    captures: Option<PostWriteCaptureAllowanceV2>,
}
impl PrCollaborationSourceV17 {
    /// The owned provider lives around the callback; neither it nor a pending
    /// session escapes. All field/body/actor/Doc/SQL work belongs IN callback,
    /// before finish's final genuine UID. Later phases get no old-clock lease.
    pub(crate) fn with_phase_v17<T>(
        &self,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        postwrite: Option<&crate::pr_actions::PrOperation>,
        prepare: impl FnOnce(ConversationPhaseV17<'_>) -> GithubResult<T>,
    ) -> GithubResult<T> {
        if database != self.opening.database() || !self.opening.admits_conversation_v51() {
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
        let mut limits = RepositoryCatalogueAccessLimits::default();
        limits.transport.max_document_bytes = 128 * 1024;
        limits.transport.max_response_bytes = 8 * 1024 * 1024;
        limits.max_repository_reads = 2;
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
        let captures = postwrite
            .map(|operation| {
                let caps = crate::providers::gh::conversation_post_caps_v19(operation)?;
                session.reserve_postwrite_capture_v2(caps, 1, cancel)
            })
            .transpose()?;
        let window =
            session.reserve_measured_external_reads(8, 1024 * 1024, 8 * 1024 * 1024, 1, cancel)?;
        let measured = provider.clone().with_measured_read_window(window)?;
        let parent = Arc::new(
            measured.pr_original_detail_parent_v17(
                session
                    .observation_v1(self.current()?.original().id)
                    .ok_or_else(failure)?,
                self.opening.parent(),
                cancel,
            )?,
        );
        if !GithubProvider::same_thread_history_parent_v1(&self.parent, &parent) {
            return Err(failure());
        }
        let identity = session.identity().clone();
        prepare(ConversationPhaseV17 {
            source: self,
            session,
            measured,
            parent,
            identity,
            cancel,
            deadline,
            captures,
        })
    }
}
impl ConversationPhaseV17<'_> {
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn observe(
        &self,
        operation: &crate::pr_actions::PrOperation,
    ) -> GithubResult<crate::providers::gh::FreshConversationPublicationV17> {
        self.measured.observe_conversation_publication_v17(
            self.parent().clone(),
            self.identity(),
            operation,
            self.cancel,
        )
    }
    pub(crate) fn local(&self, database: &Path) -> GithubResult<ResourceLocalGuard> {
        ResourceLocalGuard::capture_boundary_v13(
            database,
            self.source.local_boundary(),
            &self.identity,
            self.parent.summary(),
        )
    }
    pub(crate) fn take_captures(&mut self) -> GithubResult<PostWriteCaptureAllowanceV2> {
        self.captures.take().ok_or_else(failure)
    }
    /// No general measured-provider accessor. Only the closed typed publisher
    /// sees the genuine producer Arc; the callback consumes THIS session.
    #[expect(
        clippy::too_many_arguments,
        reason = "Same original session, sealed cursor, typed observation and exact ACK closure stay explicit; no provider or source escape"
    )]
    pub(crate) fn publish(
        self,
        ordinary: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        actual: &crate::providers::gh::FreshConversationPublicationV17,
        operation: &crate::pr_actions::PrOperation,
        ack: &crate::drafts::DraftPublicationAckV2,
        drafts: &crate::drafts::DraftClient,
        database: &Path,
        key: &str,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> Result<
        crate::providers::gh::ConversationReceiptV17,
        crate::providers::gh::ConversationPublicationFailureV17,
    > {
        let measured = self.measured.clone();
        let cancel = self.cancel;
        let source = self.source;
        ordinary.publish_conversation_v17(&measured, captures, actual, operation, cancel, || {
            let identity = self.finish(guard)?;
            // Queue-only final check after final UID, before invocation.
            // No SQL/body/source read after the source session closes.
            ack.check_current(drafts, database, source.opening.scope(), key, cancel)
                .map_err(|_| failure())?;
            Ok(identity)
        })
    }
    /// Closing raw PR, actual repository, local SQL, final original UID.
    /// No private read/large preparation follows this method's success.
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
        if !GithubProvider::same_thread_history_parent_v1(&self.parent, &closing) {
            return Err(failure());
        }
        let mut session = self.session;
        session.verify(self.source.current()?.original(), self.cancel)?;
        let closed = session.close_with_guard(self.cancel, || {
            live(self.cancel, self.deadline)?;
            guard()
        })?;
        live(self.cancel, self.deadline)?;
        Ok(closed.identity().clone())
    }
}
