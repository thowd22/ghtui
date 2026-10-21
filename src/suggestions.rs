//! Review code suggestions: authoring bodies, resulting-change previews, batch
//! combination and an explicit isolated apply.
//!
//! A suggestion replaces RIGHT-side lines `start..=end` of `path` at the PR
//! head with `replacement`. Applying never touches the user's checkout, index
//! or refs: every object is written into a private temporary bare repository
//! that borrows the user's objects through `objects/info/alternates`, and the
//! single resulting commit is pushed only after an explicit confirmation, with
//! a lease on the exact head the preview observed. The temporary repository is
//! removed on every outcome.
use crate::providers::runner::CancellationToken;
use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Longest replacement accepted for one suggestion (GitHub's own limit is far
/// larger; this keeps previews bounded).
pub const MAX_SUGGESTION_LINES: usize = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestionAuthor {
    pub login: String,
    /// Numeric account id. Without it no verifiable co-author trailer exists.
    pub account_id: Option<u64>,
}

/// One suggested change, as authored against the lines it replaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub comment_id: Option<u64>,
    pub path: String,
    /// First and last replaced RIGHT-side line, 1-based and inclusive.
    pub start: u32,
    pub end: u32,
    /// The lines the author saw (no terminators). Applying refuses when the
    /// current head no longer has exactly these lines in the range.
    pub original: Vec<String>,
    /// Replacement lines (no terminators). Empty deletes the range.
    pub replacement: Vec<String>,
    pub author: Option<SuggestionAuthor>,
    /// Where the author saw the replaced lines. When set, the apply reads
    /// `original` from that commit instead of trusting the caller's copy.
    pub original_source: Option<OriginalSource>,
}

/// The commit and 1-based inclusive lines a posted suggestion was written on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginalSource {
    pub commit: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SuggestionError {
    /// The range is empty, reversed or past the end of the file.
    Range(String),
    /// Two suggestions in one batch touch the same lines.
    Overlap {
        path: String,
        first: (u32, u32),
        second: (u32, u32),
    },
    /// The current head's lines differ from the lines the suggestion replaces.
    Conflict { path: String, start: u32, end: u32 },
    /// The PR branch no longer points at the head the preview observed.
    OutdatedHead { expected: String, actual: String },
    /// The remote refused the push (permissions, protection, a race).
    PushRejected(String),
    /// Git failed before any remote write.
    Git(String),
    /// The user cancelled before any remote write.
    Cancelled,
    /// The time limit passed before any remote write.
    TimedOut,
    /// The push started but its result could not be read back.
    Uncertain(String),
}
impl SuggestionError {
    /// True only when the remote branch is known to be unchanged.
    pub fn nothing_written(&self) -> bool {
        !matches!(self, Self::Uncertain(_))
    }
}

impl fmt::Display for SuggestionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Range(message) => write!(f, "Suggestion range refused: {message}. Nothing was written."),
            Self::Overlap { path, first, second } => write!(
                f,
                "Suggestions on {path} lines {}–{} and {}–{} overlap. Apply them separately; nothing was written.",
                first.0, first.1, second.0, second.1
            ),
            Self::Conflict { path, start, end } => write!(
                f,
                "{path} lines {start}–{end} changed since the suggestion was written. Nothing was written or pushed."
            ),
            Self::OutdatedHead { expected, actual } => write!(
                f,
                "The PR head moved from {expected} to {actual}. Refresh and preview again; nothing was pushed."
            ),
            Self::PushRejected(message) => write!(
                f,
                "The remote refused the push: {message}. The PR branch was not changed."
            ),
            Self::Git(message) => write!(f, "Git failed: {message}. Nothing was pushed."),
            Self::Cancelled => write!(f, "Cancelled before anything was pushed."),
            Self::TimedOut => write!(f, "Timed out before anything was pushed."),
            Self::Uncertain(message) => write!(
                f,
                "The push outcome is unknown: {message}. Refresh the PR before trying again."
            ),
        }
    }
}
impl std::error::Error for SuggestionError {}

