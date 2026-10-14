//! Explicit read-only search hydration. No automatic org enumeration, clone or publication.
use crate::{
    editor::TextBuffer,
    providers::{
        gh::{GithubProvider, HostIdentity, HydrationLimits},
        runner::CancellationToken,
    },
    runtime::PreparedView,
    search::{
        collaboration, commits, hydration, model::DiscussionKind, sources::MetadataSources,
        workspace::SearchWorkspace,
    },
    storage::{Cache, Scope},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub mod context;
mod read_guard;
pub mod ui;
pub mod visibility;

const SAVED_SOURCES: &str = "search-source-selection:v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    PullRequest,
    Issue,
    Commits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceAction {
    Open,
    Kind(SourceKind),
    Field(usize),
    Load,
    Remove,
    Close,
    ContextScroll(i32),
    ContextClose,
}

#[derive(Debug, Clone)]
pub struct SourceControls {
    pub kind: SourceKind,
    /// Repository, number/revision, document/commit count, total bytes, API pages.
    pub fields: [TextBuffer; 5],
    pub editing: Option<usize>,
    pub field_rects: Vec<(usize, ratatui::layout::Rect)>,
    pub message: String,
}
impl Default for SourceControls {
    fn default() -> Self {
        Self {
            kind: SourceKind::PullRequest,
            fields: ["", "1", "1000", "33554432", "20"].map(|text| TextBuffer::new(text).unwrap()),
            editing: None,
            field_rects: vec![],
            message: "Load reads gh/Git and writes only the local cache. Nothing is published."
                .into(),
        }
    }
}
impl SourceControls {
    pub fn choose_kind(&mut self, kind: SourceKind, search: Option<&SearchWorkspace>) {
        if self.kind != kind {
            self.fields[1] = TextBuffer::new(if kind == SourceKind::Commits {
                "HEAD"
            } else {
                "1"
            })
            .unwrap();
        }
        self.kind = kind;
        self.editing = None;
        if let Some(search) = search {
            let repository = if kind == SourceKind::Commits {
                search
                    .available_selections
                    .first()
                    .map(|selected| selected.repository.as_str())
            } else {
                search
                    .remote_repositories
                    .first()
                    .map(String::as_str)
                    .or_else(|| {
                        search
                            .metadata_sources
                            .discussions
                            .first()
                            .map(|target| target.repository.as_str())
                    })
                    .or_else(|| {
                        search
                            .available_selections
                            .first()
                            .map(|selected| selected.repository.as_str())
                    })
            };
            if let Some(repository) = repository {
                self.fields[0] = TextBuffer::new(repository).unwrap();
            }
        }
    }
    pub fn request(&self) -> Result<SourceRequest, String> {
        let repository = self.fields[0].text.trim().to_owned();
        crate::config::validate_repository(&repository).map_err(|error| error.to_string())?;
        let context = self.fields[1].text.trim().to_owned();
        if context.is_empty() || context.len() > 1024 || context.chars().any(char::is_control) {
            return Err("Choose an explicit PR/issue number or saved Git revision".into());
        }
        if self.kind != SourceKind::Commits {
            positive_number(&context, "PR/issue number", u64::MAX)?;
        }
        Ok(SourceRequest {
            kind: self.kind,
            repository,
            context,
            max_documents: positive_number(&self.fields[2].text, "document/commit count", 9999)?
                as usize,
            max_bytes: positive_number(&self.fields[3].text, "total bytes", 32 * 1024 * 1024)?
                as usize,
            max_pages: if self.kind == SourceKind::Commits {
                1
            } else {
                positive_number(&self.fields[4].text, "API pages", 100)? as usize
            },
        })
    }
}
fn positive_number(text: &str, label: &str, max: u64) -> Result<u64, String> {
    text.trim()
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0 && *value <= max)
        .ok_or_else(|| format!("{label} must be a positive integer, at most {max}"))
}
#[derive(Debug, Clone)]
pub struct SourceRequest {
    pub kind: SourceKind,
    pub repository: String,
    pub context: String,
    pub max_documents: usize,
    pub max_bytes: usize,
    pub max_pages: usize,
}
#[derive(Serialize, Deserialize)]
struct PersistedSources {
    sources: MetadataSources,
    max_documents: usize,
    max_bytes: usize,
}

