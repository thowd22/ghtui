//! Deliberate fresh page1 re-read. Existing gh factory supplies
//! numeric principal/owner brackets. Exact old SQL expectation transitions to
//! ONE new reservation atomically, never append cached rows or rebase intent.
use crate::{
    providers::{
        gh::{
            GithubError, GithubErrorKind, GithubProvider, RepositoryCatalogueLimits,
            RepositoryCataloguePhase,
        },
        runner::CancellationToken,
    },
    repository_catalogue_intent_v1::IntentSource,
    repository_catalogue_save_v1::{CatalogueSaveFailure, CatalogueSaveReason},
    storage::{
        Cache, CatalogueIntentBoundaryError, CatalogueIntentReservation, CatalogueIntentSource,
        Scope,
    },
};
use std::{sync::Arc, time::Instant};

/// This returns a new REAL source read, not a longer lifetime for the original
/// one. Only explicit Activate/Open can establish subsequent current access.
pub fn reload_catalogue_intent_source(
    provider: &GithubProvider,
    cache: &mut Cache,
    original: &CatalogueIntentSource,
    limits: &RepositoryCatalogueLimits,
    cancel: &CancellationToken,
) -> Result<CatalogueIntentSource, CatalogueSaveFailure> {
    let deadline = Instant::now()
        .checked_add(limits.max_duration)
        .filter(|_| !limits.max_duration.is_zero())
        .ok_or_else(|| failure(CatalogueSaveReason::InvalidDeadline))?;
    reload_catalogue_intent_source_before(provider, cache, original, limits, deadline, cancel)
}
/// Crate-only original worker clock, including bounded transient preparation.
/// A deadline is a limit, never account/source/SQL authority.
pub(crate) fn reload_catalogue_intent_source_before(
    provider: &GithubProvider,
    cache: &mut Cache,
    original: &CatalogueIntentSource,
    limits: &RepositoryCatalogueLimits,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Result<CatalogueIntentSource, CatalogueSaveFailure> {
    if cancel.is_cancelled() || Instant::now() >= deadline || limits.max_duration.is_zero() {
        return Err(failure(CatalogueSaveReason::InvalidDeadline));
    }
    if provider.host() != original.scope().host
        || original
            .source()
            .identity()
            .account_id
            .is_none_or(|uid| uid == 0)
        || original
            .intent()
            .validate_original_read(original.source())
            .is_err()
    {
        return Err(failure(CatalogueSaveReason::IdentityChanged));
    }
    let mut reservation: Option<CatalogueIntentReservation> = None;
    let mut boundary_failure = None;
    let scoped = provider.clone().with_catalogue_read_deadline(deadline);
    let mut remaining_limits = limits.clone();
    remaining_limits.max_duration = deadline.saturating_duration_since(Instant::now());
    let read = scoped
        .repository_catalogue_read_guarded(
            Some(original.source().identity()),
            original.source().source(),
            &remaining_limits,
            cancel,
            |phase, context| {
                let boundary = (|| {
                    if cancel.is_cancelled() || Instant::now() >= deadline {
                        return Err(CatalogueIntentBoundaryError::CancelledOrExpired);
                    }
                    // BEFORE any deliberate new reservation/private intent reads.
                    let current_scope =
                        Scope::github(&context.identity().host, context.identity().account_id)
                            .map_err(|_| CatalogueIntentBoundaryError::OriginalSource)?;
                    let current_source = IntentSource::from_original(context)
                        .map_err(|_| CatalogueIntentBoundaryError::OriginalSource)?;
                    if current_scope != *original.scope()
                        || context.identity().account_id.is_none_or(|uid| uid == 0)
                        || &current_source != original.intent().source()
                    {
                        return Err(CatalogueIntentBoundaryError::OriginalSource);
                    }
                    match phase {
                        RepositoryCataloguePhase::BeforeEnumeration => {
                            if reservation.is_some() {
                                return Err(CatalogueIntentBoundaryError::OriginalSource);
                            }
                            // SAME existing Immediate transaction, old issuer,
                            // source/epoch/payload checked BEFORE generation moves.
                            reservation = Some(cache.reserve_repository_catalogue_reread(
                                original, context, deadline, cancel,
                            )?);
                            Ok(())
                        }
                        RepositoryCataloguePhase::BeforeFinalProof => {
                            let reserved = reservation
                                .as_ref()
                                .ok_or(CatalogueIntentBoundaryError::OriginalSource)?;
                            reserved.matches_context(context)?;
                            cache.validate_repository_catalogue_reservation(reserved)
                        }
                        RepositoryCataloguePhase::AfterFinalProof => {
                            // PURE only; same source and original new reservation.
                            reservation
                                .as_ref()
                                .ok_or(CatalogueIntentBoundaryError::OriginalSource)?
                                .matches_context(context)
                        }
                    }
                })();
                boundary.map_err(|reason| {
                    boundary_failure = Some(reason);
                    GithubError {
                        kind: GithubErrorKind::Transport,
                        message: "Original reread reservation refused; no prior row/intent rebase"
                            .into(),
                    }
                })
            },
        )
        .map_err(|error| {
            failure(
                boundary_failure
                    .map(CatalogueSaveReason::Boundary)
                    .unwrap_or(CatalogueSaveReason::Principal(error.kind)),
            )
        })?;
    reservation
        .ok_or_else(|| {
            failure(CatalogueSaveReason::Boundary(
                CatalogueIntentBoundaryError::OriginalSource,
            ))
        })?
        .bind(Arc::new(read))
        .map_err(|reason| failure(CatalogueSaveReason::Boundary(reason)))
}
fn failure(reason: CatalogueSaveReason) -> CatalogueSaveFailure {
    CatalogueSaveFailure {
        reason,
        original_intent_committed: false,
    }
}

#[cfg(test)]
#[path = "repository_catalogue_reload_v1_tests.rs"]
mod tests;
