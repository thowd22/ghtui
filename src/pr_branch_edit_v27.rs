//! TASK-27: edit a PR branch file, or resolve the PR's conflicts with its
//! base, in an isolated revision.
//!
//! From the PR Files tab, `E Edit file` (the selected file) or `R Resolve
//! conflicts` (every file the merge with the base leaves conflicted) prepares
//! a private repository under ghtui's data directory. It borrows the
//! checkout's objects through `objects/info/alternates` and holds its own
//! copy of the PR head; the user's working tree, index, refs and config are
//! never touched. The editable files live beside it, so they survive editor
//! failures, a moved head, cancellation and restarts until the user discards
//! them or a push is confirmed.
//!
//! `e Edit` opens the file in `$VISUAL`/`$EDITOR`. `p Plan` builds the exact
//! commit (one parent for an edit; the head and the base for a resolution)
//! and shows its diff against the PR head. `C Commit & push`, then
//! `y Confirm push`, pushes that commit to the PR head branch without any
//! force: the branch must still point at the previewed head, and Git refuses
//! anything that is not a fast-forward. The push is recorded in the PR write
//! journal before it starts and closed with its read-back outcome.
//!
//! TASK-108: the plan step also reads the head repository's three-state
//! permission flags (one `gh api repos/{head}` read) and shows push
//! permission as granted, denied or unknown. Denied disables `y Confirm push`
//! with the reason; unknown says the remote decides.
use crate::{
    app::App,
    identity::ActorIdentity,
    pr_write_journal::{
        PrConfirmedEffect, PrEffectKind, PrWriteIntent, PrWriteJournal, PrWriteTarget,
    },
    providers::runner::CancellationToken,
    suggestions::{
        committer_identity, git, git_output, rejection_reason, resolve_push_remote, text, Bounds,
        PushRemote, SuggestionError,
    },
    theme::SemanticToken,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Margin, Rect},
    text::Line,
    widgets::{Clear, Paragraph},
    Frame,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const PREPARE_LIMIT: Duration = Duration::from_secs(60);
pub const PLAN_LIMIT: Duration = Duration::from_secs(60);
pub const PUSH_LIMIT: Duration = Duration::from_secs(120);
pub const JOURNAL_EDIT: &str = "branch-edit-v27";
pub const JOURNAL_RESOLVE: &str = "branch-resolve-v27";
/// The pane title; PTY journeys find the pane by it.
pub const TITLE: &str = "PR branch edit";
/// Largest file this flow edits.
pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
/// Longest diff preview kept for display.
pub const MAX_DIFF_LINES: usize = 4000;
const MAX_MESSAGE_CHARS: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditModeV27 {
    /// Edit one file of the PR head.
    File(String),
    /// Merge the base into the head and resolve every conflicted file.
    Resolve,
}

/// What the PR observation says about the branch, and where work is kept.
#[derive(Clone, Debug)]
pub struct EditContextV27 {
    pub host: String,
    pub base_repository: String,
    pub head_repository: Option<String>,
    pub number: u64,
    pub branch: String,
    pub head: String,
    pub base_ref: String,
    pub base: String,
    pub checkout: Option<PathBuf>,
    pub target: Option<PrWriteTarget>,
    pub mode: EditModeV27,
}

/// One editable file of the isolated revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditFileV27 {
    pub path: String,
    pub mode: String,
    /// The bytes the edit started from: the head's file, or the merge result
    /// with conflict markers.
    pub original: Vec<u8>,
    pub work: PathBuf,
    pub conflicted: bool,
    /// The work file already existed (kept from an earlier attempt).
    pub reused: bool,
}

#[derive(Clone, Debug)]
pub struct PreparedV27 {
    /// The PR's private workspace: `repo.git`, `files/` and `base-head`.
    pub dir: PathBuf,
    pub repo: PathBuf,
    pub remote: PushRemote,
    pub head: String,
    pub base: Option<String>,
    /// The tree the edit starts from: the head's, or the merge result's.
    pub tree: String,
    pub files: Vec<EditFileV27>,
}

#[derive(Clone, Debug)]
pub struct PlanV27 {
    pub commit: String,
    pub tree: String,
    pub parents: Vec<String>,
    pub files: Vec<(String, String)>,
    pub diff: Vec<String>,
    pub message: String,
    pub committer: (String, String),
    pub date: String,
    /// TASK-108: what GitHub reports about pushing to the head repository.
    /// `plan` leaves it unknown; the plan step reads it before showing it.
    pub permission: PushPermissionV27,
}

/// TASK-108: the three states of push permission on the head repository.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionStateV27 {
    /// GitHub reports push, maintain or admin access.
    Granted,
    /// GitHub reports no push access: Confirm push is disabled.
    Denied,
    /// Not reported, not readable or decided elsewhere: the remote decides.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushPermissionV27 {
    pub state: PermissionStateV27,
    pub repository: String,
    /// Why, and what happens next.
    pub reason: String,
}

const REMOTE_DECIDES: &str =
    "the remote decides with your Git credentials; a refusal changes nothing";

impl PushPermissionV27 {
    pub fn unknown(repository: &str, why: &str) -> Self {
        Self {
            state: PermissionStateV27::Unknown,
            repository: repository.to_owned(),
            reason: format!("{why} • {REMOTE_DECIDES}"),
        }
    }
    /// The plan row: `Push permission: granted|denied|unknown on <repo>`.
    pub fn verdict(&self) -> String {
        let state = match self.state {
            PermissionStateV27::Granted => "granted",
            PermissionStateV27::Denied => "denied",
            PermissionStateV27::Unknown => "unknown",
        };
        format!("Push permission: {state} on {}", self.repository)
    }
    pub fn role(&self) -> SemanticToken {
        match self.state {
            PermissionStateV27::Granted => SemanticToken::Success,
            PermissionStateV27::Denied => SemanticToken::Failure,
            PermissionStateV27::Unknown => SemanticToken::Pending,
        }
    }
    pub fn denied(&self) -> bool {
        self.state == PermissionStateV27::Denied
    }
}

/// TASK-108: classifies the head repository's three-state flags
/// (pull, push, maintain, admin). A missing flag is unknown, never denied. A
/// fork that reports no push access stays unknown too: the PR's "allow edits
/// by maintainers" setting may still let the push through.
pub fn push_permission(
    head_repository: Option<&str>,
    base_repository: &str,
    flags: Result<[Option<bool>; 4], String>,
) -> PushPermissionV27 {
    let Some(repository) = head_repository else {
        return PushPermissionV27::unknown(
            "the head repository",
            "GitHub reports no head repository",
        );
    };
    let [_, push, maintain, admin] = match flags {
        Ok(flags) => flags,
        Err(error) => {
            let error = error.split_whitespace().collect::<Vec<_>>().join(" ");
            return PushPermissionV27::unknown(
                repository,
                &format!("GitHub could not be read ({error})"),
            );
        }
    };
    let via = [("admin", admin), ("maintain", maintain), ("push", push)]
        .into_iter()
        .find(|(_, flag)| *flag == Some(true))
        .map(|(name, _)| name);
    if let Some(via) = via {
        return PushPermissionV27 {
            state: PermissionStateV27::Granted,
            repository: repository.to_owned(),
            reason: format!(
                "GitHub reports {via} access for your account • the push still uses your Git credentials"
            ),
        };
    }
    match push {
        Some(false) if repository.eq_ignore_ascii_case(base_repository) => PushPermissionV27 {
            state: PermissionStateV27::Denied,
            repository: repository.to_owned(),
            reason: "GitHub reports no push access for your account • y Confirm push is disabled; nothing will be pushed".into(),
        },
        Some(false) => PushPermissionV27::unknown(
            repository,
            "GitHub reports no push access to this fork, but maintainer edits may allow it",
        ),
        _ => PushPermissionV27::unknown(repository, "GitHub did not report push access"),
    }
}

