//! Final principal proof on authenticated LocalOnly publication/read failures.
use super::*;

pub(super) fn failed_publication(
    provider: &GithubProvider,
    expected: &HostIdentity,
    error: &GithubError,
    cancel: &CancellationToken,
) -> ActionsExecution {
    // LocalOnly excludes remote writes, not authenticated private reads. A
    // failed read must not preserve its old private workspace after UID drift.
    if let Err(identity_error) = fresh_identity(provider, expected, cancel) {
        return refused(identity_error, false);
    }
    refused(
        error.to_string(),
        !matches!(
            error.kind,
            GithubErrorKind::IdentityChanged | GithubErrorKind::Authentication
        ),
    )
}
