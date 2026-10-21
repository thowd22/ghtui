//! Explicit read-only corpus collection. Installed git semantics and embedded ignore handling.
use super::{
    model::{Coverage, Destination, Document, DocumentKey, Metadata, ResultKind, SourceKind},
    QueryError,
};
use crate::{
    config::validate_repository,
    git::{GitRepository, RawPath},
    providers::runner::CancellationToken,
    storage::Scope,
};
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    os::unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LocalSource {
    Worktree,
    Revision(String),
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RepositorySelection {
    pub repository: String,
    pub directory: PathBuf,
    pub source: LocalSource,
    pub sets: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct CorpusLimits {
    pub max_file_bytes: usize,
    pub max_total_bytes: usize,
    pub max_documents: usize,
    pub include_ignored: bool,
    pub include_hidden: bool,
}
impl Default for CorpusLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 2 * 1024 * 1024,
            max_total_bytes: 128 * 1024 * 1024,
            max_documents: 100_000,
            include_ignored: false,
            include_hidden: false,
        }
    }
}
#[derive(Debug, Clone)]
pub struct Corpus {
    pub scope: Scope,
    pub documents: Vec<Document>,
    pub coverage: Coverage,
}
/// The document consumer of [`Corpus::collect_with`] and what it has been handed.
struct Sink<'a> {
    visit: &'a mut dyn FnMut(&mut Vec<Document>) -> bool,
    /// Documents admitted to the corpus (some may have been taken by `visit`).
    admitted: usize,
    /// Files enumerated for reading, so an early stop can say how many it left unread.
    enumerated: usize,
    stopped: bool,
}
struct FileInput {
    path: RawPath,
    revision: Option<String>,
    bytes: Vec<u8>,
    history_overrides: Vec<String>,
}
impl Corpus {
    /// Invoke from a worker. A failed/missing selected repository remains visible in coverage.
    pub fn collect(
        scope: Scope,
        selections: &[RepositorySelection],
        limits: &CorpusLimits,
        cancellation: &CancellationToken,
    ) -> Result<Self, QueryError> {
        Self::collect_with(scope, selections, limits, cancellation, &mut |_| true)
    }
    /// [`Corpus::collect`] that hands every admitted document to `sink` as soon as it is
    /// read (TASK-112). Selections are visited in the given order and each one's files in
    /// sorted path order, so with selections sorted by repository a sink that stops early
    /// (returns `false`) has seen exactly the first documents of the full sorted corpus. The sink may take documents out of `documents`; whatever it leaves stays in
    /// the returned corpus. After a stop, coverage is partial and names how many of the
    /// enumerated files were read, so the totals never overstate what was searched.
    pub fn collect_with(
        scope: Scope,
        selections: &[RepositorySelection],
        limits: &CorpusLimits,
        cancellation: &CancellationToken,
        sink: &mut dyn FnMut(&mut Vec<Document>) -> bool,
    ) -> Result<Self, QueryError> {
        if limits.max_file_bytes == 0 || limits.max_documents == 0 || limits.max_total_bytes == 0 {
            return Err(QueryError(
                "Search resource limits must be greater than zero".into(),
            ));
        }
        let mut sink = Sink {
            visit: sink,
            admitted: 0,
            enumerated: 0,
            stopped: false,
        };
        let mut corpus = Self {
            scope,
            documents: Vec::new(),
            coverage: Coverage {
                complete: true,
                deletion_authoritative: true,
                ..Default::default()
            },
        };
        let mut unreached = 0usize;
        for selection in selections {
            validate_repository(&selection.repository)
                .map_err(|error| QueryError(error.to_string()))?;
            if corpus.coverage.repositories.contains(&selection.repository) {
                return Err(QueryError(
                    "Choose one explicit working/revision scope per repository".into(),
                ));
            }
            corpus
                .coverage
                .repositories
                .push(selection.repository.clone());
            if sink.stopped {
                // Still in the selected scope, just never reached.
                unreached += 1;
                continue;
            }
            if cancellation.is_cancelled() {
                corpus.cancel();
                break;
            }
            let result = corpus.collect_repository(selection, limits, cancellation, &mut sink);
            if let Err(error) = result {
                corpus.coverage.complete = false;
                corpus.coverage.deletion_authoritative = false;
                corpus
                    .coverage
                    .unavailable
                    .push(selection.repository.clone());
                corpus
                    .coverage
                    .reasons
                    .push(format!("{}: {error}", selection.repository));
            }
            if cancellation.is_cancelled() {
                corpus.cancel();
                break;
            }
        }
        corpus.coverage.documents = sink.admitted;
        if sink.stopped {
            corpus.coverage.complete = false;
            corpus.coverage.deletion_authoritative = false;
            corpus.coverage.reasons.push(format!(
                "Reading stopped with the scan budget: {} of {} enumerated files read{}; later files were not searched",
                sink.admitted,
                sink.enumerated,
                if unreached == 0 {
                    String::new()
                } else {
                    format!(", {unreached} selected repositories not reached")
                }
            ));
        }
        corpus.documents.sort_by(|a, b| {
            (
                &a.key.repository,
                &a.key.revision,
                &a.key.path.as_ref().map(|p| &p.0),
            )
                .cmp(&(
                    &b.key.repository,
                    &b.key.revision,
                    &b.key.path.as_ref().map(|p| &p.0),
                ))
        });
        Ok(corpus)
    }
    /// A cheap identity of everything `collect` would read for these selections, without
    /// reading file contents: each repository's HEAD, index and config file stats, replacement
    /// refs and grafts, and, for working files, the path, size, mtime, ctime and inode of every
    /// file the same ignore-aware walk visits. `None` means the inputs cannot be trusted to be
    /// unchanged (a missing repository, a walk error, cancellation, or a file modified within
    /// the last two seconds, whose timestamp may not change on a second same-size edit), and
    /// the caller must collect again.
    pub fn input_fingerprint(
        selections: &[RepositorySelection],
        limits: &CorpusLimits,
        cancellation: &CancellationToken,
    ) -> Option<String> {
        let started = SystemTime::now();
        let racy_after = started
            .checked_sub(std::time::Duration::from_secs(RACY_SECONDS))
            .unwrap_or(UNIX_EPOCH);
        let mut hasher = Sha256::new();
        let mut field = |bytes: &[u8]| {
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(bytes);
        };
        field(b"ghtui-corpus-input-v1");
        field(
            format!(
                "{} {} {} {} {}",
                limits.max_file_bytes,
                limits.max_total_bytes,
                limits.max_documents,
                limits.include_ignored,
                limits.include_hidden
            )
            .as_bytes(),
        );
        for selection in selections {
            if cancellation.is_cancelled() {
                return None;
            }
            field(selection.repository.as_bytes());
            field(selection.directory.as_os_str().as_bytes());
            field(selection.sets.join("\n").as_bytes());
            let repository = GitRepository::discover_with_cancellation(
                &selection.directory,
                cancellation.clone(),
            )
            .ok()??;
            field(repository.root.as_os_str().as_bytes());
            field(repository.git_dir.as_os_str().as_bytes());
            field(repository.common_dir.as_os_str().as_bytes());
            field(if repository.bare {
                b"bare"
            } else {
                b"worktree"
            });
            let (replacements, grafted) = repository.history_overrides().ok()?;
            field(format!("{replacements} {grafted}").as_bytes());
            match &selection.source {
                LocalSource::Revision(revision) => {
                    field(b"revision");
                    field(revision.as_bytes());
                    field(repository.resolve_original(revision).ok()?.as_bytes());
                }
                LocalSource::Worktree => {
                    field(b"worktree");
                    if repository.bare {
                        return None;
                    }
                    field(
                        repository
                            .resolve("HEAD")
                            .unwrap_or_else(|_| "unborn".into())
                            .as_bytes(),
                    );
                    for path in [
                        repository.git_dir.join("index"),
                        repository.git_dir.join("config"),
                        repository.common_dir.join("config"),
                        repository.git_dir.join("info/exclude"),
                        repository.common_dir.join("info/exclude"),
                    ] {
                        field(path.as_os_str().as_bytes());
                        field(stat_identity(fs::symlink_metadata(&path).ok().as_ref()).as_bytes());
                    }
                    let root = fs::canonicalize(&repository.root).ok()?;
                    field(root.as_os_str().as_bytes());
                    for entry in walker(&root, limits).build() {
                        if cancellation.is_cancelled() {
                            return None;
                        }
                        let entry = entry.ok()?;
                        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                            continue;
                        }
                        field(entry.path().as_os_str().as_bytes());
                        let metadata = fs::symlink_metadata(entry.path()).ok()?;
                        if metadata.modified().ok()? >= racy_after {
                            return None;
                        }
                        field(stat_identity(Some(&metadata)).as_bytes());
                    }
                }
            }
        }
        Some(format!("{:x}", hasher.finalize()))
    }
    fn collect_repository(
        &mut self,
        selection: &RepositorySelection,
        limits: &CorpusLimits,
        cancellation: &CancellationToken,
        sink: &mut Sink<'_>,
    ) -> Result<(), QueryError> {
        let repository =
            GitRepository::discover_with_cancellation(&selection.directory, cancellation.clone())
                .map_err(|e| QueryError(e.to_string()))?
                .ok_or_else(|| QueryError("No local Git repository at selected path".into()))?;
        let (replacements, grafted) = repository
            .history_overrides()
            .map_err(|error| QueryError(error.to_string()))?;
        let history_overrides = if replacements > 0 || grafted {
            vec![format!(
                "{} replacement refs; legacy graft file {}",
                replacements,
                if grafted { "present" } else { "absent" }
            )]
        } else {
            Vec::new()
        };
        if replacements > 0 || grafted {
            self.coverage.reasons.push(format!("{}: {} replacement refs; legacy graft file {}; saved scope uses original objects, working scope keeps installed-Git semantics",selection.repository,replacements,if grafted{"present"}else{"absent"}));
        }
        match &selection.source {
            LocalSource::Worktree => {
                if repository.bare {
                    return Err(QueryError(
                        "Bare repository has no working files; choose a saved version".into(),
                    ));
                }
                let root =
                    fs::canonicalize(&repository.root).map_err(|e| QueryError(e.to_string()))?;
                // Enumerate first and read in the corpus's sorted path order, so a sink that
                // stops early has seen exactly the first documents (TASK-112).
                let mut files = Vec::new();
                for entry in walker(&root, limits).build() {
                    if cancellation.is_cancelled() {
                        self.cancel();
                        return Ok(());
                    }
                    let entry = entry.map_err(|e| QueryError(e.to_string()))?;
                    if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                        continue;
                    }
                    if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                        self.coverage.skipped_special += 1;
                        continue;
                    }
                    let relative = entry
                        .path()
                        .strip_prefix(&root)
                        .map_err(|e| QueryError(e.to_string()))?
                        .as_os_str()
                        .as_bytes()
                        .to_vec();
                    files.push((relative, entry.into_path()));
                }
                files.sort_unstable_by(|a, b| a.0.cmp(&b.0));
                sink.enumerated += files.len();
                for (relative, full) in files {
                    if cancellation.is_cancelled() {
                        self.cancel();
                        break;
                    }
                    let path = RawPath::new(relative);
                    let metadata =
                        fs::symlink_metadata(&full).map_err(|e| QueryError(e.to_string()))?;
                    if metadata.len() > limits.max_file_bytes as u64 {
                        self.coverage.skipped_large += 1;
                        self.coverage.complete = false;
                        continue;
                    }
                    if !self.room(metadata.len() as usize, limits) {
                        break;
                    }
                    // Reject symlink leaves and non-regular descriptor races; no mmap of mutable files.
                    let mut file = OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                        .open(&full)
                        .map_err(|e| QueryError(e.to_string()))?;
                    if !file
                        .metadata()
                        .map_err(|e| QueryError(e.to_string()))?
                        .is_file()
                    {
                        self.coverage.skipped_special += 1;
                        continue;
                    }
                    let mut bytes = Vec::new();
                    let mut chunk = [0u8; 64 * 1024];
                    loop {
                        if cancellation.is_cancelled() {
                            self.cancel();
                            return Ok(());
                        }
                        let count = file
                            .read(&mut chunk)
                            .map_err(|e| QueryError(e.to_string()))?;
                        if count == 0 {
                            break;
                        }
                        bytes.extend_from_slice(&chunk[..count]);
                        if bytes.len() > limits.max_file_bytes {
                            break;
                        }
                    }
                    self.add_document(
                        selection,
                        &repository.root,
                        FileInput {
                            path,
                            revision: None,
                            bytes,
                            history_overrides: history_overrides.clone(),
                        },
                        limits,
                        sink,
                    )?;
                    if sink.stopped {
                        break;
                    }
                }
                self.coverage.reasons.push(format!("{} working files: ignores {} and hidden paths {}; mutable files are not a saved-version index",selection.repository,if limits.include_ignored{"included"}else{"excluded"},if limits.include_hidden{"included"}else{"excluded"}));
            }
            LocalSource::Revision(revision) => {
                let revision = repository
                    .resolve_original(revision)
                    .map_err(|e| QueryError(e.to_string()))?;
                let entries = repository
                    .tree(&revision)
                    .map_err(|e| QueryError(e.to_string()))?;
                for batch in entries.chunks(1024) {
                    if cancellation.is_cancelled() {
                        self.cancel();
                        break;
                    }
                    let regular = batch
                        .iter()
                        .filter(|entry| {
                            entry.kind == "blob"
                                && matches!(entry.mode.as_str(), "100644" | "100755")
                        })
                        .collect::<Vec<_>>();
                    self.coverage.skipped_special += batch.len() - regular.len();
                    sink.enumerated += regular.len();
                    let ids = regular
                        .iter()
                        .map(|entry| entry.id.clone())
                        .collect::<Vec<_>>();
                    let sizes = repository
                        .object_sizes(&ids)
                        .map_err(|e| QueryError(e.to_string()))?;
                    let mut start = 0;
                    while start < regular.len() {
                        if cancellation.is_cancelled() {
                            self.cancel();
                            return Ok(());
                        }
                        let mut chosen = Vec::new();
                        let mut bytes = 0usize;
                        while start < regular.len() {
                            let size = sizes[start];
                            if size > limits.max_file_bytes {
                                self.coverage.skipped_large += 1;
                                self.coverage.complete = false;
                                start += 1;
                                continue;
                            }
                            if !self.room(bytes.saturating_add(size), limits) {
                                return Ok(());
                            }
                            if !chosen.is_empty() && bytes.saturating_add(size) > 4 * 1024 * 1024 {
                                break;
                            }
                            bytes += size;
                            chosen.push(start);
                            start += 1;
                        }
                        let ids = chosen
                            .iter()
                            .map(|index| regular[*index].id.clone())
                            .collect::<Vec<_>>();
                        let contents = repository
                            .objects(&ids)
                            .map_err(|e| QueryError(e.to_string()))?;
                        for (index, bytes) in chosen.into_iter().zip(contents) {
                            if cancellation.is_cancelled() {
                                self.cancel();
                                return Ok(());
                            }
                            self.add_document(
                                selection,
                                &repository.root,
                                FileInput {
                                    path: regular[index].path.clone(),
                                    revision: Some(revision.clone()),
                                    bytes,
                                    history_overrides: history_overrides.clone(),
                                },
                                limits,
                                sink,
                            )?;
                            if sink.stopped {
                                return Ok(());
                            }
                        }
                    }
                }
                self.coverage.reasons.push(format!("{} saved version {}: tracked regular files, independent of current checkout or ignore rules",selection.repository,&revision[..8]));
            }
        }
        Ok(())
    }
    fn room(&mut self, bytes: usize, limits: &CorpusLimits) -> bool {
        if self.documents.len() >= limits.max_documents
            || self.coverage.scanned_bytes.saturating_add(bytes) > limits.max_total_bytes
        {
            self.coverage.complete = false;
            self.coverage.deletion_authoritative = false;
            if !self.coverage.reasons.iter().any(|reason| {
                reason == "Selected corpus exceeds the configured document/byte budget"
            }) {
                self.coverage
                    .reasons
                    .push("Selected corpus exceeds the configured document/byte budget".into());
            }
            false
        } else {
            true
        }
    }
    fn add_document(
        &mut self,
        selection: &RepositorySelection,
        directory: &std::path::Path,
        input: FileInput,
        limits: &CorpusLimits,
        sink: &mut Sink<'_>,
    ) -> Result<(), QueryError> {
        let FileInput {
            path,
            revision,
            bytes,
            history_overrides,
        } = input;
        let revision = revision.as_deref();
        if bytes.len() > limits.max_file_bytes {
            self.coverage.skipped_large += 1;
            self.coverage.complete = false;
            return Ok(());
        }
        if !self.room(bytes.len(), limits) {
            return Ok(());
        }
        self.coverage.scanned_bytes += bytes.len();
        if bytes.contains(&0) {
            self.coverage.skipped_binary += 1;
            return Ok(());
        }
        let Ok(body) = String::from_utf8(bytes) else {
            self.coverage.skipped_binary += 1;
            return Ok(());
        };
        if body.starts_with("version https://git-lfs.github.com/spec/v1\n") {
            self.coverage.skipped_special += 1;
            self.coverage.reasons.push(format!(
                "{} {} is an LFS pointer; payload was not downloaded",
                selection.repository,
                path.display()
            ));
            return Ok(());
        }
        let title = path.display();
        let document = Document {
            key: DocumentKey {
                scope: self.scope.clone(),
                repository: selection.repository.clone(),
                revision: revision.unwrap_or("working").into(),
                kind: ResultKind::File,
                path: Some(path.clone()),
                // Two linked worktrees/checkouts may share a GitHub repository and even HEAD
                // while having different mutable contents and navigation destinations.
                resource: format!(
                    "file-workspace:{:x}",
                    Sha256::digest(directory.as_os_str().as_bytes())
                ),
            },
            source: if revision.is_some() {
                SourceKind::SavedCommit
            } else {
                SourceKind::Worktree
            },
            title,
            body,
            metadata: Metadata {
                history_overrides,
                language: language(&path),
                fetched_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                repository_sets: selection.sets.clone(),
                complete: true,
                ..Default::default()
            },
            destination: Destination::LocalFile {
                directory: directory.into(),
                path,
                revision: revision.map(str::to_owned),
                line: 1,
                column: 1,
            },
        };
        document.validate(&self.scope)?;
        self.documents.push(document);
        sink.admitted += 1;
        if !(sink.visit)(&mut self.documents) {
            sink.stopped = true;
        }
        Ok(())
    }
    fn cancel(&mut self) {
        self.coverage.cancelled = true;
        self.coverage.complete = false;
        self.coverage.deletion_authoritative = false;
    }
}
/// Seconds within which a file's mtime is too recent to prove it unchanged later.
const RACY_SECONDS: u64 = 2;

