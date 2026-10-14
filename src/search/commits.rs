//! Explicit, bounded original-object commit-message search; never checkout/fetch or guess actors.
use super::{
    corpus::Corpus,
    model::{Coverage, Destination, Document, DocumentKey, Metadata, ResultKind, SourceKind},
    QueryError,
};
use crate::{
    git::{GitActor, GitRepository},
    providers::runner::CancellationToken,
    storage::Scope,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitSelection {
    pub repository: String,
    pub directory: PathBuf,
    pub revision: String,
    pub max_commits: usize,
}
#[derive(Debug, Clone)]
pub struct CommitLimits {
    pub max_commit_bytes: usize,
    pub max_total_bytes: usize,
    pub max_documents: usize,
}
impl Default for CommitLimits {
    fn default() -> Self {
        Self {
            max_commit_bytes: 256 * 1024,
            max_total_bytes: 32 * 1024 * 1024,
            max_documents: 5_000,
        }
    }
}
pub struct CollectedCommits {
    pub corpus: Corpus,
    pub visible_versions: BTreeMap<String, String>,
    pub resolved_revisions: BTreeMap<String, String>,
}

pub fn collect(
    scope: Scope,
    selections: &[CommitSelection],
    limits: &CommitLimits,
    cancel: &CancellationToken,
) -> Result<CollectedCommits, QueryError> {
    if limits.max_commit_bytes == 0
        || limits.max_commit_bytes > 2 * 1024 * 1024
        || limits.max_total_bytes == 0
        || limits.max_documents == 0
    {
        return Err(error(
            "Commit search budgets must be positive; individual original objects at most 2 MiB",
        ));
    }
    let mut result = CollectedCommits {
        corpus: Corpus {
            scope: scope.clone(),
            documents: vec![],
            coverage: Coverage {
                complete: true,
                deletion_authoritative: true,
                ..Default::default()
            },
        },
        visible_versions: BTreeMap::new(),
        resolved_revisions: BTreeMap::new(),
    };
    let mut repositories = BTreeSet::new();
    for selection in selections {
        crate::config::validate_repository(&selection.repository).map_err(error)?;
        if !repositories.insert(&selection.repository)
            || !(1..10_000).contains(&selection.max_commits)
        {
            return Err(error(
                "Choose one explicit 1..9999-commit history scope per repository",
            ));
        }
        result
            .corpus
            .coverage
            .repositories
            .push(selection.repository.clone());
        if cancel.is_cancelled() {
            cancelled(&mut result.corpus.coverage);
            break;
        }
        let collected = collect_repository(&scope, selection, limits, cancel, &mut result);
        if let Err(failure) = collected {
            result.corpus.coverage.complete = false;
            result.corpus.coverage.deletion_authoritative = false;
            result
                .corpus
                .coverage
                .unavailable
                .push(selection.repository.clone());
            result.corpus.coverage.reasons.push(format!(
                "{} commit history: {failure}",
                selection.repository
            ));
        }
        if cancel.is_cancelled() {
            cancelled(&mut result.corpus.coverage);
            break;
        }
    }
    result.corpus.coverage.documents = result.corpus.documents.len();
    Ok(result)
}
fn collect_repository(
    scope: &Scope,
    selection: &CommitSelection,
    limits: &CommitLimits,
    cancel: &CancellationToken,
    result: &mut CollectedCommits,
) -> Result<(), QueryError> {
    let repository =
        GitRepository::discover_with_cancellation(&selection.directory, cancel.clone())
            .map_err(error)?
            .ok_or_else(|| error("No existing Git repository at explicit commit source"))?;
    let revision = repository
        .resolve_original(&selection.revision)
        .map_err(error)?;
    result
        .resolved_revisions
        .insert(selection.repository.clone(), revision.clone());
    let mut ids = repository
        .commit_ids(&revision, 0, selection.max_commits + 1)
        .map_err(error)?;
    if ids.len() > selection.max_commits {
        ids.truncate(selection.max_commits);
        result.corpus.coverage.complete = false;
        result.corpus.coverage.reasons.push(format!(
            "{}: bounded to {} loaded saved messages, not complete history",
            selection.repository, selection.max_commits
        ));
    }
    // Saved-object search must also work on bare repositories and must not enumerate
    // unrelated working files simply to determine whether available history is shallow.
    if repository.is_shallow().map_err(error)? {
        result.corpus.coverage.complete = false;
        result.corpus.coverage.reasons.push(format!(
            "{}: shallow loaded history; no implicit fetch",
            selection.repository
        ));
    }
    let (replacements, grafted) = repository.history_overrides().map_err(error)?;
    let overrides = if replacements > 0 || grafted {
        vec![format!(
            "Original commit messages; {replacements} replacement refs; legacy graft file {}",
            if grafted { "present" } else { "absent" }
        )]
    } else {
        vec![]
    };
    result
        .corpus
        .coverage
        .reasons
        .extend(overrides.iter().cloned());
    let mut bounded = Vec::new();
    for chunk in ids.chunks(1024) {
        if cancel.is_cancelled() {
            cancelled(&mut result.corpus.coverage);
            return Ok(());
        }
        for (id, size) in chunk
            .iter()
            .zip(repository.commit_sizes(chunk).map_err(error)?)
        {
            if size > limits.max_commit_bytes {
                result.corpus.coverage.complete = false;
                result.corpus.coverage.skipped_large += 1;
                continue;
            }
            bounded.push((id.clone(), size));
        }
    }
    // Raw object payload+batch headers fit the shared runner output cap before allocation.
    let mut offset = 0usize;
    while offset < bounded.len() {
        if cancel.is_cancelled() {
            cancelled(&mut result.corpus.coverage);
            return Ok(());
        }
        let mut end = offset;
        let mut bytes = 0usize;
        while end < bounded.len()
            && end - offset < 1024
            && bytes.saturating_add(bounded[end].1 + 128) < 3 * 1024 * 1024
        {
            bytes += bounded[end].1 + 128;
            end += 1;
        }
        if end == offset {
            return Err(error("Commit batch cannot fit bounded transport"));
        }
        let batch = bounded[offset..end]
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        let objects = repository.commit_objects(&batch).map_err(error)?;
        for (id, raw) in batch.iter().zip(objects) {
            if cancel.is_cancelled() {
                cancelled(&mut result.corpus.coverage);
                return Ok(());
            }
            if result.corpus.documents.len() >= limits.max_documents
                || result
                    .corpus
                    .coverage
                    .scanned_bytes
                    .saturating_add(raw.len())
                    > limits.max_total_bytes
            {
                result.corpus.coverage.complete = false;
                result.corpus.coverage.deletion_authoritative = false;
                result.corpus.coverage.reasons.push("Commit message document/byte budget reached; unloaded messages are not admitted by an older index".into());
                return Ok(());
            }
            result.corpus.coverage.scanned_bytes += raw.len();
            let Some(document) =
                document(scope, selection, &repository.root, id, &raw, &overrides)?
            else {
                result.corpus.coverage.complete = false;
                result.corpus.coverage.skipped_special += 1;
                result.corpus.coverage.reasons.push(format!("Commit {id} has non-UTF-8 or unsupported declared message encoding; no lossy searchable text invented"));
                continue;
            };
            document.validate(scope)?;
            result
                .visible_versions
                .insert(document.key.stable_id(), document.projection_checksum()?);
            result.corpus.documents.push(document);
        }
        offset = end;
    }
    Ok(())
}
fn document(
    scope: &Scope,
    selection: &CommitSelection,
    directory: &std::path::Path,
    id: &str,
    object: &[u8],
    overrides: &[String],
) -> Result<Option<Document>, QueryError> {
    let separator = object
        .windows(2)
        .position(|bytes| bytes == b"\n\n")
        .ok_or_else(|| error("Commit object lacks its header/message boundary"))?;
    let headers = object[..separator]
        .split(|byte| *byte == b'\n')
        .collect::<Vec<_>>();
    let header = |key: &[u8]| headers.iter().find_map(|line| line.strip_prefix(key));
    if header(b"encoding ").is_some_and(|value| {
        !value.eq_ignore_ascii_case(b"utf-8")
            && !value.eq_ignore_ascii_case(b"utf8")
            && !value.eq_ignore_ascii_case(b"ascii")
            && !value.eq_ignore_ascii_case(b"us-ascii")
    }) {
        return Ok(None);
    }
    let Ok(body) = std::str::from_utf8(&object[separator + 2..]) else {
        return Ok(None);
    };
    let author =
        signature(header(b"author ").ok_or_else(|| error("Commit author signature missing"))?)?;
    let committer = signature(
        header(b"committer ").ok_or_else(|| error("Commit committer signature missing"))?,
    )?;
    Ok(Some(Document {
        key: DocumentKey {
            scope: scope.clone(),
            repository: selection.repository.clone(),
            revision: id.into(),
            kind: ResultKind::Commit,
            path: None,
            resource: format!("commit:{id}"),
        },
        source: SourceKind::SavedCommit,
        title: body.lines().next().unwrap_or("Empty commit message").into(),
        body: body.into(),
        metadata: Metadata {
            created_at: timestamp(author.timestamp),
            updated_at: timestamp(committer.timestamp),
            git_author: Some(author),
            git_committer: Some(committer),
            complete: true,
            history_overrides: overrides.to_vec(),
            ..Default::default()
        },
        destination: Destination::LocalCommit {
            directory: directory.into(),
            commit_id: id.into(),
        },
    }))
}
fn signature(bytes: &[u8]) -> Result<GitActor, QueryError> {
    let mut fields = bytes.rsplitn(3, |byte| *byte == b' ');
    let timezone = fields
        .next()
        .ok_or_else(|| error("Git signature timezone missing"))?;
    if timezone.len() != 5
        || !matches!(timezone[0], b'+' | b'-')
        || !timezone[1..].iter().all(u8::is_ascii_digit)
    {
        return Err(error("Git signature timezone malformed"));
    }
    let timestamp = fields
        .next()
        .and_then(|value| std::str::from_utf8(value).ok())
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| error("Git signature timestamp malformed"))?;
    let identity = fields
        .next()
        .ok_or_else(|| error("Git signature identity missing"))?;
    let open = identity
        .iter()
        .rposition(|byte| *byte == b'<')
        .ok_or_else(|| error("Git signature email delimiter missing"))?;
    if !identity.ends_with(b">") || open == 0 || identity[open - 1] != b' ' {
        return Err(error("Git signature identity malformed"));
    }
    let display = |value: &[u8]| {
        std::str::from_utf8(value)
            .map(str::to_owned)
            .unwrap_or_else(|_| crate::git::display_path_bytes(value))
    };
    Ok(GitActor {
        name: display(&identity[..open - 1]),
        email: display(&identity[open + 1..identity.len() - 1]),
        timestamp,
    })
}
fn timestamp(value: i64) -> Option<String> {
    let value = value as libc::time_t;
    let mut calendar = std::mem::MaybeUninit::<libc::tm>::uninit();
    // Supported macOS/Linux targets have time_t; gmtime_r writes only the provided tm.
    if unsafe { libc::gmtime_r(&value, calendar.as_mut_ptr()) }.is_null() {
        return None;
    }
    let calendar = unsafe { calendar.assume_init() };
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        calendar.tm_year + 1900,
        calendar.tm_mon + 1,
        calendar.tm_mday,
        calendar.tm_hour,
        calendar.tm_min,
        calendar.tm_sec
    ))
}
fn cancelled(coverage: &mut Coverage) {
    coverage.cancelled = true;
    coverage.complete = false;
    coverage.deletion_authoritative = false;
}
fn error(value: impl std::fmt::Display) -> QueryError {
    QueryError(value.to_string())
}