pub fn restore(cache: &Cache, search: &mut SearchWorkspace) -> Result<(), String> {
    let Some(value) = cache
        .saved_view(&search.scope, SAVED_SOURCES)
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    let saved: PersistedSources = serde_json::from_value(value)
        .map_err(|error| format!("Saved search sources are invalid: {error}"))?;
    saved
        .sources
        .validate(&search.scope)
        .map_err(|error| error.to_string())?;
    if !(1..=9999).contains(&saved.max_documents)
        || !(1..=32 * 1024 * 1024).contains(&saved.max_bytes)
    {
        return Err("Saved source budgets are invalid; no broader scope was loaded".into());
    }
    search.metadata_sources = saved.sources;
    set_limits(search, saved.max_documents, saved.max_bytes);
    Ok(())
}
fn persist(cache: &Cache, search: &SearchWorkspace) -> Result<(), String> {
    search
        .metadata_sources
        .validate(&search.scope)
        .map_err(|error| error.to_string())?;
    let saved = PersistedSources {
        sources: search.metadata_sources.clone(),
        max_documents: search.discussion_limits.max_documents,
        max_bytes: search.discussion_limits.max_total_bytes,
    };
    cache
        .save_view(
            &search.scope,
            SAVED_SOURCES,
            &serde_json::to_value(saved).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
}
fn set_limits(search: &mut SearchWorkspace, documents: usize, bytes: usize) {
    search.discussion_limits.max_documents = documents;
    search.discussion_limits.max_total_bytes = bytes;
    search.commit_limits.max_documents = documents;
    search.commit_limits.max_total_bytes = bytes;
}
pub fn verify_identity(
    search: &SearchWorkspace,
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> Result<HostIdentity, String> {
    let identity = provider
        .identity(cancel)
        .map_err(|error| format!("Account identity unavailable: {error}"))?;
    let scope = Scope::github(&identity.host, identity.account_id)
        .map_err(|error| format!("Account identity unavailable: {error}"))?;
    if scope != search.scope
        || search
            .remote_identity
            .as_ref()
            .is_none_or(|expected| !expected.same_account(&identity))
    {
        return Err(
            "Account identity changed: private search sources hidden; no prior source is unlocked"
                .into(),
        );
    }
    Ok(identity)
}

/// All I/O lives on the worker. A deliberate selection does not authorize remote mutations.
pub fn load(
    mut search: SearchWorkspace,
    request: SourceRequest,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let mut cache = Cache::open(database).map_err(|error| error.to_string())?;
    set_limits(&mut search, request.max_documents, request.max_bytes);
    let message;
    if request.kind == SourceKind::Commits {
        let selected = search
            .available_selections
            .iter()
            .find(|selected| selected.repository == request.repository)
            .ok_or("Choose an explicitly mapped local repository; no clone or fetch was made")?;
        if !search.metadata_sources.discussions.is_empty() {
            verify_identity(&search, provider, cancel)?;
        }
        let selection = commits::CommitSelection {
            repository: selected.repository.clone(),
            directory: selected.directory.clone(),
            revision: request.context,
            max_commits: request.max_documents,
        };
        let collected = commits::collect(
            search.scope.clone(),
            std::slice::from_ref(&selection),
            &search.commit_limits,
            cancel,
        )
        .map_err(|error| error.to_string())?;
        if cancel.is_cancelled() {
            return Err("Commit source cancelled; no selection saved".into());
        }
        if collected.corpus.documents.is_empty()
            && !collected.corpus.coverage.unavailable.is_empty()
        {
            return Err(collected.corpus.coverage.reasons.join(" • "));
        }
        search
            .metadata_sources
            .commits
            .retain(|previous| previous.repository != selection.repository);
        search.metadata_sources.commits.push(selection);
        message = format!(
            "Loaded {} original commit messages • {} • {}",
            collected.corpus.documents.len(),
            if collected.corpus.coverage.complete {
                "available selected history complete"
            } else {
                "partial selected history"
            },
            collected.corpus.coverage.reasons.join(" • ")
        );
        if !search.metadata_sources.discussions.is_empty() {
            verify_identity(&search, provider, cancel)?;
        }
    } else {
        let mut read_proofs = Vec::new();
        let identity = provider
            .identity(cancel)
            .map_err(|error| format!("Account identity unavailable: {error}"))?;
        let scope = Scope::github(&identity.host, identity.account_id)
            .map_err(|error| format!("Account identity unavailable: {error}"))?;
        if search
            .remote_identity
            .as_ref()
            .is_some_and(|previous| !previous.same_account(&identity))
            || search.scope.host != "local" && search.scope != scope
        {
            return Err(
                "Account identity changed: existing private selections are not reusable".into(),
            );
        }
        if search.scope != scope {
            let commits = search.metadata_sources.commits.clone();
            search.scope = scope.clone();
            search.remote_identity = Some(identity.clone());
            search.saved.clear();
            search.page = None;
            search.preview = None;
            search.remote_items.clear();
            search.remote_repositories.clear();
            restore(&cache, &mut search)?;
            for selected in commits {
                if !search
                    .metadata_sources
                    .commits
                    .iter()
                    .any(|item| item.repository == selected.repository)
                {
                    search.metadata_sources.commits.push(selected);
                }
            }
            set_limits(&mut search, request.max_documents, request.max_bytes);
        }
        search.remote_identity = Some(identity.clone());
        let number = positive_number(&request.context, "PR/issue number", u64::MAX)?;
        let kind = if request.kind == SourceKind::Issue {
            collaboration::ParentKind::Issue
        } else {
            collaboration::ParentKind::PullRequest
        };
        let mut known_targets = search
            .metadata_sources
            .discussions
            .iter()
            .filter(|target| {
                target.repository.eq_ignore_ascii_case(&request.repository)
                    && target.number == number
                    && target.parent_kind == kind
            })
            .cloned()
            .collect::<Vec<_>>();
        let canonical_repository = known_targets
            .first()
            .map_or(request.repository.as_str(), |target| {
                target.repository.as_str()
            })
            .to_owned();
        let repository = read_guard::read(
            provider.repository(&request.repository, cancel),
            &mut cache,
            &identity,
            provider,
            &canonical_repository,
            &known_targets,
            cancel,
        )?;
        if repository.id == 0
            || repository.host != scope.host
            || !repository
                .full_name
                .eq_ignore_ascii_case(&request.repository)
        {
            return Err("Hydration repository identity changed; no source published".into());
        }
        let limits = HydrationLimits {
            max_documents: request.max_documents,
            max_total_bytes: request.max_bytes,
            max_pages: request.max_pages,
            ..Default::default()
        };
        let fetched_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_secs();
        let (targets, snapshots) = if request.kind == SourceKind::PullRequest {
            let pr = read_guard::read(
                provider.pr_summary(&repository.full_name, number, cancel),
                &mut cache,
                &identity,
                provider,
                &repository.full_name,
                &known_targets,
                cancel,
            )?;
            let targets = hydration::pr_targets(scope.clone(), &repository, &pr)
                .map_err(|error| error.to_string())?;
            read_proofs.extend(visibility::before_targets(database, &scope, &targets)?);
            let replaced = known_targets
                .iter()
                .filter(|old| old.repository_id != repository.id || old.parent_node != pr.pr.id)
                .cloned()
                .collect::<Vec<_>>();
            read_guard::invalidate(&mut cache, &replaced)?;
            known_targets.extend(targets.iter().cloned());
            let reserved = reserve_all(&mut cache, &targets)?;
            let comments = read_guard::read(
                provider.pr_discussion_comments(&identity, &repository, &pr, &limits, cancel),
                &mut cache,
                &identity,
                provider,
                &repository.full_name,
                &known_targets,
                cancel,
            )?;
            let reviews = read_guard::read(
                provider.pr_discussion_reviews(&identity, &repository, &pr, &limits, cancel),
                &mut cache,
                &identity,
                provider,
                &repository.full_name,
                &known_targets,
                cancel,
            )?;
            let threads = read_guard::read(
                provider.discussion_threads(&identity, &repository, &pr, &limits, cancel),
                &mut cache,
                &identity,
                provider,
                &repository.full_name,
                &known_targets,
                cancel,
            )?;
            let mut snapshots = hydration::from_records(
                scope.clone(),
                &repository,
                &comments,
                DiscussionKind::ConversationComment,
                fetched_at,
            )
            .map_err(|error| error.to_string())?;
            snapshots.extend(
                hydration::from_records(
                    scope.clone(),
                    &repository,
                    &reviews,
                    DiscussionKind::Review,
                    fetched_at,
                )
                .map_err(|error| error.to_string())?,
            );
            snapshots.extend(
                hydration::from_threads(scope.clone(), &repository, &threads, fetched_at)
                    .map_err(|error| error.to_string())?,
            );
            (
                reserved,
                hydration::coalesce(snapshots).map_err(|error| error.to_string())?,
            )
        } else {
            let parent = read_guard::read(
                provider.issue_parent(&identity, &repository, number, cancel),
                &mut cache,
                &identity,
                provider,
                &repository.full_name,
                &known_targets,
                cancel,
            )?;
            let targets = hydration::parent_targets(scope.clone(), &repository, &parent)
                .map_err(|error| error.to_string())?;
            read_proofs.extend(visibility::before_targets(database, &scope, &targets)?);
            let replaced = known_targets
                .iter()
                .filter(|old| {
                    old.repository_id != repository.id || old.parent_node != parent.node_id
                })
                .cloned()
                .collect::<Vec<_>>();
            read_guard::invalidate(&mut cache, &replaced)?;
            known_targets.extend(targets.iter().cloned());
            let reserved = reserve_all(&mut cache, &targets)?;
            let result = read_guard::read(
                provider.issue_details(&identity, &repository, &parent, &limits, cancel),
                &mut cache,
                &identity,
                provider,
                &repository.full_name,
                &known_targets,
                cancel,
            )?;
            (
                reserved,
                hydration::from_records(
                    scope.clone(),
                    &repository,
                    &result,
                    DiscussionKind::ConversationComment,
                    fetched_at,
                )
                .map_err(|error| error.to_string())?,
            )
        };
        verify_identity(&search, provider, cancel)?;
        visibility::after(database, &read_proofs)?;
        for snapshot in &snapshots {
            let generation = *targets
                .get(&snapshot.target.source())
                .ok_or("Unexpected canonical hydration category")?;
            if !collaboration::publish(
                &mut cache,
                snapshot,
                generation,
                &search.discussion_limits,
                cancel,
            )
            .map_err(|error| error.to_string())?
            {
                return Err(
                    "Hydration was superseded or cache cleared; old reads were not published"
                        .into(),
                );
            }
        }
        // Stable node/category keys replace only this exact parent, never other selected sources.
        search.metadata_sources.discussions.retain(|previous| {
            !(previous
                .repository
                .eq_ignore_ascii_case(&repository.full_name)
                && previous.number == number
                && previous.parent_kind
                    == if request.kind == SourceKind::Issue {
                        collaboration::ParentKind::Issue
                    } else {
                        collaboration::ParentKind::PullRequest
                    })
        });
        search
            .metadata_sources
            .discussions
            .extend(snapshots.iter().map(|snapshot| snapshot.target.clone()));
        let collected = collaboration::collect_cached(
            &cache,
            search.scope.clone(),
            &search.metadata_sources.discussions,
            &search.discussion_limits,
            cancel,
        )
        .map_err(|error| error.to_string())?;
        message = format!(
            "Loaded {} cached discussion records • {} • {}",
            collected.corpus.documents.len(),
            if collected.corpus.coverage.complete {
                "selected sources complete"
            } else {
                "partial selected sources"
            },
            collected.corpus.coverage.reasons.join(" • ")
        );
        verify_identity(&search, provider, cancel)?;
        visibility::after(database, &read_proofs)?;
    }
    persist(&cache, &search)?;
    search.page = None;
    search.preview = None;
    search.remote_items.clear();
    search
        .load_saved(&cache)
        .map_err(|error| error.to_string())?;
    let identity = search
        .remote_identity
        .clone()
        .filter(|_| !search.metadata_sources.discussions.is_empty());
    Ok(PreparedView {
        context: Some(search.context()),
        status: "Search source loaded • run Exact, Regex or Ranked; cached subset labelled".into(),
        source_message: Some(message),
        search: Some(search),
        verified_identity: identity,
        ..Default::default()
    })
}
fn reserve_all(
    cache: &mut Cache,
    targets: &[collaboration::DiscussionTarget],
) -> Result<BTreeMap<String, u64>, String> {
    targets
        .iter()
        .map(|target| {
            Ok((
                target.source(),
                collaboration::reserve(cache, target).map_err(|error| error.to_string())?,
            ))
        })
        .collect()
}

pub fn remove(
    mut search: SearchWorkspace,
    request: SourceRequest,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let mut cache = Cache::open(database).map_err(|error| error.to_string())?;
    if !search.metadata_sources.discussions.is_empty() {
        verify_identity(&search, provider, cancel)?;
    }
    let before = search.metadata_sources.discussions.len() + search.metadata_sources.commits.len();
    if request.kind == SourceKind::Commits {
        search
            .metadata_sources
            .commits
            .retain(|target| target.repository != request.repository);
    } else {
        let number = positive_number(&request.context, "PR/issue number", u64::MAX)?;
        let kind = if request.kind == SourceKind::Issue {
            collaboration::ParentKind::Issue
        } else {
            collaboration::ParentKind::PullRequest
        };
        let mut retained = Vec::new();
        for target in search.metadata_sources.discussions.drain(..) {
            if target.repository.eq_ignore_ascii_case(&request.repository)
                && target.number == number
                && target.parent_kind == kind
            {
                // Fence a concurrent older fetch even when no new source body is published.
                collaboration::invalidate(&mut cache, &target)
                    .map_err(|error| error.to_string())?;
            } else {
                retained.push(target);
            }
        }
        search.metadata_sources.discussions = retained;
    }
    let removed =
        before - search.metadata_sources.discussions.len() - search.metadata_sources.commits.len();
    persist(&cache, &search)?;
    search.page = None;
    search.preview = None;
    search.remote_items.clear();
    Ok(PreparedView {context:Some(search.context()),status:"Search source selection updated • Git, drafts and cached bodies preserved".into(),
        source_message:Some(format!("Removed {removed} selected sources; old results hidden. Saved queries retain their own explicit scope.")),search:Some(search),..Default::default()})
}

#[cfg(test)]
mod tests;