/// The ignore-aware working-file walk shared by collection and its input fingerprint.
fn walker(root: &std::path::Path, limits: &CorpusLimits) -> WalkBuilder {
    let mut walk = WalkBuilder::new(root);
    walk.follow_links(false)
        .hidden(!limits.include_hidden)
        .parents(false)
        .git_global(!limits.include_ignored)
        .ignore(!limits.include_ignored)
        .git_ignore(!limits.include_ignored)
        .git_exclude(!limits.include_ignored)
        .filter_entry(|entry| entry.file_name() != std::ffi::OsStr::new(".git"));
    if !limits.include_ignored {
        walk.add_custom_ignore_filename(".rgignore");
    }
    walk
}

fn stat_identity(metadata: Option<&fs::Metadata>) -> String {
    use std::os::unix::fs::MetadataExt;
    metadata.map_or_else(
        || "absent".into(),
        |metadata| {
            format!(
                "{:o} {} {}.{} {}.{} {} {}",
                metadata.mode(),
                metadata.len(),
                metadata.mtime(),
                metadata.mtime_nsec(),
                metadata.ctime(),
                metadata.ctime_nsec(),
                metadata.ino(),
                metadata.dev()
            )
        },
    )
}

pub fn language(path: &RawPath) -> Option<String> {
    let suffix = path.0.rsplit(|byte| *byte == b'.').next()?;
    let value = match suffix {
        b"rs" => "Rust",
        b"py" => "Python",
        b"js" | b"jsx" => "JavaScript",
        b"ts" | b"tsx" => "TypeScript",
        b"go" => "Go",
        b"java" => "Java",
        b"c" | b"h" => "C",
        b"cpp" | b"hpp" | b"cc" => "C++",
        b"md" => "Markdown",
        b"toml" => "TOML",
        b"json" => "JSON",
        b"yaml" | b"yml" => "YAML",
        b"sh" => "Shell",
        b"rb" => "Ruby",
        _ => return None,
    };
    Some(value.into())
}
