//! Owner-bound selected initial PR read, using the existing gh/session factories.
//! Original scope/source are navigation owners, NOT access or write authority.
use crate::{
    active_remote_scope_v1::{ActiveRemoteScopeV1, RemoteScopeConsumerV1},
    github_workspace::{Freshness, GithubWorkspace, PrKey, PrPage, PrTab},
    providers::{
        gh::{
            ClosedCatalogueAccess, GithubError, GithubErrorKind, GithubProvider, HostIdentity,
            PrDetails, RepositoryCatalogueAccessLimits, RepositoryCatalogueAccessSession,
            RepositoryCatalogueRepository,
        },
        runner::CancellationToken,
    },
    repository_catalogue_intent_v1::IntentReference,
    storage::{Cache, CatalogueIntentBoundaryError, CatalogueIntentSource, Scope},
};
use std::{path::Path, sync::Arc};

const DETAIL_CHILDREN: usize = 7;
const CHILD_CAPTURE: usize = 1024 * 1024;
const WHOLE_CAPTURE: usize = 64 * 1024 * 1024;
pub const PREVIOUS_INBOX_NOTICE: &str =
    "Previous inbox observation only; selected PR read does not refresh this set or its counts";

#[derive(Debug)]
pub enum ScopedPrReadFailureV1 {
    OriginalPrincipal(GithubErrorKind),
    ScopeUnavailable,
    SelectedOriginalUnavailable,
    CaptureUnavailable,
    CurrentRepositoryUnavailable,
    DetailsUnavailable,
    CanonicalChanged,
    OriginalBoundary(CatalogueIntentBoundaryError),
    ClosingPrincipal(GithubErrorKind),
    PreparedPageUnavailable,
    PreparedDisplayUnavailable,
}
impl std::fmt::Display for ScopedPrReadFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::OriginalPrincipal(_) | Self::ClosingPrincipal(_) => "Scoped PR principal unavailable • previous private pages hidden",
            Self::ScopeUnavailable => "Explicit observed repository scope required • no default or narrower scope selected",
            Self::SelectedOriginalUnavailable => "Exact selected original PR row unavailable • no URL, first-repository or search-ID authority",
            Self::CaptureUnavailable => "Original scoped PR capture/deadline reservation unavailable • no detail read sent",
            Self::CurrentRepositoryUnavailable => "Selected original repository identity/read capability unavailable • private PR hidden",
            Self::DetailsUnavailable => "Fresh canonical PR details unavailable • no cached detail or empty result admitted",
            Self::CanonicalChanged => "Selected canonical repository or PR changed during collection • private PR hidden",
            Self::OriginalBoundary(_) => "Original repository intent boundary changed • scoped PR hidden • no guard rebased",
            Self::PreparedPageUnavailable => "Canonical selected PR cannot be prepared • private PR hidden",
            Self::PreparedDisplayUnavailable => "Observed PR display bound exceeded • no rows truncated or current access denial inferred",
        })
    }
}
impl std::error::Error for ScopedPrReadFailureV1 {}

/// Fresh selected details, with explicitly PREVIOUS inbox observations for Back.
/// Root must match original runtime owner/generation/cancellation before display.
/// The unchanged Arc pair preserves the whole set; this does NOT refresh other
/// repositories, rewrite their previous labels, renew Source, or permit writes.
pub struct ScopedPrReadResultV1 {
    pub workspace: GithubWorkspace,
    pub original_active: Arc<ActiveRemoteScopeV1>,
    pub original_source: Arc<CatalogueIntentSource>,
    pub closed_identity: HostIdentity,
    pub selected_original: IntentReference,
    pub selected_canonical: PrKey,
    pub reserved_detail_children: usize,
    /// Conservative subprocess capture allocation, NOT measured API bytes or
    /// a count of HTTP pages hidden inside gh --paginate.
    pub reserved_detail_capture_bytes: usize,
}
impl std::fmt::Debug for ScopedPrReadResultV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopedPrReadResultV1")
            .field("reserved_detail_children", &self.reserved_detail_children)
            .finish_non_exhaustive()
    }
}

