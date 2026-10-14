//! Worker-only collection of independently bounded rank lists. GitHub auth and
//! API transport stay in gh; no network/cache work belongs in draw or input.
use crate::{
    git::RawPath,
    providers::{
        gh::{
            BoundedRemoteSearchResult, GithubErrorKind, GithubProvider, HostIdentity,
            RemoteCanonicalIdentity, RemoteSearchItem, RemoteSearchKind, RemoteSearchLimits,
            RemoteSearchStatus, REMOTE_SEARCH_MAX_RESPONSE_BYTES,
        },
        runner::CancellationToken,
    },
    search::{
        collaboration::{DiscussionTarget, ParentKind},
        fusion::{
            blend::{
                self, BlendContext, BlendPolicy, CandidateOrigin, CurrentVisibility,
                RemoteCandidate, RemoteIdentity, RemoteRankList, SourceStatus,
            },
            RankingSource,
        },
        model::{DiscussionKind, Document},
        remote,
        workspace::SearchWorkspace,
        Query, SearchMode,
    },
    storage::{Cache, Scope},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::Path,
    sync::Arc,
    time::Duration,
};

/// Prepared provenance shared by the worker and later exact Open. The opaque
/// read proofs never become persisted authorization or displayable cache data.
pub struct BlendEvidence {
    pub origins: BTreeMap<String, CandidateOrigin>,
    pub rows: Vec<String>,
    pub headlines: Vec<(String, crate::theme::SemanticToken)>,
    visibility: CurrentVisibility,
    access_targets: Vec<DiscussionTarget>,
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
impl fmt::Debug for BlendEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlendEvidence")
            .field("results", &self.origins.len())
            .field("prepared_rows", &self.rows.len())
            .finish_non_exhaustive()
    }
}
impl BlendEvidence {
    pub fn access_targets(&self) -> &[DiscussionTarget] {
        &self.access_targets
    }
    pub fn validate(&self, cache: &Cache) -> Result<(), String> {
        self.visibility
            .validate(cache)
            .map_err(|error| error.to_string())
    }
    pub fn validate_document(&self, cache: &Cache, document: &Document) -> Result<(), String> {
        self.visibility
            .validate_document(cache, document)
            .map_err(|error| error.to_string())
            .and_then(|target| {
                target.map(|_| ()).ok_or_else(|| {
                    "The result is not an authorized current canonical parent body".into()
                })
            })
    }
}

fn verify_identity(
    provider: &GithubProvider,
    expected: &HostIdentity,
    scope: &Scope,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let actual = provider.identity(cancel).map_err(|error| {
        format!("Identity verification failed; private blended results hidden: {error}")
    })?;
    if !actual.same_account(expected) {
        return Err("Account changed during blended search; private results hidden".into());
    }
    remote::verify_scope(scope, &actual).map_err(|error| error.to_string())?;
    Ok(())
}
fn provider_kind(source: RankingSource) -> Result<RemoteSearchKind, String> {
    match source {
        RankingSource::GithubCode => Ok(RemoteSearchKind::Code),
        RankingSource::GithubPullRequests => Ok(RemoteSearchKind::PullRequests),
        RankingSource::GithubIssues => Ok(RemoteSearchKind::Issues),
        _ => Err("Only explicitly selected Code/PR/Issue sources can blend".into()),
    }
}
fn private_failure(kind: &GithubErrorKind) -> bool {
    matches!(
        kind,
        GithubErrorKind::Authentication
            | GithubErrorKind::Permission
            | GithubErrorKind::IdentityChanged
            | GithubErrorKind::Json
            | GithubErrorKind::StaleHead
    )
}

