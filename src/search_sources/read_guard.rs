//! Typed access errors never become an apparently valid cached private source.
use crate::{
    providers::{
        gh::{GithubError, GithubErrorKind, GithubProvider, HostIdentity},
        runner::CancellationToken,
    },
    search::collaboration::{self, DiscussionTarget},
    storage::{Cache, Scope},
};

pub(super) fn read<T>(
    result: Result<T, GithubError>,
    cache: &mut Cache,
    expected: &HostIdentity,
    provider: &GithubProvider,
    repository: &str,
    targets: &[DiscussionTarget],
    cancel: &CancellationToken,
) -> Result<T, String> {
    let error = match result {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };
    if matches!(
        error.kind,
        GithubErrorKind::Authentication | GithubErrorKind::IdentityChanged
    ) {
        return Err(format!("Account identity unavailable: {error}"));
    }
    let current = provider
        .identity(cancel)
        .map_err(|error| format!("Account identity unavailable during source error: {error}"))?;
    if !expected.same_account(&current) || current.account_id.is_none_or(|id| id == 0) {
        return Err(
            "Account identity changed during source failure; previous private cache is hidden"
                .into(),
        );
    }
    let scope = Scope::github(&expected.host, expected.account_id)
        .map_err(|error| format!("Account identity unavailable: {error}"))?;
    if error.repository_access_lost() {
        cache.revoke_repository(&scope,repository).map_err(|error|format!("Account identity unavailable: access-revoked cache could not be fenced: {error}"))?;
        return Err(format!("Repository access revoked • {repository} private cached matches removed; drafts preserved: {error}"));
    }
    // Offline, rate limit, parse failure and unsupported sources are NOT revocation proof.
    // They also cannot authorize the old text after a failed attempted refresh.
    invalidate(cache, targets)?;
    Err(format!("Source unavailable • old discussion matches hidden until a successful explicit refresh; no permission-loss claim: {error}"))
}

pub(super) fn invalidate(cache: &mut Cache, targets: &[DiscussionTarget]) -> Result<(), String> {
    for target in targets {
        collaboration::invalidate(cache, target).map_err(|error| {
            format!("Account identity unavailable: source visibility could not be fenced: {error}")
        })?;
    }
    Ok(())
}