pub fn open(
    original: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    previous: GithubWorkspace,
    selected: &PrKey,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<ScopedPrReadResultV1, ScopedPrReadFailureV1> {
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_response_bytes = WHOLE_CAPTURE;
    limits.max_references = 1;
    limits.max_repository_reads = 2;
    open_with_limits(
        original, active, previous, selected, provider, database, &limits, cancel,
    )
}

/// Caller may SHORTEN the same source session limits, never restart a phase's
/// clock. These scheduling limits are not an authentication/access constructor.
#[allow(clippy::too_many_arguments)]
pub fn open_with_limits(
    original: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    mut previous: GithubWorkspace,
    selected: &PrKey,
    provider: &GithubProvider,
    database: &Path,
    limits: &RepositoryCatalogueAccessLimits,
    cancel: &CancellationToken,
) -> Result<ScopedPrReadResultV1, ScopedPrReadFailureV1> {
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), limits, cancel)
        .map_err(|error| ScopedPrReadFailureV1::OriginalPrincipal(error.kind))?;
    let RemoteScopeConsumerV1::Explicit(repositories) = active.consumer(session.identity()) else {
        return Err(ScopedPrReadFailureV1::ScopeUnavailable);
    };
    if previous.pr.is_some()
        || previous.selected.as_ref() != Some(selected)
        || selected.host != original.scope().host
        || selected.number == 0
        || previous.scope().ok().as_ref() != Some(original.scope())
        || Scope::github(
            &previous.inbox.identity.host,
            previous.inbox.identity.account_id,
        )
        .ok()
        .as_ref()
            != Some(original.scope())
        || !previous
            .visible()
            .iter()
            .any(|row| PrKey::from(*row) == *selected)
    {
        return Err(ScopedPrReadFailureV1::SelectedOriginalUnavailable);
    }
    // Match only a genuinely observed member. No first-ref fallback, path/URL
    // parsing, numeric Issue-search ID -> PR database ID, or node-prefix guess.
    let mut matching = repositories.references().iter().filter(|reference| {
        reference
            .observed()
            .is_some_and(|row| row.last_name.eq_ignore_ascii_case(&selected.repository))
    });
    let reference = matching
        .next()
        .filter(|_| matching.next().is_none())
        .map(|reference| reference.original())
        .filter(|reference| belongs_to_original(&original, reference))
        .cloned()
        .ok_or(ScopedPrReadFailureV1::SelectedOriginalUnavailable)?;
    let window = session
        .reserve_external_reads(DETAIL_CHILDREN, CHILD_CAPTURE, cancel)
        .map_err(|_| ScopedPrReadFailureV1::CaptureUnavailable)?;
    let canonical = match session.verify(&reference, cancel) {
        Ok(row) if genuine_read_repository(row.repository()) => row.repository().full_name.clone(),
        _ => {
            // No private SQL on a denied/unknown initial repository. A safe
            // closing account check may take precedence, but admits no page.
            session
                .close(cancel)
                .map_err(|error| ScopedPrReadFailureV1::ClosingPrincipal(error.kind))?;
            return Err(ScopedPrReadFailureV1::CurrentRepositoryUnavailable);
        }
    };
    let scoped = provider
        .clone()
        .with_scoped_read_window(window.deadline(), window.max_output_bytes())
        .map_err(|_| ScopedPrReadFailureV1::CaptureUnavailable)?;
    let details = scoped
        .pr_details(&canonical, selected.number, cancel)
        .map_err(|_| ScopedPrReadFailureV1::DetailsUnavailable)?;
    let key = PrKey {
        host: selected.host.clone(),
        repository: canonical,
        number: selected.number,
    };
    validate_details(&key, &details)?;
    let row = session
        .verify(&reference, cancel)
        .map_err(|_| ScopedPrReadFailureV1::CanonicalChanged)?;
    if row.repository().full_name != key.repository || !genuine_read_repository(row.repository()) {
        return Err(ScopedPrReadFailureV1::CanonicalChanged);
    }
    // All private SQL BEFORE final principal. Failure/elapsed/cancellation hides
    // both fresh details and any old private diagnostics; nothing is rebased.
    let closed = close_original(session, &original, database, cancel)?;
    let mut page = PrPage {
        details,
        tab: PrTab::Summary,
        diff: None,
        checks: None,
        rows: vec![],
        context: None,
        review_progress: None,
    };
    if !crate::scoped_pr_observed_tabs_v1::initial_summary_fits(&page, cancel)
        .map_err(|_| ScopedPrReadFailureV1::PreparedDisplayUnavailable)?
    {
        return Err(ScopedPrReadFailureV1::PreparedDisplayUnavailable);
    }
    page.prepare_rows()
        .map_err(|_| ScopedPrReadFailureV1::PreparedPageUnavailable)?;
    // From here: pure preparation only. Move prior inbox into its explicitly
    // Cached navigation view, never clone old PR/detail/context/write models.
    let fetched_at = match previous.freshness {
        Freshness::Cached { fetched_at }
        | Freshness::Fresh { fetched_at }
        | Freshness::Offline { fetched_at, .. } => fetched_at,
        Freshness::Empty => 0,
    };
    previous.identity = closed.identity().clone();
    previous.inbox.identity = closed.identity().clone();
    previous.inbox.coverage.complete = false;
    previous.inbox.coverage.reasons = vec![PREVIOUS_INBOX_NOTICE.into()];
    previous.freshness = Freshness::Cached { fetched_at };
    previous.scoped_pr_navigation = Some(Arc::new(
        crate::scoped_pr_observed_tabs_v1::ScopedPrNavigationV1::bind(
            original.scope(),
            &reference,
            &key,
            &page.details.summary,
        )
        .map_err(|_| ScopedPrReadFailureV1::PreparedPageUnavailable)?,
    ));
    previous.scoped_pr_resource = None;
    previous.pr = Some(page);
    // Keep the ORIGINAL cached-row selection for pure Back. A legitimate fresh
    // same-ID rename labels this detail only, not a different cached row.
    previous.needs_refresh = false;
    Ok(ScopedPrReadResultV1 {
        workspace: previous,
        original_active: active,
        original_source: original,
        closed_identity: closed.identity().clone(),
        selected_original: reference,
        selected_canonical: key,
        reserved_detail_children: closed.external_requests_reserved(),
        reserved_detail_capture_bytes: closed.external_capture_reserved_bytes(),
    })
}