/// TASK-108: reads the head repository's permission flags through `gh` (one
/// read, nothing written).
pub fn read_push_permission(
    context: &EditContextV27,
    cancel: &CancellationToken,
) -> PushPermissionV27 {
    let flags = context.head_repository.as_deref().map(|repository| {
        crate::providers::gh::GithubProvider::new(&context.host)
            .and_then(|provider| provider.repository_permission_flags_v108(repository, cancel))
            .map_err(|error| error.to_string())
    });
    push_permission(
        context.head_repository.as_deref(),
        &context.base_repository,
        flags.unwrap_or(Ok([None; 4])),
    )
}

/// Why a step stopped, the semantic state it shows, and whether the work
/// files are still in place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepFailureV27 {
    pub message: String,
    pub role: SemanticToken,
}
impl StepFailureV27 {
    fn new(message: impl Into<String>, role: SemanticToken) -> Self {
        Self {
            message: message.into(),
            role,
        }
    }
}
impl From<String> for StepFailureV27 {
    fn from(message: String) -> Self {
        Self::new(message, SemanticToken::Failure)
    }
}
impl From<&str> for StepFailureV27 {
    fn from(message: &str) -> Self {
        Self::new(message, SemanticToken::Failure)
    }
}
impl From<SuggestionError> for StepFailureV27 {
    fn from(error: SuggestionError) -> Self {
        let role = match error {
            SuggestionError::Conflict { .. } | SuggestionError::OutdatedHead { .. } => {
                SemanticToken::Conflict
            }
            SuggestionError::Cancelled => SemanticToken::Cancelled,
            _ => SemanticToken::Failure,
        };
        Self::new(error.to_string(), role)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PushOutcomeV27 {
    Confirmed { commit: String, attempt: i64 },
    NotWritten(String),
    Uncertain(String),
    Refused(String),
}

/// A path inside the repository that is safe to mirror under `files/`.
pub fn validate_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\0')
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == ".." || part == ".git")
    {
        return Err(format!(
            "{path:?} is not a plain repository path; it cannot be edited here. Nothing was written."
        ));
    }
    Ok(())
}

/// The private workspace directory for one PR and edit.
pub fn workspace_dir(root: &Path, context: &EditContextV27) -> PathBuf {
    let repository = context.base_repository.replace('/', "__");
    let kind = match &context.mode {
        EditModeV27::File(path) => {
            let digest = format!("{:x}", Sha256::digest(path.as_bytes()));
            format!("edit-{}", &digest[..12])
        }
        EditModeV27::Resolve => "resolve".to_owned(),
    };
    root.join(format!("{repository}-pr{}-{kind}", context.number))
}

/// Whether `bytes` still carries a conflict marker line; returns its line.
pub fn marker_line(bytes: &[u8]) -> Option<usize> {
    bytes
        .split(|byte| *byte == b'\n')
        .position(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            line.starts_with(b"<<<<<<< ")
                || line == b"<<<<<<<"
                || line.starts_with(b">>>>>>> ")
                || line == b">>>>>>>"
                || line == b"======="
                || line.starts_with(b"||||||| ")
        })
        .map(|index| index + 1)
}

/// A diff line safe to paint: a carriage return shows as `␍`, a tab as four
/// spaces and any other control character as `�`.
pub fn display_line(line: &str) -> String {
    let mut shown = String::with_capacity(line.len());
    for character in line.chars() {
        match character {
            '\r' => shown.push('␍'),
            '\t' => shown.push_str("    "),
            other if other.is_control() => shown.push('\u{FFFD}'),
            other => shown.push(other),
        }
    }
    shown
}

fn read_limited(path: &Path) -> Result<Vec<u8>, StepFailureV27> {
    let bytes = std::fs::read(path).map_err(|error| {
        StepFailureV27::from(format!(
            "Cannot read the edited file {} ({error}). Nothing was written.",
            path.display()
        ))
    })?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!(
            "{} is larger than {} MiB; it cannot be committed here. Nothing was written.",
            path.display(),
            MAX_FILE_BYTES / 1024 / 1024
        )
        .into());
    }
    Ok(bytes)
}

/// The regular file `path` in `tree`: `(mode, blob)`.
fn regular_file(
    bounds: Bounds<'_>,
    repo: &Path,
    tree: &str,
    path: &str,
) -> Result<Option<(String, String)>, SuggestionError> {
    let listing = git(
        bounds,
        repo,
        &["ls-tree", "-z", tree, "--", path],
        &[],
        None,
    )?;
    let listing = String::from_utf8_lossy(&listing);
    let entry = listing.trim_end_matches('\0');
    let Some((meta, _)) = entry.split_once('\t') else {
        return Ok(None);
    };
    let fields = meta.split(' ').collect::<Vec<_>>();
    if fields.len() != 3 || fields[1] != "blob" || !matches!(fields[0], "100644" | "100755") {
        return Ok(None);
    }
    Ok(Some((fields[0].to_owned(), fields[2].to_owned())))
}