/// The fenced body GitHub renders as a suggested change. The fence grows past
/// any backtick run in the code so the block always closes where intended.
pub fn suggestion_body(lines: &[String]) -> String {
    let longest = lines
        .iter()
        .map(|line| {
            let mut best = 0;
            let mut run = 0;
            for ch in line.chars() {
                run = if ch == '`' { run + 1 } else { 0 };
                best = best.max(run);
            }
            best
        })
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    let mut body = format!("{fence}suggestion\n");
    for line in lines {
        body.push_str(line);
        body.push('\n');
    }
    body.push_str(&fence);
    body
}

/// Lines of the image as shown in a suggestion: no `\n` and no trailing `\r`.
pub fn display_lines(bytes: &[u8]) -> Vec<String> {
    split_lines(bytes)
        .into_iter()
        .map(|line| String::from_utf8_lossy(strip_terminator(line)).into_owned())
        .collect()
}

/// Every ```suggestion block in a comment body, in order. An unclosed block is
/// not a suggestion.
pub fn parse_suggestions(body: &str) -> Vec<Vec<String>> {
    let mut found = vec![];
    let mut lines = body
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line));
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let ticks = trimmed.chars().take_while(|ch| *ch == '`').count();
        if ticks < 3 || trimmed[ticks..].trim() != "suggestion" {
            continue;
        }
        let mut block = vec![];
        let mut closed = false;
        for inner in lines.by_ref() {
            let candidate = inner.trim();
            if candidate.len() >= ticks && candidate.chars().all(|ch| ch == '`') {
                closed = true;
                break;
            }
            block.push(inner.to_owned());
        }
        if closed {
            found.push(block);
        }
    }
    found
}

fn split_lines(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.split_inclusive(|byte| *byte == b'\n').collect()
}
fn strip_terminator(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

/// One previewed file: the exact resulting bytes and a readable diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultingChange {
    pub path: String,
    pub bytes: Vec<u8>,
    /// `@@ path lines a–b` headers then `-old` / `+new` rows.
    pub preview: Vec<String>,
}

/// Applies every suggestion for ONE file to `original`. Ranges are checked
/// against the current bytes and against each other before anything changes.
pub fn resulting_change(
    path: &str,
    original: &[u8],
    suggestions: &[&Suggestion],
) -> Result<ResultingChange, SuggestionError> {
    let lines = split_lines(original);
    let mut ordered = suggestions.to_vec();
    ordered.sort_by_key(|suggestion| (suggestion.start, suggestion.end));
    for pair in ordered.windows(2) {
        if pair[1].start <= pair[0].end {
            return Err(SuggestionError::Overlap {
                path: path.into(),
                first: (pair[0].start, pair[0].end),
                second: (pair[1].start, pair[1].end),
            });
        }
    }
    for suggestion in &ordered {
        if suggestion.path != path {
            return Err(SuggestionError::Range(format!(
                "{} is not {path}",
                suggestion.path
            )));
        }
        if suggestion.start == 0
            || suggestion.start > suggestion.end
            || suggestion.end as usize > lines.len()
        {
            return Err(SuggestionError::Range(format!(
                "{path} lines {}–{} are outside its {} lines",
                suggestion.start,
                suggestion.end,
                lines.len()
            )));
        }
        if suggestion.replacement.len() > MAX_SUGGESTION_LINES {
            return Err(SuggestionError::Range(format!(
                "more than {MAX_SUGGESTION_LINES} replacement lines"
            )));
        }
        let current = lines[suggestion.start as usize - 1..suggestion.end as usize]
            .iter()
            .map(|line| String::from_utf8_lossy(strip_terminator(line)).into_owned())
            .collect::<Vec<_>>();
        if current != suggestion.original {
            return Err(SuggestionError::Conflict {
                path: path.into(),
                start: suggestion.start,
                end: suggestion.end,
            });
        }
    }
    let mut bytes = Vec::with_capacity(original.len());
    let mut preview = vec![];
    let mut next = 0usize;
    for suggestion in &ordered {
        let first = suggestion.start as usize - 1;
        let last = suggestion.end as usize;
        for line in &lines[next..first] {
            bytes.extend_from_slice(line);
        }
        let replaced = &lines[first..last];
        // Keep the file's own terminators: CRLF stays CRLF, and a final line
        // without a newline stays without one.
        let crlf = replaced
            .iter()
            .chain(lines.iter())
            .find(|line| line.ends_with(b"\n"))
            .is_some_and(|line| line.ends_with(b"\r\n"));
        let terminator: &[u8] = if crlf { b"\r\n" } else { b"\n" };
        let ends_file = last == lines.len() && !replaced.last().is_some_and(|l| l.ends_with(b"\n"));
        preview.push(format!(
            "@@ {path} lines {}–{}",
            suggestion.start, suggestion.end
        ));
        for line in replaced {
            preview.push(format!(
                "-{}",
                String::from_utf8_lossy(strip_terminator(line))
            ));
        }
        for (index, line) in suggestion.replacement.iter().enumerate() {
            preview.push(format!("+{line}"));
            bytes.extend_from_slice(line.as_bytes());
            if !(ends_file && index + 1 == suggestion.replacement.len()) {
                bytes.extend_from_slice(terminator);
            }
        }
        next = last;
    }
    for line in &lines[next..] {
        bytes.extend_from_slice(line);
    }
    Ok(ResultingChange {
        path: path.into(),
        bytes,
        preview,
    })
}