fn genuine_read_repository(repository: &RepositoryCatalogueRepository) -> bool {
    repository.id > 0
        && repository.node_id.as_deref().is_some_and(valid_node)
        && repository.permissions.pull == Some(true)
        && repository.owner.as_ref().is_some_and(|owner| {
            owner.database_id.is_some_and(|id| id > 0)
                && owner.node_id.as_deref().is_some_and(valid_node)
                && matches!(owner.actor.actor_type.as_str(), "User" | "Organization")
        })
}
fn valid_node(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn validate_details(key: &PrKey, details: &PrDetails) -> Result<(), ScopedPrReadFailureV1> {
    let summary = &details.summary;
    let full_sha = |value: &str| {
        matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    };
    if PrKey::from(&summary.pr) != *key
        || !valid_node(&summary.pr.id)
        || !full_sha(&summary.head_sha)
        || !full_sha(&summary.base_sha)
        || summary.head_ref.is_empty()
        || summary.base_ref.is_empty()
        || [summary.head_ref.as_str(), summary.base_ref.as_str()]
            .iter()
            .any(|value| value.len() > 1024 || value.chars().any(char::is_control))
    {
        return Err(ScopedPrReadFailureV1::CanonicalChanged);
    }
    Ok(())
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
) -> Result<ClosedCatalogueAccess, ScopedPrReadFailureV1> {
    let mut boundary = None;
    session
        .close_with_guard(cancel, || {
            let cache = Cache::open(database).map_err(|_| {
                boundary = Some(CatalogueIntentBoundaryError::Database);
                safe_boundary()
            })?;
            cache
                .validate_repository_catalogue_intent_source(original)
                .map_err(|reason| {
                    boundary = Some(reason);
                    safe_boundary()
                })
        })
        .map_err(|error| {
            boundary
                .map(ScopedPrReadFailureV1::OriginalBoundary)
                .unwrap_or(ScopedPrReadFailureV1::ClosingPrincipal(error.kind))
        })
}
fn safe_boundary() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Original selected repository boundary unavailable".into(),
    }
}

#[cfg(test)]
#[path = "scoped_pr_read_v1_tests.rs"]
mod tests;