/// Convert RAW POSITION-ALIGNED items, never the legacy map keyed by display ID.
/// Each one-item page uses existing validation/presentation but cannot collide
/// numeric fallback 700 with the genuinely opaque node named "700".
fn convert(
    scope: &Scope,
    identity: &HostIdentity,
    source: RankingSource,
    bounded: BoundedRemoteSearchResult,
) -> Result<(RemoteRankList, Vec<RemoteSearchItem>), String> {
    if bounded.result.items.len() != bounded.provenance.len() {
        return Err("Bounded provider provenance is not aligned with raw result positions".into());
    }
    let status = match &bounded.status {
        RemoteSearchStatus::Complete => SourceStatus::Complete,
        RemoteSearchStatus::Partial { failure, cancelled } => {
            if failure
                .as_ref()
                .is_some_and(|error| private_failure(&error.kind))
            {
                return Err(
                    "Private source identity/access failed; prior blended results hidden".into(),
                );
            }
            SourceStatus::Partial(format!(
                "{} requested / {} received pages; {} received rows / {} reported; {} received stdout bytes; cancelled={cancelled}{}",
                bounded.requested_pages,
                bounded.fetched_pages,
                bounded.fetched_rows,
                bounded.reported_total.map_or_else(||"unknown".into(),|total|total.to_string()),
                bounded.response_bytes,
                failure.as_ref().map_or(String::new(), |failure| format!("; {failure}")),
            ))
        }
        RemoteSearchStatus::Unavailable { failure, cancelled } => {
            if private_failure(&failure.kind) || failure.repository_access_lost() {
                return Err(format!(
                    "Private source access failed; blended results hidden: {failure}"
                ));
            }
            SourceStatus::Unavailable(format!(
                "{failure}; no complete page; total unknown; cancelled={cancelled}"
            ))
        }
    };
    let mut coverage = crate::search::model::Coverage {
        complete: bounded.result.coverage.complete,
        documents: bounded.result.items.len(),
        reasons: bounded.result.coverage.reasons.clone(),
        repositories: bounded.requested_repositories.clone(),
        ..Default::default()
    };
    coverage
        .reasons
        .extend(bounded.result.provider_notes.clone());
    coverage.reasons.push(format!(
        "{}: cap {} results / {} pages / {} response bytes; {} requested / {} received pages",
        source.label(),
        bounded.limits.max_results,
        bounded.limits.max_pages,
        bounded.limits.max_response_bytes,
        bounded.requested_pages,
        bounded.fetched_pages,
    ));
    let raw_items = bounded.result.items.clone();
    let mut candidates = Vec::with_capacity(raw_items.len());
    for (item, provenance) in bounded.result.items.iter().zip(&bounded.provenance) {
        let provenance = provenance.as_ref().ok_or(
            "Remote candidate has no positive canonical principal provenance; no result admitted",
        )?;
        if !provenance.matches_item(identity, item) {
            return Err("Actual remote candidate contradicts its canonical provenance".into());
        }
        let canonical_identity = match provenance.identity() {
            RemoteCanonicalIdentity::OpaqueNode {
                node_id,
                database_id,
                repository_id,
            } => RemoteIdentity::OpaqueNode {
                node_id: node_id.clone(),
                database_id: *database_id,
                repository_id: *repository_id,
            },
            RemoteCanonicalIdentity::Numeric {
                database_id,
                repository_id,
            } => RemoteIdentity::Numeric {
                database_id: *database_id,
                repository_id: *repository_id,
            },
            RemoteCanonicalIdentity::CodeBlob {
                path,
                blob_sha,
                repository_id,
            } => RemoteIdentity::CodeBlob {
                path: RawPath::new(path.as_bytes().to_vec()),
                blob_sha: blob_sha.clone(),
                repository_id: *repository_id,
            },
        };
        let result = crate::providers::gh::RemoteSearchResult {
            identity: bounded.result.identity.clone(),
            query: bounded.result.query.clone(),
            items: vec![item.clone()],
            coverage: bounded.result.coverage.clone(),
            provider_notes: Vec::new(),
        };
        let (mut page, _) = remote::prepare_page(scope, &bounded.original_query, result, 1)
            .map_err(|error| error.to_string())?;
        let hit = page
            .hits
            .pop()
            .ok_or("Provider item did not produce an exact candidate")?;
        candidates.push(RemoteCandidate {
            hit,
            identity: canonical_identity,
        });
    }
    Ok((
        RemoteRankList {
            source,
            scope: scope.clone(),
            query: bounded.original_query,
            candidates,
            coverage,
            status,
        },
        raw_items,
    ))
}

