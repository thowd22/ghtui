//! Historical content is not original review placement or a current image.
//! Exact actual selected-comment producer only; no raw/serde constructor.
use super::*;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoricalCommitRoleV5 {
    Associated,
    Original,
}
impl HistoricalCommitRoleV5 {
    pub fn label(self) -> &'static str {
        match self {
            Self::Associated => "Associated C",
            Self::Original => "Original C0",
        }
    }
    pub fn observed_sha(self, comment: &MetricsThreadComment) -> Option<&str> {
        match self {
            Self::Associated => comment.commit_sha.as_deref(),
            Self::Original => comment.original_commit_sha.as_deref(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoricalContentKindV5 {
    Text,
    Binary,
    Symlink,
    Submodule,
    LfsPointer,
    CandidatePathAbsent,
    Directory,
    Oversized,
}
pub struct PrThreadHistoricalReadV5 {
    selected: Arc<OriginalThreadCommentV2>,
    role: HistoricalCommitRoleV5,
    commit: String,
    root: String,
    path: Vec<u8>,
    object: Option<String>,
    mode: Option<String>,
    bytes: Option<Vec<u8>>,
    kind: HistoricalContentKindV5,
    tree_reads: usize,
    tree_rows: usize,
}
impl std::fmt::Debug for PrThreadHistoricalReadV5 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrThreadHistoricalReadV5(<verified historical content; placement UNKNOWN; no range/write authority>)")
    }
}
impl PrThreadHistoricalReadV5 {
    pub fn selected(&self) -> &Arc<OriginalThreadCommentV2> {
        &self.selected
    }
    pub fn role(&self) -> HistoricalCommitRoleV5 {
        self.role
    }
    pub fn commit(&self) -> &str {
        &self.commit
    }
    pub fn root_tree(&self) -> &str {
        &self.root
    }
    pub fn path(&self) -> &[u8] {
        &self.path
    }
    pub fn object_sha(&self) -> Option<&str> {
        self.object.as_deref()
    }
    pub fn mode(&self) -> Option<&str> {
        self.mode.as_deref()
    }
    pub fn bytes(&self) -> Option<&[u8]> {
        self.bytes.as_deref()
    }
    pub fn kind(&self) -> HistoricalContentKindV5 {
        self.kind
    }
    pub fn tree_reads(&self) -> usize {
        self.tree_reads
    }
    pub fn tree_rows(&self) -> usize {
        self.tree_rows
    }
    pub(crate) fn retained_image_bytes_v5(&self) -> Option<usize> {
        [
            self.commit.capacity(),
            self.root.capacity(),
            self.path.capacity(),
            self.object.as_ref().map_or(0, String::capacity),
            self.mode.as_ref().map_or(0, String::capacity),
            self.bytes.as_ref().map_or(0, Vec::capacity),
            std::mem::size_of::<Self>(),
        ]
        .into_iter()
        .try_fold(0usize, usize::checked_add)
    }
}

fn components_v5<'a>(
    path: &'a [u8],
    limits: &PrImmutableFileLimitsV1,
) -> GithubResult<Vec<&'a [u8]>> {
    // API path strings are retained as exact UTF8 bytes; never normalized or
    // claimed to represent arbitrary non-UTF8 Git names that were not observed.
    let parts: Vec<_> = path.split(|byte| *byte == b'/').collect();
    if path.is_empty()
        || path.len() > 4096
        || parts.len() > limits.max_path_depth
        || parts.iter().any(|part| {
            part.is_empty()
                || *part == b"."
                || *part == b".."
                || part.contains(&0)
                || std::str::from_utf8(part).is_err()
        })
    {
        return Err(safe(GithubErrorKind::Unsupported));
    }
    Ok(parts)
}

