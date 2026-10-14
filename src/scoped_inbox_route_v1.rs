//! Genuine current-ID scoped inbox worker, with original-owner runtime routing.
//! Intent/source metadata is never permissions; every request creates an actual
//! fresh access session. No stale/name-only cache fallback or alternate writer.
//! Admission also requires the separately reviewed ExactScoped inbox schema
//! policy; an outer access session cannot repair malformed legacy coverage.
use crate::{
    active_remote_scope_v1::{ActiveRemoteScopeV1, RemoteScopeConsumerV1},
    github_workspace::GithubWorkspace,
    providers::{
        gh::{
            ClosedCatalogueAccess, GithubError, GithubErrorKind, GithubProvider, HostIdentity,
            RepositoryCatalogueAccessLimits, RepositoryCatalogueAccessSession,
        },
        runner::CancellationToken,
    },
    repository_catalogue_intent_v1::IntentReference,
    storage::{Cache, CatalogueIntentBoundaryError, CatalogueIntentSource},
};
use std::{path::Path, sync::Arc};

const CHILD_CAPTURE_BYTES: usize = 1024 * 1024;
const WHOLE_CAPTURE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug)]
pub enum ScopedInboxFailureV1 {
    OriginalPrincipal(GithubErrorKind),
    ScopeUnavailable,
    OriginalReferenceUnavailable,
    CaptureReservationUnavailable,
    CurrentReadUnavailable,
    LegacyReadUnavailable,
    CanonicalChanged,
    OriginalBoundary(CatalogueIntentBoundaryError),
    ClosingPrincipal(GithubErrorKind),
    ClosedNavigationUnavailable,
}
impl std::fmt::Display for ScopedInboxFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::OriginalPrincipal(_) | Self::ClosingPrincipal(_) => "Scoped inbox principal unavailable • previous private pages hidden",
            Self::ScopeUnavailable => "Explicit nonempty observed scope required • no default or narrower query chosen",
            Self::OriginalReferenceUnavailable => "Original scope/source reference mismatch • no cached name or different ID admitted",
            Self::CaptureReservationUnavailable => "Whole scoped inbox capture reservation cannot fit • no repository or inbox query sent",
            Self::CurrentReadUnavailable => "Original repository read capability unavailable • no grant or empty membership guessed",
            Self::LegacyReadUnavailable => "Fresh exact scoped inbox unavailable • no old named cache or partial stale page admitted",
            Self::CanonicalChanged => "Canonical repository/access changed during collection • scoped page hidden",
            Self::OriginalBoundary(_) => "Original repository intent boundary changed • no guard rebased • scoped page hidden",
            Self::ClosedNavigationUnavailable => "Closed original navigation cannot be published • private scoped page hidden",
        })
    }
}
impl std::error::Error for ScopedInboxFailureV1 {}

/// A successfully closed read, NOT mutation authority or a renewed source Read.
/// Root must still match original runtime Scope/source owner and cancellation.
/// Legacy refresh may commit derivative inbox SQL before the closing original
/// intent guard. A later refusal admits no page but does not roll that SQL back.
pub struct ScopedInboxResultV1 {
    pub workspace: GithubWorkspace,
    pub current_scope: Arc<ActiveRemoteScopeV1>,
    pub closed_identity: HostIdentity,
    /// Shared external CAPACITY/request maximum, not actual captures/children.
    pub reserved_legacy_children: usize,
    pub reserved_legacy_capture_bytes: usize,
    /// Actual sealed runner observations: both raw pipes, all identity/queue children.
    pub measured_legacy_read: crate::providers::runner::ScopedReadAccountingV1,
}
impl std::fmt::Debug for ScopedInboxResultV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopedInboxResultV1")
            .field("reserved_legacy_children", &self.reserved_legacy_children)
            .finish_non_exhaustive()
    }
}

