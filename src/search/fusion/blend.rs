//! Staged provider-free blended rank adapter. No transport/auth or draw-time I/O.
mod discovered;

use super::{fuse, Presentation, RankEntry, RankList, RankingSource};
use crate::{
    config::validate_repository,
    git::RawPath,
    providers::runner::CancellationToken,
    search::{
        collaboration::{self, DiscussionLimits, DiscussionTarget},
        model::{
            Coverage, Destination, DiscussionKind, Document, ResultKind, SearchHit, SourceKind,
        },
        scan::SearchPage,
        Query, QueryError, RemoteKind, SearchMode,
    },
    storage::{Cache, Scope},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlendPolicy {
    pub enabled: bool,
    pub sources: BTreeSet<RankingSource>,
    pub max_results: usize,
    pub per_source_results: usize,
    pub per_source_pages: usize,
}
impl Default for BlendPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            sources: BTreeSet::from([
                RankingSource::GithubCode,
                RankingSource::GithubPullRequests,
                RankingSource::GithubIssues,
            ]),
            max_results: 100,
            per_source_results: 100,
            per_source_pages: 2,
        }
    }
}
impl BlendPolicy {
    pub fn validate(&self) -> Result<(), QueryError> {
        if !(1..=5000).contains(&self.max_results)
            || !(1..=1000).contains(&self.per_source_results)
            || !(1..=10).contains(&self.per_source_pages)
            || self.enabled && self.sources.is_empty()
            || self.sources.iter().any(|source| {
                !matches!(
                    source,
                    RankingSource::GithubCode
                        | RankingSource::GithubPullRequests
                        | RankingSource::GithubIssues
                )
            })
        {
            return Err(error("Blend needs explicit Code/PR/Issue sources,1..5000 final results,1..1000 results and1..10 pages per source"));
        }
        Ok(())
    }
}

/// Declared local scope remains distinct from the remote scope: local/current
/// does not become an inferred GitHub repository merely because blending is on.
#[derive(Debug, Clone)]
pub struct BlendContext {
    pub scope: Scope,
    pub query: Query,
    /// Exact completed local worker query, including its resolved rev filter.
    /// Only that existing Git revision rewrite may differ from the user's query.
    pub local_query: Query,
    pub local_repositories: BTreeSet<String>,
    pub remote_repositories: BTreeSet<String>,
    pub repository_sets: BTreeMap<String, Vec<String>>,
    pub policy: BlendPolicy,
}
impl BlendContext {
    pub fn validate(&self) -> Result<(), QueryError> {
        self.policy.validate()?;
        if self.query.mode != SearchMode::Ranked {
            return Err(error("Blended discovery is explicit Ranked-only; Exact/Regex never broaden to GitHub discovery"));
        }
        validate_local_query(&self.query, &self.local_query)?;
        if self.local_repositories.len() > 1000 || self.remote_repositories.len() > 1000 {
            return Err(error(
                "Blend repository scope exceeds1000 explicit repositories",
            ));
        }
        for repository in self
            .local_repositories
            .iter()
            .chain(&self.remote_repositories)
        {
            validate_repository(repository).map_err(error)?;
        }
        if self.policy.enabled {
            positive_scope(&self.scope)?;
            if self.remote_repositories.is_empty() {
                return Err(error("Choose explicit remote repositories before blending; no global discovery was requested"));
            }
        }
        Ok(())
    }
    /// A source-specific query is checked before any caller invokes gh. Unsupported
    /// constraints become a visible source status, not a weakened request.
    pub fn remote_query(&self, source: RankingSource) -> Result<Query, QueryError> {
        self.validate()?;
        if !self.policy.enabled || !self.policy.sources.contains(&source) {
            return Err(error("This remote source was not explicitly enabled"));
        }
        let (kind, label) = match source {
            RankingSource::GithubCode => (RemoteKind::Code, "file"),
            RankingSource::GithubPullRequests => (RemoteKind::PullRequests, "pr"),
            RankingSource::GithubIssues => (RemoteKind::Issues, "issue"),
            _ => return Err(error(
                "Repository discovery belongs to the explicit catalogue; no org-wide blend request",
            )),
        };
        if self
            .query
            .filters
            .get("type")
            .is_some_and(|kinds| !kinds.iter().any(|kind| kind == label))
        {
            return Err(error("The explicit query type excludes this provider"));
        }
        let mut query = self.query.clone();
        query.mode = SearchMode::GitHub;
        query.filters.remove("type");
        query.github_query(kind)?;
        query.filters.insert("type".into(), vec![label.into()]);
        Ok(query)
    }
}

