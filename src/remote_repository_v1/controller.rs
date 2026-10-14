//! Worker coordinator. Private SQL follows actual current user/repository,
//! never draw/event; only original source checks precede final identity.
use super::*;
use crate::{
    providers::{
        gh::{
            GithubError, GithubErrorKind, GithubProvider, RemoteRepositoryLimitsV1,
            RemoteRepositoryReadOutcomeV1, RemoteRepositoryReadPhaseV1,
        },
        runner::CancellationToken,
    },
    storage::{Cache, CatalogueIntentSource},
};
use std::{path::Path, time::Instant};

pub enum RemoteRepositoryPreparedV1 {
    Ready {
        owner: Arc<RemoteRepositoryOwnerV1>,
        source: Arc<CatalogueIntentSource>,
        generation: u64,
        workspace: Box<RemoteRepositoryWorkspaceV1>,
    },
    Hidden {
        owner: Arc<RemoteRepositoryOwnerV1>,
        source: Arc<CatalogueIntentSource>,
        generation: u64,
        scope_unavailable: bool,
        reason: &'static str,
    },
}
/// Execution inputs only, never an access or source grant.
pub struct RemoteRepositoryEnvironmentV1<'a> {
    pub provider: &'a GithubProvider,
    pub database: &'a Path,
}
impl RemoteRepositoryPreparedV1 {
    pub fn owner(&self) -> &Arc<RemoteRepositoryOwnerV1> {
        match self {
            Self::Ready { owner, .. } | Self::Hidden { owner, .. } => owner,
        }
    }
    pub fn source(&self) -> &Arc<CatalogueIntentSource> {
        match self {
            Self::Ready { source, .. } | Self::Hidden { source, .. } => source,
        }
    }
    pub fn generation(&self) -> u64 {
        match self {
            Self::Ready { generation, .. } | Self::Hidden { generation, .. } => *generation,
        }
    }
}
pub fn read_remote_repository_v1(
    environment: RemoteRepositoryEnvironmentV1<'_>,
    source: Arc<CatalogueIntentSource>,
    owner: Arc<RemoteRepositoryOwnerV1>,
    generation: u64,
    request: RemoteRepositoryRequestV1,
    limits: &RemoteRepositoryLimitsV1,
    cancel: &CancellationToken,
) -> RemoteRepositoryPreparedV1 {
    let RemoteRepositoryEnvironmentV1 { provider, database } = environment;
    let deadline = Instant::now().checked_add(limits.max_duration);
    let result = (|| -> Result<RemoteRepositoryWorkspaceV1, bool> {
        let deadline = deadline
            .filter(|_| !limits.max_duration.is_zero())
            .ok_or(true)?;
        if !Arc::ptr_eq(source.source(), &owner.original)
            || source.scope().host != provider.host()
            || source.source().identity().account_id.is_none_or(|n| n == 0)
            || cancel.is_cancelled()
        {
            return Err(true);
        }
        let observation = provider
            .clone()
            .with_catalogue_read_deadline(deadline)
            .remote_repository_read_guarded_v1(
                owner.original.clone(),
                &owner.reference,
                request,
                limits,
                cancel,
                |phase| {
                    match phase {
                        RemoteRepositoryReadPhaseV1::BeforePayload
                        | RemoteRepositoryReadPhaseV1::BeforeFinalProof => {
                            // Producer calls only AFTER genuine current user/repo and
                            // BEFORE final user, while original deadline still applies.
                            if cancel.is_cancelled() || Instant::now() >= deadline {
                                return Err(boundary());
                            }
                            let cache = Cache::open(database).map_err(|_| boundary())?;
                            cache
                                .validate_repository_catalogue_intent_source(&source)
                                .map_err(|_| boundary())
                        }
                    }
                },
            )
            .map_err(|_| true)?;
        let observation = match observation {
            RemoteRepositoryReadOutcomeV1::Observed(value) => value,
            RemoteRepositoryReadOutcomeV1::Unavailable(_) => return Err(false),
        };
        let workspace =
            RemoteRepositoryWorkspaceV1::prepare(owner.clone(), observation, generation)
                .map_err(|_| true)?;
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err(true);
        }
        Ok(workspace)
    })();
    match result{
        Ok(workspace)=>RemoteRepositoryPreparedV1::Ready{owner,source,generation,workspace:Box::new(workspace)},
        Err(scope_unavailable)=>RemoteRepositoryPreparedV1::Hidden{owner,source,generation,scope_unavailable,
            reason:"Remote read unavailable • previous private tree/content hidden • no default/local HEAD fallback"},
    }
}
fn boundary() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Original remote source changed; no source epoch/payload rebase".into(),
    }
}

/// Completion ownership is checked BEFORE principal, privacy, status and every
/// UI effect. Equal metadata is not the same request owner/source/generation.
pub fn owns_remote_repository_result_v1(
    current_owner: Option<&Arc<RemoteRepositoryOwnerV1>>,
    current_source: Option<&Arc<CatalogueIntentSource>>,
    generation: u64,
    result: &RemoteRepositoryPreparedV1,
) -> bool {
    generation == result.generation()
        && current_owner.is_some_and(|owner| Arc::ptr_eq(owner, result.owner()))
        && current_source.is_some_and(|source| Arc::ptr_eq(source, result.source()))
}
