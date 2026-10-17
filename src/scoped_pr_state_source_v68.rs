//! State purpose of the ONE actual initial Core and local parent boundary.
//! A remembered ID/receipt never constructs this opening or source.
use super::*;
use crate::pr_write_journal::state_origin_v68::lineage_v68::StateActionBaseV68;
use crate::{pr_state_model::PrStateCommand, providers::gh::OriginalPrStateV68};

pub(crate) struct StateOpenRequestV68 {
    pub(crate) opening: Arc<PrCollaborationOpenIntentV17>,
    pub(crate) command: PrStateCommand,
}
pub(crate) struct StateHistoryReadV78<'a> {
    pub(crate) request: StateOpenRequestV68,
    pub(crate) inspection_cache: Option<&'a Arc<crate::storage::StateAckQueueBoundaryV78>>,
}
/// Fresh explicit read only. The original Core closes normally but its closed
/// producer is dropped: no publication Source/eligible Base escapes here.
pub(crate) fn with_history_v74<T>(
    request: StateHistoryReadV78<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(
        crate::providers::gh::StateRecoveryCurrentV68,
        PrLocalSourceBoundaryV13<'_>,
    ) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<(T, HostIdentity)> {
    let StateHistoryReadV78 {
        request,
        inspection_cache,
    } = request;
    let completed = initial_v17::run_state_v68(
        initial_v17::StateReadRequestV68 {
            opening: request.opening,
            command: request.command,
            initial: false,
            inspection_cache_v78: inspection_cache.map(Arc::as_ref),
        },
        provider,
        database,
        cancel,
        deadline,
        |mut read| {
            let current = read.inspect_state_v74()?;
            prepare(current, read.boundary)
        },
        validate,
    )?;
    Ok((completed.material, completed.identity))
}
pub(crate) fn open_state_v68<T>(
    request: StateOpenRequestV68,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(
        &OriginalPrStateV68,
        &ResourceLocalGuard,
        Option<&StateActionBaseV68>,
    ) -> GithubResult<T>,
    validate: impl Fn(&T) -> GithubResult<()>,
) -> GithubResult<(Arc<PrCollaborationSourceV17>, OriginalPrStateV68, T)> {
    let StateOpenRequestV68 { opening, command } = request;
    if !opening.admits_state_v68(command) {
        return Err(failure());
    }
    let completed = initial_v17::run_state_v68(
        initial_v17::StateReadRequestV68 {
            opening: Arc::clone(&opening),
            command,
            initial: true,
            inspection_cache_v78: None,
        },
        provider,
        database,
        cancel,
        deadline,
        |mut read| {
            let actual = read.observe_state_v68()?;
            // Original reference/body inspection precedes command eligibility.
            // Only the closed caller that prepares a new Preview invokes
            // require(); an old protected original is always inspect-only.
            let local = ResourceLocalGuard::capture_boundary_v13(
                database,
                read.boundary,
                read.identity,
                read.parent.summary(),
            )?;
            let base = match actual.require() {
                Ok(()) => Some(read.state_action_base_v68(&actual)?),
                Err(e)
                    if matches!(
                        e.kind,
                        GithubErrorKind::Permission | GithubErrorKind::InvalidTarget
                    ) =>
                {
                    None
                }
                Err(e) => return Err(e),
            };
            let material = prepare(&actual, &local, base.as_ref())?;
            Ok((local, actual, material))
        },
        |(local, _, material), boundary| {
            local.validate_boundary_v13(database, boundary)?;
            validate(material)
        },
    )?;
    let (local_recovery_v20, actual, material) = completed.material;
    provider.validate_original_state_generation_v68(&actual, cancel, deadline)?;
    let source = Arc::new(PrCollaborationSourceV17 {
        opening,
        parent: completed.parent,
        identity: completed.identity,
        closed: completed.closed,
        local_recovery_v20,
    });
    Ok((source, actual, material))
}
