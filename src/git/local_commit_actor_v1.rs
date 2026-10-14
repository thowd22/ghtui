//! Original raw commit actors. Actual installed Git supplies the object; this
//! token is local observation, never remote access or signature authority.
use super::*;
use sha2::{Digest, Sha256};
use std::{
    os::unix::fs::MetadataExt,
    time::{Duration, Instant},
};

const FIELD_BYTES: usize = 4096;
const MAX_OBJECT_BYTES: usize = 64 * 1024;
const ENVELOPE_BYTES: usize = 256;

#[derive(Clone, Debug)]
pub struct LocalCommitActorLimits {
    pub max_object_bytes: usize,
    pub max_duration: Duration,
}
impl Default for LocalCommitActorLimits {
    fn default() -> Self {
        Self {
            max_object_bytes: MAX_OBJECT_BYTES,
            max_duration: Duration::from_secs(30),
        }
    }
}

/// Private raw-byte construction. No lossy decoding or terminal sanitation can
/// make two different original actor headers compare equal.
pub struct OriginalGitActorHeader {
    name: Vec<u8>,
    email: Vec<u8>,
    seconds: i64,
    timezone: [u8; 5],
}
impl fmt::Debug for OriginalGitActorHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OriginalGitActorHeader(<bounded original bytes>)")
    }
}
impl OriginalGitActorHeader {
    pub fn name_bytes(&self) -> &[u8] {
        &self.name
    }
    pub fn email_bytes(&self) -> &[u8] {
        &self.email
    }
    pub fn seconds(&self) -> i64 {
        self.seconds
    }
    pub fn timezone(&self) -> &[u8; 5] {
        &self.timezone
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct LocalCommitSourceIdentity {
    root: PathBuf,
    root_file: (u64, u64),
    git_directory: PathBuf,
    common_directory: PathBuf,
    git_file: (u64, u64),
    common_file: (u64, u64),
    locator: Option<((u64, u64), u8, [u8; 32])>,
}
impl LocalCommitSourceIdentity {
    pub(crate) fn retained_memory_bytes_v2(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.root.capacity()
            + self.git_directory.capacity()
            + self.common_directory.capacity()
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn git_directory(&self) -> &Path {
        &self.git_directory
    }
    pub fn common_directory(&self) -> &Path {
        &self.common_directory
    }
    pub fn git_file_identity(&self) -> (u64, u64) {
        self.git_file
    }
    pub fn common_file_identity(&self) -> (u64, u64) {
        self.common_file
    }
}

pub struct OriginalLocalCommitActors {
    source: LocalCommitSourceIdentity,
    sha: String,
    author: OriginalGitActorHeader,
    committer: OriginalGitActorHeader,
    deadline: Instant,
    cancellation: CancellationToken,
    raw_bytes: usize,
    reserved_capture_bytes: usize,
    operation: Option<
        std::sync::Arc<crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1>,
    >,
}
impl fmt::Debug for OriginalLocalCommitActors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OriginalLocalCommitActors(<actual immutable object; no remote authority>)")
    }
}
impl OriginalLocalCommitActors {
    /// Exact producing operation, not merely a matching SHA/source/deadline.
    pub(crate) fn matches_navigation_operation_v1(
        &self,
        operation: &std::sync::Arc<
            crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
        >,
    ) -> bool {
        self.operation
            .as_ref()
            .is_some_and(|producing| std::sync::Arc::ptr_eq(producing, operation))
    }
    pub(super) fn read_under_navigation_operation_v1(
        repo: &GitRepository,
        sha: &str,
        limits: &LocalCommitActorLimits,
        operation: &std::sync::Arc<
            crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
        >,
    ) -> GitResult<Self> {
        operation.check().map_err(GitError)?;
        let bounded = repo
            .clone()
            .with_cancellation(operation.cancellation().clone())
            .strip_navigation_read_window_v1()?
            .with_navigation_read_window_v1(operation.window());
        let mut result = Self::read_under_deadline(
            &bounded,
            sha,
            limits,
            operation.deadline(),
            operation.cancellation(),
        )?;
        operation.check().map_err(GitError)?;
        result.operation = Some(operation.clone());
        Ok(result)
    }
    pub fn sha(&self) -> &str {
        &self.sha
    }
    pub fn source(&self) -> &LocalCommitSourceIdentity {
        &self.source
    }
    /// Bounds-only absolute clock used by the SAME later gh read operation.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    /// Worker-only continuity recheck BEFORE closing account proof. Pure
    /// render/input paths must instead compare their original native ticket.
    pub fn recheck_source(
        &self,
        repo: &GitRepository,
        cancel: &CancellationToken,
    ) -> GitResult<()> {
        self.remaining(cancel)?;
        if source_identity(repo)? != self.source {
            return Err(GitError(
                "Original local Git source directory was replaced".into(),
            ));
        }
        self.remaining(cancel)?;
        Ok(())
    }
    /// Exact original filesystem fence; factory calls this BEFORE final account
    /// proof even if its additional native-ticket callback merely returns Ok.
    pub fn recheck_original_source(&self, cancel: &CancellationToken) -> GitResult<()> {
        self.remaining(cancel)?;
        let current = source_paths(
            &self.source.root,
            &self.source.git_directory,
            &self.source.common_directory,
        )?;
        if current != self.source {
            return Err(GitError(
                "Original local Git directory/locator identity changed".into(),
            ));
        }
        self.remaining(cancel)?;
        Ok(())
    }
    pub fn author(&self) -> &OriginalGitActorHeader {
        &self.author
    }
    pub fn committer(&self) -> &OriginalGitActorHeader {
        &self.committer
    }
    /// Exact successful raw stdout bytes from the TWO original object reads.
    /// This is not a fabricated diagnostic-byte counter.
    pub fn stdout_bytes(&self) -> usize {
        self.raw_bytes
    }
    /// Conservative upper bound of both combined stdout/stderr capture caps;
    /// exact Git diagnostics accounting is not currently retained by GitOutput.
    pub fn reserved_capture_bytes(&self) -> usize {
        self.reserved_capture_bytes
    }
    /// Same original raw-read operation. A later fresh cancellation token does
    /// not revive this token or extend its clock.
    pub fn remaining(&self, cancel: &CancellationToken) -> GitResult<Duration> {
        remaining(self.deadline, &self.cancellation, cancel)
    }
    pub fn read(
        repo: &GitRepository,
        sha: &str,
        limits: &LocalCommitActorLimits,
        cancel: &CancellationToken,
    ) -> GitResult<Self> {
        validate_original_actor_read(sha, limits)?;
        let deadline = Instant::now()
            .checked_add(limits.max_duration)
            .ok_or_else(|| GitError("Local actor deadline unavailable".into()))?;
        Self::read_under_deadline(repo, sha, limits, deadline, cancel)
    }
    pub(super) fn read_under_deadline(
        repo: &GitRepository,
        sha: &str,
        limits: &LocalCommitActorLimits,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> GitResult<Self> {
        validate_original_actor_read(sha, limits)?;
        let deadline = deadline.min(
            Instant::now()
                .checked_add(limits.max_duration)
                .ok_or_else(|| GitError("Local actor deadline unavailable".into()))?,
        );
        remaining(deadline, cancel, cancel)?;
        let source = source_identity(repo)?;
        let input = format!("{sha}\n");
        let size_response = original_child(
            repo,
            ["cat-file", "--batch-check"],
            input.as_bytes(),
            deadline,
            cancel,
            ENVELOPE_BYTES,
        )?;
        let size = parse_size(sha, &size_response)?;
        if size > limits.max_object_bytes {
            return Err(GitError(
                "Original commit exceeds actor object bound before object allocation/read".into(),
            ));
        }
        let output_cap = size
            .checked_add(ENVELOPE_BYTES)
            .ok_or_else(|| GitError("Local object capture bound overflow".into()))?;
        let raw = original_child(
            repo,
            ["cat-file", "--batch"],
            input.as_bytes(),
            deadline,
            cancel,
            output_cap,
        )?;
        let header_end = raw
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or_else(|| GitError("Missing original commit batch header".into()))?;
        if parse_size(sha, &raw[..=header_end])? != size
            || raw.len() != header_end + 1 + size + 1
            || raw.last() != Some(&b'\n')
        {
            return Err(GitError(
                "Original commit batch identity/size was not exact".into(),
            ));
        }
        let (author, committer) = parse_headers(&raw[header_end + 1..header_end + 1 + size])?;
        // The same local source must survive BOTH actual children. This is a
        // filesystem continuity observation, never GitHub access permission.
        if source_identity(repo)? != source {
            return Err(GitError(
                "Original local Git directory identity changed".into(),
            ));
        }
        remaining(deadline, cancel, cancel)?;
        Ok(Self {
            source,
            sha: sha.into(),
            author,
            committer,
            deadline,
            cancellation: cancel.clone(),
            raw_bytes: raw.len() + size_response.len(),
            reserved_capture_bytes: ENVELOPE_BYTES + output_cap,
            operation: None,
        })
    }
}
fn validate_original_actor_read(sha: &str, limits: &LocalCommitActorLimits) -> GitResult<()> {
    if !(1..=MAX_OBJECT_BYTES).contains(&limits.max_object_bytes)
        || limits.max_duration.is_zero()
        || limits.max_duration > Duration::from_secs(30)
        || !matches!(sha.len(), 40 | 64)
        || !sha
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || sha.bytes().all(|byte| byte == b'0')
    {
        return Err(GitError(
            "Original local actor read requires a full canonical SHA and bounded object/deadline"
                .into(),
        ));
    }
    Ok(())
}
fn remaining(
    deadline: Instant,
    original: &CancellationToken,
    cancel: &CancellationToken,
) -> GitResult<Duration> {
    if original.is_cancelled() || cancel.is_cancelled() {
        return Err(GitError(
            "Original local actor read cancelled; no resend".into(),
        ));
    }
    deadline
        .checked_duration_since(Instant::now())
        .filter(|value| !value.is_zero())
        .ok_or_else(|| GitError("Original local actor aggregate deadline expired".into()))
}
pub(super) fn source_identity(repo: &GitRepository) -> GitResult<LocalCommitSourceIdentity> {
    source_paths(&repo.root, &repo.git_dir, &repo.common_dir)
}
fn source_paths(
    root: &Path,
    git_directory: &Path,
    common_directory: &Path,
) -> GitResult<LocalCommitSourceIdentity> {
    let inspect = |path: &Path| -> GitResult<(u64, u64)> {
        if path.as_os_str().len() > 4096 {
            return Err(GitError(
                "Local Git directory path exceeds source bound".into(),
            ));
        }
        let metadata = std::fs::metadata(path)
            .map_err(|_| GitError("Original local Git directory unavailable".into()))?;
        if !metadata.is_dir() {
            return Err(GitError(
                "Original local Git source is not a directory".into(),
            ));
        }
        Ok((metadata.dev(), metadata.ino()))
    };
    let root_file = inspect(root)?;
    let git_file = inspect(git_directory)?;
    let common_file = inspect(common_directory)?;
    let locator_path = root.join(".git");
    let locator = match std::fs::symlink_metadata(&locator_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(GitError("Original worktree Git locator unavailable".into())),
        Ok(metadata) => {
            let (kind, bytes) = if metadata.is_dir() {
                (0, Vec::new())
            } else if metadata.file_type().is_symlink() {
                let path = std::fs::read_link(&locator_path)
                    .map_err(|_| GitError("Original Git locator symlink unavailable".into()))?;
                if path.as_os_str().len() > 4096 {
                    return Err(GitError("Original Git locator path exceeds bound".into()));
                }
                (1, path.into_os_string().into_vec())
            } else if metadata.is_file() {
                if metadata.len() > 4096 {
                    return Err(GitError("Original Git locator file exceeds bound".into()));
                }
                let file = std::fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&locator_path)
                    .map_err(|_| GitError("Original Git locator file unavailable".into()))?;
                let opened = file.metadata().map_err(|_| {
                    GitError("Opened original Git locator identity unavailable".into())
                })?;
                if !opened.is_file()
                    || opened.dev() != metadata.dev()
                    || opened.ino() != metadata.ino()
                    || opened.len() > 4096
                {
                    return Err(GitError(
                        "Original Git locator changed before bounded read".into(),
                    ));
                }
                let mut bytes = Vec::new();
                file.take(4097)
                    .read_to_end(&mut bytes)
                    .map_err(|_| GitError("Original Git locator read unavailable".into()))?;
                if bytes.len() > 4096 {
                    return Err(GitError("Original Git locator bytes exceed bound".into()));
                }
                (2, bytes)
            } else {
                return Err(GitError("Original Git locator type unsupported".into()));
            };
            Some((
                (metadata.dev(), metadata.ino()),
                kind,
                Sha256::digest(&bytes).into(),
            ))
        }
    };
    Ok(LocalCommitSourceIdentity {
        root: root.to_path_buf(),
        root_file,
        git_directory: git_directory.to_path_buf(),
        common_directory: common_directory.to_path_buf(),
        git_file,
        common_file,
        locator,
    })
}
fn original_child(
    repo: &GitRepository,
    args: [&str; 2],
    input: &[u8],
    deadline: Instant,
    cancel: &CancellationToken,
    output_cap: usize,
) -> GitResult<Vec<u8>> {
    let timeout = remaining(deadline, cancel, cancel)?;
    let result = run_git_bounded_policy_v1(
        &repo.root,
        [
            OsString::from("--git-dir"),
            repo.git_dir.as_os_str().to_owned(),
            OsString::from("--no-replace-objects"),
        ]
        .into_iter()
        .chain(args.into_iter().map(OsString::from)),
        Some(input),
        false,
        cancel,
        GitReadPolicyV1 {
            timeout,
            output_limit: output_cap,
            window: repo.navigation_read_window.as_ref(),
            runner: Some(repo.actual_read_runner_v1()),
        },
    )?;
    remaining(deadline, cancel, cancel)?;
    if !result.success {
        return Err(result.error("Original local actor object read"));
    }
    Ok(result.stdout)
}
fn parse_size(sha: &str, bytes: &[u8]) -> GitResult<usize> {
    let record = bytes
        .strip_suffix(b"\n")
        .ok_or_else(|| GitError("Original size response lacks exact newline envelope".into()))?;
    let mut fields = record.split(|byte| *byte == b' ');
    if fields.next() != Some(sha.as_bytes()) || fields.next() != Some(b"commit".as_slice()) {
        return Err(GitError(
            "Original actor target is missing/not the exact immutable commit".into(),
        ));
    }
    let size = fields
        .next()
        .and_then(|value| std::str::from_utf8(value).ok())
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|size| *size > 0)
        .ok_or_else(|| GitError("Original commit size unavailable".into()))?;
    if fields.next().is_some() {
        return Err(GitError("Unexpected original object size fields".into()));
    }
    Ok(size)
}
fn parse_headers(raw: &[u8]) -> GitResult<(OriginalGitActorHeader, OriginalGitActorHeader)> {
    let mut author = None;
    let mut committer = None;
    let mut boundary = false;
    for line in raw.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            boundary = true;
            break;
        }
        if let Some(value) = line.strip_prefix(b"author ") {
            if author.is_some() {
                return Err(GitError("Duplicate original author header".into()));
            }
            author = Some(parse_actor(value)?);
        } else if let Some(value) = line.strip_prefix(b"committer ") {
            if committer.is_some() {
                return Err(GitError("Duplicate original committer header".into()));
            }
            committer = Some(parse_actor(value)?);
        } else if line.starts_with(b"author") || line.starts_with(b"committer") {
            return Err(GitError("Malformed original Git actor header".into()));
        }
    }
    if !boundary {
        return Err(GitError(
            "Original commit header/message boundary missing".into(),
        ));
    }
    Ok((
        author.ok_or_else(|| GitError("Original author header missing".into()))?,
        committer.ok_or_else(|| GitError("Original committer header missing".into()))?,
    ))
}
fn parse_actor(value: &[u8]) -> GitResult<OriginalGitActorHeader> {
    let close = value
        .iter()
        .rposition(|byte| *byte == b'>')
        .ok_or_else(|| GitError("Original actor email delimiter missing".into()))?;
    let open = value[..close]
        .iter()
        .rposition(|byte| *byte == b'<')
        .filter(|open| *open > 0 && value[*open - 1] == b' ')
        .ok_or_else(|| GitError("Original actor name/email delimiter missing".into()))?;
    let name = &value[..open - 1];
    let email = &value[open + 1..close];
    if name.len() > FIELD_BYTES
        || email.len() > FIELD_BYTES
        || std::str::from_utf8(name).is_err()
        || std::str::from_utf8(email).is_err()
        || name.contains(&0)
        || email.contains(&0)
    {
        return Err(GitError(
            "Original actor raw metadata is bounded but unavailable for exact JSON comparison"
                .into(),
        ));
    }
    let suffix = value[close + 1..]
        .strip_prefix(b" ")
        .ok_or_else(|| GitError("Original actor timestamp delimiter missing".into()))?;
    let mut fields = suffix.split(|byte| *byte == b' ');
    let seconds = fields
        .next()
        .and_then(|value| std::str::from_utf8(value).ok())
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| GitError("Original actor seconds malformed".into()))?;
    let timezone = fields
        .next()
        .filter(|value| {
            value.len() == 5
                && matches!(value[0], b'+' | b'-')
                && value[1..].iter().all(u8::is_ascii_digit)
        })
        .ok_or_else(|| GitError("Original actor timezone malformed".into()))?;
    let hours = (timezone[1] - b'0') * 10 + timezone[2] - b'0';
    let minutes = (timezone[3] - b'0') * 10 + timezone[4] - b'0';
    if fields.next().is_some() || hours > 23 || minutes > 59 {
        return Err(GitError("Original actor timezone/suffix malformed".into()));
    }
    Ok(OriginalGitActorHeader {
        name: name.to_vec(),
        email: email.to_vec(),
        seconds,
        timezone: timezone.try_into().expect("validated5"),
    })
}

#[cfg(test)]
#[path = "local_commit_actor_v1_tests.rs"]
mod tests;