impl GithubProvider {
    /// ONE intermediate exact object read. Caller must prepare display and
    /// finish SAME original SQL/raw-parent/repository/final-principal closure.
    pub(crate) fn pr_thread_historical_context_v5(
        &self,
        selected: Arc<OriginalThreadCommentV2>,
        role: HistoricalCommitRoleV5,
        limits: &PrImmutableFileLimitsV1,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<PrThreadHistoricalReadV5> {
        limits.validate()?;
        let standard = PrImmutableFileLimitsV1::default();
        if limits.max_parent_bytes > standard.max_parent_bytes
            || limits.max_document_bytes > standard.max_document_bytes
            || limits.max_blob_bytes > standard.max_blob_bytes
            || limits.max_pool_bytes > standard.max_pool_bytes
        {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        let (deadline, cap, timeout) = selected.historical_read_phase_v5(self, cancel, deadline)?;
        let provider = self
            .clone()
            .with_timeout(timeout)
            .with_scoped_read_window(deadline, cap)?;
        let comment = selected.comment();
        let sha = role
            .observed_sha(comment)
            .filter(|sha| full_sha(sha))
            .ok_or_else(|| safe(GithubErrorKind::Unsupported))?;
        let path = comment
            .observed_path
            .as_deref()
            .ok_or_else(|| safe(GithubErrorKind::Unsupported))?;
        let parts = components_v5(path.as_bytes(), limits)?;
        let repository = selected.original().parent().repository();
        // All endpoints use validated full object SHAs. Path components never
        // enter argv/URL parsing or cause arbitrary contents/ref requests.
        let root = commit_tree(&provider, repository, sha, limits, cancel)?;
        let mut current_tree = root.clone();
        let mut traversal = TreeReadState::default();
        let mut terminal = None;
        for (index, part) in parts.iter().enumerate() {
            selected.historical_read_phase_v5(&provider, cancel, deadline)?;
            let entries = tree(
                &provider,
                repository,
                &current_tree,
                limits,
                cancel,
                &mut traversal,
            )?;
            let Some(entry) = entries.into_iter().find(|entry| entry.path == *part) else {
                break;
            };
            if index + 1 < parts.len() {
                if entry.kind != "tree" {
                    break;
                }
                current_tree = entry.sha;
            } else {
                terminal = Some(entry);
            }
        }
        let (object, mode, bytes, kind) = match terminal {
            None => (
                None,
                None,
                None,
                HistoricalContentKindV5::CandidatePathAbsent,
            ),
            Some(entry) => {
                let (bytes, kind) = if entry.kind == "tree" {
                    (None, HistoricalContentKindV5::Directory)
                } else if entry.kind == "commit" {
                    (None, HistoricalContentKindV5::Submodule)
                } else if entry
                    .size
                    .is_some_and(|size| size > limits.max_blob_bytes as u64)
                {
                    (None, HistoricalContentKindV5::Oversized)
                } else {
                    let bytes = raw_blob(&provider, repository, &entry, limits, cancel)?;
                    let kind = if entry.mode == "120000" {
                        HistoricalContentKindV5::Symlink
                    } else if bytes.starts_with(b"version https://git-lfs.github.com/spec/v1\n") {
                        HistoricalContentKindV5::LfsPointer
                    } else if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() {
                        HistoricalContentKindV5::Binary
                    } else {
                        HistoricalContentKindV5::Text
                    };
                    (Some(bytes), kind)
                };
                (Some(entry.sha), Some(entry.mode), bytes, kind)
            }
        };
        selected.historical_read_phase_v5(&provider, cancel, deadline)?;
        let commit = sha.to_owned();
        let path = path.as_bytes().to_vec();
        let result = PrThreadHistoricalReadV5 {
            selected,
            role,
            commit,
            root,
            path,
            object,
            mode,
            bytes,
            kind,
            tree_reads: traversal.reads,
            tree_rows: traversal.rows,
        };
        result
            .selected
            .historical_read_phase_v5(&provider, cancel, deadline)?;
        Ok(result)
    }
}