/// Enrichment is at most one bounded provider GET per declared repository and
/// never invents IDs from parent numbers or opaque node prefixes. Any missing
/// identity remains unregistered and is conservatively suppressed by attachment.
fn discovered_targets(
    provider: &GithubProvider,
    scope: &Scope,
    sources: &[RemoteRankList],
    cancel: &CancellationToken,
) -> Result<Vec<DiscussionTarget>, String> {
    let mut repository_ids = BTreeMap::new();
    let mut targets = BTreeMap::<(String, String, String), DiscussionTarget>::new();
    for source in sources {
        let (parent_kind, category) = match source.source {
            RankingSource::GithubPullRequests => {
                (ParentKind::PullRequest, DiscussionKind::PullRequestBody)
            }
            RankingSource::GithubIssues => (ParentKind::Issue, DiscussionKind::IssueBody),
            _ => continue,
        };
        for candidate in &source.candidates {
            if cancel.is_cancelled() {
                return Err("Blended search cancelled before canonical parent enrichment".into());
            }
            let RemoteIdentity::OpaqueNode {
                node_id,
                repository_id,
                ..
            } = &candidate.identity
            else {
                continue;
            };
            let repository = &candidate.hit.document.key.repository;
            let repository_key = repository.to_ascii_lowercase();
            let id = if let Some(id) = repository_id {
                if repository_ids
                    .insert(repository_key.clone(), Some(*id))
                    .flatten()
                    .is_some_and(|existing| existing != *id)
                {
                    return Err(
                        "Canonical repository identity changed within the same rank lists".into(),
                    );
                }
                Some(*id)
            } else if let Some(id) = repository_ids.get(&repository_key) {
                *id
            } else {
                let id = match provider.repository(repository, cancel) {
                    Ok(actual)
                        if actual.id > 0
                            && actual.host == scope.host
                            && actual.full_name.eq_ignore_ascii_case(repository) =>
                    {
                        Some(actual.id)
                    }
                    Ok(_) => return Err(
                        "Canonical repository enrichment contradicted the original remote parent"
                            .into(),
                    ),
                    Err(error)
                        if private_failure(&error.kind) || error.repository_access_lost() =>
                    {
                        return Err(format!(
                            "Private repository identity/access failed: {error}"
                        ));
                    }
                    Err(_) => None,
                };
                repository_ids.insert(repository_key.clone(), id);
                id
            };
            let Some(repository_id) = id else {
                continue;
            };
            let number = match &candidate.hit.document.destination {
                crate::search::model::Destination::PullRequest { number, .. }
                | crate::search::model::Destination::Issue { number, .. } => *number,
                _ => return Err("Canonical parent has a different destination kind".into()),
            };
            let target = DiscussionTarget {
                scope: scope.clone(),
                repository_id,
                repository: repository_key.clone(),
                parent_kind,
                parent_node: node_id.clone(),
                number,
                category,
            };
            target.validate().map_err(|error| error.to_string())?;
            let key = (repository_key, category.label().into(), node_id.clone());
            if targets
                .insert(key, target.clone())
                .is_some_and(|old| old != target)
            {
                return Err(
                    "Canonical discovered parent has contradictory node/number identity".into(),
                );
            }
            if targets.len() > 1000 {
                return Err(
                    "Discovered canonical parents exceed the1000-target visibility budget".into(),
                );
            }
        }
    }
    Ok(targets.into_values().collect())
}