/// Captured raw JSON provenance, never guessed from an old item's display/id
/// string. The provider's additive tagged sidecar will convert into this boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteIdentity {
    OpaqueNode {
        node_id: String,
        database_id: Option<u64>,
        repository_id: Option<u64>,
    },
    Numeric {
        database_id: u64,
        repository_id: Option<u64>,
    },
    CodeBlob {
        path: RawPath,
        blob_sha: String,
        repository_id: Option<u64>,
    },
    Unavailable,
}
#[derive(Debug, Clone)]
pub struct RemoteCandidate {
    pub hit: SearchHit,
    pub identity: RemoteIdentity,
}
#[derive(Debug, Clone)]
pub enum SourceStatus {
    Complete,
    Partial(String),
    Unavailable(String),
    Unsupported(String),
}
#[derive(Debug, Clone)]
pub struct RemoteRankList {
    pub source: RankingSource,
    pub scope: Scope,
    pub query: Query,
    pub candidates: Vec<RemoteCandidate>,
    pub coverage: Coverage,
    pub status: SourceStatus,
}
#[derive(Debug, Clone)]
pub struct SourceReport {
    pub source: RankingSource,
    pub loaded: usize,
    pub admitted: usize,
    pub suppressed: usize,
    pub status: SourceStatus,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateOrigin {
    Local(usize),
    CurrentCanonical(String),
    Remote { source: RankingSource, index: usize },
}
#[derive(Debug)]
pub struct BlendPage {
    pub page: SearchPage,
    pub origins: BTreeMap<String, CandidateOrigin>,
    pub contributions: BTreeMap<String, Vec<(RankingSource, usize)>>,
    pub reports: Vec<SourceReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ParentAlias {
    host: String,
    principal: String,
    repository: String,
    kind: String,
    node: String,
}
impl ParentAlias {
    fn new(
        scope: &Scope,
        repository: &str,
        kind: ResultKind,
        node: &str,
    ) -> Result<Self, QueryError> {
        positive_scope(scope)?;
        validate_repository(repository).map_err(error)?;
        opaque(node)?;
        if !matches!(kind, ResultKind::PullRequest | ResultKind::Issue) {
            return Err(error(
                "A parent alias cannot merge a comment/thread with its PR/issue body",
            ));
        }
        Ok(Self {
            host: scope.host.clone(),
            principal: scope.login.clone(),
            repository: repository.to_ascii_lowercase(),
            kind: kind.label().into(),
            node: node.into(),
        })
    }
    fn hash(&self) -> Result<String, QueryError> {
        digest(&("parent-node-v1", self))
    }
}

/// Opaque current-source snapshot. Its constructor checks actual cache bindings;
/// final validation must be repeated after remote reads and before presentation.
pub struct CurrentVisibility {
    scope: Scope,
    versions: BTreeMap<String, String>,
    parents: BTreeMap<ParentAlias, Arc<Document>>,
    blocked_parents: BTreeSet<ParentAlias>,
    epochs: Vec<(DiscussionTarget, Option<u64>)>,
    discovered: Option<discovered::DiscoveredVisibility>,
}
impl CurrentVisibility {
    pub fn collect(
        cache: &Cache,
        context: &BlendContext,
        targets: &[DiscussionTarget],
        limits: &DiscussionLimits,
        cancel: &CancellationToken,
    ) -> Result<Self, QueryError> {
        context.validate()?;
        if targets.len() > 1000 {
            return Err(error("Visibility target budget exceeds1000"));
        }
        for target in targets {
            target.validate()?;
            if target.scope != context.scope
                || !contains_repository(&context.local_repositories, &target.repository)
            {
                return Err(error(
                    "Current visibility target is outside the declared account/local source scope",
                ));
            }
        }
        let mut result = Self {
            scope: context.scope.clone(),
            versions: BTreeMap::new(),
            parents: BTreeMap::new(),
            blocked_parents: BTreeSet::new(),
            epochs: vec![],
            discovered: None,
        };
        if targets.is_empty() {
            return Ok(result);
        }
        let collected =
            collaboration::collect_cached(cache, context.scope.clone(), targets, limits, cancel)?;
        // A capped/unavailable collection cannot prove which omitted selected
        // parent fragments are safe. Admitted local projections remain usable,
        // but remote parent snippets cannot bypass an incomplete selection.
        let complete_selected_visibility = collected.corpus.coverage.complete
            && !collected.corpus.coverage.cancelled
            && collected.corpus.coverage.unavailable.is_empty();
        for target in targets {
            let generation = bound_generation(cache, target)?;
            let kind = match target.parent_kind {
                collaboration::ParentKind::PullRequest => ResultKind::PullRequest,
                collaboration::ParentKind::Issue => ResultKind::Issue,
            };
            let alias =
                ParentAlias::new(&target.scope, &target.repository, kind, &target.parent_node)?;
            if generation.is_none() || !complete_selected_visibility {
                result.blocked_parents.insert(alias);
            }
            result.epochs.push((target.clone(), generation));
        }
        for mut document in collected.corpus.documents {
            if let Some(sets) = context
                .repository_sets
                .get(&document.key.repository.to_ascii_lowercase())
            {
                document.metadata.repository_sets = sets.clone();
            }
            result
                .versions
                .insert(document.key.stable_id(), document.projection_checksum()?);
            if let Some(alias) = body_alias(&document)? {
                if result.parents.insert(alias, Arc::new(document)).is_some() {
                    return Err(error(
                        "Conflicting current canonical parent projections; no guessed body chosen",
                    ));
                }
            }
        }
        for (target, _) in &result.epochs {
            let kind = match target.parent_kind {
                collaboration::ParentKind::PullRequest => ResultKind::PullRequest,
                collaboration::ParentKind::Issue => ResultKind::Issue,
            };
            let parent =
                ParentAlias::new(&target.scope, &target.repository, kind, &target.parent_node)?;
            if !result.parents.contains_key(&parent) {
                // A selected comment/review source does not authorize indexed
                // parent snippets. Deleted/omitted comments cannot reappear as
                // a remote parent fragment merely because its body wasn't loaded.
                result.blocked_parents.insert(parent);
            }
        }
        result.validate(cache)?;
        Ok(result)
    }
    pub fn validate(&self, cache: &Cache) -> Result<(), QueryError> {
        if let Some(discovered) = &self.discovered {
            discovered.validate(cache)?;
        }
        for (target, generation) in &self.epochs {
            if bound_generation(cache, target)? != *generation {
                return Err(error("Current source visibility changed during blended search; old counts/snippets remain hidden"));
            }
        }
        Ok(())
    }
}

pub fn blend(
    cache: &Cache,
    context: &BlendContext,
    local: &SearchPage,
    remote: &[RemoteRankList],
    visibility: &CurrentVisibility,
    cancel: &CancellationToken,
) -> Result<BlendPage, QueryError> {
    context.validate()?;
    if local.query != context.local_query || visibility.scope != context.scope {
        return Err(error("Blend pages belong to another query/principal"));
    }
    if cancel.is_cancelled() {
        return Err(error("Blended search cancelled before admission"));
    }
    visibility.validate(cache)?;
    if !context.policy.enabled && !remote.is_empty() {
        return Err(error("Remote data supplied without explicit opt-in"));
    }
    let mut candidates = BTreeMap::new();
    let mut origins = BTreeMap::new();
    let mut lists = vec![];
    let mut local_entries = vec![];
    let mut coverage = local.coverage.clone();
    let mut reports = vec![];
    if local.hits.len() > 5000 {
        return Err(error("Local rank list exceeds5000 hits"));
    }
    for (index, hit) in local.hits.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(error("Blended search cancelled during local admission"));
        }
        hit.document.validate(&context.scope)?;
        validate_hit(hit)?;
        if hit.document.source == SourceKind::GitHub
            || !contains_repository(&context.local_repositories, &hit.document.key.repository)
        {
            return Err(error("Local result is outside the declared local scope"));
        }
        if !hit.document.matches(&local.query) {
            return Err(error(
                "Local result contradicts an exact resolved query filter",
            ));
        }
        if hit.document.source == SourceKind::Collaboration
            && visibility.versions.get(&hit.document.key.stable_id())
                != Some(&hit.document.projection_checksum()?)
        {
            coverage.complete = false;
            coverage
                .reasons
                .push("Old/unavailable local discussion projection excluded before fusion".into());
            continue;
        }
        let alias = match body_alias(&hit.document)? {
            Some(parent) => parent.hash()?,
            None => digest(&("document-v1", &hit.document.key))?,
        };
        let representative = format!("local:{index}");
        candidates.insert(representative.clone(), hit.clone());
        origins.insert(representative.clone(), CandidateOrigin::Local(index));
        local_entries.push(RankEntry {
            alias,
            representative,
            presentation: Presentation::AuthorizedCurrentDocument,
        });
    }
    if !context.policy.enabled {
        let mut page = local.clone();
        page.coverage = coverage;
        page.hits = local_entries
            .iter()
            .map(|entry| candidates[&entry.representative].clone())
            .collect();
        let origins = local_entries
            .iter()
            .map(|entry| {
                (
                    candidates[&entry.representative].document.key.stable_id(),
                    origins[&entry.representative].clone(),
                )
            })
            .collect();
        visibility.validate(cache)?;
        return Ok(BlendPage {
            page,
            origins,
            contributions: BTreeMap::new(),
            reports,
        });
    }
    lists.push(RankList {
        source: RankingSource::LocalLexical,
        entries: local_entries,
    });
    let mut seen_sources = BTreeSet::new();
    let mut remote_observations =
        BTreeMap::<String, (Option<u64>, Option<u64>, Destination)>::new();
    for source in remote {
        if !seen_sources.insert(source.source) || !context.policy.sources.contains(&source.source) {
            return Err(error("Duplicate or unselected remote ranking source"));
        }
        if source.scope != context.scope {
            return Err(error(
                "Account identity changed: remote rank belongs to another principal",
            ));
        }
        if source.candidates.len() > 5000 {
            return Err(error(
                "Remote rank list exceeds its absolute 5000 candidate safety budget",
            ));
        }
        validate_status(&source.status)?;
        validate_coverage(&source.coverage)?;
        if source
            .coverage
            .repositories
            .iter()
            .any(|repository| !contains_repository(&context.remote_repositories, repository))
        {
            return Err(error(
                "Remote source coverage names a repository outside the explicit scope",
            ));
        }
        let expected = context.remote_query(source.source);
        let mut report = SourceReport {
            source: source.source,
            loaded: source.candidates.len(),
            admitted: 0,
            suppressed: 0,
            status: source.status.clone(),
        };
        if let Err(reason) = expected {
            if !source.candidates.is_empty() {
                return Err(error(
                    "Remote entries were requested with unsupported/weakened constraints",
                ));
            }
            report.status = SourceStatus::Unsupported(reason.0);
            coverage.complete = false;
            reports.push(report);
            continue;
        }
        if source.query != expected? {
            return Err(error(
                "Remote ranking query differs from its exact source-specific plan",
            ));
        }
        match &source.status {
            SourceStatus::Unavailable(reason) | SourceStatus::Unsupported(reason) => {
                if !source.candidates.is_empty() {
                    return Err(error(
                        "Unavailable remote source supplied apparently successful candidates",
                    ));
                }
                coverage.complete = false;
                coverage
                    .unavailable
                    .push(format!("{}: {reason}", source.source.label()));
                reports.push(report);
                continue;
            }
            SourceStatus::Partial(reason) => {
                coverage.complete = false;
                coverage
                    .reasons
                    .push(format!("{}: {reason}", source.source.label()));
            }
            SourceStatus::Complete => {}
        }
        coverage.complete &= source.coverage.complete;
        coverage.cancelled |= source.coverage.cancelled;
        coverage
            .reasons
            .extend(source.coverage.reasons.iter().cloned());
        coverage
            .unavailable
            .extend(source.coverage.unavailable.iter().cloned());
        if (!source.coverage.complete || source.coverage.cancelled)
            && matches!(report.status, SourceStatus::Complete)
        {
            report.status = SourceStatus::Partial(
                "Provider snapshot is incomplete/cancelled within its declared limits".into(),
            );
        }
        if source.candidates.len() > context.policy.per_source_results {
            coverage.complete = false;
            report.status = match report.status {
                SourceStatus::Partial(reason) => SourceStatus::Partial(format!(
                    "{reason}; per-source visible result cap reached"
                )),
                _ => SourceStatus::Partial("Per-source visible result cap reached".into()),
            };
        }
        let mut entries = vec![];
        for (index, candidate) in source
            .candidates
            .iter()
            .take(context.policy.per_source_results)
            .enumerate()
        {
            if cancel.is_cancelled() {
                return Err(error("Blended search cancelled during remote admission"));
            }
            let hit = &candidate.hit;
            hit.document.validate(&context.scope)?;
            validate_hit(hit)?;
            if hit.document.source != SourceKind::GitHub
                || !contains_repository(&context.remote_repositories, &hit.document.key.repository)
            {
                return Err(error("Remote result is outside the declared remote scope"));
            }
            let expected_kind = match source.source {
                RankingSource::GithubCode => ResultKind::File,
                RankingSource::GithubPullRequests => ResultKind::PullRequest,
                RankingSource::GithubIssues => ResultKind::Issue,
                _ => return Err(error("Unsupported blend provider kind")),
            };
            if hit.document.key.kind != expected_kind {
                return Err(error("Remote provider returned a different resource kind"));
            }
            validate_identity(hit, &candidate.identity)?;
            if let Some(discovered) = &visibility.discovered {
                if !discovered.accepts(hit, &candidate.identity)? {
                    report.suppressed += 1;
                    coverage.complete = false;
                    continue;
                }
            }
            validate_selected_identity(visibility, hit, &candidate.identity)?;
            let parent = match &candidate.identity {
                RemoteIdentity::OpaqueNode { node_id, .. }
                    if matches!(expected_kind, ResultKind::PullRequest | ResultKind::Issue) =>
                {
                    Some(ParentAlias::new(
                        &context.scope,
                        &hit.document.key.repository,
                        expected_kind,
                        node_id,
                    )?)
                }
                _ => None,
            };
            if parent
                .as_ref()
                .is_some_and(|alias| visibility.blocked_parents.contains(alias))
            {
                report.suppressed += 1;
                coverage.complete = false;
                continue;
            }
            if parent.is_none()
                && matches!(expected_kind, ResultKind::PullRequest | ResultKind::Issue)
                && visibility.blocked_parents.iter().any(|blocked| {
                    blocked
                        .repository
                        .eq_ignore_ascii_case(&hit.document.key.repository)
                        && blocked.kind == expected_kind.label()
                })
            {
                // Unresolved/numeric provenance cannot prove it differs from a
                // known fenced parent. This suppresses; it never aliases by number.
                report.suppressed += 1;
                coverage.complete = false;
                continue;
            }
            if let Some(alias) = &parent {
                if let Some(current) = visibility.parents.get(alias) {
                    if database_id(&candidate.identity)
                        .zip(
                            current
                                .metadata
                                .discussion
                                .as_ref()
                                .and_then(|context| context.resource_numeric_id),
                        )
                        .is_some_and(|(observed, cached)| observed != cached)
                    {
                        return Err(error("Canonical parent database identity changed: actual opaque node has contradictory numeric provenance"));
                    }
                    let remote_number =
                        match &hit.document.destination {
                            Destination::PullRequest { number, .. }
                            | Destination::Issue { number, .. } => *number,
                            _ => return Err(error(
                                "Canonical parent provenance has no matching parent destination",
                            )),
                        };
                    if current
                        .metadata
                        .discussion
                        .as_ref()
                        .is_none_or(|context| context.parent_number != remote_number)
                    {
                        return Err(error("Canonical parent destination changed: no cached body silently retargeted"));
                    }
                    if let Some(observed) = repository_id(&candidate.identity) {
                        if current
                            .metadata
                            .discussion
                            .as_ref()
                            .is_none_or(|context| context.repository_id != observed)
                        {
                            return Err(error("Canonical repository identity changed: no stale cached or remote body admitted"));
                        }
                    }
                }
            }
            let alias = match parent.as_ref() {
                Some(parent) => parent.hash()?,
                None if candidate.identity == RemoteIdentity::Unavailable => digest(&(
                    "unresolved-remote-candidate-v1",
                    source.source,
                    index,
                    &context.scope,
                    &hit.document.key,
                ))?,
                None => remote_alias(&context.scope, &hit.document, &candidate.identity)?,
            };
            let observed = (
                repository_id(&candidate.identity),
                database_id(&candidate.identity),
                canonical_remote_destination(&hit.document.destination)?,
            );
            if let Some(previous) = remote_observations.get_mut(&alias) {
                if previous
                    .0
                    .zip(observed.0)
                    .is_some_and(|(left, right)| left != right)
                    || previous
                        .1
                        .zip(observed.1)
                        .is_some_and(|(left, right)| left != right)
                    || previous.2 != observed.2
                {
                    return Err(error("Canonical remote resource has contradictory repository/database/destination provenance; no duplicate guessed away"));
                }
                previous.0 = previous.0.or(observed.0);
                previous.1 = previous.1.or(observed.1);
            } else {
                remote_observations.insert(alias.clone(), observed);
            }
            let representative = format!("remote:{:?}:{index}", source.source);
            let mut prepared = hit.clone();
            let mut presentation = Presentation::RemoteFragment;
            let mut origin = CandidateOrigin::Remote {
                source: source.source,
                index,
            };
            if let Some(current) = parent
                .as_ref()
                .and_then(|parent| visibility.parents.get(parent))
            {
                // Even if this body fell outside the local top-k, an indexed remote
                // fragment must not replace a known current canonical projection.
                prepared.document = Arc::clone(current);
                prepared.snippet = bounded(&current.body, 1000);
                prepared.highlights.clear();
                prepared.line = 0;
                prepared.column = 0;
                prepared.matched_fields = vec!["GitHub indexed parent match; current authorized cached body shown instead of remote fragments".into()];
                presentation = Presentation::AuthorizedCurrentDocument;
                origin = CandidateOrigin::CurrentCanonical(current.key.stable_id());
            } else {
                let mut document = (*prepared.document).clone();
                document.key.resource = format!("remote-discovery-v2:{alias}");
                prepared.document = Arc::new(document);
            }
            candidates.insert(representative.clone(), prepared);
            origins.insert(representative.clone(), origin);
            entries.push(RankEntry {
                alias,
                representative,
                presentation,
            });
            report.admitted += 1;
        }
        if report.suppressed > 0 {
            coverage.reasons.push(format!(
                "{}: {} parent candidates hidden by canonical visibility or unresolved parent provenance",
                source.source.label(),
                report.suppressed
            ));
            if matches!(report.status, SourceStatus::Complete) {
                report.status = SourceStatus::Partial(
                    "Canonical visibility/provenance guards suppress indexed parent fragments"
                        .into(),
                );
            }
        }
        lists.push(RankList {
            source: source.source,
            entries,
        });
        reports.push(report);
    }
    for missing in context.policy.sources.difference(&seen_sources) {
        if context.policy.enabled {
            coverage.complete = false;
            reports.push(SourceReport {
                source: *missing,
                loaded: 0,
                admitted: 0,
                suppressed: 0,
                status: SourceStatus::Unavailable(
                    "No completed source snapshot; no successful-empty claim".into(),
                ),
            });
        }
    }
    if lists
        .iter()
        .flat_map(|list| list.entries.iter().map(|entry| &entry.alias))
        .collect::<BTreeSet<_>>()
        .len()
        > context.policy.max_results
    {
        coverage.complete = false;
        coverage.reasons.push(
            "Final blended result cap reached; omitted candidates are not successful absence"
                .into(),
        );
    }
    let fused = fuse(&lists, context.policy.max_results).map_err(error)?;
    let mut output = BlendPage {
        page: SearchPage {
            query: context.query.clone(),
            hits: vec![],
            coverage,
        },
        origins: BTreeMap::new(),
        contributions: BTreeMap::new(),
        reports,
    };
    for item in fused {
        let mut hit = candidates
            .remove(&item.representative)
            .ok_or_else(|| error("Missing exact fused candidate"))?;
        let key = hit.document.key.stable_id();
        hit.score = item.score as f32;
        hit.matched_fields.push(format!(
            "RRF k=60: {}",
            item.contributions
                .iter()
                .map(|(source, rank)| format!("{} #{rank}", source.label()))
                .collect::<Vec<_>>()
                .join(" + ")
        ));
        output.origins.insert(
            key.clone(),
            origins
                .remove(&item.representative)
                .ok_or_else(|| error("Missing fused destination origin"))?,
        );
        output.contributions.insert(key, item.contributions);
        output.page.hits.push(hit);
    }
    output.page.coverage.deletion_authoritative = false;
    if cancel.is_cancelled() {
        return Err(error("Blended search cancelled before presentation"));
    }
    visibility.validate(cache)?;
    Ok(output)
}

