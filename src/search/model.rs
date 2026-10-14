//! Canonical search identities and prepared results; visible labels never replace raw paths.
use super::{Query, QueryError, SearchMode};
use crate::{config::validate_repository, git::RawPath, providers::gh::Actor, storage::Scope};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::{io, ops::Range, path::PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultKind {
    File,
    Commit,
    PullRequest,
    Issue,
    Comment,
    Run,
    Workflow,
    Repository,
}
impl ResultKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Commit => "commit",
            Self::PullRequest => "pr",
            Self::Issue => "issue",
            Self::Comment => "comment",
            Self::Run => "run",
            Self::Workflow => "workflow",
            Self::Repository => "repository",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Worktree,
    SavedCommit,
    Collaboration,
    GitHub,
}
impl SourceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Worktree => "local working files (including uncommitted content)",
            Self::SavedCommit => "local saved version (original immutable Git objects)",
            Self::Collaboration => "hydrated collaboration records",
            Self::GitHub => "GitHub remote search (provider limits apply)",
        }
    }
}

/// Account boundaries, revision and exact raw path/resource identity participate in dedup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentKey {
    pub scope: Scope,
    pub repository: String,
    pub revision: String,
    pub kind: ResultKind,
    pub path: Option<RawPath>,
    pub resource: String,
}
impl DocumentKey {
    pub fn stable_id(&self) -> String {
        // JSON keeps fields and arbitrary path bytes unambiguous, unlike joined display labels.
        let bytes = serde_json::to_vec(self).expect("DocumentKey contains serializable scalars");
        format!("{:x}", Sha256::digest(bytes))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Destination {
    LocalFile {
        directory: PathBuf,
        path: RawPath,
        /// None means mutable working files. Some is the exact immutable commit SHA.
        revision: Option<String>,
        line: usize,
        column: usize,
    },
    LocalCommit {
        directory: PathBuf,
        commit_id: String,
    },
    PullRequest {
        host: String,
        repository: String,
        number: u64,
        comment_id: Option<String>,
        thread_id: Option<String>,
    },
    Issue {
        host: String,
        repository: String,
        number: u64,
        comment_id: Option<String>,
    },
    Run {
        host: String,
        repository: String,
        id: u64,
        attempt: u64,
        job_id: Option<u64>,
    },
    Workflow {
        host: String,
        repository: String,
        id: u64,
    },
    Repository {
        host: String,
        repository: String,
    },
    RemoteFile {
        host: String,
        repository: String,
        path: RawPath,
        revision: String,
    },
    /// A legacy indexed blob match is not yet an immutable commit/file destination.
    RemoteCodeCandidate {
        host: String,
        repository: String,
        path: RawPath,
        blob_sha: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Metadata {
    pub language: Option<String>,
    pub actors: Vec<Actor>,
    pub labels: Vec<String>,
    pub state: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub fetched_at: u64,
    pub repository_sets: Vec<String>,
    pub complete: bool,
    pub history_overrides: Vec<String>,
    pub git_author: Option<crate::git::GitActor>,
    pub git_committer: Option<crate::git::GitActor>,
    pub discussion: Option<DiscussionContext>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscussionKind {
    PullRequestBody,
    IssueBody,
    ConversationComment,
    Review,
    InlineComment,
}
impl DiscussionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::PullRequestBody => "PR body",
            Self::IssueBody => "issue body",
            Self::ConversationComment => "conversation comment",
            Self::Review => "review",
            Self::InlineComment => "inline review comment",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscussionContext {
    pub kind: DiscussionKind,
    pub repository_id: u64,
    pub parent_node: String,
    pub parent_number: u64,
    pub resource_node: String,
    pub resource_numeric_id: Option<u64>,
    pub thread_id: Option<String>,
    pub path: Option<RawPath>,
    pub commit_sha: Option<String>,
    pub side: Option<String>,
    pub line: Option<u64>,
    pub start_line: Option<u64>,
    pub outdated: bool,
    pub resolved: Option<bool>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub key: DocumentKey,
    pub source: SourceKind,
    pub title: String,
    pub body: String,
    pub metadata: Metadata,
    pub destination: Destination,
}

/// Feed canonical serialization directly to the digest without allocating a second
/// complete copy of a possibly multi-megabyte document body.
struct ProjectionDigest(Sha256);
impl io::Write for ProjectionDigest {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Document {
    /// Current full canonical projection: an edited body with the same resource ID is different.
    pub fn projection_checksum(&self) -> Result<String, QueryError> {
        let mut digest = ProjectionDigest(Sha256::new());
        serde_json::to_writer(&mut digest, self).map_err(|error| {
            QueryError(format!("Canonical document cannot be serialized: {error}"))
        })?;
        Ok(format!("{:x}", digest.0.finalize()))
    }
    pub fn validate(&self, scope: &Scope) -> Result<(), QueryError> {
        Scope::new(scope.host.clone(), scope.login.clone())
            .map_err(|error| QueryError(error.to_string()))?;
        if &self.key.scope != scope {
            return Err(QueryError(
                "Search document belongs to a different host/account".into(),
            ));
        }
        validate_repository(&self.key.repository).map_err(|error| QueryError(error.to_string()))?;
        if self.key.resource.is_empty() {
            return Err(QueryError(
                "Search document needs a canonical resource identity".into(),
            ));
        }
        if self.source == SourceKind::SavedCommit && !valid_sha(&self.key.revision) {
            return Err(QueryError(
                "Saved-version search requires an immutable commit identity".into(),
            ));
        }
        match &self.destination {
            Destination::LocalFile {
                path,
                revision,
                line,
                column,
                ..
            } => {
                if self.key.kind != ResultKind::File
                    || self.key.path.as_ref() != Some(path)
                    || *line == 0
                    || *column == 0
                    || revision
                        .as_ref()
                        .is_some_and(|id| !valid_sha(id) || id != &self.key.revision)
                    || (self.source == SourceKind::SavedCommit && revision.is_none())
                    || (self.source == SourceKind::Worktree
                        && (revision.is_some() || self.key.revision != "working"))
                {
                    return Err(QueryError(
                        "Local file destination does not match its canonical document".into(),
                    ));
                }
                validate_raw_path(path)?;
            }
            Destination::LocalCommit { commit_id, .. } => {
                if self.key.kind != ResultKind::Commit
                    || !valid_sha(commit_id)
                    || commit_id != &self.key.revision
                {
                    return Err(QueryError(
                        "Commit destination does not match the searched saved version".into(),
                    ));
                }
            }
            Destination::PullRequest {
                host,
                repository,
                number,
                ..
            } => {
                self.validate_remote(host, repository)?;
                if !matches!(self.key.kind, ResultKind::PullRequest | ResultKind::Comment)
                    || *number == 0
                {
                    return Err(QueryError(
                        "Remote result has no matching PR kind/number".into(),
                    ));
                }
            }
            Destination::Issue {
                host,
                repository,
                number,
                ..
            } => {
                self.validate_remote(host, repository)?;
                if !matches!(self.key.kind, ResultKind::Issue | ResultKind::Comment) || *number == 0
                {
                    return Err(QueryError(
                        "Remote result has no valid issue/PR number".into(),
                    ));
                }
            }
            Destination::Run {
                host,
                repository,
                id,
                attempt,
                ..
            } => {
                self.validate_remote(host, repository)?;
                if self.key.kind != ResultKind::Run || *id == 0 || *attempt == 0 {
                    return Err(QueryError(
                        "Run destination requires an ID and attempt".into(),
                    ));
                }
            }
            Destination::Workflow {
                host,
                repository,
                id,
            } => {
                self.validate_remote(host, repository)?;
                if self.key.kind != ResultKind::Workflow || *id == 0 {
                    return Err(QueryError("Workflow destination requires an ID".into()));
                }
            }
            Destination::Repository { host, repository } => {
                self.validate_remote(host, repository)?;
                if self.key.kind != ResultKind::Repository {
                    return Err(QueryError(
                        "Repository destination has the wrong result kind".into(),
                    ));
                }
            }
            Destination::RemoteFile {
                host,
                repository,
                path,
                revision,
            } => {
                self.validate_remote(host, repository)?;
                validate_raw_path(path)?;
                if self.key.kind != ResultKind::File
                    || self.key.path.as_ref() != Some(path)
                    || revision != &self.key.revision
                    || !valid_sha(revision)
                {
                    return Err(QueryError(
                        "Remote file destination needs its exact searched revision/path".into(),
                    ));
                }
            }
            Destination::RemoteCodeCandidate {
                host,
                repository,
                path,
                blob_sha,
            } => {
                self.validate_remote(host, repository)?;
                validate_raw_path(path)?;
                if self.source != SourceKind::GitHub
                    || self.key.kind != ResultKind::File
                    || self.key.path.as_ref() != Some(path)
                    || !valid_sha(blob_sha)
                    || self.key.revision != format!("indexed-blob:{blob_sha}")
                {
                    return Err(QueryError("Remote code candidate needs its indexed blob/path, not an invented commit revision".into()));
                }
            }
        }
        Ok(())
    }
    fn validate_remote(&self, host: &str, repository: &str) -> Result<(), QueryError> {
        if host != self.key.scope.host || repository != self.key.repository {
            return Err(QueryError(
                "Remote destination belongs to a different host/repository".into(),
            ));
        }
        Ok(())
    }
    /// Inline paths are provenance, not part of the mutable discussion resource identity.
    pub fn search_path(&self) -> Option<&RawPath> {
        self.key.path.as_ref().or_else(|| {
            self.metadata
                .discussion
                .as_ref()
                .and_then(|context| context.path.as_ref())
        })
    }
    pub fn matches(&self, query: &Query) -> bool {
        query.filters.iter().all(|(filter, values)| {
            values.iter().any(|value| match filter.as_str() {
                "repo" => &self.key.repository == value,
                "org" => self.key.repository.split('/').next() == Some(value.as_str()),
                "set" => self.metadata.repository_sets.contains(value),
                "type" => self.key.kind.label() == value,
                "rev" => {
                    &self.key.revision == value
                        || self
                            .metadata
                            .discussion
                            .as_ref()
                            .and_then(|context| context.commit_sha.as_ref())
                            == Some(value)
                }
                "path" => self
                    .search_path()
                    .is_some_and(|path| path.display().contains(value)),
                "language" => self
                    .metadata
                    .language
                    .as_ref()
                    .is_some_and(|language| language.eq_ignore_ascii_case(value)),
                "author" => {
                    self.metadata
                        .actors
                        .iter()
                        .any(|actor| &actor.login == value)
                        || self.key.kind == ResultKind::Commit
                            && self.source == SourceKind::SavedCommit
                            && self.metadata.git_author.as_ref().is_some_and(|author| {
                                &author.name == value || &author.email == value
                            })
                }
                "label" => self.metadata.labels.contains(value),
                "state" => value == "all" || self.metadata.state.as_ref() == Some(value),
                "after" => self
                    .metadata
                    .created_at
                    .as_deref()
                    .is_some_and(|date| date >= value.as_str()),
                "before" => {
                    self.metadata.created_at.as_deref().is_some_and(|date| {
                        date.get(..10).is_some_and(|date| date <= value.as_str())
                    })
                }
                _ => false,
            })
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    /// A source file is shared by all its hits; never clone multi-MiB content per occurrence.
    pub document: std::sync::Arc<Document>,
    pub line: usize,
    pub column: usize,
    /// Byte offsets in the prepared snippet, always valid UTF-8 boundaries.
    pub highlights: Vec<Range<usize>>,
    pub snippet: String,
    pub score: f32,
    pub matched_fields: Vec<String>,
}
impl SearchHit {
    pub fn destination(&self) -> Destination {
        let mut destination = self.document.destination.clone();
        if let Destination::LocalFile { line, column, .. } = &mut destination {
            *line = self.line;
            *column = self.column;
        }
        destination
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    pub complete: bool,
    /// Successful full enumeration of the intended corpus permits deleting unseen source keys,
    /// even when intentionally excluded large/binary files keep result coverage labelled partial.
    #[serde(default)]
    pub deletion_authoritative: bool,
    pub cancelled: bool,
    pub repositories: Vec<String>,
    pub unavailable: Vec<String>,
    pub documents: usize,
    pub scanned_bytes: usize,
    pub skipped_large: usize,
    pub skipped_binary: usize,
    pub skipped_special: usize,
    pub generation: Option<u64>,
    pub reasons: Vec<String>,
}
impl Coverage {
    pub fn summary(&self) -> String {
        format!(
            "{} • {} documents across {} repositories • {} large, {} binary, {} special skipped{}",
            if self.cancelled {
                "CANCELLED"
            } else if self.complete {
                "COMPLETE within selected scope"
            } else {
                "PARTIAL"
            },
            self.documents,
            self.repositories.len(),
            self.skipped_large,
            self.skipped_binary,
            self.skipped_special,
            self.generation.map_or(String::new(), |generation| format!(
                " • index generation {generation}"
            ))
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSearch {
    pub version: u32,
    pub name: String,
    pub scope: Scope,
    pub input: String,
    pub mode: SearchMode,
    pub repositories: Vec<String>,
    pub revision: Option<String>,
    /// Per-repository source retains mixed worktree/saved scopes; old version1 records default empty.
    #[serde(default)]
    pub sources: BTreeMap<String, super::corpus::LocalSource>,
    #[serde(default)]
    pub metadata_sources: super::sources::MetadataSources,
    #[serde(default)]
    pub include_hidden: bool,
    #[serde(default)]
    pub include_ignored: bool,
    #[serde(default = "saved_file_budget")]
    pub max_file_bytes: usize,
    pub pinned: bool,
    pub sort: String,
}
impl SavedSearch {
    pub fn validate(&self, scope: &Scope) -> Result<Query, QueryError> {
        if self.version != 1 || self.name.trim().is_empty() || &self.scope != scope {
            return Err(QueryError(
                "Saved search version, name or account scope is invalid".into(),
            ));
        }
        if !["relevance", "path", "updated"].contains(&self.sort.as_str()) {
            return Err(QueryError("Unknown saved-search sort".into()));
        }
        if self.max_file_bytes == 0 {
            return Err(QueryError(
                "Saved corpus needs a positive per-file inclusion budget".into(),
            ));
        }
        for repository in &self.repositories {
            validate_repository(repository).map_err(|error| QueryError(error.to_string()))?;
        }
        self.metadata_sources.validate(scope)?;
        if self
            .metadata_sources
            .repositories()
            .iter()
            .any(|repository| !self.repositories.contains(repository))
        {
            return Err(QueryError(
                "Saved metadata includes repositories outside the exact saved selection".into(),
            ));
        }
        if self
            .sources
            .keys()
            .any(|repository| !self.repositories.contains(repository))
        {
            return Err(QueryError(
                "Saved source scope includes a repository outside the saved selection".into(),
            ));
        }
        Query::parse(&self.input, self.mode)
    }
}
fn saved_file_budget() -> usize {
    super::corpus::CorpusLimits::default().max_file_bytes
}

pub fn valid_sha(id: &str) -> bool {
    matches!(id.len(), 40 | 64) && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}
pub fn validate_raw_path(path: &RawPath) -> Result<(), QueryError> {
    if path.0.is_empty()
        || path.0.contains(&0)
        || path.0.starts_with(b"/")
        || path.0.split(|byte| *byte == b'/').any(|part| part == b"..")
    {
        return Err(QueryError(
            "Search path must be an exact repository-relative path".into(),
        ));
    }
    Ok(())
}