/// What an isolated apply will do (or did).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestionPlan {
    pub expected_head: String,
    pub commit: String,
    pub tree: String,
    pub files: Vec<(String, String)>,
    pub preview: Vec<String>,
    pub message: String,
    /// Authors that could not be credited, with the reason.
    pub attribution_limits: Vec<String>,
    pub pushed: bool,
}

pub struct ApplyRequest<'a> {
    /// Any local repository of the PR; only its objects are borrowed.
    pub repository: &'a Path,
    /// Remote URL or local path of the PR head repository.
    pub remote: &'a str,
    pub branch: &'a str,
    pub expected_head: &'a str,
    pub suggestions: &'a [Suggestion],
    pub subject: &'a str,
    /// `(name, email)` committing identity. Required; never guessed.
    pub committer: (&'a str, &'a str),
    /// False: build and report the exact commit, push nothing.
    pub push: bool,
    /// Parent of the private repository; the system temporary directory when
    /// unset.
    pub scratch_in: Option<&'a Path>,
    /// Stops every Git child; also checked between steps.
    pub cancel: Option<&'a CancellationToken>,
    /// Absolute time limit for the whole apply.
    pub deadline: Option<Instant>,
    /// Fixed `GIT_AUTHOR_DATE`/`GIT_COMMITTER_DATE` (`<unix> +0000`) so a
    /// confirmed push recreates exactly the previewed commit.
    pub date: Option<&'a str>,
    /// The previewed commit. A push refuses when the rebuilt commit differs.
    pub expected_commit: Option<&'a str>,
    /// Push URL when it differs from `remote` (`pushurl`/`pushInsteadOf`).
    pub push_to: Option<&'a str>,
}

/// Cancellation and time bounds shared by every Git child of one apply.
#[derive(Clone, Copy)]
struct Bounds<'a> {
    cancel: Option<&'a CancellationToken>,
    deadline: Option<Instant>,
}
impl Bounds<'_> {
    fn check(&self) -> Result<(), SuggestionError> {
        if self.cancel.is_some_and(CancellationToken::is_cancelled) {
            return Err(SuggestionError::Cancelled);
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(SuggestionError::TimedOut);
        }
        Ok(())
    }
}

fn git(
    bounds: Bounds<'_>,
    dir: &Path,
    args: &[&str],
    env: &[(&str, &str)],
    stdin: Option<&[u8]>,
) -> Result<Vec<u8>, SuggestionError> {
    bounds.check()?;
    let mut command = Command::new("git");
    command
        .arg("--no-pager")
        .arg("--literal-pathspecs")
        // No background maintenance may outlive the private repository.
        .args(["-c", "gc.auto=0", "-c", "maintenance.auto=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_EDITOR", "false")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command
        .spawn()
        .map_err(|error| SuggestionError::Git(format!("cannot start git: {error}")))?;
    if let Some(input) = stdin {
        use std::io::Write;
        let mut pipe = child.stdin.take().expect("piped stdin");
        if let Err(error) = pipe.write_all(input) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SuggestionError::Git(format!("git stdin: {error}")));
        }
    }
    fn drain(mut pipe: impl std::io::Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut bytes = vec![];
            let _ = pipe.read_to_end(&mut bytes);
            bytes
        })
    }
    let stdout = drain(child.stdout.take().expect("piped stdout"));
    let stderr = drain(child.stderr.take().expect("piped stderr"));
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(SuggestionError::Git(format!("git: {error}")));
            }
        }
        if let Err(stop) = bounds.check() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(stop);
        }
        // Child polling interval; cancellation and the deadline bound it.
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    if status.success() {
        Ok(stdout)
    } else {
        Err(SuggestionError::Git(format!(
            "git {}: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&stderr).trim()
        )))
    }
}
fn text(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).trim().to_owned()
}

