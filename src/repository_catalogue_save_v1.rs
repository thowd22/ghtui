//! SOURCE-ONLY durable repository-set intent coordinator. gh alone provides
//! fresh account observations; this module never asserts repository access.
use crate::{
    providers::{
        gh::{
            GithubError, GithubErrorKind, GithubProvider, HostIdentity, RepositoryCatalogueLimits,
            RepositoryCataloguePhase, RepositoryCatalogueSource,
        },
        runner::CancellationToken,
    },
    repository_catalogue_intent_v1::CatalogueIntentV1,
    storage::{
        Cache, CatalogueIntentBoundaryError, CatalogueIntentReservation, CatalogueIntentSource,
    },
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub enum CatalogueSaveReason {
    Boundary(CatalogueIntentBoundaryError),
    Principal(GithubErrorKind),
    IdentityChanged,
    InvalidDeadline,
}
#[derive(Debug)]
pub struct CatalogueSaveFailure {
    pub reason: CatalogueSaveReason,
    /// Local original-scope intent may already be committed. Never automatically
    /// resend or publish the candidate under a changed/unknown account.
    pub original_intent_committed: bool,
}
impl std::fmt::Display for CatalogueSaveFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.reason {
            CatalogueSaveReason::Boundary(_) => "Original repository intent boundary refused; refresh explicitly and retain original drafts",
            CatalogueSaveReason::Principal(_) => "Fresh account proof unavailable; private acknowledgement remains hidden",
            CatalogueSaveReason::IdentityChanged => "Original host/positive account changed; private acknowledgement remains hidden",
            CatalogueSaveReason::InvalidDeadline => "Repository intent requires a bounded original deadline",
        })
    }
}
impl std::error::Error for CatalogueSaveFailure {}

/// Actual provider factory orchestration; no Save code can reserve/rebase an
/// old read. Closing callback is deliberately PURE, with no external SQL hook.
pub fn load_catalogue_intent_source(
    provider: &GithubProvider,
    cache: &mut Cache,
    original: Option<&HostIdentity>,
    source: &RepositoryCatalogueSource,
    limits: &RepositoryCatalogueLimits,
    cancel: &CancellationToken,
) -> Result<CatalogueIntentSource, CatalogueSaveFailure> {
    let mut reservation: Option<CatalogueIntentReservation> = None;
    let mut boundary_failure = None;
    let read = provider.repository_catalogue_read_guarded(original, source, limits, cancel, |phase, context| {
        let boundary = match phase {
            RepositoryCataloguePhase::BeforeEnumeration => {
                if reservation.is_some() {
                    Err(CatalogueIntentBoundaryError::OriginalSource)
                } else {
                    cache.reserve_repository_catalogue_intent(context).map(|value| reservation = Some(value))
                }
            }
            RepositoryCataloguePhase::BeforeFinalProof => reservation.as_ref()
                .ok_or(CatalogueIntentBoundaryError::OriginalSource)
                .and_then(|original| original.matches_context(context).and_then(|()| cache.validate_repository_catalogue_reservation(original))),
            RepositoryCataloguePhase::AfterFinalProof => reservation.as_ref()
                .ok_or(CatalogueIntentBoundaryError::OriginalSource)
                .and_then(|original| original.matches_context(context)),
        };
        boundary.map_err(|reason| {
            boundary_failure = Some(reason);
            GithubError { kind: GithubErrorKind::Transport, message: "Original repository intent scope/source fence refused; no payload or stale model admitted".into() }
        })
    }).map_err(|error| CatalogueSaveFailure {
        reason: boundary_failure.map(CatalogueSaveReason::Boundary)
            .unwrap_or(CatalogueSaveReason::Principal(error.kind)),
        original_intent_committed: false,
    })?;
    reservation
        .ok_or_else(|| {
            failure(
                CatalogueSaveReason::Boundary(CatalogueIntentBoundaryError::OriginalSource),
                false,
            )
        })?
        .bind(Arc::new(read))
        .map_err(|reason| failure(CatalogueSaveReason::Boundary(reason), false))
}

/// Four genuine gh children on success: version/user, one atomic LOCAL intent
/// transaction, version/user. There is NO SQL or filesystem work after closing
/// account proof, no repository request and no permission receipt. Username
/// changes are harmless; host or positive UID changes are not.
pub fn save_catalogue_intent(
    provider: &GithubProvider,
    cache: &mut Cache,
    original: &CatalogueIntentSource,
    candidate: CatalogueIntentV1,
    max_duration: Duration,
    cancel: &CancellationToken,
) -> Result<CatalogueIntentSource, CatalogueSaveFailure> {
    if max_duration.is_zero() || max_duration > Duration::from_secs(300) {
        return Err(failure(CatalogueSaveReason::InvalidDeadline, false));
    }
    let deadline = Instant::now()
        .checked_add(max_duration)
        .ok_or_else(|| failure(CatalogueSaveReason::InvalidDeadline, false))?;
    if provider.host() != original.scope().host {
        return Err(failure(CatalogueSaveReason::IdentityChanged, false));
    }
    let pre = provider
        .identity_before(deadline, cancel)
        .map_err(|error| failure(CatalogueSaveReason::Principal(error.kind), false))?;
    same_original(original, &pre).map_err(|reason| failure(reason, false))?;
    let committed = cache
        .commit_repository_catalogue_intent(original, candidate, deadline, cancel)
        .map_err(|reason| failure(CatalogueSaveReason::Boundary(reason), false))?;
    let post = provider
        .identity_before(deadline, cancel)
        .map_err(|error| failure(CatalogueSaveReason::Principal(error.kind), true))?;
    same_original(original, &post).map_err(|reason| failure(reason, true))?;
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err(failure(
            CatalogueSaveReason::Boundary(CatalogueIntentBoundaryError::CancelledOrExpired),
            true,
        ));
    }
    // Only immutable prepared metadata is returned. Root MUST additionally
    // validate its original runtime job/epoch/cancel before showing an ack.
    // This does not revive a source or renew permission for Open.
    Ok(original.advance(committed))
}
fn same_original(
    original: &CatalogueIntentSource,
    current: &HostIdentity,
) -> Result<(), CatalogueSaveReason> {
    let scope = crate::storage::Scope::github(&current.host, current.account_id)
        .map_err(|_| CatalogueSaveReason::IdentityChanged)?;
    if scope != *original.scope() || current.account_id.is_none_or(|uid| uid == 0) {
        return Err(CatalogueSaveReason::IdentityChanged);
    }
    Ok(())
}
fn failure(reason: CatalogueSaveReason, original_intent_committed: bool) -> CatalogueSaveFailure {
    CatalogueSaveFailure {
        reason,
        original_intent_committed,
    }
}

#[cfg(test)]
#[path = "repository_catalogue_save_v1_tests.rs"]
mod tests;