/// Prepares (or reopens) the isolated revision: fetches the PR head into the
/// private repository, checks it is the observed head, and writes the files
/// to edit unless earlier work is kept there.
pub fn prepare(
    context: &EditContextV27,
    root: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedV27, StepFailureV27> {
    let deadline = Some(Instant::now() + PREPARE_LIMIT);
    let bounds = Bounds {
        cancel: Some(cancel),
        deadline,
    };
    let checkout = context.checkout.as_deref().ok_or(
        "No local checkout is open; run ghtui inside a clone of this repository. Nothing was written.",
    )?;
    let head_repository = context.head_repository.as_deref().ok_or(
        "GitHub reports no head repository for this PR (deleted fork?). Nothing was written.",
    )?;
    if let EditModeV27::File(path) = &context.mode {
        validate_path(path)?;
    }
    let remote = resolve_push_remote(
        checkout,
        &context.host,
        head_repository,
        Some(cancel),
        deadline,
    )?;
    let dir = workspace_dir(root, context);
    let repo = dir.join("repo.git");
    let files_dir = dir.join("files");
    std::fs::create_dir_all(&dir).map_err(|error| {
        StepFailureV27::from(format!(
            "Cannot create the isolated workspace {} ({error}). Nothing was written.",
            dir.display()
        ))
    })?;
    if !repo.join("HEAD").exists() {
        git(
            bounds,
            &dir,
            &["init", "--quiet", "--bare", "repo.git"],
            &[],
            None,
        )?;
    }
    let objects = text(git(
        bounds,
        checkout,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "objects",
        ],
        &[],
        None,
    )?);
    std::fs::write(repo.join("objects/info/alternates"), format!("{objects}\n"))
        .map_err(|error| StepFailureV27::from(format!("alternates: {error}")))?;
    git(
        bounds,
        &repo,
        &[
            "fetch",
            "--quiet",
            "--no-tags",
            "--no-write-fetch-head",
            &remote.fetch_url,
            &format!("+refs/heads/{}:refs/ghtui/edit-head", context.branch),
        ],
        &[],
        None,
    )?;
    let actual = text(git(
        bounds,
        &repo,
        &["rev-parse", "--verify", "refs/ghtui/edit-head^{commit}"],
        &[],
        None,
    )?);
    if actual != context.head {
        return Err(StepFailureV27::new(
            format!(
                "The PR head moved from {} to {actual}. Refresh the PR and open the edit again; nothing was written.",
                context.head
            ),
            SemanticToken::Conflict,
        ));
    }
    let recorded = std::fs::read_to_string(dir.join("base-head"))
        .ok()
        .map(|value| value.trim().to_owned());
    if let Some(recorded) = recorded.as_deref() {
        if recorded != actual && files_dir.exists() {
            return Err(StepFailureV27::new(
                format!(
                    "Your kept edit in {} was made on head {recorded}, but the PR head is now {actual}. Copy what you need from it, then D Discard to start again from the new head. Nothing was written.",
                    files_dir.display()
                ),
                SemanticToken::Conflict,
            ));
        }
    }
    let (tree, base, wanted) = match &context.mode {
        EditModeV27::File(path) => {
            let Some((mode, blob)) = regular_file(bounds, &repo, &actual, path)? else {
                return Err(format!(
                    "{path} is not a regular file at the PR head; only existing text files can be edited here. Nothing was written."
                )
                .into());
            };
            let tree = text(git(
                bounds,
                &repo,
                &["rev-parse", &format!("{actual}^{{tree}}")],
                &[],
                None,
            )?);
            (tree, None, vec![(path.clone(), mode, blob, false)])
        }
        EditModeV27::Resolve => {
            let base = base_commit(bounds, context, checkout, &repo, cancel, deadline)?;
            let (clean, stdout, stderr) = git_output(
                bounds,
                &repo,
                &[
                    "merge-tree",
                    "--write-tree",
                    "-z",
                    "--name-only",
                    "--no-messages",
                    &actual,
                    &base,
                ],
                &[],
                None,
            )?;
            let output = String::from_utf8_lossy(&stdout).into_owned();
            let mut fields = output.split('\0');
            let tree = fields.next().unwrap_or_default().trim().to_owned();
            if tree.is_empty() {
                return Err(format!(
                    "Git could not merge {} into the head: {}. Nothing was written.",
                    context.base_ref,
                    String::from_utf8_lossy(&stderr).trim()
                )
                .into());
            }
            if clean {
                return Err(StepFailureV27::new(
                    format!(
                        "No conflicts: {} merges cleanly into {}. Nothing to resolve; nothing was written.",
                        context.base_ref, context.branch
                    ),
                    SemanticToken::Success,
                ));
            }
            let mut paths: Vec<String> = vec![];
            for path in fields.take_while(|field| !field.is_empty()) {
                if !paths.iter().any(|known| known == path) {
                    paths.push(path.to_owned());
                }
            }
            let mut wanted = vec![];
            for path in paths {
                validate_path(&path)?;
                let Some((mode, blob)) = regular_file(bounds, &repo, &tree, &path)? else {
                    return Err(StepFailureV27::new(
                        format!(
                            "{path} conflicts in a way that is not file content (deleted, renamed or a mode change on one side). Resolve it in a local clone; nothing was written."
                        ),
                        SemanticToken::Conflict,
                    ));
                };
                wanted.push((path, mode, blob, true));
            }
            (tree, Some(base), wanted)
        }
    };
    let mut files = vec![];
    for (path, mode, blob, conflicted) in wanted {
        let original = git(bounds, &repo, &["cat-file", "blob", &blob], &[], None)?;
        if original.len() > MAX_FILE_BYTES || original.contains(&0) {
            return Err(format!(
                "{path} is binary or larger than {} MiB; it cannot be edited here. Nothing was written.",
                MAX_FILE_BYTES / 1024 / 1024
            )
            .into());
        }
        let work = files_dir.join(&path);
        let reused = work.is_file();
        if !reused {
            if let Some(parent) = work.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| StepFailureV27::from(format!("{path}: {error}")))?;
            }
            std::fs::write(&work, &original)
                .map_err(|error| StepFailureV27::from(format!("{path}: {error}")))?;
        }
        files.push(EditFileV27 {
            path,
            mode,
            original,
            work,
            conflicted,
            reused,
        });
    }
    std::fs::write(dir.join("base-head"), format!("{actual}\n"))
        .map_err(|error| StepFailureV27::from(format!("base-head: {error}")))?;
    Ok(PreparedV27 {
        dir,
        repo,
        remote,
        head: actual,
        base,
        tree,
        files,
    })
}

/// The observed base commit, from the borrowed objects or fetched from the
/// base repository's remote. A base that moved since the observation is
/// refused.
fn base_commit(
    bounds: Bounds<'_>,
    context: &EditContextV27,
    checkout: &Path,
    repo: &Path,
    cancel: &CancellationToken,
    deadline: Option<Instant>,
) -> Result<String, StepFailureV27> {
    let spec = format!("{}^{{commit}}", context.base);
    if git(bounds, repo, &["cat-file", "-e", &spec], &[], None).is_ok() {
        return Ok(context.base.clone());
    }
    let remote = resolve_push_remote(
        checkout,
        &context.host,
        &context.base_repository,
        Some(cancel),
        deadline,
    )?;
    git(
        bounds,
        repo,
        &[
            "fetch",
            "--quiet",
            "--no-tags",
            "--no-write-fetch-head",
            &remote.fetch_url,
            &format!("+refs/heads/{}:refs/ghtui/edit-base", context.base_ref),
        ],
        &[],
        None,
    )?;
    let actual = text(git(
        bounds,
        repo,
        &["rev-parse", "--verify", "refs/ghtui/edit-base^{commit}"],
        &[],
        None,
    )?);
    if actual != context.base {
        return Err(StepFailureV27::new(
            format!(
                "The base branch {} moved from {} to {actual}. Refresh the PR and resolve again; nothing was written.",
                context.base_ref, context.base
            ),
            SemanticToken::Conflict,
        ));
    }
    Ok(actual)
}