#[allow(clippy::too_many_arguments)]
pub fn execute(
    search: &mut SearchWorkspace,
    input: &str,
    policy: &BlendPolicy,
    cache: &mut Cache,
    index_root: &Path,
    database: &Path,
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> Result<Arc<BlendEvidence>, String> {
    policy.validate().map_err(|error| error.to_string())?;
    if search.mode != SearchMode::Ranked || !policy.enabled {
        return Err("Explicit enabled Ranked blending is required".into());
    }
    let identity = search
        .remote_identity
        .clone()
        .ok_or("Verify the selected GitHub account before blending")?;
    remote::verify_scope(&search.scope, &identity).map_err(|error| error.to_string())?;
    verify_identity(provider, &identity, &search.scope, cancel)?;
    crate::search_sources::visibility::access(
        provider,
        &identity,
        &search.scope,
        &search.metadata_sources.discussions,
        cancel,
    )?;
    let original_query =
        Query::parse(input, SearchMode::Ranked).map_err(|error| error.to_string())?;
    let original_guards = crate::search_sources::visibility::before(database, search)?;
    if search.selections.is_empty() && search.metadata_sources.is_empty() {
        search.page = Some(crate::search::scan::SearchPage {
            query: original_query.clone(),
            hits: Vec::new(),
            coverage: crate::search::model::Coverage {
                complete: true,
                reasons: vec![
                    "No local corpus selected; only explicit remote sources requested".into(),
                ],
                ..Default::default()
            },
        });
    } else {
        search
            .search_ranked(input, cache, index_root, cancel)
            .map_err(|error| error.to_string())?;
    }
    let local = search
        .page
        .as_ref()
        .ok_or("Local ranked worker returned no page")?;
    let local_repositories = search
        .selections
        .iter()
        .map(|selection| selection.repository.clone())
        .chain(
            search
                .metadata_sources
                .discussions
                .iter()
                .map(|target| target.repository.clone()),
        )
        .chain(
            search
                .metadata_sources
                .commits
                .iter()
                .map(|selection| selection.repository.clone()),
        )
        .collect::<BTreeSet<_>>();
    let repository_sets = search
        .selections
        .iter()
        .map(|selection| {
            (
                selection.repository.to_ascii_lowercase(),
                selection.sets.clone(),
            )
        })
        .collect();
    let context = BlendContext {
        scope: search.scope.clone(),
        query: original_query,
        local_query: local.query.clone(),
        local_repositories,
        remote_repositories: search.remote_repositories.iter().cloned().collect(),
        repository_sets,
        policy: policy.clone(),
    };
    context.validate().map_err(|error| error.to_string())?;
    let mut visibility = CurrentVisibility::collect(
        cache,
        &context,
        &search.metadata_sources.discussions,
        &search.discussion_limits,
        cancel,
    )
    .map_err(|error| error.to_string())?;
    let mut sources = Vec::new();
    let mut originals = BTreeMap::new();
    for source in &policy.sources {
        let query = match context.remote_query(*source) {
            Ok(query) => query,
            Err(reason) => {
                let mut query = context.query.clone();
                query.mode = SearchMode::GitHub;
                sources.push(RemoteRankList {
                    source: *source,
                    scope: search.scope.clone(),
                    query,
                    candidates: Vec::new(),
                    coverage: Default::default(),
                    status: SourceStatus::Unsupported(reason.to_string()),
                });
                continue;
            }
        };
        let kind = provider_kind(*source)?;
        let bounded = provider
            .remote_search_bounded(
                &identity,
                &query,
                kind,
                &search.remote_repositories,
                &RemoteSearchLimits {
                    selected_kinds: vec![kind],
                    max_pages: policy.per_source_pages,
                    max_results: policy.per_source_results,
                    max_response_bytes: REMOTE_SEARCH_MAX_RESPONSE_BYTES,
                    max_duration: Duration::from_secs(30),
                },
                cancel,
            )
            .map_err(|error| error.to_string())?;
        let (list, items) = convert(&search.scope, &identity, *source, bounded)?;
        for (index, item) in items.into_iter().enumerate() {
            originals.insert((*source, index), item);
        }
        sources.push(list);
    }
    let parents = discovered_targets(provider, &search.scope, &sources, cancel)?;
    crate::search_sources::visibility::access(
        provider,
        &identity,
        &search.scope,
        &parents,
        cancel,
    )?;
    visibility
        .attach_discovered(
            database,
            cache,
            &context,
            &parents,
            &search.discussion_limits,
            cancel,
        )
        .map_err(|error| error.to_string())?;
    let prepared = blend::blend(cache, &context, local, &sources, &visibility, cancel)
        .map_err(|error| error.to_string())?;
    let mut final_access_targets = search.metadata_sources.discussions.clone();
    final_access_targets.extend_from_slice(&parents);
    crate::search_sources::visibility::access(
        provider,
        &identity,
        &search.scope,
        &final_access_targets,
        cancel,
    )?;
    let mut items = BTreeMap::new();
    for (key, origin) in &prepared.origins {
        if let CandidateOrigin::Remote { source, index } = origin {
            items.insert(
                key.clone(),
                originals
                    .get(&(*source, *index))
                    .ok_or("Exact remote origin was lost during fusion")?
                    .clone(),
            );
        }
    }
    let headlines = prepared
        .reports
        .iter()
        .map(|report| {
            use crate::theme::SemanticToken;
            let (state, detail, token) = match &report.status {
                SourceStatus::Complete => (
                    "COMPLETE",
                    format!("{} admitted", report.admitted),
                    SemanticToken::Success,
                ),
                SourceStatus::Partial(reason) => {
                    ("PARTIAL", reason.clone(), SemanticToken::Pending)
                }
                SourceStatus::Unavailable(reason) => {
                    ("UNAVAILABLE", reason.clone(), SemanticToken::Muted)
                }
                SourceStatus::Unsupported(reason) => {
                    ("UNSUPPORTED", reason.clone(), SemanticToken::Muted)
                }
            };
            (
                format!(
                    "{} • {state} • {}",
                    report.source.label(),
                    crate::app::safe_text(&detail)
                        .chars()
                        .take(240)
                        .collect::<String>()
                ),
                token,
            )
        })
        .collect();
    let rows = prepared
        .reports
        .iter()
        .map(|report| {
            let status = match &report.status {
                SourceStatus::Complete => "complete provider snapshot".into(),
                SourceStatus::Partial(reason) => format!("partial: {reason}"),
                SourceStatus::Unavailable(reason) => format!("unavailable: {reason}"),
                SourceStatus::Unsupported(reason) => format!("unsupported: {reason}"),
            };
            format!(
                "{} • {} loaded / {} admitted / {} hidden • {status}",
                report.source.label(),
                report.loaded,
                report.admitted,
                report.suppressed
            )
        })
        .collect();
    verify_identity(provider, &identity, &search.scope, cancel)?;
    crate::search_sources::visibility::after(database, &original_guards)?;
    visibility
        .validate(cache)
        .map_err(|error| error.to_string())?;
    let evidence = Arc::new(BlendEvidence {
        origins: prepared.origins,
        rows,
        headlines,
        visibility,
        access_targets: parents,
    });
    search.page = Some(prepared.page);
    search.preview = None;
    search.remote_items = items;
    Ok(evidence)
}