pub fn refresh(
    original: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<ScopedInboxResultV1, ScopedInboxFailureV1> {
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_response_bytes = WHOLE_CAPTURE_BYTES;
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), &limits, cancel)
        .map_err(|error| ScopedInboxFailureV1::OriginalPrincipal(error.kind))?;
    let RemoteScopeConsumerV1::Explicit(repositories) = active.consumer(session.identity()) else {
        return Err(ScopedInboxFailureV1::ScopeUnavailable);
    };
    let references = repositories.references();
    if references.is_empty() {
        return Err(ScopedInboxFailureV1::ScopeUnavailable);
    }
    // Three queues, at most ten pages EACH for EVERY explicit repository.
    // Bound every possible child but share ONE measured raw capture pool.
    // Preserve all original parent/final reads before any repo GET/Cache/legacy
    // child; never silently select only the first member of an oversized scope.
    let legacy_children = references
        .len()
        .checked_mul(30)
        .and_then(|n| n.checked_add(2))
        .ok_or(ScopedInboxFailureV1::CaptureReservationUnavailable)?;
    let remaining_original_reads = references
        .len()
        .checked_mul(2)
        .ok_or(ScopedInboxFailureV1::CaptureReservationUnavailable)?;
    let window = session
        .reserve_measured_external_reads(
            legacy_children,
            CHILD_CAPTURE_BYTES,
            32 * 1024 * 1024,
            remaining_original_reads,
            cancel,
        )
        .map_err(|_| ScopedInboxFailureV1::CaptureReservationUnavailable)?;
    // Match the retained source pair BEFORE following any original locator.
    // Saved ghosts remain immutable refs only; explicit Activate must already
    // have resolved them with genuine current access for consumer==Explicit.
    for reference in references {
        if !belongs_to_original(&original, reference.original()) {
            return Err(ScopedInboxFailureV1::OriginalReferenceUnavailable);
        }
    }
    let metadata_identity = session.identity().clone();
    let mut repository_metadata: Option<crate::providers::gh::ExactInboxRepositoryMetadataV1> =
        None;
    let mut names = Vec::with_capacity(references.len());
    for reference in references {
        let row = match session.verify(reference.original(), cancel) {
            Ok(row) if row.repository().permissions.pull == Some(true) => row,
            _ => {
                // Nonfatal unavailable/ref-loss closes actual principal+SQL;
                // fatal auth/transport session rejects before callback/final2.
                close_original(session, &original, database, cancel)?;
                return Err(ScopedInboxFailureV1::CurrentReadUnavailable);
            }
        };
        match &mut repository_metadata {
            Some(metadata) => metadata
                .capture_current(row)
                .map_err(|_| ScopedInboxFailureV1::CurrentReadUnavailable)?,
            None => {
                repository_metadata = Some(
                    crate::providers::gh::ExactInboxRepositoryMetadataV1::from_current(
                        &metadata_identity,
                        row,
                    )
                    .map_err(|_| ScopedInboxFailureV1::CurrentReadUnavailable)?,
                )
            }
        }
        names.push(row.repository().full_name.clone());
    }
    let scoped = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(|_| ScopedInboxFailureV1::CaptureReservationUnavailable)?;
    let mut workspace = GithubWorkspace::empty(
        session.identity().clone(),
        names.clone(),
        Default::default(),
    )
    .refresh_exact_scope_current(
        original.scope(),
        repository_metadata
            .as_ref()
            .ok_or(ScopedInboxFailureV1::ScopeUnavailable)?,
        &scoped,
        database,
        cancel,
    )
    .map_err(|_| ScopedInboxFailureV1::LegacyReadUnavailable)?;
    for (reference, canonical) in references.iter().zip(&names) {
        let row = session
            .verify(reference.original(), cancel)
            .map_err(|_| ScopedInboxFailureV1::CanonicalChanged)?;
        if row.repository().full_name != *canonical
            || row.repository().permissions.pull != Some(true)
        {
            return Err(ScopedInboxFailureV1::CanonicalChanged);
        }
    }
    let closed = close_original(session, &original, database, cancel)?;
    let mut current_scope = active.as_ref().clone();
    current_scope
        .observe_closed_access(original.source(), &closed)
        .map_err(|_| ScopedInboxFailureV1::ClosedNavigationUnavailable)?;
    // All post-proof work is pure bounded metadata. No SQL/filesystem/transport,
    // source reservation, rewritten original ref or manufactured new Read.
    workspace.identity = closed.identity().clone();
    workspace.inbox.identity = closed.identity().clone();
    Ok(ScopedInboxResultV1 {
        workspace,
        current_scope: Arc::new(current_scope),
        closed_identity: closed.identity().clone(),
        reserved_legacy_children: closed.external_requests_reserved(),
        reserved_legacy_capture_bytes: closed.external_capture_reserved_bytes(),
        measured_legacy_read: *closed
            .external_read_accounting()
            .ok_or(ScopedInboxFailureV1::ClosedNavigationUnavailable)?,
    })
}
fn belongs_to_original(original: &CatalogueIntentSource, reference: &IntentReference) -> bool {
    original.source().repositories().iter().any(|row| {
        row.id == reference.id
            && row.node_id == reference.node_id
            && row.full_name == reference.last_name
    }) || original
        .intent()
        .sets()
        .values()
        .any(|members| members.get(&reference.id) == Some(reference))
}
fn close_original(
    session: RepositoryCatalogueAccessSession<'_>,
    original: &CatalogueIntentSource,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<ClosedCatalogueAccess, ScopedInboxFailureV1> {
    let mut failed_boundary = None;
    let closed = session.close_with_guard(cancel, || {
        let cache = Cache::open(database).map_err(|_| {
            failed_boundary = Some(CatalogueIntentBoundaryError::Database);
            boundary_error()
        })?;
        cache
            .validate_repository_catalogue_intent_source(original)
            .map_err(|reason| {
                failed_boundary = Some(reason);
                boundary_error()
            })
    });
    closed.map_err(|error| {
        failed_boundary
            .map(ScopedInboxFailureV1::OriginalBoundary)
            .unwrap_or(ScopedInboxFailureV1::ClosingPrincipal(error.kind))
    })
}
fn boundary_error() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Original repository intent changed; no read guard rebased".into(),
    }
}

#[cfg(test)]
#[path = "scoped_inbox_route_v1_tests.rs"]
mod tests;
