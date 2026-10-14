//! Installed-Git original DAG page. Opaque observations, never auth or a
//! reusable read budget. New child is undeclared until Root admission.
use super::*;
#[path = "original_graph_memory_v2.rs"]
mod shared_memory_v2;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    sync::Arc,
    time::Instant,
};

const PAGE: usize = 200;
const MAX_OFFSET: usize = 10_000;
const MAX_OBJECT: usize = 1024 * 1024;
const MAX_RAW_PAGE: usize = 16 * 1024 * 1024;
const MAX_MESSAGE: usize = 16 * 1024;
const MAX_REFS: usize = 128 * 1024;
const MAX_SHALLOW: usize = 128 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
struct EpochFileV1 {
    device: u64,
    inode: u64,
    bytes: usize,
    modified: (i64, i64),
    changed: (i64, i64),
    hash: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginalGraphEpochV1 {
    path: PathBuf,
    parent: (u64, u64, i64, i64),
    file: Option<EpochFileV1>,
    shallow: Arc<HashSet<String>>,
}
impl OriginalGraphEpochV1 {
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(self.path.as_os_str().as_bytes());
        digest.update(self.parent.0.to_be_bytes());
        digest.update(self.parent.1.to_be_bytes());
        digest.update(self.parent.2.to_be_bytes());
        digest.update(self.parent.3.to_be_bytes());
        if let Some(file) = &self.file {
            digest.update(file.device.to_be_bytes());
            digest.update(file.inode.to_be_bytes());
            digest.update(file.bytes.to_be_bytes());
            digest.update(file.modified.0.to_be_bytes());
            digest.update(file.modified.1.to_be_bytes());
            digest.update(file.changed.0.to_be_bytes());
            digest.update(file.changed.1.to_be_bytes());
            digest.update(file.hash);
        }
        digest.finalize().into()
    }
    pub fn is_shallow_commit(&self, sha: &str) -> bool {
        self.shallow.contains(sha)
    }
}
#[derive(Clone, Debug)]
pub struct OriginalGraphCommitV1 {
    commit: Commit,
    message: String,
    message_complete: bool,
    retained_bytes: usize,
}
impl OriginalGraphCommitV1 {
    pub fn commit(&self) -> &Commit {
        &self.commit
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn message_complete(&self) -> bool {
        self.message_complete
    }
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
}
#[derive(Clone, Debug)]
struct RefSnapshotV1 {
    by_oid: HashMap<String, Vec<(Vec<u8>, String)>>,
    captured_bytes: Arc<[u8]>,
    head: Option<String>,
}
pub enum OriginalGraphReadV1<'a> {
    Start(&'a str),
    Continue(&'a OriginalGraphPageV1),
}
pub struct OriginalGraphPageV1 {
    source: Arc<LocalCommitSourceIdentity>,
    walk_tip: String,
    offset: usize,
    next_offset: usize,
    lookahead: Option<String>,
    epoch: OriginalGraphEpochV1,
    refs: Arc<RefSnapshotV1>,
    commits: Arc<[OriginalGraphCommitV1]>,
}
impl fmt::Debug for OriginalGraphPageV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OriginalGraphPageV1")
            .field("walk_tip", &self.walk_tip)
            .field("offset", &self.offset)
            .field("rows", &self.commits.len())
            .field("more", &self.lookahead.is_some())
            .finish()
    }
}
impl OriginalGraphPageV1 {
    pub fn walk_tip(&self) -> &str {
        &self.walk_tip
    }
    pub fn offset(&self) -> usize {
        self.offset
    }
    pub fn next_offset(&self) -> usize {
        self.next_offset
    }
    pub fn has_more(&self) -> bool {
        self.lookahead.is_some() && self.next_offset < MAX_OFFSET
    }
    pub fn capped(&self) -> bool {
        self.lookahead.is_some() && self.next_offset >= MAX_OFFSET
    }
    pub fn commits(&self) -> &[OriginalGraphCommitV1] {
        &self.commits
    }
    pub fn epoch(&self) -> &OriginalGraphEpochV1 {
        &self.epoch
    }
    pub fn observed_head(&self) -> Option<&str> {
        self.refs.head.as_deref()
    }
    pub(crate) fn matches_source(&self, origin: &OriginalLocalActorOriginV1) -> bool {
        self.source.as_ref() == origin.source()
    }
}
impl GitRepository {
    pub(crate) fn original_graph_member_v1(
        &self,
        page: &OriginalGraphPageV1,
        expected: &OriginalGraphCommitV1,
        deadline: Instant,
    ) -> GitResult<()> {
        self.validate_graph_epoch_v1(page, deadline)?;
        let id = expected.commit().id.clone();
        let raw = objects(self, std::slice::from_ref(&id), deadline)?
            .pop()
            .ok_or_else(|| GitError("Original graph exact member object unavailable".into()))?;
        let actual = parse_commit(&id, &raw)?;
        let expected = expected.commit();
        if actual.commit.parents != expected.parents
            || actual.commit.author != expected.author
            || actual.commit.committer != expected.committer
            || actual.commit.subject != expected.subject
        {
            return fail("Original Graph exact member header changed before parent comparison");
        }
        self.validate_graph_epoch_v1(page, deadline)
    }
    pub(crate) fn validate_graph_fingerprint_v1(
        &self,
        expected: [u8; 32],
        deadline: Instant,
    ) -> GitResult<()> {
        if self.navigation_read_window.is_none() {
            return fail("Original Graph restore needs its actual new operation window");
        }
        if epoch(self, deadline)?.fingerprint() != expected {
            return fail("Original Graph saved visit epoch changed before object reads");
        }
        clock(self, deadline)
    }
    pub(crate) fn original_graph_deadline_v1(&self) -> Option<Instant> {
        self.navigation_read_window
            .as_ref()
            .map(|window| window.deadline_v1())
    }
    pub(crate) fn original_graph_cancellation_v1(&self) -> CancellationToken {
        self.cancellation.clone()
    }
    /// A real handle and actual original objects produce this observation.
    /// Bounds-only deadline cannot authenticate a source or revive a ledger.
    pub fn original_graph_page_v1(
        &self,
        request: OriginalGraphReadV1<'_>,
        deadline: Instant,
    ) -> GitResult<Arc<OriginalGraphPageV1>> {
        if self.navigation_read_window.is_none() {
            return fail("Original graph pages require the actual whole-operation read window");
        }
        clock(self, deadline)?;
        let source = Arc::new(local_commit_actor_v1::source_identity(self)?);
        let (tip, offset, previous) = match request {
            OriginalGraphReadV1::Start(tip) => {
                saved_oid(tip)?;
                (tip, 0, None)
            }
            OriginalGraphReadV1::Continue(old) => {
                if old.source.as_ref() != source.as_ref() || !old.has_more() {
                    return fail("Original graph source/cursor has no admissible continuation");
                }
                (old.walk_tip.as_str(), old.next_offset, Some(old))
            }
        };
        let before = epoch(self, deadline)?;
        if previous.is_some_and(|old| old.epoch != before) {
            return fail(
                "Original graph shallow walk changed; explicitly restart, no cursor rebase",
            );
        }
        let references = if let Some(old) = previous {
            old.refs.clone()
        } else {
            Arc::new(ref_snapshot(self, deadline)?)
        };
        let limit = format!("--max-count={}", (PAGE + 1).min(MAX_OFFSET + 1 - offset));
        let skip = format!("--skip={offset}");
        let bytes = read(
            self,
            [
                "log",
                "--topo-order",
                &limit,
                &skip,
                "--format=%H",
                tip,
                "--",
            ],
            None,
            (PAGE + 1) * 66,
            deadline,
        )?;
        let mut ids = Vec::new();
        let mut seen = HashSet::new();
        for line in bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
        {
            let id = std::str::from_utf8(line)
                .map_err(|_| GitError("Original graph ID is not canonical ASCII".into()))?;
            saved_oid(id)?;
            if ids.len() > PAGE || !seen.insert(id) {
                return fail("Original graph IDs exceeded page or duplicate bounds");
            }
            ids.push(id.to_owned());
        }
        if (offset == 0 && ids.first().is_none_or(|id| id != tip))
            || previous.is_some_and(|old| ids.first() != old.lookahead.as_ref())
        {
            return fail("Original graph cursor/lookahead changed; no skipped or rebased row");
        }
        let lookahead = (ids.len() > PAGE).then(|| ids.pop().expect("bounded lookahead"));
        let objects = objects(self, &ids, deadline)?;
        let mut commits = Vec::new();
        let mut prepared_bytes = 0usize;
        for (id, object) in ids.iter().zip(objects) {
            let mut observed = parse_commit(id, &object)?;
            if let Some(names) = references.by_oid.get(id) {
                for (raw, label) in names {
                    observed.commit.raw_refs.push(raw.clone());
                    observed.commit.refs.push(label.clone());
                }
            }
            if references.head.as_ref() == Some(id) {
                observed.commit.refs.insert(0, "HEAD at graph load".into());
            }
            observed.retained_bytes += observed.commit.raw_refs.iter().map(Vec::len).sum::<usize>()
                + observed.commit.refs.iter().map(String::len).sum::<usize>()
                + observed.commit.refs.len() * 128;
            prepared_bytes = prepared_bytes
                .checked_add(observed.retained_bytes)
                .filter(|n| *n <= 8 * 1024 * 1024)
                .ok_or_else(|| GitError("Original graph prepared page exceeds8MiB".into()))?;
            commits.push(observed);
        }
        if epoch(self, deadline)? != before
            || local_commit_actor_v1::source_identity(self)? != *source
        {
            return fail("Original graph source/shallow epoch changed before admission");
        }
        if previous.is_none() {
            let current = ref_snapshot(self, deadline)?;
            if current.captured_bytes != references.captured_bytes
                || current.head != references.head
            {
                return fail("Graph initial names/HEAD changed during original read; no atomic snapshot guessed");
            }
        }
        clock(self, deadline)?;
        Ok(Arc::new(OriginalGraphPageV1 {
            source,
            walk_tip: tip.into(),
            offset,
            next_offset: offset + commits.len(),
            lookahead,
            epoch: before,
            refs: references,
            commits: Arc::from(commits),
        }))
    }
    pub(crate) fn validate_graph_epoch_v1(
        &self,
        original: &OriginalGraphPageV1,
        deadline: Instant,
    ) -> GitResult<()> {
        if local_commit_actor_v1::source_identity(self)? != *original.source
            || epoch(self, deadline)? != original.epoch
        {
            return fail("Original graph source/walk epoch changed; no fresh parent read");
        }
        clock(self, deadline)
    }
}
fn clock(repo: &GitRepository, deadline: Instant) -> GitResult<()> {
    if repo.cancellation.is_cancelled() || Instant::now() >= deadline {
        return fail("Original graph deadline/cancellation reached before another read");
    }
    if let Some(window) = &repo.navigation_read_window {
        window
            .check(&repo.cancellation)
            .map_err(|error| GitError(error.to_string()))?;
    }
    Ok(())
}
fn read<I, S>(
    repo: &GitRepository,
    args: I,
    input: Option<&[u8]>,
    cap: usize,
    deadline: Instant,
) -> GitResult<Vec<u8>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    clock(repo, deadline)?;
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_owned())
        .collect::<Vec<_>>();
    let operation = args
        .first()
        .map_or("read".into(), |arg| arg.to_string_lossy());
    let args = [
        OsString::from("--git-dir"),
        repo.git_dir.as_os_str().to_owned(),
        OsString::from("--no-replace-objects"),
    ]
    .into_iter()
    .chain(args.iter().cloned());
    let result = run_git_bounded_policy_v1(
        &repo.root,
        args,
        input,
        false,
        &repo.cancellation,
        GitReadPolicyV1 {
            timeout: deadline.saturating_duration_since(Instant::now()),
            output_limit: cap,
            window: repo.navigation_read_window.as_ref(),
            runner: Some(repo.actual_read_runner_v1()),
        },
    )
    .map_err(|error| GitError(format!("Original graph {operation}: {error}")))?;
    clock(repo, deadline)?;
    if !result.success {
        return Err(result.error("Original graph read"));
    }
    Ok(result.stdout)
}
fn epoch(repo: &GitRepository, deadline: Instant) -> GitResult<OriginalGraphEpochV1> {
    let raw = read(
        repo,
        [
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "shallow",
        ],
        None,
        8192,
        deadline,
    )?;
    let path = PathBuf::from(OsString::from_vec(trim_line(&raw).to_vec()));
    if !path.is_absolute() || path.as_os_str().as_bytes().len() > 4096 {
        return fail("Original graph shallow locator unavailable/bounded");
    }
    let parent_metadata = std::fs::metadata(
        path.parent()
            .ok_or_else(|| GitError("Original shallow parent unavailable".into()))?,
    )
    .map_err(|_| GitError("Original shallow parent metadata unavailable".into()))?;
    if !parent_metadata.is_dir() {
        return fail("Original shallow parent is not a directory");
    }
    let parent = (
        parent_metadata.dev(),
        parent_metadata.ino(),
        parent_metadata.ctime(),
        parent_metadata.ctime_nsec(),
    );
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(OriginalGraphEpochV1 {
                path,
                parent,
                file: None,
                shallow: Arc::new(HashSet::new()),
            })
        }
        Err(_) => return fail("Original graph shallow metadata unavailable"),
    };
    if !metadata.is_file() || metadata.len() > MAX_SHALLOW as u64 {
        return fail("Original graph shallow file is not a bounded regular file");
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|_| GitError("Original graph shallow file cannot be opened safely".into()))?;
    let opened = file
        .metadata()
        .map_err(|_| GitError("Original shallow source changed".into()))?;
    if opened.dev() != metadata.dev()
        || opened.ino() != metadata.ino()
        || opened.len() != metadata.len()
        || opened.mtime() != metadata.mtime()
        || opened.mtime_nsec() != metadata.mtime_nsec()
        || opened.ctime() != metadata.ctime()
        || opened.ctime_nsec() != metadata.ctime_nsec()
    {
        return fail("Original shallow file changed before bounded read");
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take((MAX_SHALLOW + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| GitError("Original shallow file read unavailable".into()))?;
    if bytes.len() > MAX_SHALLOW || bytes.len() != metadata.len() as usize {
        return fail("Original shallow read exceeded/changed source");
    }
    let mut ids = HashSet::new();
    for row in bytes
        .split(|byte| *byte == b'\n')
        .filter(|row| !row.is_empty())
    {
        let id = std::str::from_utf8(row)
            .map_err(|_| GitError("Malformed original shallow ID".into()))?;
        saved_oid(id)?;
        if !ids.insert(id.to_owned()) {
            return fail("Duplicate original shallow metadata");
        }
    }
    clock(repo, deadline)?;
    let after = file
        .metadata()
        .map_err(|_| GitError("Original shallow file metadata lost".into()))?;
    if after.dev() != opened.dev()
        || after.ino() != opened.ino()
        || after.len() != opened.len()
        || after.mtime() != opened.mtime()
        || after.mtime_nsec() != opened.mtime_nsec()
        || after.ctime() != opened.ctime()
        || after.ctime_nsec() != opened.ctime_nsec()
    {
        return fail("Original shallow source changed during bounded read");
    }
    Ok(OriginalGraphEpochV1 {
        path,
        parent,
        file: Some(EpochFileV1 {
            device: metadata.dev(),
            inode: metadata.ino(),
            bytes: bytes.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
            hash: Sha256::digest(&bytes).into(),
        }),
        shallow: Arc::new(ids),
    })
}
fn ref_snapshot(repo: &GitRepository, deadline: Instant) -> GitResult<RefSnapshotV1> {
    let bytes = read(
        repo,
        [
            "for-each-ref",
            "--format=%(objectname)%00%(refname)%00%(*objectname)%00%(objecttype)%00%(*objecttype)",
        ],
        None,
        MAX_REFS,
        deadline,
    )?;
    let head = read(
        repo,
        ["rev-parse", "--verify", "--end-of-options", "HEAD^{commit}"],
        None,
        128,
        deadline,
    )?;
    let head = std::str::from_utf8(trim_line(&head))
        .map_err(|_| GitError("Original HEAD is not a full saved SHA".into()))?;
    saved_oid(head)?;
    let mut names: HashMap<String, Vec<(Vec<u8>, String)>> = HashMap::new();
    let mut count = 0usize;
    for row in bytes
        .split(|byte| *byte == b'\n')
        .filter(|row| !row.is_empty())
    {
        count += 1;
        if count > 4096 {
            return fail("Original graph ref snapshot exceeds4096");
        }
        let parts = row.split(|byte| *byte == 0).collect::<Vec<_>>();
        if parts.len() != 5 || parts[1].len() > 1024 || parts[1].is_empty() {
            return fail("Original graph ref row malformed or oversized");
        }
        let object = if parts[3] == b"commit" {
            parts[0]
        } else if parts[4] == b"commit" {
            parts[2]
        } else {
            continue;
        };
        let id = std::str::from_utf8(object)
            .map_err(|_| GitError("Original ref object is not ASCII".into()))?;
        saved_oid(id)?;
        names
            .entry(id.into())
            .or_default()
            .push((parts[1].to_vec(), display_bytes(parts[1])));
    }
    Ok(RefSnapshotV1 {
        by_oid: names,
        captured_bytes: Arc::from(bytes),
        head: Some(head.into()),
    })
}
fn objects(repo: &GitRepository, ids: &[String], deadline: Instant) -> GitResult<Vec<Vec<u8>>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let input = ids
        .iter()
        .flat_map(|id| id.as_bytes().iter().copied().chain([b'\n']))
        .collect::<Vec<_>>();
    let sizes = read(
        repo,
        ["cat-file", "--batch-check"],
        Some(&input),
        ids.len() * 160,
        deadline,
    )?;
    let records = sizes
        .split(|byte| *byte == b'\n')
        .filter(|row| !row.is_empty())
        .collect::<Vec<_>>();
    if records.len() != ids.len() {
        return fail("Original graph size preflight is incomplete");
    }
    let mut total = 0usize;
    let mut expected_sizes = Vec::new();
    for (id, row) in ids.iter().zip(records) {
        let fields = row.split(|byte| *byte == b' ').collect::<Vec<_>>();
        if fields.len() != 3 || fields[0] != id.as_bytes() || fields[1] != b"commit" {
            return fail("Original graph object missing/not exact commit");
        }
        let size = std::str::from_utf8(fields[2])
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| *n <= MAX_OBJECT)
            .ok_or_else(|| {
                GitError("Original graph commit exceeds1MiB preflight; not read".into())
            })?;
        expected_sizes.push(size);
        total = total
            .checked_add(size + 160)
            .filter(|n| *n <= MAX_RAW_PAGE)
            .ok_or_else(|| GitError("Original graph raw page exceeds16MiB; not read".into()))?;
    }
    let bytes = read(repo, ["cat-file", "--batch"], Some(&input), total, deadline)?;
    let mut remaining = bytes.as_slice();
    let mut output = Vec::new();
    for (id, expected_size) in ids.iter().zip(expected_sizes) {
        let split = remaining
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or_else(|| GitError("Original graph object envelope truncated".into()))?;
        let fields = remaining[..split]
            .split(|byte| *byte == b' ')
            .collect::<Vec<_>>();
        if fields.len() != 3 || fields[0] != id.as_bytes() || fields[1] != b"commit" {
            return fail("Original graph exact object envelope changed");
        }
        let size = std::str::from_utf8(fields[2])
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| *n <= MAX_OBJECT)
            .ok_or_else(|| GitError("Original graph object size changed".into()))?;
        if size != expected_size {
            return fail("Original graph object envelope differs from size preflight");
        }
        remaining = &remaining[split + 1..];
        if remaining.len() <= size || remaining[size] != b'\n' {
            return fail("Original graph raw object response incomplete");
        }
        output.push(remaining[..size].to_vec());
        remaining = &remaining[size + 1..];
    }
    if !remaining.is_empty() {
        return fail("Original graph batch has extra data");
    }
    Ok(output)
}
fn parse_actor(raw: &[u8]) -> GitResult<GitActor> {
    if raw.len() > 8192 {
        return fail("Original graph actor header exceeds bounds");
    }
    let bracket = raw
        .windows(2)
        .rposition(|pair| pair == b" <")
        .ok_or_else(|| GitError("Original graph actor envelope malformed".into()))?;
    let end = raw[bracket + 2..]
        .iter()
        .position(|byte| *byte == b'>')
        .map(|i| i + bracket + 2)
        .ok_or_else(|| GitError("Original graph actor email envelope malformed".into()))?;
    let name = &raw[..bracket];
    let email = &raw[bracket + 2..end];
    if name.len() > 4096 || email.len() > 4096 {
        return fail("Original graph actor field exceeds4KiB");
    }
    let clock = std::str::from_utf8(&raw[end + 1..])
        .map_err(|_| GitError("Original graph actor timestamp malformed".into()))?;
    let values = clock.split_whitespace().collect::<Vec<_>>();
    if values.len() != 2
        || values[1].len() != 5
        || !matches!(values[1].as_bytes()[0], b'+' | b'-')
        || !values[1].as_bytes()[1..].iter().all(u8::is_ascii_digit)
    {
        return fail("Original graph actor UTC offset malformed");
    }
    let timestamp = values[0]
        .parse::<i64>()
        .map_err(|_| GitError("Original graph actor date malformed".into()))?;
    Ok(GitActor {
        name: display_bytes(name),
        email: display_bytes(email),
        timestamp,
    })
}
fn parse_commit(id: &str, object: &[u8]) -> GitResult<OriginalGraphCommitV1> {
    saved_oid(id)?;
    let split = object
        .windows(2)
        .position(|pair| pair == b"\n\n")
        .ok_or_else(|| GitError("Original graph commit header lacks message separator".into()))?;
    let mut parents = Vec::new();
    let mut author = None;
    let mut committer = None;
    for row in object[..split].split(|byte| *byte == b'\n') {
        if let Some(parent) = row.strip_prefix(b"parent ") {
            let parent = std::str::from_utf8(parent)
                .map_err(|_| GitError("Original graph parent ID malformed".into()))?;
            saved_oid(parent)?;
            if parent.len() != id.len()
                || parents.len() >= 64
                || parents.iter().any(|old| old == parent)
            {
                return fail("Original graph parent algorithm/order/bounds invalid");
            }
            parents.push(parent.to_owned());
        } else if let Some(actor) = row.strip_prefix(b"author ") {
            if author.is_some() {
                return fail("Duplicate original graph author");
            }
            author = Some(parse_actor(actor)?);
        } else if let Some(actor) = row.strip_prefix(b"committer ") {
            if committer.is_some() {
                return fail("Duplicate original graph committer");
            }
            committer = Some(parse_actor(actor)?);
        }
    }
    let raw = &object[split + 2..];
    let subject = raw.split(|byte| *byte == b'\n').next().unwrap_or_default();
    if subject.len() > 8192 {
        return fail("Original graph subject exceeds8KiB; not cloned");
    }
    let message = raw[..raw.len().min(MAX_MESSAGE)]
        .split(|byte| *byte == b'\n')
        .map(display_bytes)
        .collect::<Vec<_>>()
        .join("\n");
    let commit = Commit {
        id: id.into(),
        parents,
        author: author.ok_or_else(|| GitError("Original graph author missing".into()))?,
        committer: committer.ok_or_else(|| GitError("Original graph committer missing".into()))?,
        subject: display_bytes(subject),
        refs: Vec::new(),
        raw_refs: Vec::new(),
    };
    let retained_bytes = commit.id.len()
        + commit.parents.iter().map(String::len).sum::<usize>()
        + commit.author.name.len()
        + commit.author.email.len()
        + commit.committer.name.len()
        + commit.committer.email.len()
        + commit.subject.len()
        + message.len()
        + 256;
    Ok(OriginalGraphCommitV1 {
        commit,
        message,
        message_complete: raw.len() <= MAX_MESSAGE,
        retained_bytes,
    })
}
fn saved_oid(id: &str) -> GitResult<()> {
    if !original_actor_saved_sha(id) {
        return fail("Original graph requires a genuine full40/64 lowercase saved SHA");
    }
    Ok(())
}
fn fail<T>(message: &str) -> GitResult<T> {
    Err(GitError(message.into()))
}

#[cfg(test)]
#[path = "original_graph_page_v1_tests.rs"]
mod tests;
