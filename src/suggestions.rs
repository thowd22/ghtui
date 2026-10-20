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
use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
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
}

fn git(
    dir: &Path,
    args: &[&str],
    env: &[(&str, &str)],
    stdin: Option<&[u8]>,
) -> Result<Vec<u8>, SuggestionError> {
    let mut command = Command::new("git");
    command
        .arg("--no-pager")
        .arg("--literal-pathspecs")
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
        pipe.write_all(input)
            .map_err(|error| SuggestionError::Git(format!("git stdin: {error}")))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| SuggestionError::Git(format!("git: {error}")))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(SuggestionError::Git(format!(
            "git {}: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&output.stderr).trim()
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

/// Builds the suggestion commit in a private repository and, when `push` is
/// set, pushes it with a lease on `expected_head`.
pub fn apply_isolated(request: &ApplyRequest<'_>) -> Result<SuggestionPlan, SuggestionError> {
    if request.suggestions.is_empty() {
        return Err(SuggestionError::Range("no suggestions selected".into()));
    }
    if request.committer.0.trim().is_empty() || request.committer.1.trim().is_empty() {
        return Err(SuggestionError::Git(
            "no commit identity (user.name and user.email) is configured".into(),
        ));
    }
    let objects = text(git(
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
    git(dir, &["init", "--quiet", "--bare", "."], &[], None)?;
    std::fs::write(dir.join("objects/info/alternates"), format!("{objects}\n"))
        .map_err(|error| SuggestionError::Git(format!("alternates: {error}")))?;
    let refname = format!("refs/heads/{}", request.branch);
    let fetched = git(
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
    );
    fetched?;
    let actual = text(git(
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
    let mut by_path: BTreeMap<&str, Vec<&Suggestion>> = BTreeMap::new();
    for suggestion in request.suggestions {
        by_path
            .entry(&suggestion.path)
            .or_default()
            .push(suggestion);
    }
    let index = dir.join("ghtui-index");
    let index_path = index.to_string_lossy().into_owned();
    let index_env = [("GIT_INDEX_FILE", index_path.as_str())];
    git(dir, &["read-tree", &actual], &index_env, None)?;
    let mut files = vec![];
    let mut preview = vec![];
    for (&path, suggestions) in &by_path {
        let listing = git(dir, &["ls-tree", "-z", &actual, "--", path], &[], None)?;
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
        let original = git(dir, &["cat-file", "blob", fields[2]], &[], None)?;
        let change = resulting_change(path, &original, suggestions)?;
        let blob = text(git(
            dir,
            &["hash-object", "-w", "--stdin"],
            &[],
            Some(change.bytes.as_slice()),
        )?);
        git(
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
    let tree = text(git(dir, &["write-tree"], &index_env, None)?);
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
    let identity = [
        ("GIT_AUTHOR_NAME", name),
        ("GIT_AUTHOR_EMAIL", email),
        ("GIT_COMMITTER_NAME", name),
        ("GIT_COMMITTER_EMAIL", email),
    ];
    let commit = text(git(
        dir,
        &["commit-tree", &tree, "-p", &actual, "-F", "-"],
        &identity,
        Some(message.as_bytes()),
    )?);
    let mut pushed = false;
    if request.push {
        let lease = format!("--force-with-lease={refname}:{actual}");
        git(
            dir,
            &[
                "push",
                "--quiet",
                "--porcelain",
                &lease,
                request.remote,
                &format!("{commit}:{refname}"),
            ],
            &[],
            None,
        )
        .map_err(|error| match error {
            SuggestionError::Git(message) => SuggestionError::PushRejected(message),
            other => other,
        })?;
        pushed = true;
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