fn validate_local_query(original: &Query, local: &Query) -> Result<(), QueryError> {
    if original == local {
        return Ok(());
    }
    let mut original_filters = original.filters.clone();
    let mut local_filters = local.filters.clone();
    let original_rev = original_filters.remove("rev");
    let local_rev = local_filters.remove("rev");
    if original.mode != local.mode
        || original.text != local.text
        || original.terms != local.terms
        || original_filters != local_filters
        || original_rev.is_none()
        || original
            .filters
            .get("type")
            .is_some_and(|kinds| !kinds.iter().any(|kind| kind == "file"))
        || local_rev.is_none_or(|values| {
            values.iter().any(|value| {
                !matches!(value.len(), 40 | 64)
                    || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        })
    {
        return Err(error("Local blend query weakened or changed constraints instead of resolving the exact Git revision filter"));
    }
    Ok(())
}
fn validate_status(status: &SourceStatus) -> Result<(), QueryError> {
    let reason = match status {
        SourceStatus::Complete => return Ok(()),
        SourceStatus::Partial(reason)
        | SourceStatus::Unavailable(reason)
        | SourceStatus::Unsupported(reason) => reason,
    };
    if reason.is_empty() || reason.len() > 16 * 1024 {
        return Err(error("Remote source status needs a bounded honest reason"));
    }
    Ok(())
}
fn validate_coverage(coverage: &Coverage) -> Result<(), QueryError> {
    if coverage.repositories.len() > 1000
        || coverage
            .reasons
            .len()
            .saturating_add(coverage.unavailable.len())
            > 1000
        || coverage
            .reasons
            .iter()
            .chain(&coverage.unavailable)
            .any(|reason| reason.len() > 16 * 1024)
    {
        return Err(error("Remote coverage exceeds its bounded metadata budget"));
    }
    Ok(())
}

fn validate_hit(hit: &SearchHit) -> Result<(), QueryError> {
    if hit.snippet.len() > 64 * 1024
        || hit.highlights.len() > 1000
        || hit.matched_fields.len() > 256
        || hit
            .matched_fields
            .iter()
            .any(|field| field.len() > 16 * 1024)
        || hit.highlights.iter().any(|range| {
            range.start >= range.end
                || range.end > hit.snippet.len()
                || !hit.snippet.is_char_boundary(range.start)
                || !hit.snippet.is_char_boundary(range.end)
        })
    {
        return Err(error(
            "Prepared blend hit exceeds snippet/evidence bounds or has invalid UTF-8 highlights",
        ));
    }
    if matches!(hit.document.destination, Destination::LocalFile { .. })
        && (hit.line == 0 || hit.column == 0)
    {
        return Err(error(
            "Prepared local file hit has no exact positive line/column destination",
        ));
    }
    Ok(())
}
fn body_alias(document: &Document) -> Result<Option<ParentAlias>, QueryError> {
    let Some(context) = &document.metadata.discussion else {
        return Ok(None);
    };
    if !matches!(
        context.kind,
        DiscussionKind::PullRequestBody | DiscussionKind::IssueBody
    ) {
        return Ok(None);
    }
    if document.key.kind
        != match context.kind {
            DiscussionKind::PullRequestBody => ResultKind::PullRequest,
            _ => ResultKind::Issue,
        }
        || context.parent_node != context.resource_node
    {
        return Err(error(
            "Cached parent body has contradictory canonical identity",
        ));
    }
    ParentAlias::new(
        &document.key.scope,
        &document.key.repository,
        document.key.kind,
        &context.parent_node,
    )
    .map(Some)
}
fn validate_identity(hit: &SearchHit, identity: &RemoteIdentity) -> Result<(), QueryError> {
    if repository_id(identity).is_some_and(|id| id == 0) {
        return Err(error("Remote canonical repository ID is invalid"));
    }
    match identity {
        RemoteIdentity::OpaqueNode {
            node_id,
            database_id,
            ..
        } => {
            opaque(node_id)?;
            if database_id == &Some(0) || hit.document.key.kind == ResultKind::File {
                return Err(error("Wrong or invalid node provenance"));
            }
        }
        RemoteIdentity::Numeric { database_id, .. }
            if *database_id > 0 && hit.document.key.kind != ResultKind::File => {}
        RemoteIdentity::CodeBlob { path, blob_sha, .. } => {
            let Destination::RemoteCodeCandidate {
                path: actual_path,
                blob_sha: actual_blob,
                ..
            } = &hit.document.destination
            else {
                return Err(error(
                    "Code provenance has no exact indexed-blob destination",
                ));
            };
            if path != actual_path || !blob_sha.eq_ignore_ascii_case(actual_blob) {
                return Err(error(
                    "Remote code provenance differs from its raw path/blob",
                ));
            }
        }
        RemoteIdentity::Unavailable => {} // Separate document/candidate identity; no parent alias guessed.
        _ => return Err(error("Invalid numeric remote provenance")),
    }
    Ok(())
}
fn validate_selected_identity(
    visibility: &CurrentVisibility,
    hit: &SearchHit,
    identity: &RemoteIdentity,
) -> Result<(), QueryError> {
    for (target, _) in &visibility.epochs {
        if !target
            .repository
            .eq_ignore_ascii_case(&hit.document.key.repository)
        {
            continue;
        }
        if repository_id(identity).is_some_and(|observed| observed != target.repository_id) {
            return Err(error("Canonical repository identity changed: selected bound source and remote repository IDs disagree"));
        }
        let (kind, number) = match &hit.document.destination {
            Destination::PullRequest { number, .. } => {
                (collaboration::ParentKind::PullRequest, *number)
            }
            Destination::Issue { number, .. } => (collaboration::ParentKind::Issue, *number),
            _ => continue,
        };
        if kind == target.parent_kind && number == target.number {
            if let RemoteIdentity::OpaqueNode { node_id, .. } = identity {
                if node_id != &target.parent_node {
                    return Err(error("Canonical parent identity changed: matching selected parent number has a different actual opaque node; no number-based alias or stale fallback"));
                }
            }
        }
    }
    Ok(())
}
fn repository_id(identity: &RemoteIdentity) -> Option<u64> {
    match identity {
        RemoteIdentity::OpaqueNode { repository_id, .. }
        | RemoteIdentity::Numeric { repository_id, .. }
        | RemoteIdentity::CodeBlob { repository_id, .. } => *repository_id,
        RemoteIdentity::Unavailable => None,
    }
}
fn database_id(identity: &RemoteIdentity) -> Option<u64> {
    match identity {
        RemoteIdentity::OpaqueNode { database_id, .. } => *database_id,
        RemoteIdentity::Numeric { database_id, .. } => Some(*database_id),
        RemoteIdentity::CodeBlob { .. } | RemoteIdentity::Unavailable => None,
    }
}
fn canonical_remote_destination(destination: &Destination) -> Result<Destination, QueryError> {
    let mut destination = destination.clone();
    match &mut destination {
        Destination::PullRequest { host, repository, comment_id, thread_id, .. } if comment_id.is_none() && thread_id.is_none() => {
            *host = host.to_ascii_lowercase();
            *repository = repository.to_ascii_lowercase();
        }
        Destination::Issue { host, repository, comment_id, .. } if comment_id.is_none() => {
            *host = host.to_ascii_lowercase();
            *repository = repository.to_ascii_lowercase();
        }
        Destination::RemoteCodeCandidate { host, repository, blob_sha, .. } => {
            *host = host.to_ascii_lowercase();
            *repository = repository.to_ascii_lowercase();
            *blob_sha = blob_sha.to_ascii_lowercase();
        }
        _ => return Err(error("Remote parent/indexed-blob provenance has no matching exact destination; a parent match cannot invent a comment target")),
    }
    Ok(destination)
}
fn remote_alias(
    scope: &Scope,
    document: &Document,
    identity: &RemoteIdentity,
) -> Result<String, QueryError> {
    // Optional enrichment is checked separately; missing numeric/repository
    // metadata cannot create a second vote or turn an opaque node into a number.
    let core = match identity {
        RemoteIdentity::OpaqueNode { node_id, .. } => serde_json::json!({"opaque_node":node_id}),
        RemoteIdentity::Numeric { database_id, .. } => serde_json::json!({"numeric":database_id}),
        RemoteIdentity::CodeBlob { path, blob_sha, .. } => {
            serde_json::json!({"code_path":path,"blob_sha":blob_sha.to_ascii_lowercase()})
        }
        RemoteIdentity::Unavailable => {
            return Err(error("Missing provenance has no canonical remote alias"))
        }
    };
    digest(&(
        "remote-typed-v2",
        scope,
        document.key.repository.to_ascii_lowercase(),
        document.key.kind,
        core,
    ))
}
fn bound_generation(cache: &Cache, target: &DiscussionTarget) -> Result<Option<u64>, QueryError> {
    let Some(binding) = cache
        .binding_status(&target.scope, &target.source())
        .map_err(error)?
    else {
        return Ok(None);
    };
    if binding.binding != target.binding()? {
        return Err(error(
            "Current visibility binding differs from its canonical target",
        ));
    }
    let checkpoint = cache
        .sync_checkpoint(&target.scope, &target.source())
        .map_err(error)?;
    Ok(binding
        .visible_generation
        .filter(|generation| checkpoint.is_some_and(|(stored, _)| stored == *generation)))
}
fn contains_repository(scope: &BTreeSet<String>, repository: &str) -> bool {
    scope
        .iter()
        .any(|selected| selected.eq_ignore_ascii_case(repository))
}
fn positive_scope(scope: &Scope) -> Result<(), QueryError> {
    let uid = scope
        .login
        .strip_prefix("user-id:")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|id| *id > 0)
        .ok_or_else(|| error("Blended private discovery requires a verified numeric account"))?;
    if Scope::github(scope.host.to_ascii_lowercase(), Some(uid)).map_err(error)? != *scope {
        return Err(error(
            "Blended discovery requires canonical host/principal scope",
        ));
    }
    Ok(())
}
fn opaque(node: &str) -> Result<(), QueryError> {
    if node.is_empty() || node.len() > 512 || !node.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(error("Canonical node provenance is missing or malformed"));
    }
    Ok(())
}
fn digest(value: &impl Serialize) -> Result<String, QueryError> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(error)?)
    ))
}
fn bounded(value: &str, bytes: usize) -> String {
    let mut end = value.len().min(bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].into()
}
fn error(value: impl std::fmt::Display) -> QueryError {
    QueryError(value.to_string())
}

#[cfg(test)]
mod tests;
