//! Prepared search state for the single application owner; all collection/search is worker work.
use super::{
    collaboration::{self, DiscussionLimits},
    commits::{self, CommitLimits},
    corpus::{Corpus, CorpusLimits, LocalSource, RepositorySelection},
    index::{RankedIndex, RankedLimits},
    model::{Destination, SavedSearch, SourceKind},
    scan::{exact_search, SearchPage},
    sources::MetadataSources,
    Query, QueryError, ScanLimits, SearchMode,
};
use crate::{
    config::Config,
    git::GitRepository,
    providers::gh::{
        GithubProvider, HostIdentity, RemoteSearchItem, RemoteSearchKind, ResolvedRemoteFile,
    },
    providers::runner::CancellationToken,
    storage::{Cache, Scope},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

#[cfg(test)]
#[path = "ranked_reuse_tests.rs"]
mod ranked_reuse_tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchAction {
    Query,
    Mode(SearchMode),
    Worktree,
    Revision,
    Submit,
    Preview(usize),
    Open(usize),
    Save,
    Saved(String),
    /// Toggle the pin of the named saved search; pinned searches are listed first.
    PinSaved(String),
    /// Rename the named saved search to the current Name field.
    RenameSaved(String),
    /// Remove the named saved search from this account's saved views.
    DeleteSaved(String),
    Corpus,
    Name,
    SavedVersion,
    ToggleHidden,
    ToggleIgnored,
    ToggleRepository(String),
    PreviewScroll(i32),
    BlendControls,
    BlendPreferences,
}
fn merge_corpus(target: &mut Corpus, mut incoming: Corpus) {
    target.coverage.complete &= incoming.coverage.complete;
    target.coverage.deletion_authoritative &= incoming.coverage.deletion_authoritative;
    target.coverage.cancelled |= incoming.coverage.cancelled;
    target.coverage.scanned_bytes = target
        .coverage
        .scanned_bytes
        .saturating_add(incoming.coverage.scanned_bytes);
    target.coverage.skipped_large += incoming.coverage.skipped_large;
    target.coverage.skipped_binary += incoming.coverage.skipped_binary;
    target.coverage.skipped_special += incoming.coverage.skipped_special;
    target
        .coverage
        .repositories
        .append(&mut incoming.coverage.repositories);
    target.coverage.repositories.sort();
    target.coverage.repositories.dedup();
    target
        .coverage
        .unavailable
        .append(&mut incoming.coverage.unavailable);
    target
        .coverage
        .reasons
        .append(&mut incoming.coverage.reasons);
    target.documents.append(&mut incoming.documents);
    target.coverage.documents = target.documents.len();
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewLine {
    pub number: usize,
    pub text: String,
    pub selected: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPreview {
    pub title: String,
    pub context: String,
    pub source: SourceKind,
    pub matched_fields: Vec<String>,
    pub destination: Destination,
    /// When the previewed content was read (see [`Document::freshness`]).
    pub freshness: String,
    pub lines: Vec<PreviewLine>,
}
#[derive(Debug, Clone)]
pub struct SearchWorkspace {
    pub scope: Scope,
    pub selections: Vec<RepositorySelection>,
    pub available_selections: Vec<RepositorySelection>,
    pub input: String,
    pub mode: SearchMode,
    pub corpus_limits: CorpusLimits,
    pub scan_limits: ScanLimits,
    /// Account-wide derivative safety budgets are separate from per-corpus inclusion policy.
    pub ranked_limits: RankedLimits,
    pub metadata_sources: MetadataSources,
    pub discussion_limits: DiscussionLimits,
    pub commit_limits: CommitLimits,
    pub page: Option<SearchPage>,
    pub preview: Option<SearchPreview>,
    pub saved: Vec<SavedSearch>,
    pub warnings: Vec<String>,
    pub remote_identity: Option<HostIdentity>,
    pub remote_repositories: Vec<String>,
    pub remote_items: BTreeMap<String, RemoteSearchItem>,
    pub blend_policy: super::fusion::blend::BlendPolicy,
    pub blend_evidence: Option<std::sync::Arc<crate::search_blend::worker::BlendEvidence>>,
}
impl SearchWorkspace {
    pub fn new(scope: Scope, selections: Vec<RepositorySelection>) -> Self {
        Self {
            scope,
            available_selections: selections.clone(),
            selections,
            input: String::new(),
            mode: SearchMode::Exact,
            corpus_limits: CorpusLimits::default(),
            scan_limits: ScanLimits::default(),
            ranked_limits: RankedLimits::default(),
            metadata_sources: MetadataSources::default(),
            discussion_limits: DiscussionLimits::default(),
            commit_limits: CommitLimits::default(),
            page: None,
            preview: None,
            saved: Vec::new(),
            warnings: Vec::new(),
            remote_identity: None,
            remote_repositories: Vec::new(),
            remote_items: BTreeMap::new(),
            blend_policy: super::fusion::blend::BlendPolicy::default(),
            blend_evidence: None,
        }
    }
    /// Existing mapped checkouts only; never clones/fetches or discovers a remote account.
    /// Run in a worker because local Git discovery is a subprocess.
    pub fn from_config(
        scope: Scope,
        config: &Config,
        current_directory: &Path,
        cancellation: &CancellationToken,
    ) -> Result<Self, QueryError> {
        let active = config
            .active_set
            .as_ref()
            .and_then(|set| config.repository_sets.get(set));
        let selections = config
            .local_repositories
            .iter()
            .filter(|(repository, _)| {
                !config.excluded_repositories.contains(repository)
                    && active.is_none_or(|selected| selected.contains(repository))
            })
            .map(|(repository, directory)| RepositorySelection {
                repository: repository.clone(),
                directory: directory.clone(),
                source: LocalSource::Worktree,
                sets: config
                    .repository_sets
                    .iter()
                    .filter(|(_, repos)| repos.contains(repository))
                    .map(|(set, _)| set.clone())
                    .collect(),
            })
            .collect::<Vec<_>>();
        let mut workspace = Self::new(scope, selections);
        workspace.corpus_limits.max_file_bytes = config.max_file_bytes as usize;
        workspace.ranked_limits.max_documents = config.index_max_documents;
        workspace.ranked_limits.max_total_bytes =
            usize::try_from(config.index_max_total_bytes).unwrap_or(usize::MAX);
        if workspace.selections.is_empty()
            && config.local_repositories.is_empty()
            && config.active_set.is_none()
        {
            if let Some(repository) =
                GitRepository::discover_with_cancellation(current_directory, cancellation.clone())
                    .map_err(|error| QueryError(error.to_string()))?
            {
                // The origin is a hint for the explicit Corpus mapping step,
                // never an inferred mapping of this checkout.
                workspace.warnings.push(
                    match crate::origin_repository::detect(&repository.root, &config.host) {
                        Some(origin) => format!("Current checkout labelled local/current; origin {origin} is not mapped. Press c Corpus, then Map existing, to search it as {origin}"),
                        None => "Current checkout labelled local/current; it has no GitHub origin mapping. Press c Corpus to map it".into(),
                    },
                );
                workspace.selections.push(RepositorySelection {
                    repository: "local/current".into(),
                    directory: repository.root,
                    source: LocalSource::Worktree,
                    sets: Vec::new(),
                });
            }
        }
        if let Some(active) = active {
            workspace.warnings.extend(active.iter().filter(|repository|!workspace.selections.iter().any(|selection|&selection.repository==*repository)).map(|repository|format!("{repository} is selected but has no accessible local mapping; map or explicitly clone it")));
        }
        workspace.available_selections = workspace.selections.clone();
        Ok(workspace)
    }
    pub fn context(&self) -> String {
        if self.mode == SearchMode::GitHub {
            let actor = self
                .remote_identity
                .as_ref()
                .map_or("identity/scope verified on Search", |identity| {
                    identity.login.as_str()
                });
            return format!(
                "{} / {} / GitHub remote discovery / {}",
                self.scope.host,
                actor,
                if self.remote_repositories.is_empty() {
                    if self.remote_identity.is_some() {
                        "explicit organization query".into()
                    } else {
                        "remote repository scope chosen on Search".into()
                    }
                } else {
                    self.remote_repositories.join(", ")
                }
            );
        }
        let mut sources = self
            .selections
            .iter()
            .map(|selection| {
                format!(
                    "{} @ {}",
                    selection.repository,
                    match &selection.source {
                        LocalSource::Worktree => "working files".into(),
                        LocalSource::Revision(revision) => format!("saved {revision}"),
                    }
                )
            })
            .collect::<Vec<_>>();
        sources.extend(self.metadata_sources.discussions.iter().map(|target| {
            format!(
                "{} #{} {} • cached discussion",
                target.repository,
                target.number,
                target.category.label()
            )
        }));
        sources.extend(self.metadata_sources.commits.iter().map(|target| {
            format!(
                "{} @ {} • up to {} original commit messages",
                target.repository, target.revision, target.max_commits
            )
        }));
        let sources = sources.join(", ");
        let operator = self.remote_identity.as_ref().filter(|identity| {
            Scope::github(&identity.host, identity.account_id)
                .is_ok_and(|scope| scope == self.scope)
        });
        let account = operator.map_or_else(
            || {
                if self.scope.host == "local" && self.scope.login == "workspace" {
                    "@workspace".into()
                } else if let Some(id) = self.scope.login.strip_prefix("user-id:") {
                    format!("account #{id}")
                } else {
                    format!("account {} (unverified)", self.scope.login)
                }
            },
            |identity| format!("@{}", identity.login),
        );
        format!(
            "{} / {} / {}",
            self.scope.host,
            account,
            if sources.is_empty() {
                "no selected local corpus".into()
            } else {
                sources
            }
        )
    }
    pub fn set_mode(&mut self, mode: SearchMode) {
        self.mode = mode;
        self.page = None;
        self.preview = None;
        self.remote_items.clear();
        self.blend_evidence = None;
    }
    pub fn set_source(&mut self, source: LocalSource) {
        for selection in &mut self.selections {
            selection.source = source.clone();
        }
        self.page = None;
        self.preview = None;
        self.blend_evidence = None;
    }
    pub fn set_metadata_sources(&mut self, sources: MetadataSources) -> Result<(), QueryError> {
        sources.validate(&self.scope)?;
        self.metadata_sources = sources;
        self.page = None;
        self.preview = None;
        self.remote_items.clear();
        self.blend_evidence = None;
        Ok(())
    }
    /// Blocking only on the owning background worker. Do not invoke from draw/input handlers.
    pub fn search(
        &mut self,
        input: &str,
        cancellation: &CancellationToken,
    ) -> Result<(), QueryError> {
        if !matches!(self.mode, SearchMode::Exact | SearchMode::Regex) {
            return Err(QueryError("This scan needs Exact or Regex. Ranked uses the selected Tantivy index; GitHub uses the authenticated gh search provider.".into()));
        }
        let (query, selections, corpus) = self.prepare_corpus(input, cancellation)?;
        let page = exact_search(&corpus, &query, &self.scan_limits, cancellation)?;
        self.accept_page(input, selections, page);
        Ok(())
    }
    /// Cached discussion bytes require explicit source intent; collection never calls gh.
    /// A positive-account caller must bracket this worker with fresh identity checks.
    pub fn search_cached(
        &mut self,
        input: &str,
        cache: &Cache,
        cancellation: &CancellationToken,
    ) -> Result<(), QueryError> {
        if !matches!(self.mode, SearchMode::Exact | SearchMode::Regex) {
            return Err(QueryError(
                "Cached scanning needs Exact or Regex mode".into(),
            ));
        }
        let (mut query, selections) = self.prepare_selections(input)?;
        let (corpus, _) = self.collect_combined(&query, &selections, cache, cancellation)?;
        Self::resolve_file_filter(&mut query, &corpus);
        let page = exact_search(&corpus, &query, &self.scan_limits, cancellation)?;
        self.accept_page(input, selections, page);
        Ok(())
    }

    /// Worker-only ranked collection/publication/query. Same local scope rules as Exact/Regex.
    pub fn search_ranked(
        &mut self,
        input: &str,
        cache: &mut Cache,
        index_root: &Path,
        cancellation: &CancellationToken,
    ) -> Result<(), QueryError> {
        if self.mode != SearchMode::Ranked {
            return Err(QueryError("Choose Ranked mode for Tantivy indexing".into()));
        }
        let (mut query, selections) = self.prepare_selections(input)?;
        let mut context = selections
            .iter()
            .map(|selection| {
                (
                    &selection.repository,
                    selection.directory.as_os_str().as_bytes().to_vec(),
                    &selection.source,
                    &selection.sets,
                )
            })
            .collect::<Vec<_>>();
        context.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        let key = serde_json::to_vec(&(
            context,
            self.corpus_limits.include_ignored,
            self.corpus_limits.include_hidden,
            self.corpus_limits.max_file_bytes,
            &self.metadata_sources,
        ))
        .map_err(|error| QueryError(error.to_string()))?;
        let source = format!("selected-local:{:x}", Sha256::digest(key));
        let mut limits = self.ranked_limits.clone();
        limits.max_hits = self.scan_limits.max_matches.min(limits.max_hits);
        // Working and saved files only: cached discussions and commit messages are always
        // collected again, because their visibility is proved per query. A rev: filter is
        // resolved against the collected corpus, so it collects too.
        let fingerprint = (self.metadata_sources.is_empty()
            && !selections.is_empty()
            && !query.filters.contains_key("rev"))
        .then(|| {
            Corpus::input_fingerprint(&selections, &self.ranked_file_limits(0, 0), cancellation)
        })
        .flatten();
        if let Some(fingerprint) = &fingerprint {
            let index = RankedIndex::open(
                index_root,
                self.scope.clone(),
                cache,
                limits.clone(),
                cancellation,
            )?;
            if index.reusable_generation(&source, fingerprint).is_some() {
                let page =
                    index.search_source_current(&source, &query, &BTreeMap::new(), cancellation)?;
                // A clear, revoke or other publication committed while searching withdraws
                // this page; collect again instead.
                if index.sources_unchanged(cache)? && !page.coverage.cancelled {
                    self.accept_page(input, selections, page);
                    return Ok(());
                }
            }
        }
        // Reserve before any file/Git/cache collection. Clear/revoke fences late completion.
        let generation = RankedIndex::reserve_generation(cache, &self.scope, &source)?;
        let (corpus, versions) = self.collect_combined(&query, &selections, cache, cancellation)?;
        Self::resolve_file_filter(&mut query, &corpus);
        let mut index =
            RankedIndex::open(index_root, self.scope.clone(), cache, limits, cancellation)?;
        index.publish_reserved_from_input(
            cache,
            &source,
            generation,
            &corpus,
            fingerprint,
            cancellation,
        )?;
        let page = index.search_source_current(&source, &query, &versions, cancellation)?;
        self.accept_page(input, selections, page);
        Ok(())
    }

    /// Worker-only gh discovery. Caller supplies a freshly verified positive account and exact
    /// selected remote repository scope; an empty selection never becomes a global code search.
    pub fn search_github(
        &mut self,
        input: &str,
        provider: &GithubProvider,
        identity: &HostIdentity,
        repositories: &[String],
        cancellation: &CancellationToken,
    ) -> Result<(), QueryError> {
        if self.mode != SearchMode::GitHub {
            return Err(QueryError("Choose GitHub mode for remote discovery".into()));
        }
        super::remote::verify_scope(&self.scope, identity)?;
        let query = Query::parse(input, SearchMode::GitHub)?;
        let kind = super::remote::kind(&query)?;
        let explicit_scope = kind != RemoteSearchKind::Repositories
            && (query
                .filters
                .get("repo")
                .is_some_and(|repos| !repos.is_empty())
                || query.first("org").is_some());
        let repositories = if kind == RemoteSearchKind::Repositories {
            if query.first("org").is_none() {
                return Err(QueryError(
                    "Repository discovery needs an explicit org: scope; no global search was made"
                        .into(),
                ));
            }
            Vec::new()
        } else if let Some(explicit) = query.filters.get("repo").filter(|repos| !repos.is_empty()) {
            // Typed repo: qualifiers are the user's explicit scope for this query;
            // they replace (never broaden past) the default checkout/set scope.
            let mut selected = explicit.clone();
            for repo in &selected {
                crate::config::validate_repository(repo)
                    .map_err(|error| QueryError(error.to_string()))?;
            }
            selected.sort();
            selected.dedup();
            selected
        } else if query.first("org").is_some() {
            // An explicit org: qualifier scopes the provider query by itself.
            Vec::new()
        } else {
            if repositories.is_empty() {
                return Err(QueryError("Choose explicit remote repositories before GitHub code/PR/issue search; no broader request was made".into()));
            }
            let mut selected = repositories.to_vec();
            for repo in &selected {
                crate::config::validate_repository(repo)
                    .map_err(|error| QueryError(error.to_string()))?;
            }
            selected.sort();
            selected.dedup();
            selected
        };
        let result = provider
            .remote_search(identity, &query, kind, &repositories, cancellation)
            .map_err(|error| QueryError(error.to_string()))?;
        let (page, items) =
            super::remote::prepare_page(&self.scope, &query, result, self.scan_limits.max_matches)?;
        self.input = input.into();
        self.remote_identity = Some(identity.clone());
        if !explicit_scope {
            self.remote_repositories = repositories;
        }
        self.remote_items = items;
        self.page = Some(page);
        self.preview = None;
        Ok(())
    }

    pub fn remote_item(&self, index: usize) -> Result<&RemoteSearchItem, QueryError> {
        let hit = self
            .page
            .as_ref()
            .and_then(|page| page.hits.get(index))
            .ok_or_else(|| QueryError("Selected remote result is no longer available".into()))?;
        self.remote_items
            .get(&hit.document.key.stable_id())
            .ok_or_else(|| {
                QueryError("Selected result has no verified remote candidate provenance".into())
            })
    }

    /// Actual GitHub actors only; repository ownership never becomes a code-author association.
    pub fn result_actors(
        &self,
        index: usize,
    ) -> Result<Vec<(String, crate::providers::gh::Actor)>, QueryError> {
        let document = self.result_document(index)?;
        if document.source == SourceKind::Collaboration {
            let identity = self
                .remote_identity
                .as_ref()
                .ok_or_else(|| QueryError("Discussion identity has not been verified".into()))?;
            super::remote::verify_scope(&self.scope, identity)?;
            let role = match document
                .metadata
                .discussion
                .as_ref()
                .map(|context| context.kind)
            {
                Some(super::model::DiscussionKind::Review) => "review author",
                Some(super::model::DiscussionKind::InlineComment) => "inline comment author",
                Some(super::model::DiscussionKind::PullRequestBody) => "PR author",
                Some(super::model::DiscussionKind::IssueBody) => "issue author",
                _ => "comment author",
            };
            return Ok(if document.metadata.actors.is_empty() {
                vec![(
                    role.into(),
                    crate::providers::gh::Actor {
                        actor_type: "Unknown".into(),
                        ..Default::default()
                    },
                )]
            } else {
                document
                    .metadata
                    .actors
                    .iter()
                    .cloned()
                    .map(|actor| (role.into(), actor))
                    .collect()
            });
        }
        if document.source != SourceKind::GitHub {
            return Ok(vec![]);
        }
        let identity = self
            .remote_identity
            .as_ref()
            .ok_or_else(|| QueryError("Search identity has not been verified".into()))?;
        super::remote::verify_scope(&self.scope, identity)?;
        let item = self.remote_item(index)?;
        let mut actors = Vec::new();
        if let Some(actor) = &item.author {
            actors.push(("author".into(), actor.clone()));
        }
        if let Some(actor) = &item.repository_owner {
            actors.push(("repository owner (not file author)".into(), actor.clone()));
        }
        Ok(actors)
    }
    pub fn result_document(&self, index: usize) -> Result<&super::model::Document, QueryError> {
        self.page
            .as_ref()
            .and_then(|page| page.hits.get(index))
            .map(|hit| hit.document.as_ref())
            .ok_or_else(|| QueryError("Selected search result is no longer available".into()))
    }
    /// Prepared result opening must recheck the current bound canonical discussion version.
    /// Local immutable commits are instead reopened by exact original Git object identity.
    pub fn validate_metadata_result(
        &self,
        index: usize,
        cache: &Cache,
        cancellation: &CancellationToken,
    ) -> Result<(), QueryError> {
        let document = self.result_document(index)?;
        if let Some(evidence) = &self.blend_evidence {
            evidence.validate(cache).map_err(QueryError)?;
            if matches!(
                evidence.origins.get(&document.key.stable_id()),
                Some(super::fusion::blend::CandidateOrigin::CurrentCanonical(_))
            ) {
                return evidence
                    .validate_document(cache, document)
                    .map_err(QueryError);
            }
        }
        if document.source != SourceKind::Collaboration {
            return Ok(());
        }
        let context = document
            .metadata
            .discussion
            .as_ref()
            .ok_or_else(|| QueryError("Discussion result lacks typed context".into()))?;
        let target = self
            .metadata_sources
            .discussions
            .iter()
            .find(|target| {
                target.repository_id == context.repository_id
                    && target.parent_node == context.parent_node
                    && target.number == context.parent_number
                    && target.category == context.kind
            })
            .ok_or_else(|| {
                QueryError(
                    "Discussion result no longer belongs to the explicit selected source".into(),
                )
            })?;
        let mut collected = collaboration::collect_cached(
            cache,
            self.scope.clone(),
            std::slice::from_ref(target),
            &self.discussion_limits,
            cancellation,
        )?;
        let current=collected.corpus.documents.iter_mut().find(|candidate|candidate.key.stable_id()==document.key.stable_id())
            .ok_or_else(||QueryError("Discussion result is no longer current/authorized; refresh source before opening".into()))?;
        if let Some(selection) = self
            .selections
            .iter()
            .find(|selection| selection.repository == current.key.repository)
        {
            current.metadata.repository_sets = selection.sets.clone();
        }
        if current.projection_checksum()? != document.projection_checksum()? {
            return Err(QueryError("Discussion body/context changed since search; refresh before opening the exact resource".into()));
        }
        Ok(())
    }

    /// Bounded background avatar prefetch inventory; no transport or drawing occurs here.
    pub fn actors(&self) -> Vec<crate::providers::gh::Actor> {
        crate::actor_avatar_projection_v1::search(self)
            .map(crate::actor_avatar_projection_v1::distinct)
            .unwrap_or_default()
    }

    /// Prepared-only immutable remote viewer, never a fabricated local Git workspace. The gh
    /// provider must have verified the full decoded bytes against the indexed blob first.
    pub fn open_remote_file(
        &mut self,
        index: usize,
        file: &ResolvedRemoteFile,
        content: &[u8],
    ) -> Result<(), QueryError> {
        let candidate = self.remote_item(index)?;
        if candidate.host != file.host
            || candidate.repository != file.repository
            || candidate.path.as_deref() != Some(&file.path)
            || candidate.blob_sha.as_deref() != Some(&file.blob_sha)
            || !super::model::valid_sha(&file.commit_sha)
            || !super::model::valid_sha(&file.blob_sha)
            || file.host != self.scope.host
        {
            return Err(QueryError(
                "Verified remote file does not match the selected indexed blob/path/host".into(),
            ));
        }
        let maximum = self.corpus_limits.max_file_bytes.min(2 * 1024 * 1024);
        if content.len() > maximum {
            return Err(QueryError(format!("Remote file exceeds the {maximum}-byte native viewer budget; no different or partial bytes were substituted")));
        }
        crate::providers::gh::verify_blob_digest(content, &file.blob_sha)
            .map_err(|error| QueryError(error.to_string()))?;
        let path = crate::git::RawPath::new(file.path.as_bytes().to_vec());
        super::model::validate_raw_path(&path)?;
        let mut truncated = false;
        let lines = if content.contains(&0) {
            vec![PreviewLine {
                number: 0,
                selected: false,
                text: format!(
                    "Binary file • {} verified bytes • no text lines invented",
                    content.len()
                ),
            }]
        } else if let Ok(text) = std::str::from_utf8(content) {
            let mut lines = Vec::new();
            for (index, line) in text.lines().enumerate() {
                if index >= 10_000 {
                    truncated = true;
                    break;
                }
                let mut displayed = line.chars().take(2000).collect::<String>();
                if line.chars().count() > 2000 {
                    displayed.push_str(" … [line display truncated]");
                    truncated = true;
                }
                lines.push(PreviewLine {
                    number: index + 1,
                    selected: false,
                    text: displayed,
                });
            }
            if lines.is_empty() {
                lines.push(PreviewLine {
                    number: 0,
                    selected: false,
                    text: "Empty verified file".into(),
                });
            }
            lines
        } else {
            vec![PreviewLine {
                number: 0,
                selected: false,
                text: format!(
                    "Non-UTF-8 file • {} verified bytes • no lossy text locations invented",
                    content.len()
                ),
            }]
        };
        self.preview = Some(SearchPreview {
            freshness: format!(
                "fetched {}",
                crate::git_actor_labels_v1::utc_datetime(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |elapsed| elapsed.as_secs() as i64)
                )
            ),
            title: path.display(),
            context: format!(
                "{} / saved {} / verified blob {} • immutable remote file{}",
                file.repository,
                file.commit_sha,
                file.blob_sha,
                if truncated {
                    " • display truncated to 10,000 lines / 2,000 characters per line"
                } else {
                    ""
                }
            ),
            source: SourceKind::GitHub,
            matched_fields: vec![
                "Exact content verified by gh; original search fragment had no full-file line"
                    .into(),
            ],
            destination: Destination::RemoteFile {
                host: file.host.clone(),
                repository: file.repository.clone(),
                path,
                revision: file.commit_sha.clone(),
            },
            lines,
        });
        Ok(())
    }

    fn prepare_corpus(
        &self,
        input: &str,
        cancellation: &CancellationToken,
    ) -> Result<(Query, Vec<RepositorySelection>, Corpus), QueryError> {
        let (mut query, selections) = self.prepare_selections(input)?;
        if self.selections.is_empty() {
            return Err(QueryError("File-only scanning needs an existing local checkout; cached metadata uses search_cached".into()));
        }
        let corpus = Corpus::collect(
            self.scope.clone(),
            &selections,
            &self.corpus_limits,
            cancellation,
        )?;
        Self::resolve_file_filter(&mut query, &corpus);
        Ok((query, selections, corpus))
    }
    fn prepare_selections(
        &self,
        input: &str,
    ) -> Result<(Query, Vec<RepositorySelection>), QueryError> {
        self.metadata_sources.validate(&self.scope)?;
        let query = Query::parse(input, self.mode)?;
        if self.selections.is_empty() && self.metadata_sources.is_empty() {
            return Err(QueryError(
                "Select an existing local checkout or explicitly managed clone before scanning"
                    .into(),
            ));
        }
        let mut selections = self.selections.clone();
        if let Some(revisions) = query.filters.get("rev") {
            if revisions.len() != 1 {
                return Err(QueryError("Choose one explicit saved revision per search; history indexing is separately opt-in".into()));
            }
            let files_included = query
                .filters
                .get("type")
                .is_none_or(|kinds| kinds.iter().any(|kind| kind == "file"));
            if files_included
                && selections
                    .iter()
                    .any(|selection| selection.source == LocalSource::Worktree)
            {
                return Err(QueryError("A rev filter needs Saved version scope; working files include uncommitted content".into()));
            }
            if files_included {
                for selection in &mut selections {
                    selection.source = LocalSource::Revision(revisions[0].clone());
                }
            } else if query
                .filters
                .get("type")
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "commit"))
                && !super::model::valid_sha(&revisions[0])
            {
                return Err(QueryError("Commit-message rev filters need an exact commit SHA; choose a branch/reference in the opted-in history source".into()));
            }
        }
        Ok((query, selections))
    }
    fn resolve_file_filter(query: &mut Query, corpus: &Corpus) {
        if query.filters.contains_key("rev")
            && query
                .filters
                .get("type")
                .is_none_or(|kinds| kinds.iter().any(|kind| kind == "file"))
        {
            let mut revisions = corpus
                .documents
                .iter()
                .filter(|document| document.key.kind == super::model::ResultKind::File)
                .map(|document| document.key.revision.clone())
                .collect::<Vec<_>>();
            revisions.sort();
            revisions.dedup();
            query.filters.insert("rev".into(), revisions);
        }
    }
    /// File collection limits left after `metadata_bytes`/`metadata_documents` of metadata.
    fn ranked_file_limits(&self, metadata_bytes: usize, metadata_documents: usize) -> CorpusLimits {
        let mut files = self.corpus_limits.clone();
        files.max_total_bytes = files.max_total_bytes.min(
            self.ranked_limits
                .max_total_bytes
                .saturating_sub(metadata_bytes),
        );
        files.max_documents = files.max_documents.min(
            self.ranked_limits
                .max_documents
                .saturating_sub(metadata_documents),
        );
        files
    }
    fn collect_combined(
        &self,
        query: &Query,
        selections: &[RepositorySelection],
        cache: &Cache,
        cancellation: &CancellationToken,
    ) -> Result<(Corpus, BTreeMap<String, String>), QueryError> {
        let mut metadata = Corpus {
            scope: self.scope.clone(),
            documents: vec![],
            coverage: super::model::Coverage {
                complete: true,
                deletion_authoritative: true,
                ..Default::default()
            },
        };
        let mut versions = BTreeMap::new();
        if !self.metadata_sources.discussions.is_empty() {
            let collected = collaboration::collect_cached(
                cache,
                self.scope.clone(),
                &self.metadata_sources.discussions,
                &self.discussion_limits,
                cancellation,
            )?;
            versions.extend(collected.visible_versions);
            metadata = collected.corpus;
        }
        if !self.metadata_sources.commits.is_empty() {
            let mut limits = self.commit_limits.clone();
            limits.max_total_bytes = limits.max_total_bytes.min(
                self.ranked_limits
                    .max_total_bytes
                    .saturating_sub(metadata.coverage.scanned_bytes),
            );
            limits.max_documents = limits.max_documents.min(
                self.ranked_limits
                    .max_documents
                    .saturating_sub(metadata.documents.len()),
            );
            if limits.max_documents > 0 && limits.max_total_bytes > 0 {
                let collected = commits::collect(
                    self.scope.clone(),
                    &self.metadata_sources.commits,
                    &limits,
                    cancellation,
                )?;
                versions.extend(collected.visible_versions);
                merge_corpus(&mut metadata, collected.corpus);
            } else {
                metadata.coverage.complete = false;
                metadata.coverage.deletion_authoritative = false;
                metadata.coverage.reasons.push("Metadata safety budget reached before commit collection; no older messages implicitly admitted".into());
            }
        }
        let files =
            self.ranked_file_limits(metadata.coverage.scanned_bytes, metadata.documents.len());
        let mut corpus = if selections.is_empty() {
            Corpus {
                scope: self.scope.clone(),
                documents: vec![],
                coverage: super::model::Coverage {
                    complete: true,
                    deletion_authoritative: true,
                    ..Default::default()
                },
            }
        } else if files.max_documents > 0 && files.max_total_bytes > 0 {
            Corpus::collect(self.scope.clone(), selections, &files, cancellation)?
        } else {
            Corpus {
                scope: self.scope.clone(),
                documents: vec![],
                coverage: super::model::Coverage {
                    reasons: vec!["Account safety budget reached before file collection".into()],
                    ..Default::default()
                },
            }
        };
        merge_corpus(&mut corpus, metadata);
        // Set membership tags from explicit mapped intent, never repository-name inference.
        for document in &mut corpus.documents {
            if document.source == SourceKind::Collaboration
                || document.key.kind == super::model::ResultKind::Commit
            {
                if let Some(selection) = selections
                    .iter()
                    .find(|selection| selection.repository == document.key.repository)
                {
                    document.metadata.repository_sets = selection.sets.clone();
                }
                versions.insert(document.key.stable_id(), document.projection_checksum()?);
            }
        }
        let _ = query; // Query filters apply uniformly in exact_search/ranked metadata filtering.
        Ok((corpus, versions))
    }

    fn accept_page(
        &mut self,
        input: &str,
        selections: Vec<RepositorySelection>,
        mut page: SearchPage,
    ) {
        if !self.warnings.is_empty() {
            page.coverage.complete = false;
            page.coverage.reasons.extend(self.warnings.clone());
        }
        self.input = input.into();
        self.selections = selections;
        self.page = Some(page);
        self.preview = None;
        self.remote_items.clear();
        self.blend_evidence = None;
    }
    pub fn preview(&mut self, index: usize) -> Result<(), QueryError> {
        let hit = self
            .page
            .as_ref()
            .and_then(|page| page.hits.get(index))
            .ok_or_else(|| QueryError("Selected search result is no longer available".into()))?;
        let first = hit.line.saturating_sub(9);
        let fragment_only = hit.document.source == SourceKind::GitHub;
        self.preview = Some(SearchPreview {
            freshness: hit.document.freshness(),
            title: hit.document.title.clone(),
            context: if fragment_only {
                format!(
                    "{} / {} / provider fragment • full-file location unknown / {}{}",
                    hit.document.key.repository,
                    hit.document.key.revision,
                    hit.document.source.label(),
                    self.remote_items.get(&hit.document.key.stable_id()).map_or(
                        String::new(),
                        |item| {
                            let mut roles = Vec::new();
                            if let Some(actor) = &item.author {
                                roles.push(format!("author {}", actor.label()));
                            }
                            if let Some(actor) = &item.repository_owner {
                                roles.push(format!(
                                    "repository owner {} (not file author)",
                                    actor.label()
                                ));
                            }
                            if roles.is_empty() {
                                String::new()
                            } else {
                                format!(" / {}", roles.join(" / "))
                            }
                        }
                    )
                )
            } else if let Some(context) = metadata_preview_context(hit) {
                context
            } else {
                format!(
                    "{} / {} / {}:{} / {}{}",
                    hit.document.key.repository,
                    hit.document.key.revision,
                    hit.line,
                    hit.column,
                    hit.document.source.label(),
                    if hit.document.metadata.history_overrides.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " / overrides: {}",
                            hit.document.metadata.history_overrides.join(", ")
                        )
                    }
                )
            },
            source: hit.document.source,
            matched_fields: hit.matched_fields.clone(),
            destination: hit.destination(),
            lines: hit
                .document
                .body
                .lines()
                .enumerate()
                .skip(first)
                .take(120)
                .map(|(index, line)| PreviewLine {
                    number: if fragment_only { 0 } else { index + 1 },
                    text: line.chars().take(2000).collect(),
                    selected: !fragment_only && index + 1 == hit.line,
                })
                .collect(),
        });
        self.extend_remote_preview(index);
        Ok(())
    }
    /// GitHub issue/PR/repository fragments omit the item's identity and full
    /// description; show both so a discovery result is readable in place.
    fn extend_remote_preview(&mut self, index: usize) {
        let Some(item) = self
            .page
            .as_ref()
            .and_then(|page| page.hits.get(index))
            .filter(|hit| hit.document.source == SourceKind::GitHub)
            .and_then(|hit| self.remote_items.get(&hit.document.key.stable_id()))
            .filter(|item| item.kind != RemoteSearchKind::Code)
        else {
            return;
        };
        let line = |text: String| PreviewLine {
            number: 0,
            text: text.chars().take(2000).collect(),
            selected: false,
        };
        let mut header = vec![
            line(format!(
                "{}{}{}",
                item.repository,
                item.number
                    .map_or(String::new(), |number| format!("#{number}")),
                if item.state.is_empty() {
                    String::new()
                } else {
                    format!(" • {}", item.state)
                }
            )),
            line(item.url.clone()),
        ];
        let Some(preview) = &mut self.preview else {
            return;
        };
        let mut lines = std::mem::take(&mut preview.lines);
        if !item.text_matches.is_empty() && !item.body.trim().is_empty() {
            lines.push(line(String::new()));
            lines.push(line("Description".into()));
            lines.extend(item.body.lines().take(200).map(|text| line(text.into())));
        }
        header.append(&mut lines);
        preview.lines = header;
    }
    /// Issue and repository results have no native ghtui viewer. Open keeps the
    /// readable provider preview and names the canonical URL instead of failing.
    pub fn open_remote_overview(&mut self, index: usize) -> Result<Option<String>, QueryError> {
        if !matches!(
            self.destination(index)?,
            Destination::Issue { .. } | Destination::Repository { .. }
        ) {
            return Ok(None);
        }
        let item = self.remote_item(index)?.clone();
        self.preview(index)?;
        let label = match item.number {
            Some(number) => format!("{}#{number}", item.repository),
            None => item.repository.clone(),
        };
        Ok(Some(format!(
            "Showing {label} in the preview • no in-app issue/repository viewer; open {} in a browser",
            item.url
        )))
    }
    pub fn destination(&self, index: usize) -> Result<Destination, QueryError> {
        self.page
            .as_ref()
            .and_then(|page| page.hits.get(index))
            .map(|hit| hit.destination())
            .ok_or_else(|| QueryError("Selected search result is no longer available".into()))
    }
    pub fn save(&mut self, cache: &Cache, name: &str, pinned: bool) -> Result<(), QueryError> {
        if name.trim().is_empty() || name.len() > 128 {
            return Err(QueryError("Saved search name needs 1–128 bytes".into()));
        }
        let revision = if self.mode == SearchMode::GitHub {
            None
        } else {
            self.selections
                .first()
                .and_then(|selection| match &selection.source {
                    LocalSource::Revision(revision) => Some(revision.clone()),
                    LocalSource::Worktree => None,
                })
        };
        let saved = SavedSearch {
            version: 1,
            name: name.into(),
            scope: self.scope.clone(),
            input: self.input.clone(),
            mode: self.mode,
            repositories: if self.mode == SearchMode::GitHub {
                self.remote_repositories.clone()
            } else {
                self.selections
                    .iter()
                    .map(|selection| selection.repository.clone())
                    .collect()
            },
            revision,
            sources: if self.mode == SearchMode::GitHub {
                BTreeMap::new()
            } else {
                self.selections
                    .iter()
                    .map(|selection| (selection.repository.clone(), selection.source.clone()))
                    .collect()
            },
            // GitHub mode has its own explicit remote repository scope. Inactive cached
            // metadata must not extend that endpoint's repositories on saved restoration.
            metadata_sources: if self.mode == SearchMode::GitHub {
                MetadataSources::default()
            } else {
                self.metadata_sources.clone()
            },
            include_hidden: self.corpus_limits.include_hidden,
            include_ignored: self.corpus_limits.include_ignored,
            max_file_bytes: self.corpus_limits.max_file_bytes,
            pinned,
            sort: "relevance".into(),
        };
        let mut saved = saved;
        saved
            .repositories
            .extend(saved.metadata_sources.repositories());
        saved.repositories.sort();
        saved.repositories.dedup();
        saved.validate(&self.scope)?;
        cache
            .save_view(
                &self.scope,
                &format!("search:{name}"),
                &serde_json::to_value(&saved).map_err(|error| QueryError(error.to_string()))?,
            )
            .map_err(|error| QueryError(error.to_string()))?;
        self.load_saved(cache)
    }
    fn saved_named(&self, name: &str) -> Result<SavedSearch, QueryError> {
        self.saved
            .iter()
            .find(|saved| saved.name == name)
            .cloned()
            .ok_or_else(|| {
                QueryError("Saved search was removed or belongs to another account".into())
            })
    }
    fn write_saved(&self, cache: &Cache, saved: &SavedSearch) -> Result<(), QueryError> {
        saved.validate(&self.scope)?;
        cache
            .save_view(
                &self.scope,
                &format!("search:{}", saved.name),
                &serde_json::to_value(saved).map_err(|error| QueryError(error.to_string()))?,
            )
            .map_err(|error| QueryError(error.to_string()))
    }
    /// Pin or unpin one saved search; returns the new pin state. Only `pinned` changes.
    pub fn toggle_saved_pin(&mut self, cache: &Cache, name: &str) -> Result<bool, QueryError> {
        let mut saved = self.saved_named(name)?;
        saved.pinned = !saved.pinned;
        self.write_saved(cache, &saved)?;
        self.load_saved(cache)?;
        Ok(saved.pinned)
    }
    /// Rename one saved search, keeping its query, mode, scope, sort and pin unchanged.
    pub fn rename_saved(&mut self, cache: &Cache, from: &str, to: &str) -> Result<(), QueryError> {
        let to = to.trim();
        if to.is_empty() || to.len() > 128 {
            return Err(QueryError("Saved search name needs 1–128 bytes".into()));
        }
        if to == from {
            return Err(QueryError(format!(
                "Saved query is already named {to} • edit the Name field first"
            )));
        }
        if self.saved.iter().any(|saved| saved.name == to) {
            return Err(QueryError(format!(
                "A saved query named {to} already exists • nothing renamed"
            )));
        }
        let mut saved = self.saved_named(from)?;
        saved.name = to.into();
        self.write_saved(cache, &saved)?;
        cache
            .remove_saved_view(&self.scope, &format!("search:{from}"))
            .map_err(|error| QueryError(error.to_string()))?;
        self.load_saved(cache)
    }
    /// Delete one saved search. Other saved views and caches are untouched.
    pub fn delete_saved(&mut self, cache: &Cache, name: &str) -> Result<(), QueryError> {
        self.saved_named(name)?;
        cache
            .remove_saved_view(&self.scope, &format!("search:{name}"))
            .map_err(|error| QueryError(error.to_string()))?;
        self.load_saved(cache)
    }
    pub fn load_saved(&mut self, cache: &Cache) -> Result<(), QueryError> {
        self.saved.clear();
        for (name, value) in cache
            .saved_views(&self.scope)
            .map_err(|error| QueryError(error.to_string()))?
        {
            if !name.starts_with("search:") {
                continue;
            }
            let saved: SavedSearch = serde_json::from_value(value)
                .map_err(|error| QueryError(format!("Saved query {name} cannot load: {error}")))?;
            saved.validate(&self.scope)?;
            self.saved.push(saved);
        }
        self.saved
            .sort_by(|a, b| (!a.pinned, &a.name).cmp(&(!b.pinned, &b.name)));
        Ok(())
    }
    pub fn restore_saved(&mut self, name: &str) -> Result<(), QueryError> {
        let saved = self
            .saved
            .iter()
            .find(|saved| saved.name == name)
            .ok_or_else(|| {
                QueryError("Saved search was removed or belongs to another account".into())
            })?
            .clone();
        saved.validate(&self.scope)?;
        self.warnings.clear();
        if saved.mode == SearchMode::GitHub {
            self.remote_repositories = saved.repositories.clone();
            self.remote_items.clear();
        }
        for repository in &saved.repositories {
            if saved.mode == SearchMode::GitHub {
                continue;
            }
            if !saved.metadata_sources.repositories().contains(repository)
                && !self
                    .available_selections
                    .iter()
                    .any(|selection| &selection.repository == repository)
            {
                self.warnings.push(format!("Saved repository {repository} has no available local mapping; scope was not broadened"));
            }
        }
        if saved.mode != SearchMode::GitHub {
            self.selections = self
                .available_selections
                .iter()
                .filter(|selection| saved.repositories.contains(&selection.repository))
                .cloned()
                .collect();
        }
        for selection in &mut self.selections {
            selection.source = saved
                .sources
                .get(&selection.repository)
                .cloned()
                .unwrap_or_else(|| {
                    saved
                        .revision
                        .as_ref()
                        .map_or(LocalSource::Worktree, |revision| {
                            LocalSource::Revision(revision.clone())
                        })
                });
        }
        self.input = saved.input;
        self.metadata_sources = saved.metadata_sources;
        self.mode = saved.mode;
        self.corpus_limits.include_hidden = saved.include_hidden;
        self.corpus_limits.include_ignored = saved.include_ignored;
        self.corpus_limits.max_file_bytes = saved.max_file_bytes;
        self.page = None;
        self.preview = None;
        Ok(())
    }
}
fn metadata_preview_context(hit: &super::model::SearchHit) -> Option<String> {
    if let Some(context) = &hit.document.metadata.discussion {
        return Some(format!(
            "{} #{} / {} {}{}{} / body {}:{} / fetched {} • {}",
            hit.document.key.repository,
            context.parent_number,
            context.kind.label(),
            context.resource_node,
            context
                .thread_id
                .as_ref()
                .map_or(String::new(), |thread| format!(" / thread {thread}")),
            context
                .commit_sha
                .as_ref()
                .map_or(String::new(), |sha| format!(
                    " / original commit {sha}{}",
                    if context.outdated {
                        " • outdated anchor"
                    } else {
                        ""
                    }
                )),
            hit.line,
            hit.column,
            hit.document.metadata.fetched_at,
            if hit.document.metadata.complete {
                "complete"
            } else {
                "cached partial"
            }
        ));
    }
    hit.document.metadata.git_author.as_ref().map(|author| {
        format!(
            "{} / {} / original saved message • Git author {} <{}>{}",
            hit.document.key.repository,
            hit.document.key.revision,
            author.name,
            author.email,
            hit.document.metadata.git_committer.as_ref().map_or(
                String::new(),
                |committer| format!(
                    " / Git committer {} <{}> • no verified GitHub association",
                    committer.name, committer.email
                )
            )
        )
    })
}