/// Removes the private repository however the apply ends.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn co_author_trailer(author: &SuggestionAuthor) -> Option<String> {
    author.account_id.map(|id| {
        format!(
            "Co-authored-by: {} <{id}+{}@users.noreply.github.com>",
            author.login, author.login
        )
    })
}

/// The checkout remote that a suggestion commit is fetched from and pushed to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushRemote {
    pub name: String,
    /// The URL as configured; it names the PR head repository on the host.
    pub configured_url: String,
    /// The effective fetch and push URLs after `insteadOf` rewriting.
    pub fetch_url: String,
    pub push_url: String,
}

/// Finds the checkout remote whose configured URL is `head_repository` on
/// `host`. A remote that only looks plausible (same name, unparseable URL) is
/// never chosen: the caller explains how to add one instead.
pub fn resolve_push_remote(
    repository: &Path,
    host: &str,
    head_repository: &str,
    cancel: Option<&CancellationToken>,
    deadline: Option<Instant>,
) -> Result<PushRemote, String> {
    let bounds = Bounds { cancel, deadline };
    let listing = git(
        bounds,
        repository,
        &["config", "--null", "--get-regexp", r"^remote\..*\.url$"],
        &[],
        None,
    )
    .unwrap_or_default();
    let listing = String::from_utf8_lossy(&listing);
    let found = listing.split('\0').find_map(|entry| {
        let (key, url) = entry.split_once('\n')?;
        let name = key.strip_prefix("remote.")?.strip_suffix(".url")?;
        crate::origin_repository::parse(url, host)
            .filter(|repo| repo.eq_ignore_ascii_case(head_repository))
            .map(|_| (name.to_owned(), url.to_owned()))
    });
    let Some((name, configured_url)) = found else {
        return Err(format!(
            "No remote of this checkout points at {head_repository} on {host}. Add one (git remote add <name> <url>) and plan again; nothing was written."
        ));
    };
    let url = |push: bool| {
        let mut args = vec!["remote", "get-url"];
        if push {
            args.push("--push");
        }
        args.push(&name);
        git(bounds, repository, &args, &[], None)
            .map(text)
            .map_err(|error| error.to_string())
    };
    Ok(PushRemote {
        fetch_url: url(false)?,
        push_url: url(true)?,
        name,
        configured_url,
    })
}

/// The checkout's configured `user.name` and `user.email`; never guessed.
pub fn committer_identity(
    repository: &Path,
    cancel: Option<&CancellationToken>,
    deadline: Option<Instant>,
) -> Result<(String, String), String> {
    let bounds = Bounds { cancel, deadline };
    let read = |key: &str| {
        git(bounds, repository, &["config", "--get", key], &[], None)
            .map(text)
            .ok()
            .filter(|value| !value.is_empty())
    };
    match (read("user.name"), read("user.email")) {
        (Some(name), Some(email)) => Ok((name, email)),
        _ => Err(
            "No commit identity: set git config user.name and user.email in this checkout. Nothing was written."
                .into(),
        ),
    }
}