/// Builds the exact commit from the work files (nothing is pushed) and the
/// diff it makes against the PR head.
pub fn plan(
    prepared: &PreparedV27,
    message: &str,
    committer: (&str, &str),
    date: &str,
    cancel: &CancellationToken,
) -> Result<PlanV27, StepFailureV27> {
    let bounds = Bounds {
        cancel: Some(cancel),
        deadline: Some(Instant::now() + PLAN_LIMIT),
    };
    let message = message.trim();
    if message.is_empty() {
        return Err(StepFailureV27::new(
            "The commit message is empty. m Message to write one; nothing was written.",
            SemanticToken::Muted,
        ));
    }
    if committer.0.trim().is_empty() || committer.1.trim().is_empty() {
        return Err("No commit identity (user.name and user.email). Nothing was written.".into());
    }
    let mut contents = vec![];
    for file in &prepared.files {
        let bytes = read_limited(&file.work)?;
        if file.conflicted {
            if let Some(line) = marker_line(&bytes) {
                return Err(StepFailureV27::new(
                    format!(
                        "{} still has a conflict marker on line {line}. e Edit to finish resolving it; nothing was written.",
                        file.path
                    ),
                    SemanticToken::Conflict,
                ));
            }
        }
        contents.push(bytes);
    }
    if prepared.base.is_none()
        && prepared
            .files
            .iter()
            .zip(&contents)
            .all(|(file, bytes)| file.original == *bytes)
    {
        return Err(StepFailureV27::new(
            "No change: the edited file matches the PR head. e Edit first; nothing was written.",
            SemanticToken::Muted,
        ));
    }
    let index = prepared.dir.join("plan-index");
    let _ = std::fs::remove_file(&index);
    let index_path = index.to_string_lossy().into_owned();
    let index_env = [("GIT_INDEX_FILE", index_path.as_str())];
    let repo = prepared.repo.as_path();
    git(
        bounds,
        repo,
        &["read-tree", &prepared.tree],
        &index_env,
        None,
    )?;
    let mut files = vec![];
    for (file, bytes) in prepared.files.iter().zip(&contents) {
        let blob = text(git(
            bounds,
            repo,
            &["hash-object", "-w", "--stdin"],
            &[],
            Some(bytes.as_slice()),
        )?);
        git(
            bounds,
            repo,
            &[
                "update-index",
                "--cacheinfo",
                &format!("{},{blob},{}", file.mode, file.path),
            ],
            &index_env,
            None,
        )?;
        files.push((file.path.clone(), blob));
    }
    let tree = text(git(bounds, repo, &["write-tree"], &index_env, None)?);
    let _ = std::fs::remove_file(&index);
    let mut parents = vec![prepared.head.clone()];
    parents.extend(prepared.base.clone());
    let mut args = vec!["commit-tree".to_owned(), tree.clone()];
    for parent in &parents {
        args.push("-p".into());
        args.push(parent.clone());
    }
    args.extend(["-F".into(), "-".into()]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (name, email) = committer;
    let identity = [
        ("GIT_AUTHOR_NAME", name),
        ("GIT_AUTHOR_EMAIL", email),
        ("GIT_COMMITTER_NAME", name),
        ("GIT_COMMITTER_EMAIL", email),
        ("GIT_AUTHOR_DATE", date),
        ("GIT_COMMITTER_DATE", date),
    ];
    let body = format!("{message}\n");
    let commit = text(git(bounds, repo, &args, &identity, Some(body.as_bytes()))?);
    let diff = git(
        bounds,
        repo,
        &[
            "diff-tree",
            "-p",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            &prepared.head,
            &commit,
        ],
        &[],
        None,
    )?;
    let diff = String::from_utf8_lossy(&diff)
        .lines()
        .take(MAX_DIFF_LINES)
        .map(display_line)
        .collect();
    Ok(PlanV27 {
        commit,
        tree,
        parents,
        files,
        diff,
        message: body,
        committer: (name.to_owned(), email.to_owned()),
        date: date.to_owned(),
        permission: PushPermissionV27::unknown("the head repository", "Not read yet"),
    })
}

/// Pushes the planned commit to the PR head branch with no force: only when
/// the work files are unchanged since the plan and the branch still points
/// at the previewed head. The branch is read back on every outcome.
pub fn push_branch(
    context: &EditContextV27,
    prepared: &PreparedV27,
    planned: &PlanV27,
    cancel: &CancellationToken,
) -> Result<(), SuggestionError> {
    let bounds = Bounds {
        cancel: Some(cancel),
        deadline: Some(Instant::now() + PUSH_LIMIT),
    };
    let repo = prepared.repo.as_path();
    for (file, (_, blob)) in prepared.files.iter().zip(&planned.files) {
        let bytes = std::fs::read(&file.work)
            .map_err(|error| SuggestionError::Git(format!("{}: {error}", file.path)))?;
        let now = text(git(
            bounds,
            repo,
            &["hash-object", "--stdin"],
            &[],
            Some(bytes.as_slice()),
        )?);
        if &now != blob {
            return Err(SuggestionError::Range(format!(
                "{} changed after the plan; p Plan again to see what would be pushed",
                file.path
            )));
        }
    }
    let refname = format!("refs/heads/{}", context.branch);
    let push_to = prepared.remote.push_url.as_str();
    let observed = text(git(
        bounds,
        repo,
        &["ls-remote", "--", push_to, &refname],
        &[],
        None,
    )?)
    .split_whitespace()
    .next()
    .unwrap_or_default()
    .to_owned();
    if observed != prepared.head {
        return Err(SuggestionError::OutdatedHead {
            expected: prepared.head.clone(),
            actual: if observed.is_empty() {
                "a deleted branch".into()
            } else {
                observed
            },
        });
    }
    // No `+`, no `--force`, no lease: Git refuses anything but a fast-forward.
    let attempt = git(
        bounds,
        repo,
        &[
            "push",
            "--quiet",
            "--porcelain",
            push_to,
            &format!("{}:{refname}", planned.commit),
        ],
        &[],
        None,
    );
    let readback = git(
        Bounds {
            cancel: None,
            deadline: Some(Instant::now() + Duration::from_secs(20)),
        },
        repo,
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
        (_, Ok(remote)) if remote == planned.commit => Ok(()),
        (Ok(_), Ok(remote)) => Err(SuggestionError::Uncertain(format!(
            "the push reported success but the branch reads {remote}"
        ))),
        (Err(SuggestionError::Git(message)), Ok(_)) => {
            Err(SuggestionError::PushRejected(rejection_reason(&message)))
        }
        (Err(stop @ (SuggestionError::Cancelled | SuggestionError::TimedOut)), Ok(remote))
            if remote == prepared.head =>
        {
            Err(stop)
        }
        (Err(error), Ok(_)) => Err(SuggestionError::PushRejected(error.to_string())),
        (_, Err(error)) => Err(SuggestionError::Uncertain(format!(
            "the branch could not be read back ({error})"
        ))),
    }
}

fn intent(context: &EditContextV27, prepared: &PreparedV27, planned: &PlanV27) -> PrWriteIntent {
    let (command, kind) = match context.mode {
        EditModeV27::File(_) => (JOURNAL_EDIT, "edit"),
        EditModeV27::Resolve => (JOURNAL_RESOLVE, "resolve"),
    };
    let mut values = BTreeMap::from([
        ("branch".into(), context.branch.clone()),
        ("commit".into(), planned.commit.clone()),
        ("remote".into(), prepared.remote.push_url.clone()),
        ("tree".into(), planned.tree.clone()),
    ]);
    if let Some(base) = &prepared.base {
        values.insert("base".into(), base.clone());
    }
    PrWriteIntent {
        command: command.into(),
        selected_resource: format!("{kind}/{}", planned.commit),
        observed_head: context.head.clone(),
        values,
    }
}

/// Journals and runs the push. A confirmed push removes the private
/// workspace; every other outcome keeps the edited files.
pub fn push(
    database: &Path,
    context: &EditContextV27,
    prepared: &PreparedV27,
    planned: &PlanV27,
    cancel: &CancellationToken,
) -> PushOutcomeV27 {
    let Some(target) = &context.target else {
        return PushOutcomeV27::Refused(
            "The verified account or PR identity is incomplete; nothing was pushed.".into(),
        );
    };
    if planned.permission.denied() {
        return PushOutcomeV27::Refused(format!(
            "{}: GitHub reports no push access; nothing was pushed.",
            planned.permission.verdict()
        ));
    }
    let intent = intent(context, prepared, planned);
    let mut journal = match PrWriteJournal::open(database) {
        Ok(journal) => journal,
        Err(error) => {
            return PushOutcomeV27::Refused(format!(
                "The write journal is unavailable ({error}); nothing was pushed."
            ))
        }
    };
    let permit = match journal.begin(target, &intent) {
        Ok(permit) => permit,
        Err(error) => {
            return PushOutcomeV27::Refused(format!(
                "Another write for this PR is pending or uncertain ({error}); inspect it first. Nothing was pushed."
            ))
        }
    };
    if let Err(error) = journal.claim(&permit, target, &intent) {
        return PushOutcomeV27::Refused(format!(
            "The write journal refused the attempt ({error}); nothing was pushed."
        ));
    }
    let attempt = permit.marker().attempt_id;
    match push_branch(context, prepared, planned, cancel) {
        Ok(()) => {
            let effect = PrConfirmedEffect {
                kind: PrEffectKind::BranchContent,
                resource_id: planned.commit.clone(),
                node_id: None,
            };
            let _ = std::fs::remove_dir_all(&prepared.dir);
            match journal.mark_confirmed(&permit, &effect) {
                Ok(()) => PushOutcomeV27::Confirmed {
                    commit: planned.commit.clone(),
                    attempt,
                },
                Err(error) => PushOutcomeV27::Uncertain(format!(
                    "pushed {} but the journal could not record it ({error})",
                    planned.commit
                )),
            }
        }
        Err(error) if error.nothing_written() => {
            let message = error.to_string();
            match journal.mark_not_written(&permit) {
                Ok(()) => PushOutcomeV27::NotWritten(message),
                Err(_) => PushOutcomeV27::Uncertain(message),
            }
        }
        Err(error) => {
            let _ = journal.mark_uncertain(&permit);
            PushOutcomeV27::Uncertain(error.to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// View

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BranchEditActionV27 {
    /// Footer: edit the selected file of the PR Files tab.
    OpenEdit,
    /// Footer: resolve the PR's conflicts with its base.
    OpenResolve,
    Select(usize),
    Move(i32),
    Edit,
    Message,
    MessageDone,
    Plan,
    Commit,
    Confirm,
    KeepPlan,
    Cancel,
    Discard,
    Scroll(i32),
    Back,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Request {
    Prepare,
    Plan,
    Push,
}

enum JobResult {
    Prepared(Result<Box<PreparedV27>, StepFailureV27>),
    Planned(Result<Box<PlanV27>, StepFailureV27>),
    Pushed(PushOutcomeV27),
}

struct Job {
    push: bool,
    cancel: CancellationToken,
    slot: Arc<Mutex<Option<JobResult>>>,
}
impl Drop for Job {
    fn drop(&mut self) {
        // Closing the view cancels preparing and planning. A push already
        // started still reads its branch back and closes its journal record.
        if !self.push {
            self.cancel.cancel();
        }
    }
}

pub struct PrBranchEditViewV27 {
    pub context: EditContextV27,
    pub prepared: Option<PreparedV27>,
    pub planned: Option<PlanV27>,
    pub message: String,
    pub editing_message: bool,
    pub cursor: usize,
    pub scroll: usize,
    pub confirming: bool,
    pub done: bool,
    pub status: String,
    pub status_role: SemanticToken,
    pub hits: Vec<(Rect, BranchEditActionV27)>,
    /// The work file the main loop opens in the external editor.
    pub editor_request: Option<PathBuf>,
    request: Option<Request>,
    job: Option<Job>,
}

impl PrBranchEditViewV27 {
    pub fn new(context: EditContextV27) -> Self {
        let message = match &context.mode {
            EditModeV27::File(path) => format!("Edit {path}"),
            EditModeV27::Resolve => format!(
                "Merge branch '{}' into {}",
                context.base_ref, context.branch
            ),
        };
        Self {
            context,
            prepared: None,
            planned: None,
            message,
            editing_message: false,
            cursor: 0,
            scroll: 0,
            confirming: false,
            done: false,
            status: "Preparing the isolated revision • x Cancel".into(),
            status_role: SemanticToken::Running,
            hits: vec![],
            editor_request: None,
            request: Some(Request::Prepare),
            job: None,
        }
    }
    pub fn busy(&self) -> bool {
        self.job.is_some() || self.request.is_some()
    }
    fn enabled(&self, action: BranchEditActionV27) -> bool {
        use BranchEditActionV27::*;
        match action {
            Back | Quit | Scroll(_) => true,
            OpenEdit | OpenResolve => false,
            MessageDone => self.editing_message,
            Cancel => self.job.is_some(),
            _ if self.busy() || self.done => false,
            Select(_) | Move(_) => !self.confirming,
            Edit | Message | Discard => self.prepared.is_some() && !self.confirming,
            Plan => self.prepared.is_some() && !self.confirming && !self.message.trim().is_empty(),
            Commit => self.planned.is_some() && !self.confirming,
            Confirm => self.confirming && self.denied().is_none(),
            KeepPlan => self.confirming,
        }
    }
    /// TASK-108: why Confirm push is disabled, when GitHub denies the push.
    fn denied(&self) -> Option<String> {
        self.planned
            .as_ref()
            .map(|planned| &planned.permission)
            .filter(|permission| permission.denied())
            .map(|permission| {
                format!(
                    "Confirm push unavailable • {}: GitHub reports no push access for your account. Nothing was pushed • n Keep plan",
                    permission.verdict()
                )
            })
    }
    fn invalidate_plan(&mut self, why: &str) {
        if self.planned.take().is_some() {
            self.status = format!("{why} • p Plan again before committing");
            self.status_role = SemanticToken::Pending;
        }
        self.confirming = false;
    }
    fn kept(&self) -> String {
        self.prepared.as_ref().map_or_else(
            || "nothing was kept".into(),
            |prepared| {
                format!(
                    "your edit is kept in {}",
                    prepared.dir.join("files").display()
                )
            },
        )
    }
}

/// The edit target of the PR Files tab, or why there is none.
pub fn context(app: &App, resolve: bool) -> Result<EditContextV27, String> {
    let github = app.github.as_ref().ok_or("Open a pull request first")?;
    let page = github
        .pr
        .as_ref()
        .filter(|page| page.tab == crate::github_workspace::PrTab::Files)
        .ok_or("Open the PR's Files tab first")?;
    let resource = github
        .scoped_pr_resource
        .as_ref()
        .ok_or("Branch edits need the PR opened from a repository set; its Files are not loaded")?;
    let parent = resource.observed().parent();
    let mode = if resolve {
        EditModeV27::Resolve
    } else {
        let row = app.inbox_list.selected;
        let path = resource
            .file_path_at(row)
            .or_else(|| page.file_path_at(row))
            .ok_or("Select a changed file first")?;
        let path = String::from_utf8(path.0)
            .map_err(|_| "This file's path is not UTF-8; edit it in a local clone".to_owned())?;
        validate_path(&path)?;
        EditModeV27::File(path)
    };
    let checkout = app
        .workspace
        .as_ref()
        .map(|workspace| workspace.directory.clone())
        .ok_or("Run ghtui inside a clone of this repository to edit its PR branch")?;
    let summary = parent.summary();
    Ok(EditContextV27 {
        host: github.identity.host.clone(),
        base_repository: parent.repository().to_owned(),
        head_repository: parent.head_repository_name().map(str::to_owned),
        number: parent.number(),
        branch: parent.head_ref().to_owned(),
        head: parent.head_sha().to_owned(),
        base_ref: summary.base_ref.clone(),
        base: parent.base_tip_sha().to_owned(),
        checkout: Some(checkout),
        target: PrWriteTarget::new(&github.identity, summary).ok(),
        mode,
    })
}

/// `E Edit file` and `R Resolve conflicts` for the PR Files key bar. They
/// rank last, so they leave a narrow bar first; Commands always lists them.
pub fn footer_keys(app: &App) -> Vec<crate::footer::KeyControl> {
    [
        (
            false,
            "E",
            "Edit file",
            "Edit",
            BranchEditActionV27::OpenEdit,
            95,
        ),
        (
            true,
            "R",
            "Resolve conflicts",
            "Resolve",
            BranchEditActionV27::OpenResolve,
            96,
        ),
    ]
    .into_iter()
    .map(|(resolve, key, label, compact, action, priority)| {
        crate::footer::KeyControl::new(key, label, crate::app::Action::PrBranchEditV27(action))
            .with_compact(compact)
            .with_priority(priority)
            .disabled_by(context(app, resolve).err())
    })
    .collect()
}

pub fn open(app: &mut App, resolve: bool) {
    match context(app, resolve) {
        Ok(context) => app.pr_branch_edit_v27 = Some(PrBranchEditViewV27::new(context)),
        Err(reason) => app.dispatch(crate::app::Action::Explain(reason)),
    }
}

pub fn event(app: &mut App, event: &Event) -> bool {
    let Some(view) = app.pr_branch_edit_v27.as_mut() else {
        return false;
    };
    use BranchEditActionV27 as A;
    let action = match event {
        Event::Resize(..) => {
            view.hits.clear();
            None
        }
        Event::Paste(pasted) if view.editing_message => {
            let line = pasted.lines().next().unwrap_or_default();
            view.message.push_str(line);
            view.message = view.message.chars().take(MAX_MESSAGE_CHARS).collect();
            view.invalidate_plan("Message changed");
            None
        }
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                Some(A::Quit)
            } else if view.editing_message {
                match key.code {
                    KeyCode::Enter | KeyCode::Esc => Some(A::MessageDone),
                    KeyCode::Backspace => {
                        view.message.pop();
                        view.invalidate_plan("Message changed");
                        None
                    }
                    KeyCode::Char(character) => {
                        if view.message.chars().count() < MAX_MESSAGE_CHARS {
                            view.message.push(character);
                        }
                        view.invalidate_plan("Message changed");
                        None
                    }
                    _ => None,
                }
            } else {
                match key.code {
                    KeyCode::Char('q') => Some(A::Quit),
                    KeyCode::Esc | KeyCode::Char('b') => Some(A::Back),
                    KeyCode::Up | KeyCode::Char('k') => Some(A::Move(-1)),
                    KeyCode::Down | KeyCode::Char('j') => Some(A::Move(1)),
                    KeyCode::PageUp => Some(A::Scroll(-10)),
                    KeyCode::PageDown => Some(A::Scroll(10)),
                    KeyCode::Char('e') => Some(A::Edit),
                    KeyCode::Char('m') => Some(A::Message),
                    KeyCode::Char('p') => Some(A::Plan),
                    KeyCode::Char('C') => Some(A::Commit),
                    KeyCode::Char('y') => Some(A::Confirm),
                    KeyCode::Char('n') => Some(A::KeepPlan),
                    KeyCode::Char('x') => Some(A::Cancel),
                    KeyCode::Char('D') => Some(A::Discard),
                    _ => None,
                }
            }
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => view
                .hits
                .iter()
                .find(|(rect, _)| rect.contains((mouse.column, mouse.row).into()))
                .map(|(_, action)| *action),
            MouseEventKind::ScrollDown => Some(A::Scroll(3)),
            MouseEventKind::ScrollUp => Some(A::Scroll(-3)),
            _ => None,
        },
        _ => None,
    };
    if let Some(action) = action {
        dispatch(app, action);
    }
    true
}

pub fn dispatch(app: &mut App, action: BranchEditActionV27) {
    use BranchEditActionV27::*;
    match action {
        OpenEdit => return open(app, false),
        OpenResolve => return open(app, true),
        Quit => {
            if let Some(view) = app.pr_branch_edit_v27.take() {
                app.status = format!("Branch edit closed • {}", view.kept());
            }
            app.quit = true;
            return;
        }
        Back => {
            if let Some(view) = app.pr_branch_edit_v27.take() {
                app.status = if view.done {
                    view.status.clone()
                } else if view.job.as_ref().is_some_and(|job| job.push) {
                    "Branch edit closed while pushing • the push still reads its branch back and closes its journal record".into()
                } else {
                    format!(
                        "Branch edit closed • nothing was pushed; {} • E Edit file or R Resolve conflicts reopens it",
                        view.kept()
                    )
                };
            }
            return;
        }
        _ => {}
    }
    let Some(view) = app.pr_branch_edit_v27.as_mut() else {
        return;
    };
    view.hits.clear();
    if !view.enabled(action) {
        view.status = if view.busy() && action != Cancel {
            "Busy • x Cancel stops the running step".into()
        } else if view.done {
            "This edit is finished • Esc Back".into()
        } else if let Some(reason) = view.denied().filter(|_| action == Confirm) {
            view.status = reason;
            view.status_role = SemanticToken::Failure;
            return;
        } else if view.confirming {
            "Finish the confirmation first: y Confirm push or n Keep plan".into()
        } else {
            match action {
                Plan if view.prepared.is_some() => {
                    "Plan unavailable • the commit message is empty; m Message".into()
                }
                Commit => "Commit & push unavailable • p Plan first".into(),
                Cancel => "Nothing is running".into(),
                _ => "Unavailable now • nothing written".into(),
            }
        };
        view.status_role = SemanticToken::Muted;
        return;
    }
    match action {
        Select(index) => {
            let count = view.prepared.as_ref().map_or(0, |p| p.files.len());
            if index < count {
                view.cursor = index;
            }
        }
        Move(delta) => {
            let count = view.prepared.as_ref().map_or(0, |p| p.files.len());
            view.cursor = view
                .cursor
                .saturating_add_signed(delta as isize)
                .min(count.saturating_sub(1));
        }
        Scroll(delta) => view.scroll = view.scroll.saturating_add_signed(delta as isize),
        Edit => {
            let prepared = view.prepared.as_ref().expect("enabled only when prepared");
            if let Some(file) = prepared.files.get(view.cursor) {
                view.editor_request = Some(file.work.clone());
                view.status = format!("Opening {} in your editor", file.path);
                view.status_role = SemanticToken::Running;
            }
        }
        Message => {
            view.editing_message = true;
            view.status = "Type the commit message • Backspace deletes • Enter Done".into();
            view.status_role = SemanticToken::Pending;
        }
        MessageDone => {
            view.editing_message = false;
            view.status = if view.message.trim().is_empty() {
                "The commit message is empty • m Message to write one".into()
            } else {
                "Message set • p Plan builds the commit; nothing is written".into()
            };
            view.status_role = SemanticToken::Muted;
        }
        Plan => {
            view.request = Some(Request::Plan);
            view.editing_message = false;
            view.status =
                "Planning • building the commit in the isolated revision • x Cancel".into();
            view.status_role = SemanticToken::Running;
        }
        Commit => {
            let planned = view.planned.as_ref().expect("enabled only with a plan");
            let prepared = view.prepared.as_ref().expect("planned implies prepared");
            view.confirming = true;
            let permission = &planned.permission;
            let next = if permission.denied() {
                "y Confirm push is disabled • n Keep plan"
            } else {
                "y Confirm push • n Keep plan"
            };
            view.status = format!(
                "Push commit {} to {} on {} ({}) only if it still points at {}? No force push. {}. {next}",
                short(&planned.commit),
                view.context.branch,
                prepared.remote.name,
                prepared.remote.push_url,
                short(&prepared.head),
                permission.verdict(),
            );
            view.status_role = if permission.denied() {
                SemanticToken::Failure
            } else {
                SemanticToken::Conflict
            };
        }
        KeepPlan => {
            view.confirming = false;
            view.status = "Plan kept • nothing was written".into();
            view.status_role = SemanticToken::Muted;
        }
        Confirm => {
            view.confirming = false;
            view.request = Some(Request::Push);
            view.status = "Pushing • the journal records this write first • x Cancel".into();
            view.status_role = SemanticToken::Running;
        }
        Cancel => {
            if let Some(job) = &view.job {
                job.cancel.cancel();
                view.status = if job.push {
                    "Cancelling the push • reading the branch back to learn the outcome".into()
                } else {
                    format!("Cancelled • nothing was written; {}", view.kept())
                };
                view.status_role = SemanticToken::Cancelled;
            }
        }
        Discard => {
            let dir = view.prepared.as_ref().map(|prepared| prepared.dir.clone());
            if let Some(dir) = dir {
                let _ = std::fs::remove_dir_all(&dir);
                view.prepared = None;
                view.planned = None;
                view.cursor = 0;
                view.status = format!(
                    "Discarded the isolated revision {} • nothing was pushed • Esc Back, then E or R to start again",
                    dir.display()
                );
                view.status_role = SemanticToken::Cancelled;
            }
        }
        OpenEdit | OpenResolve | Back | Quit => unreachable!("handled above"),
    }
}

/// The work file the external editor should open, taken once.
pub fn take_editor_request(app: &mut App) -> Option<PathBuf> {
    app.pr_branch_edit_v27
        .as_mut()
        .and_then(|view| view.editor_request.take())
}

/// The external editor's result for the requested work file. The file stays
/// in place whatever happened.
pub fn editor_finished(app: &mut App, result: Result<(), String>) {
    let Some(view) = app.pr_branch_edit_v27.as_mut() else {
        return;
    };
    view.hits.clear();
    let path = view
        .prepared
        .as_ref()
        .and_then(|prepared| prepared.files.get(view.cursor))
        .map(|file| (file.path.clone(), file.work.clone()));
    let Some((path, work)) = path else {
        return;
    };
    match result {
        Ok(()) => {
            view.planned = None;
            view.confirming = false;
            view.status =
                format!("Edited {path} • p Plan shows the resulting diff; nothing is written");
            view.status_role = SemanticToken::Success;
        }
        Err(error) => {
            view.planned = None;
            view.confirming = false;
            view.status = format!(
                "Editor failed ({error}). Whatever it saved is kept in {} • e Edit to try again; nothing was written.",
                work.display()
            );
            view.status_role = SemanticToken::Failure;
        }
    }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

fn unix_date() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    format!("{seconds} +0000")
}

fn data_root(database: &Path) -> PathBuf {
    database
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("pr-branch-edits")
}

/// Starts requested work and collects finished work. Called once per loop.
pub fn tick(app: &mut App, database: &Path) {
    let Some(view) = app.pr_branch_edit_v27.as_mut() else {
        return;
    };
    let finished = view
        .job
        .as_ref()
        .and_then(|job| job.slot.lock().ok().and_then(|mut slot| slot.take()));
    if let Some(result) = finished {
        view.job = None;
        view.hits.clear();
        match result {
            JobResult::Prepared(Ok(prepared)) => {
                let reused = prepared.files.iter().filter(|file| file.reused).count();
                let what = match &view.context.mode {
                    EditModeV27::File(path) => format!("{path} is ready to edit"),
                    EditModeV27::Resolve => {
                        format!("{} conflicted file(s) to resolve", prepared.files.len())
                    }
                };
                view.status = if reused > 0 {
                    format!(
                        "Isolated revision ready • {what} • your kept edit from an earlier attempt is reused • e Edit, then p Plan"
                    )
                } else {
                    format!("Isolated revision ready • {what} • e Edit, then p Plan; nothing is written")
                };
                view.status_role = SemanticToken::Success;
                view.prepared = Some(*prepared);
                view.cursor = 0;
            }
            JobResult::Prepared(Err(failure)) | JobResult::Planned(Err(failure)) => {
                view.status = failure.message;
                view.status_role = failure.role;
            }
            JobResult::Planned(Ok(planned)) => {
                view.status = format!(
                    "Planned commit {} • nothing written yet • {} • C Commit & push to confirm",
                    short(&planned.commit),
                    planned.permission.verdict(),
                );
                view.status_role = if planned.permission.denied() {
                    SemanticToken::Failure
                } else {
                    SemanticToken::Success
                };
                view.planned = Some(*planned);
                view.scroll = 0;
            }
            JobResult::Pushed(outcome) => {
                view.planned = None;
                let kept = view.kept();
                let (status, role) = match outcome {
                    PushOutcomeV27::Confirmed { commit, attempt } => {
                        view.done = true;
                        view.prepared = None;
                        (
                            format!(
                                "Pushed {commit} to {} • journal #{attempt} confirmed • refresh the PR to see it",
                                view.context.branch
                            ),
                            SemanticToken::Success,
                        )
                    }
                    PushOutcomeV27::NotWritten(message) | PushOutcomeV27::Refused(message) => (
                        format!(
                            "{} The PR branch is unchanged; {kept}.",
                            message.split_whitespace().collect::<Vec<_>>().join(" ")
                        ),
                        if message.contains("head moved") {
                            SemanticToken::Conflict
                        } else {
                            SemanticToken::Failure
                        },
                    ),
                    PushOutcomeV27::Uncertain(message) => (
                        format!(
                            "The push outcome is unknown: {message}. The journal marks it uncertain; {kept}."
                        ),
                        SemanticToken::Conflict,
                    ),
                };
                app.status = status.clone();
                view.status = status;
                view.status_role = role;
            }
        }
        return;
    }
    let Some(request) = view.request.take() else {
        return;
    };
    let cancel = CancellationToken::default();
    let slot = Arc::new(Mutex::new(None));
    let (worker_cancel, worker_slot) = (cancel.clone(), slot.clone());
    let context = view.context.clone();
    match request {
        Request::Prepare => {
            let root = data_root(database);
            std::thread::spawn(move || {
                let result = prepare(&context, &root, &worker_cancel).map(Box::new);
                if let Ok(mut slot) = worker_slot.lock() {
                    *slot = Some(JobResult::Prepared(result));
                }
            });
        }
        Request::Plan => {
            let Some(prepared) = view.prepared.clone() else {
                return;
            };
            let message = view.message.clone();
            std::thread::spawn(move || {
                let deadline = Some(Instant::now() + PLAN_LIMIT);
                let result = context
                    .checkout
                    .as_deref()
                    .ok_or_else(|| StepFailureV27::from("No local checkout is open."))
                    .and_then(|checkout| {
                        committer_identity(checkout, Some(&worker_cancel), deadline)
                            .map_err(StepFailureV27::from)
                    })
                    .and_then(|(name, email)| {
                        plan(
                            &prepared,
                            &message,
                            (&name, &email),
                            &unix_date(),
                            &worker_cancel,
                        )
                    })
                    .and_then(|mut planned| {
                        // TASK-108: read the head repository's permission
                        // before the plan (and its Confirm) is shown.
                        planned.permission = read_push_permission(&context, &worker_cancel);
                        if worker_cancel.is_cancelled() {
                            return Err(StepFailureV27::from(SuggestionError::Cancelled));
                        }
                        Ok(planned)
                    })
                    .map(Box::new);
                if let Ok(mut slot) = worker_slot.lock() {
                    *slot = Some(JobResult::Planned(result));
                }
            });
        }
        Request::Push => {
            let (Some(prepared), Some(planned)) = (view.prepared.clone(), view.planned.clone())
            else {
                return;
            };
            let database = database.to_owned();
            std::thread::spawn(move || {
                let outcome = push(&database, &context, &prepared, &planned, &worker_cancel);
                if let Ok(mut slot) = worker_slot.lock() {
                    *slot = Some(JobResult::Pushed(outcome));
                }
            });
        }
    }
    view.job = Some(Job {
        push: request == Request::Push,
        cancel,
        slot,
    });
}

/// One row of the detail pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetailRowV27 {
    pub text: String,
    pub role: SemanticToken,
    pub actor: Option<ActorIdentity>,
}

pub fn detail_lines(view: &PrBranchEditViewV27) -> Vec<DetailRowV27> {
    let row = |text: String, role| DetailRowV27 {
        text,
        role,
        actor: None,
    };
    let mut lines = vec![];
    let Some(prepared) = &view.prepared else {
        lines.push(row(
            format!(
                "PR #{} • head {} on {} • base {} ({})",
                view.context.number,
                short(&view.context.head),
                view.context.branch,
                short(&view.context.base),
                view.context.base_ref
            ),
            SemanticToken::Reference,
        ));
        return lines;
    };
    let Some(planned) = &view.planned else {
        lines.push(row(
            format!("Isolated revision {}", prepared.repo.display()),
            SemanticToken::Reference,
        ));
        lines.push(row(
            "Your checkout, index, refs and config are never used for this edit".into(),
            SemanticToken::Muted,
        ));
        lines.push(row(
            format!("Head {} (PR head, {})", prepared.head, view.context.branch),
            SemanticToken::Head,
        ));
        if let Some(base) = &prepared.base {
            lines.push(row(
                format!("Base {base} ({})", view.context.base_ref),
                SemanticToken::Ancestor,
            ));
        }
        if let Some(file) = prepared.files.get(view.cursor) {
            lines.push(row(
                format!("Edit file {}", file.work.display()),
                SemanticToken::Changed,
            ));
        }
        return lines;
    };
    let committer = ActorIdentity::local(
        &crate::git::GitActor {
            name: planned.committer.0.clone(),
            email: planned.committer.1.clone(),
            timestamp: 0,
        },
        crate::identity::GitRole::Committer,
    );
    lines.push(row(
        format!("Plan commit {}", planned.commit),
        SemanticToken::Head,
    ));
    lines.push(row(
        format!("Parent {} (PR head)", planned.parents[0]),
        SemanticToken::Ancestor,
    ));
    if let Some(base) = planned.parents.get(1) {
        lines.push(row(
            format!("Parent {base} (base {})", view.context.base_ref),
            SemanticToken::Ancestor,
        ));
    }
    lines.push(row(
        format!(
            "Branch {} • only if it still points at {} • no force push",
            view.context.branch,
            short(&prepared.head)
        ),
        SemanticToken::Reference,
    ));
    lines.push(row(
        format!(
            "Push to {} {}",
            prepared.remote.name, prepared.remote.push_url
        ),
        SemanticToken::Reference,
    ));
    lines.push(row(planned.permission.verdict(), planned.permission.role()));
    lines.push(row(planned.permission.reason.clone(), SemanticToken::Muted));
    lines.push(DetailRowV27 {
        actor: Some(committer),
        ..row(
            format!(
                "Committer {} <{}> • date {} • local Git identity",
                planned.committer.0, planned.committer.1, planned.date
            ),
            SemanticToken::Actor,
        )
    });
    lines.push(row(
        format!("Message: {}", planned.message.trim()),
        SemanticToken::Normal,
    ));
    for (path, blob) in &planned.files {
        lines.push(row(
            format!("File {path} → blob {blob}"),
            SemanticToken::Changed,
        ));
    }
    lines.push(row(
        "Nothing is written until y Confirm push • resulting diff against the PR head:".into(),
        SemanticToken::Muted,
    ));
    for line in &planned.diff {
        let role = if line.starts_with("+++") || line.starts_with("---") {
            SemanticToken::Reference
        } else if line.starts_with('+') {
            SemanticToken::Added
        } else if line.starts_with('-') {
            SemanticToken::Removed
        } else {
            SemanticToken::Reference
        };
        lines.push(row(line.clone(), role));
    }
    lines
}

pub fn draw(frame: &mut Frame, app: &mut App) -> bool {
    let theme = app.theme();
    let avatars = &app.avatar_states;
    let Some(view) = app.pr_branch_edit_v27.as_mut() else {
        return false;
    };
    let area = frame.area();
    view.hits.clear();
    frame.render_widget(Clear, area);
    let kind = match &view.context.mode {
        EditModeV27::File(_) => "Edit file",
        EditModeV27::Resolve => "Resolve conflicts",
    };
    frame.render_widget(
        crate::ui_chrome::pane_block(theme, &[TITLE, kind], true),
        area,
    );
    let inside = area.inner(Margin::new(1, 1));
    if inside.width == 0 || inside.height == 0 {
        return true;
    }
    use BranchEditActionV27::*;
    let controls: Vec<(&str, BranchEditActionV27)> = if view.editing_message {
        vec![("[Enter Done]", MessageDone)]
    } else if view.confirming {
        vec![
            ("[y Confirm push]", Confirm),
            ("[n Keep plan]", KeepPlan),
            ("[Esc Back]", Back),
            ("[q Quit]", Quit),
        ]
    } else {
        vec![
            ("[e Edit]", Edit),
            ("[m Message]", Message),
            ("[p Plan]", Plan),
            ("[C Commit & push]", Commit),
            ("[x Cancel]", Cancel),
            ("[D Discard]", Discard),
            ("[PgUp]", Scroll(-10)),
            ("[PgDn]", Scroll(10)),
            ("[Esc Back]", Back),
            ("[q Quit]", Quit),
        ]
    };
    let mut x = inside.x;
    let mut y = inside.y;
    for (label, action) in controls {
        let width = label.chars().count() as u16;
        if x > inside.x && x.saturating_add(width) > inside.right() {
            x = inside.x;
            y += 1;
        }
        if y >= inside.bottom() {
            return true;
        }
        let rect = Rect::new(x, y, width.min(inside.right() - x), 1);
        frame.render_widget(
            Paragraph::new(label).style(theme.style(if view.enabled(action) {
                SemanticToken::Reference
            } else {
                SemanticToken::Muted
            })),
            rect,
        );
        view.hits.push((rect, action));
        x += width + 1;
    }
    y += 1;
    if y >= inside.bottom() {
        return true;
    }
    let status = format!("{} {}", view.status_role.symbol(), view.status);
    let width = Line::from(status.as_str()).width() as u16;
    let rows = if width <= inside.width {
        1
    } else {
        width.div_ceil(inside.width.max(1)) + 1
    };
    let height = rows.clamp(1, 4).min(inside.bottom() - y);
    frame.render_widget(
        Paragraph::new(status)
            .style(theme.style(view.status_role))
            .wrap(ratatui::widgets::Wrap { trim: true }),
        Rect::new(inside.x, y, inside.width, height),
    );
    y += height;
    if y >= inside.bottom() {
        return true;
    }
    let message = if view.editing_message {
        format!("Message (typing): {}▏", view.message)
    } else {
        format!("Message: {}", view.message)
    };
    let rect = Rect::new(inside.x, y, inside.width, 1);
    frame.render_widget(
        Paragraph::new(message).style(theme.style(if view.editing_message {
            SemanticToken::Focus
        } else {
            SemanticToken::Normal
        })),
        rect,
    );
    if !view.editing_message && !view.confirming {
        view.hits.push((rect, Message));
    }
    y += 1;
    if let Some(prepared) = &view.prepared {
        let count = prepared.files.len();
        let rows = count.min(6);
        let first = view
            .cursor
            .saturating_sub(rows.saturating_sub(1))
            .min(count.saturating_sub(rows));
        for index in first..first + rows {
            if y >= inside.bottom() {
                return true;
            }
            let file = &prepared.files[index];
            let selected = index == view.cursor;
            let state = if file.conflicted {
                "conflicted"
            } else {
                "edit"
            };
            let kept = if file.reused { " • kept edit" } else { "" };
            let text = format!(
                "{}[{state}] {}{kept}",
                if selected { "▌" } else { " " },
                file.path
            );
            let rect = Rect::new(inside.x, y, inside.width, 1);
            frame.render_widget(
                Paragraph::new(text).style(if selected {
                    theme.selected(if file.conflicted {
                        SemanticToken::Conflict
                    } else {
                        SemanticToken::Changed
                    })
                } else {
                    theme.style(if file.conflicted {
                        SemanticToken::Conflict
                    } else {
                        SemanticToken::Changed
                    })
                }),
                rect,
            );
            if !view.confirming {
                view.hits.push((rect, Select(index)));
            }
            y += 1;
        }
    }
    let lines = detail_lines(view);
    let height = inside.bottom().saturating_sub(y) as usize;
    view.scroll = view.scroll.min(lines.len().saturating_sub(height));
    for line in lines.iter().skip(view.scroll).take(height) {
        let mut rect = Rect::new(inside.x, y, inside.width, 1);
        if let Some(identity) = &line.actor {
            let lead =
                crate::suggestion_apply_v1::avatar_lead(frame, avatars, theme, identity, rect);
            rect.x += lead;
            rect.width -= lead;
        }
        frame.render_widget(
            Paragraph::new(line.text.as_str()).style(theme.style(line.role)),
            rect,
        );
        y += 1;
    }
    true
}

#[cfg(test)]
#[path = "pr_branch_edit_v27_tests.rs"]
mod tests;