/// Builds the suggestion commit in a private repository and, when `push` is
/// set, pushes it with a lease on `expected_head`.
pub fn apply_isolated(request: &ApplyRequest<'_>) -> Result<SuggestionPlan, SuggestionError> {
    let bounds = Bounds {
        cancel: request.cancel,
        deadline: request.deadline,
    };
    if request.suggestions.is_empty() {
        return Err(SuggestionError::Range("no suggestions selected".into()));
    }
    if request.committer.0.trim().is_empty() || request.committer.1.trim().is_empty() {
        return Err(SuggestionError::Git(
            "no commit identity (user.name and user.email) is configured".into(),
        ));
    }
    let objects = text(git(
        bounds,
        request.repository,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "objects",
        ],
        &[],
        None,
    )?);
    let scratch = Scratch(
        match request.scratch_in {
            Some(parent) => tempfile::Builder::new()
                .prefix("ghtui-suggestion-")
                .tempdir_in(parent),
            None => tempfile::Builder::new()
                .prefix("ghtui-suggestion-")
                .tempdir(),
        }
        .map_err(|error| SuggestionError::Git(format!("temporary repository: {error}")))?
        .keep(),
    );
    let dir = scratch.0.as_path();
    git(bounds, dir, &["init", "--quiet", "--bare", "."], &[], None)?;
    std::fs::write(dir.join("objects/info/alternates"), format!("{objects}\n"))
        .map_err(|error| SuggestionError::Git(format!("alternates: {error}")))?;
    let refname = format!("refs/heads/{}", request.branch);
    git(
        bounds,
        dir,
        &[
            "fetch",
            "--quiet",
            "--no-tags",
            "--no-write-fetch-head",
            request.remote,
            &format!("+{refname}:refs/ghtui/suggestion-head"),
        ],
        &[],
        None,
    )?;
    let actual = text(git(
        bounds,
        dir,
        &[
            "rev-parse",
            "--verify",
            "refs/ghtui/suggestion-head^{commit}",
        ],
        &[],
        None,
    )?);
    if actual != request.expected_head {
        return Err(SuggestionError::OutdatedHead {
            expected: request.expected_head.into(),
            actual,
        });
    }
    // Posted suggestions carry the lines their author saw: read them from
    // that commit, never from the caller.
    let mut resolved = request.suggestions.to_vec();
    for suggestion in &mut resolved {
        let Some(source) = &suggestion.original_source else {
            continue;
        };
        let spec = format!("{}:{}", source.commit, suggestion.path);
        let bytes =
            git(bounds, dir, &["cat-file", "blob", &spec], &[], None).map_err(
                |error| match error {
                    SuggestionError::Git(_) => SuggestionError::Range(format!(
                        "{} is not readable at the suggestion's commit {}",
                        suggestion.path, source.commit
                    )),
                    other => other,
                },
            )?;
        let lines = display_lines(&bytes);
        if source.start == 0 || source.start > source.end || source.end as usize > lines.len() {
            return Err(SuggestionError::Range(format!(
                "{} lines {}–{} are outside the suggestion's commit",
                suggestion.path, source.start, source.end
            )));
        }
        suggestion.original = lines[source.start as usize - 1..source.end as usize].to_vec();
    }
    let mut by_path: BTreeMap<&str, Vec<&Suggestion>> = BTreeMap::new();
    for suggestion in &resolved {
        by_path
            .entry(&suggestion.path)
            .or_default()
            .push(suggestion);
    }
    let index = dir.join("ghtui-index");
    let index_path = index.to_string_lossy().into_owned();
    let index_env = [("GIT_INDEX_FILE", index_path.as_str())];
    git(bounds, dir, &["read-tree", &actual], &index_env, None)?;
    let mut files = vec![];
    let mut preview = vec![];
    for (&path, suggestions) in &by_path {
        let listing = git(
            bounds,
            dir,
            &["ls-tree", "-z", &actual, "--", path],
            &[],
            None,
        )?;
        let listing = String::from_utf8_lossy(&listing);
        let entry = listing.trim_end_matches('\0');
        let Some((meta, _)) = entry.split_once('\t') else {
            return Err(SuggestionError::Range(format!(
                "{path} does not exist at the PR head"
            )));
        };
        let fields = meta.split(' ').collect::<Vec<_>>();
        if fields.len() != 3 || fields[1] != "blob" || !matches!(fields[0], "100644" | "100755") {
            return Err(SuggestionError::Range(format!(
                "{path} is not a regular file at the PR head"
            )));
        }
        let original = git(bounds, dir, &["cat-file", "blob", fields[2]], &[], None)?;
        let change = resulting_change(path, &original, suggestions)?;
        let blob = text(git(
            bounds,
            dir,
            &["hash-object", "-w", "--stdin"],
            &[],
            Some(change.bytes.as_slice()),
        )?);
        git(
            bounds,
            dir,
            &[
                "update-index",
                "--cacheinfo",
                &format!("{},{blob},{path}", fields[0]),
            ],
            &index_env,
            None,
        )?;
        files.push((path.to_owned(), blob));
        preview.extend(change.preview);
    }
    let tree = text(git(bounds, dir, &["write-tree"], &index_env, None)?);
    let mut message = request.subject.trim().to_owned();
    let mut trailers = vec![];
    let mut attribution_limits = vec![];
    for suggestion in request.suggestions {
        let Some(author) = &suggestion.author else {
            attribution_limits
                .push("A suggestion has no known author; no co-author trailer was added.".into());
            continue;
        };
        match co_author_trailer(author) {
            Some(trailer) if !trailers.contains(&trailer) => trailers.push(trailer),
            Some(_) => {}
            None => attribution_limits.push(format!(
                "@{} has no known numeric account; no co-author trailer was added.",
                author.login
            )),
        }
    }
    attribution_limits.dedup();
    if !trailers.is_empty() {
        message.push_str("\n\n");
        message.push_str(&trailers.join("\n"));
    }
    message.push('\n');
    let (name, email) = request.committer;
    let mut identity = vec![
        ("GIT_AUTHOR_NAME", name),
        ("GIT_AUTHOR_EMAIL", email),
        ("GIT_COMMITTER_NAME", name),
        ("GIT_COMMITTER_EMAIL", email),
    ];
    if let Some(date) = request.date {
        identity.push(("GIT_AUTHOR_DATE", date));
        identity.push(("GIT_COMMITTER_DATE", date));
    }
    let commit = text(git(
        bounds,
        dir,
        &["commit-tree", &tree, "-p", &actual, "-F", "-"],
        &identity,
        Some(message.as_bytes()),
    )?);
    if request
        .expected_commit
        .is_some_and(|expected| expected != commit)
    {
        return Err(SuggestionError::Git(format!(
            "the rebuilt commit {commit} differs from the previewed one; preview again"
        )));
    }
    let mut pushed = false;
    if request.push {
        let push_to = request.push_to.unwrap_or(request.remote);
        let lease = format!("--force-with-lease={refname}:{actual}");
        let attempt = git(
            bounds,
            dir,
            &[
                "push",
                "--quiet",
                "--porcelain",
                &lease,
                push_to,
                &format!("{commit}:{refname}"),
            ],
            &[],
            None,
        );
        // Read the branch back on every outcome. Only this readback decides
        // whether the write happened; it ignores cancellation so a cancelled
        // push still gets an answer, within its own short limit.
        let readback = git(
            Bounds {
                cancel: None,
                deadline: Some(Instant::now() + Duration::from_secs(20)),
            },
            dir,
            &["ls-remote", "--", push_to, &refname],
            &[],
            None,
        )
        .map(|bytes| {
            text(bytes)
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned()
        });
        match (attempt, readback) {
            (_, Ok(remote)) if remote == commit => pushed = true,
            (Ok(_), Ok(remote)) => {
                return Err(SuggestionError::Uncertain(format!(
                    "the push reported success but the branch reads {remote}"
                )))
            }
            (Err(SuggestionError::Git(message)), Ok(_)) => {
                return Err(SuggestionError::PushRejected(message))
            }
            (Err(stop @ (SuggestionError::Cancelled | SuggestionError::TimedOut)), Ok(remote))
                if remote == actual =>
            {
                return Err(stop)
            }
            (Err(error), Ok(_)) => return Err(SuggestionError::PushRejected(error.to_string())),
            (_, Err(error)) => {
                return Err(SuggestionError::Uncertain(format!(
                    "the branch could not be read back ({error})"
                )))
            }
        }
    }
    drop(scratch);
    Ok(SuggestionPlan {
        expected_head: actual,
        commit,
        tree,
        files,
        preview,
        message,
        attribution_limits,
        pushed,
    })
}

#[cfg(test)]
#[path = "suggestions_tests.rs"]
mod tests;
